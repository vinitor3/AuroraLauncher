package com.aurora.core.runtime.event;

import com.aurora.core.api.event.AuroraEvent;
import com.aurora.core.api.event.AuroraEventBus;
import com.aurora.core.api.event.AuroraEventListener;
import com.aurora.core.api.event.AuroraSubscription;
import java.util.ArrayList;
import java.util.List;

public final class ScopedEventBus implements AuroraEventBus, AutoCloseable {
    private final AuroraEventBus delegate;
    private final List<AuroraSubscription> subscriptions = new ArrayList<AuroraSubscription>();

    public ScopedEventBus(AuroraEventBus delegate) { this.delegate = delegate; }

    @Override public synchronized <E extends AuroraEvent> AuroraSubscription subscribe(
            Class<E> eventType, AuroraEventListener<E> listener) {
        AuroraSubscription subscription = delegate.subscribe(eventType, listener);
        subscriptions.add(subscription);
        return subscription;
    }

    @Override public void publish(AuroraEvent event) { delegate.publish(event); }

    @Override public synchronized void close() {
        for (AuroraSubscription subscription : subscriptions) subscription.close();
        subscriptions.clear();
    }
}
