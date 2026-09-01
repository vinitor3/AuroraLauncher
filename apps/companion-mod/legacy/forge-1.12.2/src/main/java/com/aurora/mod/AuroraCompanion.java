package com.aurora.mod;

import com.aurora.mod.profile.AuroraProfileAppearance;

import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.concurrent.Executors;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.TimeUnit;

/** Código comum do Companion para a linha legada Forge 1.12.2. */
public final class AuroraCompanion {
    public static final String MOD_ID = "aurora_companion";
    private static volatile AuroraIpcClient ipcClient;
    private static ScheduledExecutorService telemetryExecutor;
    private static boolean overlayShortcutWasDown;
    private static boolean overlayShortcutAnnounced;
    private static boolean waitingForCoreAnnounced;

    private AuroraCompanion() { }

    public static synchronized void initialize() {
        if (ipcClient != null) return;
        System.out.println("[Aurora] Companion 1.12.2 carregado.");
        System.setProperty("java.awt.headless", "false");
        AuroraProfileAppearance.initialize();
        attachToCoreIfReady();
        startTelemetrySampler();
    }

    private static synchronized void attachToCoreIfReady() {
        if (ipcClient != null) return;
        AuroraIpcClient candidate = AuroraIpcClient.fromSystemProperties();
        if (candidate == null) {
            if (!waitingForCoreAnnounced) {
                waitingForCoreAnnounced = true;
                System.out.println("[Aurora Companion] Aguardando o Aurora Core concluir a inicialização.");
            }
            return;
        }
        ipcClient = candidate;
        ipcClient.connect();
        registerAuroraModule();
        System.out.println("[Aurora Companion] Integração com o Aurora Core concluída.");
    }

    static synchronized void shutdown() {
        if (ipcClient != null) ipcClient.close();
        ipcClient = null;
        if (telemetryExecutor != null) telemetryExecutor.shutdownNow();
        telemetryExecutor = null;
        overlayShortcutWasDown = false;
        overlayShortcutAnnounced = false;
        waitingForCoreAnnounced = false;
    }

    private static void registerAuroraModule() {
        try {
            com.aurora.core.api.Aurora.services().modules().register(new AuroraCompanionModule(ipcClient));
        } catch (com.aurora.core.api.module.AuroraModuleRegistrationException error) {
            System.err.println("[Aurora Companion] " + error.userMessage());
        }
    }

    public static void publishTelemetry(float fps, float mspt, String dimension) {
        if (ipcClient != null) ipcClient.publishTelemetry(fps, mspt, dimension);
    }

    private static synchronized void startTelemetrySampler() {
        if (telemetryExecutor != null) return;
        telemetryExecutor = Executors.newScheduledThreadPool(2, runnable -> {
            Thread thread = new Thread(runnable, "Aurora-Companion");
            thread.setDaemon(true);
            return thread;
        });
        telemetryExecutor.scheduleAtFixedRate(new Runnable() {
            @Override public void run() {
                try {
                    attachToCoreIfReady();
                    publishTelemetry(readFps(), 0.0F, "");
                } catch (Throwable ignored) { }
            }
        }, 1L, 1L, TimeUnit.SECONDS);
        telemetryExecutor.scheduleAtFixedRate(new Runnable() {
            @Override public void run() {
                try {
                    if (isOverlayShortcutPressed()) AuroraAssistantOverlay.toggle(ipcClient);
                } catch (Throwable ignored) { }
            }
        }, 250L, 75L, TimeUnit.MILLISECONDS);
    }

    private static float readFps() {
        try {
            Field field = Class.forName("net.minecraft.client.Minecraft").getField("debugFPS");
            Object value = field.get(null);
            return value instanceof Number ? Math.max(0.0F, ((Number) value).floatValue()) : 0.0F;
        } catch (ReflectiveOperationException ignored) {
            return 0.0F;
        }
    }

    /** Forge 1.12.2 usa LWJGL 2; este caminho não depende de Mixins. */
    private static boolean isOverlayShortcutPressed() {
        if (!overlayShortcutAnnounced) {
            overlayShortcutAnnounced = true;
            System.out.println("[Aurora] Atalho conectado ao teclado LWJGL 2.");
        }
        try {
            Class<?> keyboard = Class.forName("org.lwjgl.input.Keyboard");
            int rightAlt = keyboard.getField("KEY_RMENU").getInt(null);
            int slash = keyboard.getField("KEY_SLASH").getInt(null);
            Method isKeyDown = keyboard.getMethod("isKeyDown", int.class);
            boolean down = ((Boolean) isKeyDown.invoke(null, rightAlt))
                && ((Boolean) isKeyDown.invoke(null, slash));
            boolean triggered = down && !overlayShortcutWasDown;
            overlayShortcutWasDown = down;
            return triggered;
        } catch (ReflectiveOperationException ignored) {
            return false;
        }
    }
}
