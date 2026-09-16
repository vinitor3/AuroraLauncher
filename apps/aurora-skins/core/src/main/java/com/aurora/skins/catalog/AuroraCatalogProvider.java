package com.aurora.skins.catalog;

import com.aurora.skins.api.SkinEntry;
import com.aurora.skins.api.SkinModel;
import com.aurora.skins.api.SkinPage;
import com.aurora.skins.api.SkinProvider;
import com.aurora.skins.api.PlayerSuggestion;
import com.aurora.skins.storage.AuroraSkinStore;
import com.aurora.skins.storage.SkinValidationException;

import java.io.BufferedInputStream;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.net.HttpURLConnection;
import java.net.InetAddress;
import java.net.URI;
import java.net.URLEncoder;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Base64;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.CompletionException;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.ThreadFactory;

/** HTTPS catalog provider with bounded I/O, metadata cache, and an original offline fallback. */
public final class AuroraCatalogProvider implements SkinProvider {
    public static final String DEFAULT_ENDPOINT =
        "https://aurora-api.aurora-edge-proxy.workers.dev/v1/skins/catalog";
    private static final int MAX_RESPONSE_BYTES = 256 * 1024;
    private static final int MAX_SKIN_BYTES = 2 * 1024 * 1024;
    private static final long CACHE_MILLIS = 5L * 60L * 1000L;
    private static final long FAILURE_COOLDOWN_MILLIS = 30L * 1000L;
    private static final ExecutorService EXECUTOR = Executors.newFixedThreadPool(2, new ThreadFactory() {
        private int sequence;
        @Override public synchronized Thread newThread(Runnable runnable) {
            Thread thread = new Thread(runnable, "Aurora-Skins-Catalog-" + (++sequence));
            thread.setDaemon(true);
            return thread;
        }
    });

    private final AuroraSkinStore store;
    private final BuiltinSkinCatalog fallback;
    private final URI endpoint;
    private final Map<String, CacheEntry> cache = new ConcurrentHashMap<String, CacheEntry>();
    private final SearchDebouncer debouncer = new SearchDebouncer();
    private volatile long unavailableUntil;
    private volatile boolean online;

    public AuroraCatalogProvider(AuroraSkinStore store) {
        this(store, configuredEndpoint());
    }

    AuroraCatalogProvider(AuroraSkinStore store, String endpoint) {
        this.store = store;
        this.fallback = new BuiltinSkinCatalog(store);
        this.endpoint = validateEndpoint(endpoint);
    }

    @Override public String id() { return "aurora-catalog"; }
    public boolean online() { return online; }

    public CompletableFuture<SkinPage> searchDebounced(final String query, final int page, final int pageSize) {
        return debouncer.submit(() -> search(query, page, pageSize));
    }
    public void cancelSearch() { debouncer.cancel(); }

    public CompletableFuture<List<PlayerSuggestion>> searchPlayers(final String query) {
        return CompletableFuture.supplyAsync(() -> {
            String clean = query == null ? "" : query.trim();
            if (!clean.matches("[A-Za-z0-9_]{3,16}")) return Collections.<PlayerSuggestion>emptyList();
            try {
                URI uri = endpoint.resolve("players?format=tsv&query=" + URLEncoder.encode(clean, "UTF-8"));
                String[] lines = fetchText(uri).replace("\r", "").split("\n");
                if (lines.length == 0 || !"AURORA_PLAYERS_V1".equals(lines[0])) throw new IOException("Busca inválida.");
                List<PlayerSuggestion> result = new ArrayList<PlayerSuggestion>();
                for (int i = 1; i < lines.length; i++) {
                    if (lines[i].trim().isEmpty()) continue;
                    String[] fields = lines[i].split("\t", -1);
                    if (fields.length != 4 || !fields[0].matches("[A-Za-z0-9_]{1,16}")
                            || !fields[1].matches("(?:[0-9a-f]{32})?")) throw new IOException("Jogador inválido.");
                    result.add(new PlayerSuggestion(fields[0], fields[1], fields[2], decode(fields[3], 16384)));
                    if (result.size() > 12) throw new IOException("Muitos resultados.");
                }
                return result;
            } catch (IOException error) { throw new CompletionException(error); }
        }, EXECUTOR);
    }

