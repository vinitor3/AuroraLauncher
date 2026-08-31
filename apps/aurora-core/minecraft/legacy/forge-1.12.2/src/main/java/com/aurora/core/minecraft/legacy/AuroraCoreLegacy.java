package com.aurora.core.minecraft.legacy;

import com.aurora.core.runtime.AuroraCoreRuntime;
import net.minecraftforge.common.MinecraftForge;

public final class AuroraCoreLegacy {
    private static boolean initialized;
    private AuroraCoreLegacy() { }

    public static synchronized void initialize() {
        if (initialized) return;
        initialized = true;
        try {
            LegacyAuroraUi ui = new LegacyAuroraUi();
            AuroraCoreRuntime.start(new LegacyMinecraftPlatform(ui));
            MinecraftForge.EVENT_BUS.register(ui);
        } catch (Throwable error) {
            System.err.println("[Aurora Core] Core startup failed; Minecraft will continue: " + error.getClass().getSimpleName());
        }
    }
}
