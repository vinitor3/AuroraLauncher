package com.aurora.mod.profile;

import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.nio.charset.StandardCharsets;
import java.util.Base64;
import java.util.Collection;
import java.util.Map;

/**
 * Adaptador de aparência sem dependência binária das classes do Minecraft.
 *
 * <p>As linhas 1.12.2 e 1.21.1 usam APIs de renderização incompatíveis. Este
 * adaptador injeta a propriedade padrão "textures" no GameProfile local antes
 * de o jogo criar o cache de skins. O valor continua sendo consumido pelo
 * sistema nativo de skins do Minecraft.</p>
 */
public final class AuroraProfileAppearance {
    private static volatile boolean initialized;

    private AuroraProfileAppearance() { }

    public static synchronized void initialize() {
        if (initialized) return;
        initialized = true;
        final String skinUrl = safeHttps(System.getProperty("aurora.profile.skinUrl", ""));
        final String capeUrl = safeHttps(System.getProperty("aurora.profile.capeUrl", ""));
        if (skinUrl.isEmpty() && capeUrl.isEmpty()) return;

        Thread installer = new Thread(() -> installWhenAvailable(skinUrl, capeUrl),
            "Aurora-Appearance");
        installer.setDaemon(true);
        installer.start();
    }

    private static void installWhenAvailable(String skinUrl, String capeUrl) {
        for (int attempt = 0; attempt < 240; attempt++) {
            try {
                Object client = clientInstance();
                Object profile = findProfile(client);
                if (profile != null && installTextureProperty(profile, skinUrl, capeUrl)) {
                    clearAppearanceCaches(client);
                    System.out.println("[Aurora] Aparência do perfil aplicada à sessão local.");
                    return;
                }
            } catch (Throwable ignored) {
                // Aparência é opcional e nunca pode impedir o jogo de abrir.
            }
            try {
                Thread.sleep(250L);
            } catch (InterruptedException interrupted) {
                Thread.currentThread().interrupt();
                return;
            }
        }
        System.err.println("[Aurora] Não foi possível localizar o perfil local para aplicar a aparência.");
    }

    private static boolean installTextureProperty(Object profile, String skinUrl, String capeUrl)
        throws Exception {
        Method getProperties = zeroArgumentMethod(profile.getClass(), "getProperties");
        if (getProperties == null) return false;
        Object properties = getProperties.invoke(profile);
        if (properties == null) return false;

        String model = "slim".equals(System.getProperty("aurora.profile.skinModel"))
            ? "slim" : "default";
        StringBuilder textures = new StringBuilder("{\"timestamp\":")
            .append(System.currentTimeMillis()).append(",\"textures\":{");
        boolean hasPrevious = false;
        if (!skinUrl.isEmpty()) {
            textures.append("\"SKIN\":{\"url\":\"").append(json(skinUrl))
                .append("\",\"metadata\":{\"model\":\"").append(model).append("\"}}");
            hasPrevious = true;
        }
        if (!capeUrl.isEmpty()) {
            if (hasPrevious) textures.append(',');
            textures.append("\"CAPE\":{\"url\":\"").append(json(capeUrl)).append("\"}");
        }
        textures.append("}}");
        String value = Base64.getEncoder().encodeToString(
            textures.toString().getBytes(StandardCharsets.UTF_8));

        Class<?> propertyClass = Class.forName("com.mojang.authlib.properties.Property");
        Object property = propertyClass.getConstructor(String.class, String.class)
            .newInstance("textures", value);
        removeExistingTextures(properties);
        Method put = methodWithParameters(properties.getClass(), "put", 2);
        if (put == null) return false;
        put.invoke(properties, "textures", property);
        return true;
    }

    private static void removeExistingTextures(Object properties) {
        try {
            Method removeAll = properties.getClass().getMethod("removeAll", Object.class);
            removeAll.invoke(properties, "textures");
            return;
        } catch (Throwable ignored) { }
        if (properties instanceof Map) ((Map<?, ?>) properties).remove("textures");
    }