    public CompletableFuture<SkinEntry> getPlayerProfile(final String identifier) {
        return CompletableFuture.supplyAsync(() -> {
            if (identifier == null || !identifier.matches("(?:[A-Za-z0-9_]{1,16}|[0-9a-fA-F]{32}|[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12})"))
                throw new CompletionException(new IOException("Nome ou UUID inválido."));
            try {
                URI uri = endpoint.resolve("profile/" + identifier + "?format=catalog");
                SkinPage page = parse(fetchText(uri), uri);
                if (page.entries().size() != 1) throw new IOException("Perfil sem skin.");
                return page.entries().get(0);
            } catch (IOException error) { throw new CompletionException(error); }
        }, EXECUTOR);
    }

    public CompletableFuture<List<PlayerSuggestion>> autocompleteSkinNames(String query) { return searchPlayers(query); }
    public String getSkinImageUrlFromProfile(SkinEntry profile) { return profile.source(); }

    @Override public CompletableFuture<SkinPage> search(final String query, final int page, final int pageSize) {
        if (query != null && query.trim().startsWith("@")) {
            final String name = query.trim().substring(1).trim();
            if (name.matches("(?:[0-9a-fA-F]{32}|[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12})")) {
                return getPlayerProfile(name).thenApply(entry -> new SkinPage(Collections.singletonList(entry), 0, false));
            }
            return autocompleteSkinNames(name).thenApply(players -> {
                List<SkinEntry> entries = new ArrayList<SkinEntry>();
                for (PlayerSuggestion player : players) entries.add(new SkinEntry("player-" + player.identifier(),
                    player.name(), player.provider(), SkinModel.WIDE, "aurora-player", player.identifier(),
                    "pending-" + player.identifier(), null, Collections.singletonList("player")));
                online = true;
                return new SkinPage(entries, 0, false);
            });
        }
        return CompletableFuture.supplyAsync(() -> {
            int current = Math.max(0, page);
            int size = Math.max(1, Math.min(24, pageSize));
            String safeQuery = bounded(query == null ? "" : query.trim(), 80);
            String key = safeQuery + "\n" + current + "\n" + size;
            CacheEntry cached = cache.get(key);
            long now = System.currentTimeMillis();
            if (cached != null && now - cached.createdAt < CACHE_MILLIS) return cached.page;
            if (now >= unavailableUntil) {
                try {
                    SkinPage result = fetchPage(safeQuery, current, size);
                    if (cache.size() >= 128) cache.clear();
                    cache.put(key, new CacheEntry(result, now));
                    online = true;
                    return result;
                } catch (IOException error) {
                    online = false;
                    unavailableUntil = now + FAILURE_COOLDOWN_MILLIS;
                    System.err.println("[Aurora Skins] Catálogo online temporariamente indisponível: "
                        + error.getClass().getSimpleName());
                }
            }
            try {
                return fallback.search(safeQuery, current, size);
            } catch (IOException error) {
                throw new CompletionException(new IOException("Não foi possível abrir o catálogo Aurora.", error));
            }
        }, EXECUTOR);
    }

    public CompletableFuture<SkinEntry> materialize(final SkinEntry entry) {
        return prepare(entry, true);
    }

    public CompletableFuture<SkinEntry> preview(final SkinEntry entry) { return prepare(entry, false); }

    private CompletableFuture<SkinEntry> prepare(final SkinEntry entry, final boolean importToLibrary) {
        if (entry != null && "aurora-player".equals(entry.providerId()))
            return getPlayerProfile(entry.source()).thenCompose(profile -> prepare(profile, importToLibrary));
        return CompletableFuture.supplyAsync(() -> {
            try {
                if (entry == null) throw new IOException("Skin inválida.");
                if (entry.file() != null && Files.isRegularFile(entry.file())) {
                    return importToLibrary ? store.importCatalogSkin(entry.file(), entry) : entry;
                }
                if (!"aurora-online".equals(entry.providerId())) {
                    throw new IOException("A origem desta skin não está disponível.");
                }
                Path downloaded = download(entry);
                try {
                    return importToLibrary ? store.importCatalogSkin(downloaded, entry) : store.cacheCatalogSkin(downloaded, entry);
                } finally {
                    Files.deleteIfExists(downloaded);
                }
            } catch (IOException | SkinValidationException error) {
                throw new CompletionException(error);
            }
        }, EXECUTOR);
    }

