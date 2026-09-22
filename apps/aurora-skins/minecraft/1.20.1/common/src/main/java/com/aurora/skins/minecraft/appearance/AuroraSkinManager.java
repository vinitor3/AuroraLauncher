package com.aurora.skins.minecraft.appearance;

import com.aurora.skins.AuroraSkinsRuntime;
import com.aurora.skins.api.SkinEntry;
import com.aurora.skins.api.SkinModel;
import com.mojang.blaze3d.platform.NativeImage;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.texture.DynamicTexture;
import net.minecraft.resources.ResourceLocation;

import java.io.BufferedInputStream;
import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.InputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.security.MessageDigest;
import java.util.Arrays;
import java.util.HashMap;
import java.util.Iterator;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.RejectedExecutionException;
import java.util.concurrent.ThreadPoolExecutor;
import java.util.concurrent.TimeUnit;

/** Loads validated files off the render thread and publishes textures on the client thread. */
public final class AuroraSkinManager {
    private static final long MAX_DOWNLOAD = 2L * 1024L * 1024L;
    private static final int MAX_REMOTE_PLAYERS = 256;
    private static final ExecutorService IO = Executors.newSingleThreadExecutor(runnable -> {
        Thread thread = new Thread(runnable, "Aurora-Skins-IO");
        thread.setDaemon(true);
        return thread;
    });
    private static final AuroraSkinsRuntime.Listener LISTENER = AuroraSkinManager::load;
    private static final java.util.concurrent.atomic.AtomicLong LOAD_REQUEST = new java.util.concurrent.atomic.AtomicLong();
    private static volatile AuroraSkinsRuntime runtime;
    private static volatile ResourceLocation skin;
    private static volatile String model = "classic";
    // A request owns its texture, so a delayed release can never remove a newer upload.
    private static final Map<UUID, RemoteSkin> REMOTE = new HashMap<>();
    private static final java.util.concurrent.atomic.AtomicLong REMOTE_REQUEST = new java.util.concurrent.atomic.AtomicLong();
    private static final ExecutorService REMOTE_IO = new ThreadPoolExecutor(1, 1, 0L, TimeUnit.MILLISECONDS,
        new ArrayBlockingQueue<>(MAX_REMOTE_PLAYERS), runnable -> {
            Thread thread = new Thread(runnable, "Aurora-Skins-Remote-IO");
            thread.setDaemon(true);
            return thread;
        });

    private AuroraSkinManager() { }

    public static synchronized void initialize(AuroraSkinsRuntime skinsRuntime) {
        if (runtime == skinsRuntime) return;
        if (runtime != null) shutdown();
        runtime = skinsRuntime;
        skinsRuntime.addListener(LISTENER);
        SkinEntry active = skinsRuntime.active();
        if (active != null) load(active);
        String localProfile = System.getProperty("aurora.profile.skinFile", "").trim();
        if (localProfile.isEmpty() || !Files.isRegularFile(Paths.get(localProfile))) {
            importRemoteProfileSkin();
        }
    }

    public static ResourceLocation skin() { return skin; }
    public static String model() { return model; }

    public static synchronized ResourceLocation skin(UUID playerId) {
        if (playerId == null) return null;
        if (isLocalPlayer(playerId)) return skin;
        RemoteSkin remote = REMOTE.get(playerId);
        return remote == null ? null : remote.texture;
    }

    /** Null means vanilla owns both the texture and its model. */
    public static synchronized String model(UUID playerId) {
        if (playerId == null) return null;
        if (isLocalPlayer(playerId)) return skin == null ? null : model;
        RemoteSkin remote = REMOTE.get(playerId);
        return remote == null || remote.texture == null ? null : remote.publishedModel;
    }

    private static boolean isLocalPlayer(UUID playerId) {
        Minecraft minecraft = Minecraft.getInstance();
        return minecraft.player != null && playerId.equals(minecraft.player.getUUID());
    }

