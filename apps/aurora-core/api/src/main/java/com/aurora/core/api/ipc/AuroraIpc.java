package com.aurora.core.api.ipc;

import java.util.Map;

public interface AuroraIpc {
    boolean isConnected();
    boolean send(String kind, Map<String, ?> payload);
}
