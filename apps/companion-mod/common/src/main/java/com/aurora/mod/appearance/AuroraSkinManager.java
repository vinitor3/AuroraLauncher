package com.aurora.mod.appearance;

import java.io.File;
import java.io.FileInputStream;
import java.io.InputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

import com.mojang.blaze3d.platform.NativeImage;

import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.texture.DynamicTexture;
import net.minecraft.resources.ResourceLocation;

/** Carrega a aparência pública do perfil sem entregar credenciais ao jogo. */
public final class AuroraSkinManager {
    private static final ExecutorService DOWNLOADS = Executors.newSingleThreadExecutor(runnable -> {
        Thread thread = new Thread(runnable, "Aurora-Appearance");
        thread.setDaemon(true);
        return thread;
    });
    private static volatile ResourceLocation skin;
    private static volatile ResourceLocation cape;

    private AuroraSkinManager() { }

    public static void initialize() {
        load(
            System.getProperty("aurora.profile.skinUrl", ""),
            System.getProperty("aurora.profile.skinFile", ""),
            "skin",
            true
        );
        load(System.getProperty("aurora.profile.capeUrl", ""), "", "cape", false);
    }

    public static ResourceLocation skin() { return skin; }
    public static ResourceLocation cape() { return cape; }

    private static void load(String source, String localFile, String name, boolean validateSkin) {
        boolean remote = source.startsWith("https://") && source.length() <= 2_048;
        boolean local = !localFile.isEmpty() && localFile.length() <= 4_096 && new File(localFile).isFile();
        if (!remote && !local) return;
        DOWNLOADS.execute(() -> {
            HttpURLConnection connection = null;
            try {
                InputStream opened;
                if (local) {
                    opened = new FileInputStream(localFile);
                } else {
                    connection = (HttpURLConnection) new URL(source).openConnection();
                    connection.setConnectTimeout(8_000);
                    connection.setReadTimeout(12_000);
                    connection.setInstanceFollowRedirects(true);
                    connection.setRequestProperty("User-Agent", "AuroraCompanion/0.1");
                    if (connection.getResponseCode() < 200 || connection.getResponseCode() >= 300) {
                        System.err.println("[Aurora] Não foi possível baixar " + name + ": HTTP " + connection.getResponseCode());
                        return;
                    }
                    opened = connection.getInputStream();
                }
                try (InputStream input = opened) {
                    NativeImage downloaded = NativeImage.read(input);
                    if (validateSkin && (downloaded.getWidth() != 64 || (downloaded.getHeight() != 64 && downloaded.getHeight() != 32))) {
                        System.err.println("[Aurora] Skin ignorada: use PNG 64x64 ou 64x32.");
                        downloaded.close();
                        return;
                    }
                    NativeImage image = validateSkin ? normalizeLegacySkin(downloaded) : downloaded;
                    Minecraft minecraft = Minecraft.getInstance();
                    minecraft.execute(() -> {
                        ResourceLocation location = new ResourceLocation("aurora", "profile/" + name);
                        minecraft.getTextureManager().register(location, new DynamicTexture(image));
                        if ("skin".equals(name)) skin = location;
                        else cape = location;
                        System.out.println("[Aurora] " + ("skin".equals(name) ? "Skin" : "Capa") + " do perfil carregada dentro do jogo.");
                    });
                }
            } catch (Exception error) {
                System.err.println("[Aurora] Aparência " + name + " indisponível: " + error.getClass().getSimpleName());
            } finally {
                if (connection != null) connection.disconnect();
            }
        });
    }

    /** Converte o layout legado 64x32 para o mapa 64x64 usado pelo modelo atual. */
    private static NativeImage normalizeLegacySkin(NativeImage image) {
        if (image.getHeight() != 32) return image;
        NativeImage normalized = new NativeImage(64, 64, true);
        normalized.copyFrom(image);
        image.close();
        normalized.fillRect(0, 32, 64, 32, 0);

        // Perna esquerda: espelha as faces da perna direita do layout legado.
        normalized.copyRect(4, 16, 16, 32, 4, 4, true, false);
        normalized.copyRect(8, 16, 16, 32, 4, 4, true, false);
        normalized.copyRect(0, 20, 24, 32, 4, 12, true, false);
        normalized.copyRect(4, 20, 16, 32, 4, 12, true, false);
        normalized.copyRect(8, 20, 8, 32, 4, 12, true, false);
        normalized.copyRect(12, 20, 16, 32, 4, 12, true, false);

        // Braço esquerdo: espelha as faces do braço direito do layout legado.
        normalized.copyRect(44, 16, -8, 32, 4, 4, true, false);
        normalized.copyRect(48, 16, -8, 32, 4, 4, true, false);
        normalized.copyRect(40, 20, 0, 32, 4, 12, true, false);
        normalized.copyRect(44, 20, -8, 32, 4, 12, true, false);
        normalized.copyRect(48, 20, -16, 32, 4, 12, true, false);
        normalized.copyRect(52, 20, -8, 32, 4, 12, true, false);
        return normalized;
    }
}
