package com.aurora.core.api.event;

public interface AuroraSubscription extends AutoCloseable {
    @Override void close();
}
