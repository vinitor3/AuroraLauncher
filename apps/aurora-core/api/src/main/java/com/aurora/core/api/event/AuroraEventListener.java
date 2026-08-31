package com.aurora.core.api.event;

public interface AuroraEventListener<E extends AuroraEvent> {
    void onEvent(E event) throws Exception;
}
