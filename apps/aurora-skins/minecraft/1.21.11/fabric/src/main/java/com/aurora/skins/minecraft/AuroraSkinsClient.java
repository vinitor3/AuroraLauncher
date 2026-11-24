package com.aurora.skins.minecraft;

import com.aurora.core.api.AuroraServices;
import com.aurora.skins.AuroraSkinsRuntime;
import com.aurora.skins.minecraft.appearance.AuroraSkinManager;
import com.aurora.skins.minecraft.network.SkinSyncClientBridge;
import com.aurora.skins.minecraft.ui.AuroraSkinsScreen;
import com.aurora.skins.minecraft.ui.MinecraftSkinPreviewRenderer;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.Screen;

public final class AuroraSkinsClient {
    private static volatile AuroraSkinsRuntime runtime;
    private static volatile AuroraServices services;

    private AuroraSkinsClient() { }

    public static synchronized void initialize(AuroraServices auroraServices) throws Exception {
        if (runtime != null) return;
        services = auroraServices;
        runtime = AuroraSkinsRuntime.start();
        AuroraSkinManager.initialize(runtime);
        SkinSyncClientBridge.initialize(runtime);
        auroraServices.previews().register("aurora_skins", new MinecraftSkinPreviewRenderer());
    }

    public static AuroraSkinsRuntime runtime() {
        AuroraSkinsRuntime current = runtime;
        if (current == null) throw new IllegalStateException("Aurora Skins is not initialized");
        return current;
    }

    public static void open(Object parent) {
        Minecraft minecraft = Minecraft.getInstance();
        Screen screen = parent instanceof Screen ? (Screen) parent : minecraft.screen;
        minecraft.setScreen(new AuroraSkinsScreen(screen));
    }

    public static synchronized void shutdown() {
        AuroraServices current = services;
        if (current != null) current.previews().unregister("aurora_skins");
        SkinSyncClientBridge.shutdown();
        AuroraSkinManager.shutdown();
        services = null;
        runtime = null;
    }
}
