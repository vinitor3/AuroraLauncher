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

final class LegacySkinManager {
    private static final long MAX_DOWNLOAD = 2L * 1024L * 1024L;
    private static final ExecutorService IO = Executors.newSingleThreadExecutor(runnable -> {
        Thread thread = new Thread(runnable, "Aurora-Skins-1.12-IO"); thread.setDaemon(true); return thread;
    });
    private static final AuroraSkinsRuntime.Listener LISTENER = LegacySkinManager::load;
    private static final AtomicLong LOAD_REQUEST = new AtomicLong();
    private static final LegacyPlayerSkinOverride PLAYER_SKIN = new LegacyPlayerSkinOverride();
    private static volatile AuroraSkinsRuntime runtime;
    private static volatile Object texture;
    private static volatile String model = "classic";

    private LegacySkinManager() { }
    static void initialize(AuroraSkinsRuntime value) {
        runtime = value;
        value.addListener(LISTENER);
        if (value.active() != null) load(value.active());
        String localProfile = System.getProperty("aurora.profile.skinFile", "").trim();
        if (localProfile.isEmpty() || !Files.isRegularFile(Paths.get(localProfile))) importRemoteProfileSkin();
    }
    static void shutdown() {
        if (runtime != null) runtime.removeListener(LISTENER);
        runtime = null; LOAD_REQUEST.incrementAndGet();
        LegacyMinecraft.execute(() -> { PLAYER_SKIN.clear(); deleteTexture(texture); texture = null; });
    }
    static Object texture() { return texture; }
    static String model() { return model; }

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
        Object location = texture;
        if (location == null) return;
        try {
            Object minecraft = LegacyMinecraft.minecraft();
            Object player = LegacyMinecraft.field(minecraft, "player", "field_71439_g");
            Object connection = LegacyMinecraft.invoke(minecraft, new String[] {"getConnection", "func_147114_u"});
            Object uuid = LegacyMinecraft.invoke(player, new String[] {"getUniqueID", "func_110124_au"});
            if (!(uuid instanceof UUID) || connection == null) { PLAYER_SKIN.clear(); return; }
            Object info = LegacyMinecraft.invoke(connection, new String[] {"getPlayerInfo", "func_175102_a"}, uuid);
            if (info == null) { PLAYER_SKIN.clear(); return; }
            // Start vanilla's texture request, preserving its cape and elytra callbacks.
            LegacyMinecraft.invoke(info, new String[] {"getLocationSkin", "func_178837_g"});
            Object skinKey = Class.forName("com.mojang.authlib.minecraft.MinecraftProfileTexture$Type")
                .getField("SKIN").get(null);
            PLAYER_SKIN.apply(info, skinKey, location, model);
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
