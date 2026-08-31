package com.aurora.core.minecraft.legacy;

import com.aurora.core.runtime.AuroraCorePlatform;
import com.aurora.core.runtime.AuroraLog;
import java.io.File;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.nio.file.Path;

final class LegacyMinecraftPlatform implements AuroraCorePlatform {
    private final LegacyAuroraUi ui;
    LegacyMinecraftPlatform(LegacyAuroraUi ui) { this.ui = ui; }

    @Override public Path gameDirectory() {
        Object minecraft = minecraft();
        Object directory = field(minecraft, "mcDataDir", "field_71412_D");
        return directory instanceof File ? ((File) directory).toPath() : new File(".").toPath().toAbsolutePath().normalize();
    }
    @Override public String minecraftVersion() { return "1.12.2"; }
    @Override public String loader() { return "forge"; }
    @Override public void runOnClient(Runnable action) {
        Object minecraft = minecraft();
        if (!invokeBoolean(minecraft, action, "addScheduledTask", "func_152344_a")) action.run();
    }
    @Override public void openAuroraSettings(Object parentScreen) { ui.open(parentScreen); }
    @Override public void log(AuroraLog.Level level, String message, Throwable error) {
        if (level == AuroraLog.Level.ERROR || level == AuroraLog.Level.WARN) System.err.println(message); else System.out.println(message);
        if (error != null) System.err.println("[Aurora Core] Cause: " + error.getClass().getSimpleName());
    }

    static Object minecraft() {
        try {
            Class<?> type = Class.forName("net.minecraft.client.Minecraft");
            for (String name : new String[] { "getMinecraft", "func_71410_x" }) {
                try { return type.getMethod(name).invoke(null); } catch (ReflectiveOperationException ignored) { }
            }
        } catch (ClassNotFoundException ignored) { }
        return null;
    }
    static Object field(Object target, String... names) {
        if (target == null) return null;
        for (String name : names) {
            Class<?> type = target.getClass();
            while (type != null) {
                try {
                    Field field = type.getDeclaredField(name);
                    field.setAccessible(true);
                    return field.get(target);
                } catch (ReflectiveOperationException ignored) { type = type.getSuperclass(); }
            }
        }
        return null;
    }
    private static boolean invokeBoolean(Object target, Object argument, String... names) {
        if (target == null) return false;
        for (String name : names) {
            for (Method method : target.getClass().getMethods()) {
                if (!method.getName().equals(name) || method.getParameterTypes().length != 1) continue;
                try { method.invoke(target, argument); return true; } catch (ReflectiveOperationException ignored) { }
            }
        }
        return false;
    }
}
