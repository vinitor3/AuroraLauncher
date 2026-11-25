package com.aurora.skins.legacy;

import com.aurora.skins.AuroraSkinsRuntime;
import com.aurora.skins.api.SkinEntry;
import com.aurora.skins.api.SkinModel;
import com.aurora.skins.image.SkinImageNormalizer;

import javax.imageio.ImageIO;
import java.awt.image.BufferedImage;
import java.io.BufferedInputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.lang.reflect.Constructor;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.UUID;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.atomic.AtomicLong;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.concurrent.ThreadPoolExecutor;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.RejectedExecutionException;
import java.io.ByteArrayInputStream;
import com.aurora.skins.sync.SkinSyncProtocol;

final class LegacySkinManager {
    private static final long MAX_DOWNLOAD = 2L * 1024L * 1024L;
    private static final ExecutorService IO = Executors.newSingleThreadExecutor(runnable -> {
        Thread thread = new Thread(runnable, "Aurora-Skins-1.12-IO"); thread.setDaemon(true); return thread;
    });
    private static final AuroraSkinsRuntime.Listener LISTENER = LegacySkinManager::load;
    private static final AtomicLong LOAD_REQUEST = new AtomicLong();
    private static final LegacyPlayerSkinOverride PLAYER_SKIN = new LegacyPlayerSkinOverride();
    private static final LegacyCapeOverride PLAYER_CAPE = new LegacyCapeOverride();
    private static volatile Object cape;
    private static volatile AuroraSkinsRuntime runtime;
    private static volatile Object texture;
    private static volatile String model = "classic";
    private static final Map<UUID, RemoteSkin> REMOTE = new LinkedHashMap<UUID, RemoteSkin>();
    private static final Map<UUID, LegacyCapeOverride> REMOTE_CAPE_OVERRIDES = new LinkedHashMap<>();
    private static final com.aurora.skins.sync.RemoteCapes<Object> REMOTE_CAPES =
        new com.aurora.skins.sync.RemoteCapes<>((player, png) -> {
            BufferedImage image = ImageIO.read(new ByteArrayInputStream(png));
            Object dynamic = Class.forName("net.minecraft.client.renderer.texture.DynamicTexture")
                .getConstructor(BufferedImage.class).newInstance(image);
            Object manager = LegacyMinecraft.invoke(LegacyMinecraft.minecraft(), new String[] {"getTextureManager", "func_110434_K"});
            return LegacyMinecraft.invoke(manager, new String[] {"getDynamicTextureLocation", "func_110578_a"}, "aurora_cape_" + player, dynamic);
        }, LegacySkinManager::deleteTexture);
    private static final ThreadPoolExecutor REMOTE_IO = new ThreadPoolExecutor(1, 1, 0, TimeUnit.MILLISECONDS,
        new ArrayBlockingQueue<Runnable>(256), runnable -> {
            Thread thread = new Thread(runnable, "Aurora-Skins-1.12-Remote"); thread.setDaemon(true); return thread;
        }, new ThreadPoolExecutor.AbortPolicy());
    private static final class RemoteSkin {
        final String fingerprint, model;
        final LegacyPlayerSkinOverride override = new LegacyPlayerSkinOverride();
        Object texture;
        RemoteSkin(String fingerprint, String model) { this.fingerprint = fingerprint; this.model = model; }
    }