    private SkinPage fetchPage(String query, int page, int pageSize) throws IOException {
        String separator = endpoint.toString().contains("?") ? "&" : "?";
        URI requestUri = URI.create(endpoint + separator + "query="
            + URLEncoder.encode(query, "UTF-8") + "&page=" + page + "&pageSize=" + pageSize);
        return parse(fetchText(requestUri), requestUri);
    }

    private String fetchText(URI requestUri) throws IOException {
        requireSameOrigin(requestUri, endpoint);
        HttpURLConnection connection = open(requestUri);
        connection.setRequestProperty("Accept", "text/tab-separated-values");
        int status = connection.getResponseCode();
        if (status != 200) {
            closeQuietly(connection.getErrorStream());
            connection.disconnect();
            throw new IOException("HTTP " + status);
        }
        byte[] response;
        try (InputStream input = new BufferedInputStream(connection.getInputStream())) {
            response = readBounded(input, MAX_RESPONSE_BYTES);
        } finally {
            connection.disconnect();
        }
        return new String(response, StandardCharsets.UTF_8);
    }

    private Path download(SkinEntry entry) throws IOException {
        URI source = URI.create(entry.source());
        requireSameOrigin(source, endpoint);
        HttpURLConnection connection = open(source);
        connection.setRequestProperty("Accept", "image/png");
        int status = connection.getResponseCode();
        if (status != 200) {
            closeQuietly(connection.getErrorStream());
            connection.disconnect();
            throw new IOException("Download HTTP " + status);
        }
        Path directory = store.root().resolve("downloads");
        Files.createDirectories(directory);
        Path temporary = Files.createTempFile(directory, "catalog-", ".png.part");
        try (InputStream input = new BufferedInputStream(connection.getInputStream())) {
            byte[] bytes = readBounded(input, MAX_SKIN_BYTES);
            Files.write(temporary, bytes);
            return temporary;
        } catch (IOException error) {
            Files.deleteIfExists(temporary);
            throw error;
        } finally {
            connection.disconnect();
        }
    }

    static SkinPage parse(String payload, URI requestUri) throws IOException {
        String[] lines = payload.replace("\r", "").split("\n");
        if (lines.length < 2 || !"AURORA_SKINS_CATALOG_V1".equals(lines[0])) {
            throw new IOException("Resposta de catálogo inválida.");
        }
        String[] pagination = lines[1].split("\t", -1);
        if (pagination.length != 4 || !"page".equals(pagination[0]) || !"more".equals(pagination[2])) {
            throw new IOException("Paginação inválida.");
        }
        int page = parseInteger(pagination[1], 0, 100000);
        boolean hasMore = "1".equals(pagination[3]);
        List<SkinEntry> entries = new ArrayList<SkinEntry>();
        for (int index = 2; index < lines.length; index++) {
            if (lines[index].trim().isEmpty()) continue;
            String[] fields = lines[index].split("\t", -1);
            if (fields.length != 7 || !fields[0].matches("[a-z0-9_-]{1,64}")
                    || !fields[4].matches("[0-9a-f]{64}")) throw new IOException("Item de catálogo inválido.");
            String source = decode(fields[5], 2048);
            URI sourceUri;
            try { sourceUri = requestUri.resolve(source); }
            catch (IllegalArgumentException error) { throw new IOException("URL de skin inválida.", error); }
            requireSameOrigin(sourceUri, requestUri);
            String tagsValue = decode(fields[6], 512);
            List<String> tags = tagsValue.isEmpty() ? Collections.<String>emptyList()
                : Arrays.asList(tagsValue.split(","));
            entries.add(new SkinEntry(fields[0], decode(fields[1], 96), decode(fields[2], 96),
                SkinModel.fromId(fields[3]), "aurora-online", sourceUri.toString(), fields[4], null, tags));
            if (entries.size() > 24) throw new IOException("Catálogo excedeu o limite de itens.");
        }
        return new SkinPage(entries, page, hasMore);
    }

