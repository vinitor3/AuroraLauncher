package com.aurora.skins.legacy;

import java.lang.reflect.Field;
import java.lang.reflect.Method;

final class LegacyMinecraft {
    private LegacyMinecraft() { }

    static Object minecraft() {
        try {
            Class<?> type = Class.forName("net.minecraft.client.Minecraft");
            for (String name : new String[] {"getMinecraft", "func_71410_x"}) {
                try { return type.getMethod(name).invoke(null); } catch (ReflectiveOperationException ignored) { }
            }
        } catch (ClassNotFoundException ignored) { }
        return null;
    }

    static Object field(Object target, String... names) {
        if (target == null) return null;
        Class<?> type = target.getClass();
        while (type != null) {
            for (String name : names) {
                try { Field field = type.getDeclaredField(name); field.setAccessible(true); return field.get(target); }
                catch (ReflectiveOperationException ignored) { }
            }
            type = type.getSuperclass();
        }
        return null;
    }

    static boolean setField(Object target, Object value, String... names) {
        if (target == null) return false;
        Class<?> type = target.getClass();
        while (type != null) {
            for (String name : names) {
                try { Field field = type.getDeclaredField(name); field.setAccessible(true); field.set(target, value); return true; }
                catch (ReflectiveOperationException ignored) { }
            }
            type = type.getSuperclass();
        }
        return false;
    }

    static Object invoke(Object target, String[] names, Object... arguments) {
        if (target == null) return null;
        for (String name : names) {
            for (Method method : target.getClass().getMethods()) {
                if (!method.getName().equals(name) || method.getParameterTypes().length != arguments.length) continue;
                try { method.setAccessible(true); return method.invoke(target, arguments); }
                catch (ReflectiveOperationException ignored) { }
            }
        }
        return null;
    }

    static void execute(Runnable runnable) {
        Object minecraft = minecraft();
        if (minecraft != null) {
            for (String name : new String[] {"addScheduledTask", "func_152344_a"}) {
                try {
                    Method method = minecraft.getClass().getMethod(name, Runnable.class);
                    method.invoke(minecraft, runnable);
                    return;
                } catch (ReflectiveOperationException ignored) { }
            }
        }
        runnable.run();
    }
}
