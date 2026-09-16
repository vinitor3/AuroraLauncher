package com.aurora.skins.minecraft.appearance;

import com.aurora.skins.AuroraSkinsRuntime;
import com.aurora.skins.api.SkinEntry;
import com.aurora.skins.api.SkinModel;
import com.mojang.blaze3d.platform.NativeImage;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.texture.DynamicTexture;
import net.minecraft.resources.ResourceLocation;

import java.io.BufferedInputStream;
import java.io.FileInputStream;
import java.io.InputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/** Loads validated files off the render thread and publishes textures on the client thread. */
public final class AuroraSkinManager {
    private static final long MAX_DOWNLOAD = 2L * 1024L * 1024L;
    private static final ExecutorService IO = Executors.newSingleThreadExecutor(runnable -> {
        Thread thread = new Thread(runnable, "Aurora-Skins-IO");
        thread.setDaemon(true);
        return thread;
    });
    private static final AuroraSkinsRuntime.Listener LISTENER = AuroraSkinManager::load;
    private static volatile AuroraSkinsRuntime runtime;
    private static volatile ResourceLocation skin;
    private static volatile String model = "classic";

    private AuroraSkinManager() { }

    public static synchronized void initialize(AuroraSkinsRuntime skinsRuntime) {
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

    public static synchronized void shutdown() {
        AuroraSkinsRuntime current = runtime;
        if (current != null) current.removeListener(LISTENER);
        runtime = null;
        skin = null;
        model = "classic";
    }

    private static void load(SkinEntry entry) {
        if (entry == null || entry.file() == null) return;
        IO.execute(() -> {
            try (InputStream input = new BufferedInputStream(new FileInputStream(entry.file().toFile()))) {
                NativeImage decoded = NativeImage.read(input);
                if (decoded.getWidth() != 64 || (decoded.getHeight() != 64 && decoded.getHeight() != 32)) {
                    decoded.close();
                    throw new IllegalArgumentException("Skin precisa ter 64x64 ou 64x32 pixels");
                }
                NativeImage image = normalizeLegacySkin(decoded);
                Minecraft.getInstance().execute(() -> {
                    ResourceLocation location = new ResourceLocation("aurora_skins", "active/" + entry.contentHash());
                    Minecraft.getInstance().getTextureManager().register(location, new DynamicTexture(image));
                    skin = location;
                    model = entry.model().id();
                    System.out.println("[Aurora Skins] Textura ativa carregada: " + entry.name());
                });
            } catch (Exception error) {
                System.err.println("[Aurora Skins] Não foi possível carregar a textura: " + error.getMessage());
            }
        });
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
