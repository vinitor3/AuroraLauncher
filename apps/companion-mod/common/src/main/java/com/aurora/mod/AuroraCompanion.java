package com.aurora.mod;

import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.concurrent.Executors;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.TimeUnit;

/** Código compartilhado entre os loaders suportados. */
public final class AuroraCompanion {
    public static final String MOD_ID = "aurora_companion";
    private static AuroraIpcClient ipcClient;
    private static ScheduledExecutorService telemetryExecutor;
    private static boolean overlayShortcutWasDown;
    private static boolean overlayTickAnnounced;

    private AuroraCompanion() { }

    public static synchronized void initialize() {
        if (ipcClient != null) {
            return;
        }
        System.out.println("[Aurora] Companion carregado.");
        System.setProperty("java.awt.headless", "false");
        initializeAppearance();
        ipcClient = AuroraIpcClient.fromSystemProperties();
        if (ipcClient != null) {
            ipcClient.connect();
            registerAuroraModule();
            startTelemetrySampler();
        }
    }

    private static void registerAuroraModule() {
        try {
            com.aurora.core.api.Aurora.services().modules().register(new AuroraCompanionModule(ipcClient));
        } catch (com.aurora.core.api.module.AuroraModuleRegistrationException error) {
            System.err.println("[Aurora Companion] " + error.userMessage());
        }
    }

    /** Porta de loopback fornecida somente pelo Launcher Core. */
    public static int ipcPort() {
        return Integer.getInteger("aurora.ipc.port", 45882);
    }

    private static void initializeAppearance() {
        String[] managers = {
            "com.aurora.mod.appearance.AuroraSkinManager",
            "com.aurora.mod.profile.AuroraProfileAppearance"
        };
        for (String manager : managers) {
            try {
                Class.forName(manager).getMethod("initialize").invoke(null);
                return;
            } catch (ReflectiveOperationException ignored) { }
        }
    }

    /**
     * Chamado pelos adaptadores de cada versão no ciclo de jogo. Os valores são
     * exclusivamente de diagnóstico local e não incluem chat, IP ou credenciais.
     */
    public static void publishTelemetry(float fps, float mspt, String dimension) {
        if (ipcClient != null) {
            ipcClient.publishTelemetry(fps, mspt, dimension);
        }
    }

    /**
     * A amostragem usa reflexão para não acoplar o JAR às mappings de uma única
     * versão do Minecraft. Isso mantém o mesmo Companion utilizável nas linhas
     * Fabric/Forge suportadas, sem acessar chat, IP ou dados da conta.
     */
    private static synchronized void startTelemetrySampler() {
        if (telemetryExecutor != null) {
            return;
        }
        telemetryExecutor = Executors.newScheduledThreadPool(1, runnable -> {
            Thread thread = new Thread(runnable, "Aurora-Telemetry");
            thread.setDaemon(true);
            return thread;
        });
        telemetryExecutor.scheduleAtFixedRate(() -> {
            try {
                publishTelemetry(readFps(), 0.0F, "");
            } catch (Throwable ignored) {
                // Telemetria é opcional e não pode afetar a execução do jogo.
            }
        }, 1L, 1L, TimeUnit.SECONDS);
    }

    private static float readFps() {
        Object minecraft = clientInstance();
        if (minecraft != null) {
            Float fps = numberFromMethod(minecraft, "getFps");
            if (fps == null) fps = numberFromMethod(minecraft, "getCurrentFps");
            if (fps != null) return Math.max(0.0F, fps);
        }
        Float debugFps = staticNumber("net.minecraft.client.Minecraft", "debugFPS");
        if (debugFps == null) debugFps = staticNumber("net.minecraft.client.MinecraftClient", "debugFPS");
        return debugFps == null ? 0.0F : Math.max(0.0F, debugFps);
    }

    private static Object clientInstance() {
        String[] classes = { "net.minecraft.client.Minecraft", "net.minecraft.client.MinecraftClient" };
        String[] methods = { "getInstance", "getMinecraft" };
        for (String className : classes) {
            try {
                Class<?> type = Class.forName(className);
                for (String methodName : methods) {
                    try {
                        Method method = type.getMethod(methodName);
                        return method.invoke(null);
                    } catch (ReflectiveOperationException ignored) { }
                }
            } catch (ClassNotFoundException ignored) { }
        }
        return null;
    }

    private static Float numberFromMethod(Object target, String methodName) {
        try {
            Object value = target.getClass().getMethod(methodName).invoke(target);
            return value instanceof Number ? ((Number) value).floatValue() : null;
        } catch (ReflectiveOperationException ignored) {
            return null;
        }
    }

    private static Float staticNumber(String className, String fieldName) {
        try {
            Field field = Class.forName(className).getField(fieldName);
            Object value = field.get(null);
            return value instanceof Number ? ((Number) value).floatValue() : null;
        } catch (ReflectiveOperationException ignored) {
            return null;
        }
    }

    /**
     * Leitura reflexiva do GLFW para evitar Mixins frágeis entre as versões.
     * Em Windows, a tecla Right Alt é a tecla AltGr. O Launcher ainda mantém
     * o atalho global como fallback quando uma versão não expõe a janela.
     */
    public static void tickOverlayShortcut(long nativeWindow) {
        if (!overlayTickAnnounced) {
            overlayTickAnnounced = true;
            System.out.println("[Aurora] Atalho conectado ao ciclo do cliente.");
        }
        try {
            Class<?> glfw = Class.forName("org.lwjgl.glfw.GLFW");
            int rightAlt = glfw.getField("GLFW_KEY_RIGHT_ALT").getInt(null);
            int slash = glfw.getField("GLFW_KEY_SLASH").getInt(null);
            int pressed = glfw.getField("GLFW_PRESS").getInt(null);
            Method getKey = glfw.getMethod("glfwGetKey", long.class, int.class);
            boolean down = ((Integer) getKey.invoke(null, nativeWindow, rightAlt)) == pressed
                && ((Integer) getKey.invoke(null, nativeWindow, slash)) == pressed;
            boolean triggered = down && !overlayShortcutWasDown;
            overlayShortcutWasDown = down;
            if (triggered) AuroraAssistantOverlay.toggle(ipcClient);
        } catch (ReflectiveOperationException ignored) {
            // O Assistente é opcional e nunca pode interromper o jogo.
        }
    }
}