    private LegacySkinManager() { }
    static void initialize(AuroraSkinsRuntime value) {
        runtime = value;
        IO.execute(() -> {
            try {
                byte[] png = com.aurora.skins.ProfileCape.readSelected();
                if (png == null) return;
                BufferedImage image = ImageIO.read(new ByteArrayInputStream(png));
                if (image == null) return;
                LegacyMinecraft.execute(() -> {
                    if (runtime != value) return;
                    try {
                        Object dynamic = Class.forName("net.minecraft.client.renderer.texture.DynamicTexture")
                            .getConstructor(BufferedImage.class).newInstance(image);
                        Object manager = LegacyMinecraft.invoke(LegacyMinecraft.minecraft(), new String[] {"getTextureManager", "func_110434_K"});
                        cape = LegacyMinecraft.invoke(manager, new String[] {"getDynamicTextureLocation", "func_110578_a"}, "aurora_skins_cape", dynamic);
                    } catch (ReflectiveOperationException error) {
                        System.err.println("[Aurora Skins] Capa indisponível: " + error.getClass().getSimpleName());
                    }
                });
            } catch (Exception error) {
                System.err.println("[Aurora Skins] Capa indisponível: " + error.getClass().getSimpleName());
            }
        });
        value.addListener(LISTENER);
        if (value.active() != null) load(value.active());
        String localProfile = System.getProperty("aurora.profile.skinFile", "").trim();
        if (localProfile.isEmpty() || !Files.isRegularFile(Paths.get(localProfile))) importRemoteProfileSkin();
    }
    static void shutdown() {
        if (runtime != null) runtime.removeListener(LISTENER);
        runtime = null; LOAD_REQUEST.incrementAndGet();
        LegacyMinecraft.execute(() -> {
            clearRemoteSkins(); PLAYER_SKIN.clear(); PLAYER_CAPE.clear();
            deleteTexture(texture); texture = null;
            deleteTexture(cape); cape = null;
        });
    }
    static Object texture() { return texture; }
    static String model() { return model; }

