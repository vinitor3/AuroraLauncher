package com.aurora.core.api.event;

public interface AuroraEventBus {
    <E extends AuroraEvent> AuroraSubscription subscribe(Class<E> eventType, AuroraEventListener<E> listener);
    void publish(AuroraEvent event);
}
