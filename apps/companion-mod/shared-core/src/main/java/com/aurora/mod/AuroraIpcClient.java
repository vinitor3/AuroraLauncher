package com.aurora.mod;

import com.aurora.core.api.Aurora;
import com.aurora.core.api.event.AuroraEventListener;
import com.aurora.core.api.event.AuroraSubscription;
import com.aurora.core.api.ipc.AuroraIpcMessageEvent;

import java.util.LinkedHashMap;
import java.util.Map;
import java.util.concurrent.atomic.AtomicLong;

/** Adapter used by the Companion; Aurora Core owns the authenticated socket. */
final class AuroraIpcClient {
    private static final long MINIMUM_INTERVAL_MS = 1_000L;
    private final AtomicLong lastTelemetryAt = new AtomicLong(0L);
    private AuroraSubscription subscription;

    static AuroraIpcClient fromSystemProperties() {
        if (!Aurora.isAvailable()) {
            System.err.println("[Aurora Companion] Aurora Core 1.0.0 or newer is required.");
            return null;
        }
        return new AuroraIpcClient();
    }

    void connect() {
        subscription = Aurora.services().events().subscribe(AuroraIpcMessageEvent.class,
            new AuroraEventListener<AuroraIpcMessageEvent>() {
                @Override public void onEvent(AuroraIpcMessageEvent event) {
                    if ("toggleAssistant".equals(event.kind())) {
                        AuroraAssistantOverlay.toggle(AuroraIpcClient.this);
                    } else {
                        AuroraAssistantOverlay.receive(encode(event));
                    }
                }
            });
        System.out.println("[Aurora Companion] Using Aurora Core IPC.");
    }

    void close() {
        if (subscription != null) subscription.close();
        subscription = null;
    }

    boolean isOpen() { return Aurora.services().ipc().isConnected(); }

    void publishTelemetry(float fps, float mspt, String dimension) {
        if (!isOpen()) return;
        long now = System.currentTimeMillis();
        long previous = lastTelemetryAt.get();
        if (now - previous < MINIMUM_INTERVAL_MS || !lastTelemetryAt.compareAndSet(previous, now)) return;
        long usedMemoryMb = (Runtime.getRuntime().totalMemory() - Runtime.getRuntime().freeMemory()) / (1024L * 1024L);
        Map<String, Object> payload = new LinkedHashMap<String, Object>();
        payload.put("fps", Math.max(0.0F, fps));
        payload.put("mspt", Math.max(0.0F, mspt));
        payload.put("usedMemoryMb", usedMemoryMb);
        payload.put("dimension", dimension == null ? "" : dimension);
        Aurora.services().ipc().send("telemetry", payload);
    }

    void publishOverlayToggle() {
        Aurora.services().ipc().send("overlay", java.util.Collections.<String, Object>emptyMap());
    }

    void publishAssistantRequest(String requestId, String message, String screenshotBase64) {
        Map<String, Object> payload = new LinkedHashMap<String, Object>();
        payload.put("requestId", requestId);
        payload.put("message", message);
        payload.put("screenshotBase64", screenshotBase64);
        Aurora.services().ipc().send("assistantRequest", payload);
    }

    void publishAssistantListen(String requestId) {
        Map<String, Object> payload = new LinkedHashMap<String, Object>();
        payload.put("requestId", requestId);
        Aurora.services().ipc().send("assistantListen", payload);
    }

    private static String encode(AuroraIpcMessageEvent event) {
        StringBuilder json = new StringBuilder("{\"kind\":\"").append(escape(event.kind())).append('"');
        for (Map.Entry<String, Object> entry : event.payload().entrySet()) {
            json.append(",\"").append(escape(entry.getKey())).append("\":");
            Object value = entry.getValue();
            if (value == null) json.append("null");
            else if (value instanceof Number || value instanceof Boolean) json.append(String.valueOf(value));
            else json.append('"').append(escape(String.valueOf(value))).append('"');
        }
        return json.append('}').toString();
    }

    private static String escape(String value) {
        return value == null ? "" : value.replace("\\", "\\\\").replace("\"", "\\\"")
            .replace("\n", "\\n").replace("\r", "\\r");
    }
}
