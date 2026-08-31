package com.aurora.core.runtime.ipc;

import com.aurora.core.api.event.AuroraEventBus;
import com.aurora.core.api.event.AuroraEvents;
import com.aurora.core.api.ipc.AuroraIpc;
import com.aurora.core.api.ipc.AuroraIpcMessageEvent;
import com.aurora.core.api.session.AuroraSession;
import com.aurora.core.api.session.AuroraSessionState;
import com.aurora.core.runtime.AuroraCorePlatform;
import com.aurora.core.runtime.AuroraCoreRuntime;
import com.aurora.core.runtime.AuroraLog;
import com.aurora.core.runtime.session.DefaultSessionService;
import com.google.gson.Gson;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParseException;
import com.google.gson.reflect.TypeToken;
import org.java_websocket.client.WebSocketClient;
import org.java_websocket.handshake.ServerHandshake;

import java.lang.reflect.Type;
import java.net.URI;
import java.net.URISyntaxException;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import java.util.concurrent.atomic.AtomicBoolean;

public final class LauncherIpcClient implements AuroraIpc, AutoCloseable {
    private static final int MAX_MESSAGE_LENGTH = 1024 * 1024;
    private static final Type MAP_TYPE = new TypeToken<LinkedHashMap<String, Object>>() { }.getType();
    private final AuroraCorePlatform platform;
    private final AuroraEventBus events;
    private final DefaultSessionService sessions;
    private final AuroraLog log;
    private final Gson gson = new Gson();
    private final AtomicBoolean authenticated = new AtomicBoolean(false);
    private volatile Client client;

    public LauncherIpcClient(AuroraCorePlatform platform, AuroraEventBus events,
                             DefaultSessionService sessions, AuroraLog log) {
        this.platform = platform;
        this.events = events;
        this.sessions = sessions;
        this.log = log;
    }

    public void connectIfConfigured() {
        String nonce = System.getProperty("aurora.session.nonce", "").trim();
        int port = Integer.getInteger("aurora.ipc.port", 0);
        if (!nonce.matches("[0-9a-fA-F]{32}") || port < 1 || port > 65535) {
            log.info("Launcher IPC is not configured; local features remain available.");
            return;
        }
        try {
            Client created = new Client(new URI("ws://127.0.0.1:" + port + "/aurora"), nonce);
            client = created;
            created.connect();
        } catch (URISyntaxException error) {
            log.error("Launcher IPC endpoint is invalid; local features remain available.", error);
        }
    }

    @Override public boolean isConnected() {
        Client current = client;
        return authenticated.get() && current != null && current.isOpen();
    }

    @Override public boolean send(String kind, Map<String, ?> payload) {
        if (!isConnected() || kind == null || !kind.matches("[A-Za-z][A-Za-z0-9_.-]{0,63}")) return false;
        if (payload != null && (payload.containsKey("kind") || containsSensitiveField(payload))) return false;
        Map<String, Object> message = new LinkedHashMap<String, Object>();
        message.put("kind", kind);
        if (payload != null) message.putAll(payload);
        String encoded = gson.toJson(message);
        if (encoded.length() > MAX_MESSAGE_LENGTH) return false;
        try {
            client.send(encoded);
            return true;
        } catch (RuntimeException error) {
            log.warn("Launcher IPC send failed; the game will continue locally.");
            return false;
        }
    }

    @Override public void close() {
        Client current = client;
        client = null;
        authenticated.set(false);
        if (current != null) {
            try { current.close(); } catch (RuntimeException ignored) { }
        }
        sessions.update(AuroraSession.offline());
    }