    private static Object clientInstance() {
        String[] names = { "net.minecraft.client.Minecraft", "net.minecraft.client.MinecraftClient" };
        for (String name : names) {
            try {
                Class<?> type = Class.forName(name);
                for (Method method : type.getDeclaredMethods()) {
                    if (Modifier.isStatic(method.getModifiers()) && method.getParameterCount() == 0
                        && type.isAssignableFrom(method.getReturnType())) {
                        method.setAccessible(true);
                        Object value = method.invoke(null);
                        if (value != null) return value;
                    }
                }
                for (Field field : type.getDeclaredFields()) {
                    if (Modifier.isStatic(field.getModifiers())
                        && type.isAssignableFrom(field.getType())) {
                        field.setAccessible(true);
                        Object value = field.get(null);
                        if (value != null) return value;
                    }
                }
            } catch (Throwable ignored) { }
        }
        return null;
    }

    private static Object findProfile(Object client) {
        if (client == null) return null;
        Object direct = profileFrom(client);
        if (direct != null) return direct;
        for (Field field : allFields(client.getClass())) {
            try {
                field.setAccessible(true);
                Object child = field.get(client);
                Object profile = profileFrom(child);
                if (profile != null) return profile;
            } catch (Throwable ignored) { }
        }
        return null;
    }

    private static Object profileFrom(Object candidate) {
        if (candidate == null) return null;
        if (isGameProfile(candidate)) return candidate;
        for (Method method : candidate.getClass().getMethods()) {
            try {
                if (method.getParameterCount() == 0
                    && isGameProfileType(method.getReturnType())) {
                    Object profile = method.invoke(candidate);
                    if (profile != null) return profile;
                }
            } catch (Throwable ignored) { }
        }
        for (Field field : allFields(candidate.getClass())) {
            try {
                if (isGameProfileType(field.getType())) {
                    field.setAccessible(true);
                    Object profile = field.get(candidate);
                    if (profile != null) return profile;
                }
            } catch (Throwable ignored) { }
        }
        return null;
    }

    private static void clearAppearanceCaches(Object client) {
        if (client == null) return;
        for (Field field : allFields(client.getClass())) {
            try {
                field.setAccessible(true);
                Object value = field.get(client);
                if (value == null) continue;
                String type = value.getClass().getName().toLowerCase();
                if (type.contains("networkplayerinfo") || type.contains("playerlistentry")) {
                    clearTextureFields(value);
                }
            } catch (Throwable ignored) { }
        }
    }

    private static void clearTextureFields(Object target) {
        for (Field field : allFields(target.getClass())) {
            try {
                String name = field.getName().toLowerCase();
                String type = field.getType().getName().toLowerCase();
                if (name.contains("skin") || name.contains("cape") || type.contains("resourceskin")) {
                    field.setAccessible(true);
                    if (!field.getType().isPrimitive()) field.set(target, null);
                }
            } catch (Throwable ignored) { }
        }
    }

    private static Field[] allFields(Class<?> type) {
        return type == null ? new Field[0] : type.getDeclaredFields();
    }

    private static Method zeroArgumentMethod(Class<?> type, String preferredName) {
        try {
            return type.getMethod(preferredName);
        } catch (Throwable ignored) { }
        for (Method method : type.getMethods()) {
            if (method.getParameterCount() == 0
                && method.getReturnType().getName().endsWith("PropertyMap")) return method;
        }
        return null;
    }

    private static Method methodWithParameters(Class<?> type, String preferredName, int count) {
        for (Method method : type.getMethods()) {
            if (method.getName().equals(preferredName) && method.getParameterCount() == count) {
                return method;
            }
        }
        for (Method method : type.getMethods()) {
            if (method.getParameterCount() == count
                && Collection.class.isAssignableFrom(method.getReturnType())) return method;
        }
        return null;
    }

    private static boolean isGameProfile(Object value) {
        return value != null && isGameProfileType(value.getClass());
    }

    private static boolean isGameProfileType(Class<?> type) {
        return type != null && "com.mojang.authlib.GameProfile".equals(type.getName());
    }

    private static String safeHttps(String value) {
        String trimmed = value == null ? "" : value.trim();
        return trimmed.startsWith("https://") && trimmed.length() <= 2048 ? trimmed : "";
    }

    private static String json(String value) {
        return value.replace("\\", "\\\\").replace("\"", "\\\"");
    }
}
