package com.aurora.core.runtime;

import java.nio.file.Path;

/** Narrow boundary implemented by each Minecraft/loader adapter. */
public interface AuroraCorePlatform {
    Path gameDirectory();
    String minecraftVersion();
    String loader();
    void runOnClient(Runnable action);
    void openAuroraSettings(Object parentScreen);
    void log(AuroraLog.Level level, String message, Throwable error);
}