    /** Called on the client thread; each completion must still own its UUID slot. */
    static void setRemoteSkin(UUID id, byte[] bytes, SkinModel selectedModel) {
        if (id == null || bytes == null || bytes.length > SkinSyncProtocol.MAX_PNG_BYTES || runtime == null) return;
        String fingerprint = SkinSyncProtocol.hash(bytes) + selectedModel.id();
        RemoteSkin previous = REMOTE.get(id);
        if (previous != null && previous.fingerprint.equals(fingerprint)) return;
        if (previous == null && REMOTE.size() >= SkinSyncProtocol.MAX_PLAYERS) return;
        byte[] png = bytes.clone();
        RemoteSkin replacement = new RemoteSkin(fingerprint, selectedModel.id());
        REMOTE.put(id, replacement);
        if (previous != null) { previous.override.clear(); deleteTexture(previous.texture); }
        try {
            REMOTE_IO.execute(() -> {
                try {
                    BufferedImage image = ImageIO.read(new ByteArrayInputStream(png));
                    if (image == null || image.getWidth() != 64 || image.getHeight() != 64) throw new IllegalArgumentException("Invalid remote skin");
                    LegacyMinecraft.execute(() -> {
                        if (REMOTE.get(id) != replacement || runtime == null) return;
                        try {
                            Object dynamic = Class.forName("net.minecraft.client.renderer.texture.DynamicTexture")
                                .getConstructor(BufferedImage.class).newInstance(image);
                            Object manager = LegacyMinecraft.invoke(LegacyMinecraft.minecraft(), new String[] {"getTextureManager", "func_110434_K"});
                            replacement.texture = LegacyMinecraft.invoke(manager, new String[] {"getDynamicTextureLocation", "func_110578_a"}, "aurora_skins_remote", dynamic);
                            if (replacement.texture == null) { REMOTE.remove(id); return; }
                            applyRemotePlayers();
                        } catch (ReflectiveOperationException failed) { REMOTE.remove(id); }
                    });
                } catch (Exception invalid) { LegacyMinecraft.execute(() -> { if (REMOTE.get(id) == replacement) REMOTE.remove(id); }); }
            });
        } catch (RejectedExecutionException full) { REMOTE.remove(id); }
    }
    static void setRemoteCape(UUID id, byte[] png) {
        if (id == null || runtime == null) return;
        LegacyCapeOverride previous = REMOTE_CAPE_OVERRIDES.remove(id);
        if (previous != null) previous.clear();
        REMOTE_CAPES.update(id, png);
        if (REMOTE_CAPES.get(id) != null) REMOTE_CAPE_OVERRIDES.put(id, new LegacyCapeOverride());
    }
    static void removeRemoteSkin(UUID id) {
        RemoteSkin previous = REMOTE.remove(id);
        if (previous != null) { previous.override.clear(); deleteTexture(previous.texture); }
    }
    static void clearRemoteSkins() {
        REMOTE_IO.getQueue().clear();
        for (RemoteSkin value : REMOTE.values()) { value.override.clear(); deleteTexture(value.texture); }
        REMOTE.clear();
        for (LegacyCapeOverride override : REMOTE_CAPE_OVERRIDES.values()) override.clear();
        REMOTE_CAPE_OVERRIDES.clear();
        REMOTE_CAPES.clear();
    }
    private static void applyRemotePlayers() {
        Object minecraft = LegacyMinecraft.minecraft();
        Object connection = LegacyMinecraft.invoke(minecraft, new String[] {"getConnection", "func_147114_u"});
        if (connection == null) { clearRemoteSkins(); return; }
        try {
            Object key = Class.forName("com.mojang.authlib.minecraft.MinecraftProfileTexture$Type").getField("SKIN").get(null);
            Object capeKey = Class.forName("com.mojang.authlib.minecraft.MinecraftProfileTexture$Type").getField("CAPE").get(null);
            Object local = LegacyMinecraft.field(minecraft, "player", "field_71439_g");
            Object localId = LegacyMinecraft.invoke(local, new String[] {"getUniqueID", "func_110124_au"});
            for (Map.Entry<UUID, LegacyCapeOverride> entry : REMOTE_CAPE_OVERRIDES.entrySet()) {
                Object info = LegacyMinecraft.invoke(connection, new String[] {"getPlayerInfo", "func_175102_a"}, entry.getKey());
                if (info == null || entry.getKey().equals(localId)) { entry.getValue().clear(); continue; }
                LegacyMinecraft.invoke(info, new String[] {"getLocationSkin", "func_178837_g"});
                entry.getValue().apply(info, capeKey, REMOTE_CAPES.get(entry.getKey()));
            }
            for (Map.Entry<UUID, RemoteSkin> entry : REMOTE.entrySet()) {
                RemoteSkin skin = entry.getValue(); if (skin.texture == null) continue;
                Object info = LegacyMinecraft.invoke(connection, new String[] {"getPlayerInfo", "func_175102_a"}, entry.getKey());
                if (info == null) { skin.override.clear(); continue; }
                LegacyMinecraft.invoke(info, new String[] {"getLocationSkin", "func_178837_g"});
                skin.override.apply(info, key, skin.texture, skin.model);
            }
        } catch (ReflectiveOperationException ignored) { }
    }

    private static void load(final SkinEntry entry) {
        if (entry == null || entry.file() == null) return;
        final long request = LOAD_REQUEST.incrementAndGet();
        IO.execute(new Runnable() { @Override public void run() {
            try {
                BufferedImage source = ImageIO.read(entry.file().toFile());
                if (source == null || source.getWidth() != 64 || (source.getHeight() != 64 && source.getHeight() != 32)) {
                    throw new IllegalArgumentException("Skin precisa ter 64x64 ou 64x32 pixels");
                }
                final BufferedImage image = SkinImageNormalizer.normalize(source);
                LegacyMinecraft.execute(new Runnable() { @Override public void run() {
                    if (request != LOAD_REQUEST.get() || runtime == null) return;
                    try {
                        Class<?> dynamicType = Class.forName("net.minecraft.client.renderer.texture.DynamicTexture");
                        Constructor<?> constructor = dynamicType.getConstructor(BufferedImage.class);
                        Object dynamic = constructor.newInstance(image);
                        Object minecraft = LegacyMinecraft.minecraft();
                        Object manager = LegacyMinecraft.invoke(minecraft,
                            new String[] {"getTextureManager", "func_110434_K"});
                        Object location = LegacyMinecraft.invoke(manager,
                            new String[] {"getDynamicTextureLocation", "func_110578_a"}, "aurora_skins_active", dynamic);
                        if (location == null) throw new IllegalStateException("TextureManager indisponível");
                        Object previous = texture;
                        texture = location; model = entry.model().id();
                        applyToLocalPlayer();
                        deleteTexture(previous);
                        System.out.println("[Aurora Skins] Textura 1.12.2 carregada: " + entry.name());
                    } catch (Throwable error) {
                        System.err.println("[Aurora Skins] Falha ao registrar textura 1.12.2: " + error.getClass().getSimpleName());
                    }
                }});
            } catch (Throwable error) {
                System.err.println("[Aurora Skins] Skin 1.12.2 indisponível: " + error.getMessage());
            }
        }});
    }