    private void receive(String message) {
        if (message == null || message.length() > MAX_MESSAGE_LENGTH) return;
        final JsonObject root;
        try {
            root = gson.fromJson(message, JsonObject.class);
        } catch (JsonParseException error) {
            return;
        }
        if (root == null || !root.has("kind") || !root.get("kind").isJsonPrimitive()) return;
        String kind = root.get("kind").getAsString();
        if ("accepted".equals(kind)) {
            if (authenticated.compareAndSet(false, true)) {
                events.publish(new AuroraEvents.LauncherConnected());
                log.info("Launcher connection established.");
            }
            return;
        }
        if (!authenticated.get()) return;
        if ("session".equals(kind)) {
            updateSession(root);
            return;
        }
        if ("skinChanged".equals(kind)) {
            events.publish(new AuroraEvents.SkinChanged(string(root, "minecraftUuid")));
        }
        Map<String, Object> payload = gson.fromJson(root, MAP_TYPE);
        payload.remove("kind");
        events.publish(new AuroraIpcMessageEvent(kind, payload));
    }

    private void updateSession(JsonObject root) {
        UUID minecraftUuid = null;
        try {
            String value = string(root, "minecraftUuid");
            if (!value.isEmpty()) minecraftUuid = UUID.fromString(value);
        } catch (IllegalArgumentException ignored) { }
        Set<String> scopes = new LinkedHashSet<String>();
        JsonElement scopeElement = root.get("scopes");
        if (scopeElement != null && scopeElement.isJsonArray()) {
            for (JsonElement value : scopeElement.getAsJsonArray()) {
                if (value.isJsonPrimitive() && value.getAsString().matches("[a-z][a-z0-9_.:-]{0,63}")) {
                    scopes.add(value.getAsString());
                }
            }
        }
        AuroraSessionState state;
        try { state = AuroraSessionState.valueOf(string(root, "state").toUpperCase(java.util.Locale.ROOT)); }
        catch (IllegalArgumentException error) { state = AuroraSessionState.OFFLINE; }
        sessions.update(new AuroraSession(
            string(root, "auroraUserId"), minecraftUuid, string(root, "username"), state, scopes));
    }

    private static String string(JsonObject root, String name) {
        JsonElement value = root.get(name);
        return value != null && value.isJsonPrimitive() ? value.getAsString() : "";
    }

    private static boolean containsSensitiveField(Object value) {
        if (value instanceof Map<?, ?>) {
            for (Map.Entry<?, ?> entry : ((Map<?, ?>) value).entrySet()) {
                String key = String.valueOf(entry.getKey()).toLowerCase(java.util.Locale.ROOT);
                if ("token".equals(key) || "accesstoken".equals(key) || "refreshtoken".equals(key)
                        || "password".equals(key) || "secret".equals(key)
                        || "authorization".equals(key) || "cookie".equals(key)
                        || containsSensitiveField(entry.getValue())) return true;
            }
        } else if (value instanceof Iterable<?>) {
            for (Object item : (Iterable<?>) value) if (containsSensitiveField(item)) return true;
        } else if (value != null && value.getClass().isArray()) {
            int length = java.lang.reflect.Array.getLength(value);
            for (int index = 0; index < length; index++) {
                if (containsSensitiveField(java.lang.reflect.Array.get(value, index))) return true;
            }
        }
        return false;
    }

    private final class Client extends WebSocketClient {
        private final String nonce;
        private Client(URI endpoint, String nonce) { super(endpoint); this.nonce = nonce; }

        @Override public void onOpen(ServerHandshake handshake) {
            Map<String, Object> hello = new LinkedHashMap<String, Object>();
            hello.put("kind", "hello");
            hello.put("nonce", nonce);
            hello.put("loader", platform.loader());
            hello.put("minecraftVersion", platform.minecraftVersion());
            hello.put("coreVersion", AuroraCoreRuntime.VERSION);
            hello.put("protocol", 1);
            send(gson.toJson(hello));
        }

        @Override public void onMessage(String message) { receive(message); }

        @Override public void onClose(int code, String reason, boolean remote) {
            if (authenticated.compareAndSet(true, false)) {
                events.publish(new AuroraEvents.LauncherDisconnected());
                sessions.update(AuroraSession.offline());
                log.info("Launcher connection closed; local features remain available.");
            }
        }

        @Override public void onError(Exception error) {
            log.warn("Launcher IPC is unavailable; local features remain available.");
        }
    }
}