    /** The sync layer supplies a validated, session-owned file; never a URL or a player name. */
    public static synchronized void setRemoteSkin(UUID playerId, SkinEntry entry) {
        if (runtime == null || playerId == null || isLocalPlayer(playerId)) return;
        if (entry == null) { removeRemoteSkin(playerId); return; }
        if (entry.file() == null || !entry.contentHash().matches("[0-9a-f]{64}")) return;
        enqueueRemoteSkin(playerId, entry, null, entry.contentHash(), entry.model());
    }

    /** Copies the canonical PNG so the network thread never owns a queued decoder's data. */
    public static synchronized void setRemoteSkin(UUID playerId, byte[] png, SkinModel selectedModel) {
        if (runtime == null || playerId == null || isLocalPlayer(playerId)) return;
        if (png == null) { removeRemoteSkin(playerId); return; }
        if (png.length == 0 || png.length > 24576 || selectedModel == null) return;
        byte[] copy = png.clone();
        enqueueRemoteSkin(playerId, null, copy, hash(copy), selectedModel);
    }

    private static void enqueueRemoteSkin(UUID playerId, SkinEntry entry, byte[] png,
                                          String contentHash, SkinModel selectedModel) {
        RemoteSkin previous = REMOTE.get(playerId);
        if (previous != null && previous.contentHash.equals(contentHash)
                && previous.model == selectedModel) return;
        if (previous == null && REMOTE.size() >= MAX_REMOTE_PLAYERS) return;
        RemoteSkin request = new RemoteSkin(entry, png, contentHash, selectedModel,
            REMOTE_REQUEST.incrementAndGet(), previous);
        REMOTE.put(playerId, request);
        try {
            REMOTE_IO.execute(() -> loadRemoteSkin(playerId, request));
        } catch (RejectedExecutionException ignored) {
            // Remove the failed request so a later sync snapshot can retry it.
            removeRemoteSkin(playerId);
        }
    }

    public static synchronized void removeRemoteSkin(UUID playerId) {
        RemoteSkin removed = REMOTE.remove(playerId);
        if (removed != null) release(removed.texture);
    }

    public static synchronized void retainRemoteSkins(Set<UUID> playerIds) {
        Iterator<Map.Entry<UUID, RemoteSkin>> iterator = REMOTE.entrySet().iterator();
        while (iterator.hasNext()) {
            Map.Entry<UUID, RemoteSkin> remote = iterator.next();
            if (!playerIds.contains(remote.getKey())) {
                iterator.remove();
                release(remote.getValue().texture);
            }
        }
    }

    public static synchronized void clearRemoteSkins() {
        for (RemoteSkin remote : REMOTE.values()) release(remote.texture);
        REMOTE.clear();
        ((ThreadPoolExecutor) REMOTE_IO).getQueue().clear();
    }

    private static synchronized boolean isCurrent(UUID playerId, RemoteSkin request) {
        return runtime != null && REMOTE.get(playerId) == request;
    }

    private static void loadRemoteSkin(UUID playerId, RemoteSkin request) {
        if (!isCurrent(playerId, request)) return;
        try {
            NativeImage image = request.png == null ? readSkin(request.entry) : readSkin(request.png, request.contentHash);
            Minecraft.getInstance().execute(() -> {
                synchronized (AuroraSkinManager.class) {
                    if (!isCurrent(playerId, request)) { image.close(); return; }
                    ResourceLocation location = new ResourceLocation("aurora_skins",
                        "players/" + playerId + "/" + request.generation);
                    Minecraft.getInstance().getTextureManager().register(location, new DynamicTexture(image));
                    ResourceLocation previous = request.texture;
                    request.texture = location;
                    request.publishedModel = request.model.id();
                    release(previous);
                }
            });
        } catch (Exception ignored) {
            synchronized (AuroraSkinManager.class) {
                if (isCurrent(playerId, request)) removeRemoteSkin(playerId);
            }
        }
    }

