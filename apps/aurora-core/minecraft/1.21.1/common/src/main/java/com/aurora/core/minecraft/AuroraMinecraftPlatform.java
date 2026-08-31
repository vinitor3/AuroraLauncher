package com.aurora.core.minecraft;
import com.aurora.core.runtime.AuroraCorePlatform;
import com.aurora.core.runtime.AuroraLog;
import com.aurora.core.minecraft.ui.AuroraSettingsScreen;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.Screen;
import java.nio.file.Path;
final class AuroraMinecraftPlatform implements AuroraCorePlatform {
    @Override public Path gameDirectory() { return Minecraft.getInstance().gameDirectory.toPath(); }
    @Override public String minecraftVersion() { return System.getProperty("aurora.minecraft.version", "1.21.1"); }
    @Override public String loader() { return System.getProperty("aurora.loader", "unknown"); }
    @Override public void runOnClient(Runnable action) { Minecraft.getInstance().execute(action); }
    @Override public void openAuroraSettings(Object parentScreen) {
        Screen parent = parentScreen instanceof Screen ? (Screen) parentScreen : Minecraft.getInstance().screen;
        Minecraft.getInstance().setScreen(new AuroraSettingsScreen(parent));
    }
    @Override public void log(AuroraLog.Level level, String message, Throwable error) {
        if (level == AuroraLog.Level.ERROR || level == AuroraLog.Level.WARN) System.err.println(message); else System.out.println(message);
        if (error != null) System.err.println("[Aurora Core] Cause: " + error.getClass().getSimpleName());
    }
}