    private static void importRemoteProfileSkin() {
        final String source = System.getProperty("aurora.profile.skinUrl", "").trim();
        if (!source.startsWith("https://") || source.length() > 2048) return;
        IO.execute(new Runnable() { @Override public void run() {
            Path temporary = null;
            try {
                temporary = Files.createTempFile("aurora-skin-legacy-", ".png");
                download(source, temporary);
                AuroraSkinsRuntime current = runtime;
                if (current == null) return;
                SkinEntry entry = current.store().importSkin(temporary, "Skin do perfil",
                    SkinModel.fromId(System.getProperty("aurora.profile.skinModel", "classic")));
                current.apply(entry);
            } catch (Throwable error) {
                System.err.println("[Aurora Skins] Skin remota 1.12.2 indisponível: " + error.getMessage());
            } finally {
                if (temporary != null) try { Files.deleteIfExists(temporary); } catch (Exception ignored) { }
            }
        }});
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
             OutputStream output = Files.newOutputStream(target)) {
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

    static void applyToLocalPlayer() {
        applyRemotePlayers();
        Object location = texture;
        if (location == null && cape == null) return;
        try {
            Object minecraft = LegacyMinecraft.minecraft();
            Object player = LegacyMinecraft.field(minecraft, "player", "field_71439_g");
            Object connection = LegacyMinecraft.invoke(minecraft, new String[] {"getConnection", "func_147114_u"});
            Object uuid = LegacyMinecraft.invoke(player, new String[] {"getUniqueID", "func_110124_au"});
            if (!(uuid instanceof UUID) || connection == null) { PLAYER_SKIN.clear(); PLAYER_CAPE.clear(); return; }
            Object info = LegacyMinecraft.invoke(connection, new String[] {"getPlayerInfo", "func_175102_a"}, uuid);
            if (info == null) { PLAYER_SKIN.clear(); PLAYER_CAPE.clear(); return; }
            // Start vanilla's texture request, preserving its cape and elytra callbacks.
            LegacyMinecraft.invoke(info, new String[] {"getLocationSkin", "func_178837_g"});
            Class<?> keys = Class.forName("com.mojang.authlib.minecraft.MinecraftProfileTexture$Type");
            if (location != null) PLAYER_SKIN.apply(info, keys.getField("SKIN").get(null), location, model);
            if (cape != null) PLAYER_CAPE.apply(info, keys.getField("CAPE").get(null), cape);
        } catch (Throwable ignored) { }
    }

    private static void deleteTexture(Object location) {
        if (location == null) return;
        Object manager = LegacyMinecraft.invoke(LegacyMinecraft.minecraft(), new String[] {"getTextureManager", "func_110434_K"});
        LegacyMinecraft.invoke(manager, new String[] {"deleteTexture", "func_147645_c"}, location);
    }

    static Object defaultTexture() {
        try {
            Class<?> type = Class.forName("net.minecraft.client.resources.DefaultPlayerSkin");
            for (java.lang.reflect.Method method : type.getMethods()) {
                if ((method.getName().equals("getDefaultSkin") || method.getName().equals("func_177334_a"))
                        && method.getParameterTypes().length == 1) return method.invoke(null, new UUID(0L, 0L));
            }
        } catch (Throwable ignored) { }
        return null;
    }
}