    private static HttpURLConnection open(URI uri) throws IOException {
        validateNetworkUri(uri);
        HttpURLConnection connection = (HttpURLConnection) uri.toURL().openConnection();
        connection.setConnectTimeout(2500);
        connection.setReadTimeout(12000);
        connection.setUseCaches(false);
        connection.setInstanceFollowRedirects(false);
        connection.setRequestMethod("GET");
        return connection;
    }

    private static URI validateEndpoint(String value) {
        try {
            URI uri = URI.create(value == null ? "" : value.trim());
            validateNetworkUri(uri);
            return uri;
        } catch (IOException | IllegalArgumentException error) {
            throw new IllegalArgumentException("Endpoint do catálogo Aurora inválido.", error);
        }
    }

    private static void validateNetworkUri(URI uri) throws IOException {
        if (uri == null || uri.getHost() == null || uri.getUserInfo() != null || uri.getFragment() != null) {
            throw new IOException("URL de catálogo inválida.");
        }
        boolean https = "https".equalsIgnoreCase(uri.getScheme());
        boolean localHttp = "http".equalsIgnoreCase(uri.getScheme()) && isLoopback(uri.getHost());
        if (!https && !localHttp) throw new IOException("O catálogo exige HTTPS.");
    }

    private static boolean isLoopback(String host) {
        try { return InetAddress.getByName(host).isLoopbackAddress(); }
        catch (IOException error) { return false; }
    }

    private static void requireSameOrigin(URI candidate, URI expected) throws IOException {
        validateNetworkUri(candidate);
        if (!candidate.getScheme().equalsIgnoreCase(expected.getScheme())
                || !candidate.getHost().equalsIgnoreCase(expected.getHost())
                || effectivePort(candidate) != effectivePort(expected)) {
            throw new IOException("A skin aponta para uma origem não permitida.");
        }
    }

    private static int effectivePort(URI uri) {
        if (uri.getPort() >= 0) return uri.getPort();
        return "https".equalsIgnoreCase(uri.getScheme()) ? 443 : 80;
    }

    private static byte[] readBounded(InputStream input, int maximum) throws IOException {
        ByteArrayOutputStream output = new ByteArrayOutputStream();
        byte[] buffer = new byte[8192];
        int total = 0;
        int read;
        while ((read = input.read(buffer)) >= 0) {
            if (read == 0) continue;
            total += read;
            if (total > maximum) throw new IOException("Resposta excedeu o limite permitido.");
            output.write(buffer, 0, read);
        }
        return output.toByteArray();
    }

    private static String decode(String value, int maximum) throws IOException {
        try {
            byte[] decoded = Base64.getUrlDecoder().decode(value);
            if (decoded.length > maximum) throw new IOException("Campo de catálogo muito grande.");
            return new String(decoded, StandardCharsets.UTF_8).trim();
        } catch (IllegalArgumentException error) {
            throw new IOException("Campo de catálogo inválido.", error);
        }
    }

    private static int parseInteger(String value, int minimum, int maximum) throws IOException {
        try {
            int parsed = Integer.parseInt(value);
            if (parsed < minimum || parsed > maximum) throw new IOException("Número fora do limite.");
            return parsed;
        } catch (NumberFormatException error) {
            throw new IOException("Número inválido.", error);
        }
    }

    private static String bounded(String value, int maximum) {
        return value.length() <= maximum ? value : value.substring(0, maximum);
    }

    private static String configuredEndpoint() {
        String configured = System.getProperty("aurora.skins.catalogUrl", "").trim();
        return configured.isEmpty() ? DEFAULT_ENDPOINT : configured;
    }

    private static void closeQuietly(InputStream input) {
        if (input == null) return;
        try { input.close(); } catch (IOException ignored) { }
    }

    private static final class CacheEntry {
        private final SkinPage page;
        private final long createdAt;
        private CacheEntry(SkinPage page, long createdAt) { this.page = page; this.createdAt = createdAt; }
    }
}