    private static void release(ResourceLocation location) {
        if (location != null) Minecraft.getInstance().execute(() ->
            Minecraft.getInstance().getTextureManager().release(location));
    }

    private static final class RemoteSkin {
        private final SkinEntry entry;
        private final byte[] png;
        private final String contentHash;
        private final SkinModel model;
        private final long generation;
        private ResourceLocation texture;
        private String publishedModel;

        private RemoteSkin(SkinEntry entry, byte[] png, String contentHash, SkinModel model,
                           long generation, RemoteSkin previous) {
            this.entry = entry;
            this.png = png;
            this.contentHash = contentHash;
            this.model = model;
            this.generation = generation;
            this.texture = previous == null ? null : previous.texture;
            this.publishedModel = previous == null ? null : previous.publishedModel;
        }
    }

    public static synchronized void shutdown() {
        AuroraSkinsRuntime current = runtime;
        if (current != null) current.removeListener(LISTENER);
        runtime = null;
        LOAD_REQUEST.incrementAndGet();
        ResourceLocation previous = skin;
        skin = null;
        model = "classic";
        release(previous);
        clearRemoteSkins();
    }

    private static void load(SkinEntry entry) {
        if (entry == null || entry.file() == null) return;
        final long request = LOAD_REQUEST.incrementAndGet();
        IO.execute(() -> {
            try {
                NativeImage image = readSkin(entry);
                Minecraft.getInstance().execute(() -> {
                    synchronized (AuroraSkinManager.class) {
                        if (request != LOAD_REQUEST.get() || runtime == null) { image.close(); return; }
                        ResourceLocation location = new ResourceLocation("aurora_skins", "active/" + request);
                        Minecraft.getInstance().getTextureManager().register(location, new DynamicTexture(image));
                        ResourceLocation previous = skin;
                        skin = location;
                        model = entry.model().id();
                        release(previous);
                    }
                });
            } catch (Exception error) {
                System.err.println("[Aurora Skins] Não foi possível carregar a textura: " + error.getMessage());
            }
        });
    }

    private static NativeImage readSkin(SkinEntry entry) throws Exception {
        byte[] bytes;
        try (InputStream input = Files.newInputStream(entry.file());
             ByteArrayOutputStream output = new ByteArrayOutputStream()) {
            byte[] buffer = new byte[8192];
            int read;
            while ((read = input.read(buffer)) != -1) {
                if (output.size() + read > MAX_DOWNLOAD) throw new IllegalArgumentException("Skin excede 2 MB");
                output.write(buffer, 0, read);
            }
            bytes = output.toByteArray();
        }
        return readSkin(bytes, entry.contentHash());
    }

    private static NativeImage readSkin(byte[] bytes, String expectedHash) throws Exception {
        byte[] signature = {(byte) 137, 80, 78, 71, 13, 10, 26, 10};
        if (bytes.length < 33 || !Arrays.equals(signature, Arrays.copyOf(bytes, 8))
                || java.nio.ByteBuffer.wrap(bytes, 8, 4).getInt() != 13
                || java.nio.ByteBuffer.wrap(bytes, 12, 4).getInt() != 0x49484452
                || java.nio.ByteBuffer.wrap(bytes, 16, 4).getInt() != 64
                || (java.nio.ByteBuffer.wrap(bytes, 20, 4).getInt() != 64
                    && java.nio.ByteBuffer.wrap(bytes, 20, 4).getInt() != 32)) {
            throw new IllegalArgumentException("Skin precisa ser PNG de 64x64 ou 64x32 pixels");
        }
        if (!hash(bytes).equals(expectedHash)) throw new IllegalArgumentException("Hash da skin inválido");
        NativeImage image = NativeImage.read(new ByteArrayInputStream(bytes));
        if (image.getWidth() != 64 || (image.getHeight() != 32 && image.getHeight() != 64)) {
            image.close();
            throw new IllegalArgumentException("Dimensões de skin inválidas");
        }
        return normalizeLegacySkin(image);
    }

