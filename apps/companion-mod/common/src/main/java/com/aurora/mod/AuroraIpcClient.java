package com.aurora.mod;

import java.net.URI;
import java.net.URISyntaxException;
import java.util.Locale;
import java.util.concurrent.atomic.AtomicLong;

import org.java_websocket.client.WebSocketClient;
import org.java_websocket.handshake.ServerHandshake;

/** Cliente WebSocket local compartilhado pelas versões do Companion. */
final class AuroraIpcClient extends WebSocketClient {
    private static final long MINIMUM_INTERVAL_MS = 1_000L;
    private final String nonce;
    private final AtomicLong lastTelemetryAt = new AtomicLong(0L);

    private AuroraIpcClient(URI endpoint, String nonce) {
        super(endpoint);
        this.nonce = nonce;
    }

    static AuroraIpcClient fromSystemProperties() {
        String nonce = System.getProperty("aurora.session.nonce", "").trim();
        int port = Integer.getInteger("aurora.ipc.port", 0);
        if (!nonce.matches("[0-9a-fA-F]{32}") || port < 1 || port > 65535) {
            return null;
        }
        try {
            return new AuroraIpcClient(new URI("ws://127.0.0.1:" + port + "/aurora"), nonce);
        } catch (URISyntaxException ignored) {
            return null;
        }
    }

    @Override
    public void onOpen(ServerHandshake handshake) {
        System.out.println("[Aurora] IPC local conectado.");
        send("{\"kind\":\"hello\",\"nonce\":\"" + nonce
            + "\",\"loader\":\"" + escape(System.getProperty("aurora.loader", "unknown"))
            + "\",\"minecraftVersion\":\"" + escape(System.getProperty("aurora.minecraft.version", "unknown")) + "\"}");
    }

    @Override public void onMessage(String message) {
        if (message.contains("\"kind\":\"accepted\"")) {
            System.out.println("[Aurora] Sessão IPC autenticada pelo launcher.");
        } else if (message.contains("\"kind\":\"toggleAssistant\"")) {
            System.out.println("[Aurora] Comando para alternar o Assistente recebido.");
            AuroraAssistantOverlay.toggle(this);
        } else {
            AuroraAssistantOverlay.receive(message);
        }
    }

    @Override public void onClose(int code, String reason, boolean remote) {
        System.out.println("[Aurora] IPC local desconectado.");
    }

    @Override public void onError(Exception exception) {
        System.err.println("[Aurora] IPC indisponível: " + exception.getClass().getSimpleName());
    }

    void publishTelemetry(float fps, float mspt, String dimension) {
        if (!isOpen()) {
            return;
        }
        long now = System.currentTimeMillis();
        long previous = lastTelemetryAt.get();
        if (now - previous < MINIMUM_INTERVAL_MS || !lastTelemetryAt.compareAndSet(previous, now)) {
            return;
        }
        long usedMemoryMb = (Runtime.getRuntime().totalMemory() - Runtime.getRuntime().freeMemory()) / (1024L * 1024L);
        send(String.format(Locale.ROOT,
            "{\"kind\":\"telemetry\",\"fps\":%.2f,\"mspt\":%.2f,\"usedMemoryMb\":%d,\"dimension\":\"%s\"}",
            Math.max(0.0F, fps), Math.max(0.0F, mspt), usedMemoryMb, escape(dimension)));
    }

    void publishOverlayToggle() {
        if (isOpen()) {
            send("{\"kind\":\"overlay\"}");
        }
    }

    void publishAssistantRequest(String requestId, String message, String screenshotBase64) {
        if (!isOpen()) return;
        String screenshot = screenshotBase64 == null ? "null" : "\"" + escape(screenshotBase64) + "\"";
        send("{\"kind\":\"assistantRequest\",\"requestId\":\"" + escape(requestId)
            + "\",\"message\":\"" + escape(message) + "\",\"screenshotBase64\":" + screenshot + "}");
    }

    void publishAssistantListen(String requestId) {
        if (isOpen()) {
            send("{\"kind\":\"assistantListen\",\"requestId\":\"" + escape(requestId) + "\"}");
        }
    }

    private static String escape(String value) {
        if (value == null) {
            return "";
        }
        return value.replace("\\", "\\\\").replace("\"", "\\\"");
    }
}
