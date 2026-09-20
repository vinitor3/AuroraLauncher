package com.aurora.skins.minecraft.appearance;

import com.aurora.skins.AuroraSkinsRuntime;
import com.aurora.skins.api.SkinEntry;
import com.mojang.blaze3d.platform.NativeImage;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.texture.DynamicTexture;
import net.minecraft.resources.ResourceLocation;

import java.io.BufferedInputStream;
import java.io.FileInputStream;
import java.io.InputStream;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

public final class AuroraSkinManager {
    private static final ExecutorService IO = Executors.newSingleThreadExecutor(runnable -> {
        Thread thread = new Thread(runnable, "Aurora-Skins-IO"); thread.setDaemon(true); return thread;
    });
    private static final AuroraSkinsRuntime.Listener LISTENER = AuroraSkinManager::load;
    private static final java.util.concurrent.atomic.AtomicLong LOAD_REQUEST = new java.util.concurrent.atomic.AtomicLong();
    private static volatile AuroraSkinsRuntime runtime;
    private static volatile ResourceLocation skin;
    private static volatile String model = "classic";
    private AuroraSkinManager() { }

    public static synchronized void initialize(AuroraSkinsRuntime skinsRuntime) {
        runtime = skinsRuntime; skinsRuntime.addListener(LISTENER);
        if (skinsRuntime.active() != null) load(skinsRuntime.active());
    }
    public static ResourceLocation skin() { return skin; }
    public static String model() { return model; }
    public static synchronized void shutdown() {
        if (runtime != null) runtime.removeListener(LISTENER);
        runtime = null; LOAD_REQUEST.incrementAndGet();
        ResourceLocation previous = skin; skin = null; model = "classic";
        if (previous != null) Minecraft.getInstance().execute(() -> Minecraft.getInstance().getTextureManager().release(previous));
    }
    private static void load(SkinEntry entry) {
        if (entry == null || entry.file() == null) return;
        final long request = LOAD_REQUEST.incrementAndGet();
        IO.execute(() -> { try (InputStream input = new BufferedInputStream(new FileInputStream(entry.file().toFile()))) {
            NativeImage decoded = NativeImage.read(input);
            if (decoded.getWidth() != 64 || (decoded.getHeight() != 64 && decoded.getHeight() != 32)) {
                decoded.close(); throw new IllegalArgumentException("Skin precisa ter 64x64 ou 64x32 pixels");
            }
            NativeImage image = normalizeLegacySkin(decoded);
            Minecraft.getInstance().execute(() -> {
                if (request != LOAD_REQUEST.get() || runtime == null) { image.close(); return; }
                ResourceLocation location = ResourceLocation.fromNamespaceAndPath("aurora_skins", "active/" + entry.contentHash());
                Minecraft.getInstance().getTextureManager().register(location, new DynamicTexture(image));
                ResourceLocation previous = skin;
                skin = location; model = entry.model().id();
                if (previous != null && !previous.equals(location)) Minecraft.getInstance().getTextureManager().release(previous);
                System.out.println("[Aurora Skins] Textura ativa carregada: " + entry.name());
            });
        } catch (Exception error) { System.err.println("[Aurora Skins] Não foi possível carregar a textura: " + error.getMessage()); } });
    }
    public static NativeImage normalizeLegacySkin(NativeImage image) {
        if (image.getHeight() != 32) return image;
        NativeImage normalized = new NativeImage(64, 64, true); normalized.copyFrom(image); image.close(); normalized.fillRect(0, 32, 64, 32, 0);
        normalized.copyRect(4,16,16,32,4,4,true,false); normalized.copyRect(8,16,16,32,4,4,true,false);
        normalized.copyRect(0,20,24,32,4,12,true,false); normalized.copyRect(4,20,16,32,4,12,true,false);
        normalized.copyRect(8,20,8,32,4,12,true,false); normalized.copyRect(12,20,16,32,4,12,true,false);
        normalized.copyRect(44,16,-8,32,4,4,true,false); normalized.copyRect(48,16,-8,32,4,4,true,false);
        normalized.copyRect(40,20,0,32,4,12,true,false); normalized.copyRect(44,20,-8,32,4,12,true,false);
        normalized.copyRect(48,20,-16,32,4,12,true,false); normalized.copyRect(52,20,-8,32,4,12,true,false);
        return normalized;
    }
}
