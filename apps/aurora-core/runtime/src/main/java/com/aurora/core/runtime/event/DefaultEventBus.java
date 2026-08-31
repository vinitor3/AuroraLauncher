package com.aurora.core.runtime.event;

import com.aurora.core.api.event.AuroraEvent;
import com.aurora.core.api.event.AuroraEventBus;
import com.aurora.core.api.event.AuroraEventListener;
import com.aurora.core.api.event.AuroraSubscription;
import com.aurora.core.runtime.AuroraLog;

import java.util.List;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.CopyOnWriteArrayList;

public final class DefaultEventBus implements AuroraEventBus {
    private final ConcurrentHashMap<Class<?>, CopyOnWriteArrayList<AuroraEventListener<?>>> listeners =
        new ConcurrentHashMap<Class<?>, CopyOnWriteArrayList<AuroraEventListener<?>>>();
    private final AuroraLog log;

    public DefaultEventBus(AuroraLog log) {
        this.log = log;
    }

    @Override public <E extends AuroraEvent> AuroraSubscription subscribe(
            final Class<E> eventType, final AuroraEventListener<E> listener) {
        if (eventType == null || listener == null) throw new IllegalArgumentException("event subscription");
        final CopyOnWriteArrayList<AuroraEventListener<?>> bucket = listeners.computeIfAbsent(
            eventType, ignored -> new CopyOnWriteArrayList<AuroraEventListener<?>>());
        bucket.add(listener);
        return new AuroraSubscription() {
            @Override public void close() { bucket.remove(listener); }
        };
    }

    @Override public void publish(AuroraEvent event) {
        if (event == null) return;
        for (Class<?> type : listeners.keySet()) {
            if (!type.isAssignableFrom(event.getClass())) continue;
            List<AuroraEventListener<?>> bucket = listeners.get(type);
            if (bucket == null) continue;
            for (AuroraEventListener<?> listener : bucket) invoke(listener, event);
        }
    }

    @SuppressWarnings("unchecked")
    private void invoke(AuroraEventListener<?> listener, AuroraEvent event) {
        try {
            ((AuroraEventListener<AuroraEvent>) listener).onEvent(event);
        } catch (Throwable error) {
            log.error("An Aurora event listener failed and was isolated: " + event.getClass().getSimpleName(), error);
        }
    }
}