    private static String hash(byte[] bytes) {
        try {
            byte[] digest = MessageDigest.getInstance("SHA-256").digest(bytes);
            StringBuilder hash = new StringBuilder(64);
            for (byte value : digest) hash.append(String.format(java.util.Locale.ROOT, "%02x", value & 0xff));
            return hash.toString();
        } catch (java.security.NoSuchAlgorithmException impossible) {
            throw new IllegalStateException(impossible);
        }
    }

    private static void importRemoteProfileSkin() {
        String source = System.getProperty("aurora.profile.skinUrl", "").trim();
        if (!source.startsWith("https://") || source.length() > 2048) return;
        IO.execute(() -> {
            Path temporary = null;
            try {
                temporary = Files.createTempFile("aurora-skin-", ".png");
                download(source, temporary);
                AuroraSkinsRuntime current = runtime;
                if (current == null) return;
                SkinEntry entry = current.store().importSkin(temporary, "Skin do perfil",
                    SkinModel.fromId(System.getProperty("aurora.profile.skinModel", "classic")));
                current.apply(entry);
            } catch (Exception error) {
                System.err.println("[Aurora Skins] Skin remota indisponível: " + error.getMessage());
            } finally {
                if (temporary != null) try { Files.deleteIfExists(temporary); } catch (Exception ignored) { }
            }
        });
    }

    private static void download(String source, Path target) throws Exception {
        HttpURLConnection connection = (HttpURLConnection) new URL(source).openConnection();
        connection.setConnectTimeout(8_000);
        connection.setReadTimeout(12_000);
        connection.setInstanceFollowRedirects(false);
        connection.setRequestProperty("User-Agent", "AuroraSkins/0.1");
        int status = connection.getResponseCode();
        if (status < 200 || status >= 300) {
            connection.disconnect();
            throw new IllegalStateException("HTTP " + status);
        }
        long declared = connection.getContentLengthLong();
        if (declared > MAX_DOWNLOAD) {
            connection.disconnect();
            throw new IllegalStateException("arquivo maior que 2 MB");
        }
        try (InputStream input = new BufferedInputStream(connection.getInputStream());
             java.io.OutputStream output = Files.newOutputStream(target)) {
            byte[] buffer = new byte[8192];
            long total = 0;
            int read;
            while ((read = input.read(buffer)) >= 0) {
                if (read == 0) continue;
                total += read;
                if (total > MAX_DOWNLOAD) throw new IllegalStateException("arquivo maior que 2 MB");
                output.write(buffer, 0, read);
            }
        } finally {
            connection.disconnect();
        }
    }

    public static NativeImage normalizeLegacySkin(NativeImage image) {
        if (image.getHeight() != 32) return image;
        NativeImage normalized = new NativeImage(64, 64, true);
        normalized.copyFrom(image);
        image.close();
        normalized.fillRect(0, 32, 64, 32, 0);
        normalized.copyRect(4, 16, 16, 32, 4, 4, true, false);
        normalized.copyRect(8, 16, 16, 32, 4, 4, true, false);
        normalized.copyRect(0, 20, 24, 32, 4, 12, true, false);
        normalized.copyRect(4, 20, 16, 32, 4, 12, true, false);
        normalized.copyRect(8, 20, 8, 32, 4, 12, true, false);
        normalized.copyRect(12, 20, 16, 32, 4, 12, true, false);
        normalized.copyRect(44, 16, -8, 32, 4, 4, true, false);
        normalized.copyRect(48, 16, -8, 32, 4, 4, true, false);
        normalized.copyRect(40, 20, 0, 32, 4, 12, true, false);
        normalized.copyRect(44, 20, -8, 32, 4, 12, true, false);
        normalized.copyRect(48, 20, -16, 32, 4, 12, true, false);
        normalized.copyRect(52, 20, -8, 32, 4, 12, true, false);
        return normalized;
    }
}
