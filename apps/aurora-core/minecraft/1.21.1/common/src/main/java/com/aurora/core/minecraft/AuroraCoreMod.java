package com.aurora.core.minecraft;
import com.aurora.core.runtime.AuroraCoreRuntime;
public final class AuroraCoreMod {
    public static final String MOD_ID = "aurora_core";
    private static boolean initialized;
    private AuroraCoreMod() { }
    public static synchronized void initialize() {
        if (initialized) return;
        initialized = true;
        try { AuroraCoreRuntime.start(new AuroraMinecraftPlatform()); }
        catch (Throwable error) { System.err.println("[Aurora Core] Core startup failed; Minecraft will continue: " + error.getClass().getSimpleName()); }
    }
}
