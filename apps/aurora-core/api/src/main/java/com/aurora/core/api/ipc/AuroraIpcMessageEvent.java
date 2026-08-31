package com.aurora.core.api.ipc;

import com.aurora.core.api.event.AuroraEvent;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.Map;

public final class AuroraIpcMessageEvent implements AuroraEvent {
    private final String kind;
    private final Map<String, Object> payload;

    public AuroraIpcMessageEvent(String kind, Map<String, Object> payload) {
        this.kind = kind;
        this.payload = Collections.unmodifiableMap(new LinkedHashMap<String, Object>(payload));
    }

    public String kind() { return kind; }
    public Map<String, Object> payload() { return payload; }
}
