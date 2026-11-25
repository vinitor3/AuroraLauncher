package com.aurora.skins.catalog;

import com.aurora.skins.api.SkinEntry;
import com.aurora.skins.api.PlayerSuggestion;
import com.aurora.skins.storage.AuroraSkinStore;
import com.sun.net.httpserver.HttpServer;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;
import javax.imageio.ImageIO;
import java.awt.image.BufferedImage;
import java.io.ByteArrayOutputStream;
import java.net.InetSocketAddress;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.util.Base64;
import java.util.List;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;
import static org.junit.jupiter.api.Assertions.*;

final class PlayerSkinIntegrationTest {
    @TempDir Path temporary;
    @Test void debounceOnlyStartsLatestOperation() throws Exception {
        SearchDebouncer debounce = new SearchDebouncer();
        AtomicInteger calls = new AtomicInteger();
        CompletableFuture<Integer> old = debounce.submit(() -> CompletableFuture.completedFuture(calls.incrementAndGet()));
        CompletableFuture<Integer> current = debounce.submit(() -> CompletableFuture.completedFuture(calls.incrementAndGet()));
        assertEquals(1, current.get(3, TimeUnit.SECONDS));
        assertTrue(old.isCancelled()); assertEquals(1, calls.get());
        CompletableFuture<Integer> closed = debounce.submit(() -> CompletableFuture.completedFuture(calls.incrementAndGet()));
        debounce.cancel(); assertTrue(closed.isCancelled());
    }
    @Test void playerSearchProfilePreviewAndImportUseSameOriginWithoutChangingActiveSkin() throws Exception {
        ByteArrayOutputStream bytes = new ByteArrayOutputStream();
        ImageIO.write(new BufferedImage(64, 64, BufferedImage.TYPE_INT_ARGB), "PNG", bytes);
        byte[] png = bytes.toByteArray(); StringBuilder digest = new StringBuilder();
        for (byte b : MessageDigest.getInstance("SHA-256").digest(png)) digest.append(String.format("%02x", b & 255));
        String uuid = "069a79f444e94726a5befca90e38aaf5";
        String profile = "AURORA_SKINS_CATALOG_V1\npage\t0\tmore\t0\nplayer-" + uuid + "\t" + b64("Notch")
            + "\t" + b64("mc-api") + "\tclassic\t" + digest + "\t" + b64("/skin.png") + "\t" + b64("player") + "\n";
        HttpServer server = HttpServer.create(new InetSocketAddress("127.0.0.1", 0), 0);
        AtomicInteger requests = new AtomicInteger();
        server.createContext("/", exchange -> {
            requests.incrementAndGet(); assertNull(exchange.getRequestHeaders().getFirst("X-API-Key"));
            assertEquals("AuroraSkins/0.3.4", exchange.getRequestHeaders().getFirst("User-Agent"));
            String path = exchange.getRequestURI().getPath();
            byte[] body = path.equals("/skin.png") ? png : (path.equals("/v1/skins/players")
                ? "AURORA_PLAYERS_V1\nNotch\t" + uuid + "\tmc-api\t" + b64("{\"current\":true}") + "\n" : profile).getBytes(StandardCharsets.UTF_8);
            exchange.sendResponseHeaders(200, body.length);
            try { exchange.getResponseBody().write(body); } finally { exchange.close(); }
        });
        server.start();
        try {
            AuroraSkinStore store = new AuroraSkinStore(temporary.resolve("skins"));
            AuroraCatalogProvider provider = new AuroraCatalogProvider(store, "http://127.0.0.1:" + server.getAddress().getPort() + "/v1/skins/catalog");
            assertEquals(1, provider.search("", 0, 12).get(3, TimeUnit.SECONDS).entries().size());
            assertTrue(provider.online());
            int afterCatalog = requests.get();
            provider.search("", 0, 12).get(3, TimeUnit.SECONDS);
            assertEquals(afterCatalog, requests.get(), "Repeated catalog search must use memory cache");
            requests.set(0);
            assertTrue(provider.searchPlayers("No").get(3, TimeUnit.SECONDS).isEmpty()); assertEquals(0, requests.get());
            List<PlayerSuggestion> players = provider.autocompleteSkinNames("Not").get(3, TimeUnit.SECONDS);
            assertEquals("Notch", players.get(0).name()); assertEquals(uuid, players.get(0).uuid());
            SkinEntry suggestion = provider.search("@Not", 0, 12).get(3, TimeUnit.SECONDS).entries().get(0);
            SkinEntry preview = provider.preview(suggestion).get(3, TimeUnit.SECONDS);
            assertTrue(Files.isRegularFile(preview.file())); assertTrue(store.list().isEmpty()); assertNull(store.active());
            SkinEntry installed = provider.materialize(preview).get(3, TimeUnit.SECONDS);
            assertEquals(1, store.list().size()); assertEquals(digest.toString(), installed.contentHash()); assertNull(store.active());
            assertEquals("Notch", provider.getPlayerProfile(uuid).get(3, TimeUnit.SECONDS).name());
            assertTrue(provider.getSkinImageUrlFromProfile(preview).endsWith("/skin.png"));
            assertThrows(java.util.concurrent.ExecutionException.class, () -> provider.getPlayerProfile("../bad").get(3, TimeUnit.SECONDS));
        } finally { server.stop(0); }
    }
    private static String b64(String text) { return Base64.getUrlEncoder().withoutPadding().encodeToString(text.getBytes(StandardCharsets.UTF_8)); }
}
