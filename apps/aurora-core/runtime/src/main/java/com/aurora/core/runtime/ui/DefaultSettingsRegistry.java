package com.aurora.core.runtime.ui;

import com.aurora.core.api.event.AuroraEventBus;
import com.aurora.core.api.event.AuroraEvents;
import com.aurora.core.api.ui.AuroraSettingsPage;
import com.aurora.core.api.ui.AuroraSettingsRegistry;

import java.util.ArrayList;
import java.util.Collections;
import java.util.Comparator;
import java.util.List;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;

public final class DefaultSettingsRegistry implements AuroraSettingsRegistry {
    private final ConcurrentHashMap<String, AuroraSettingsPage> pages = new ConcurrentHashMap<String, AuroraSettingsPage>();
    private final AuroraEventBus events;

    public DefaultSettingsRegistry(AuroraEventBus events) { this.events = events; }

    @Override public void register(AuroraSettingsPage page) {
        if (page == null) throw new IllegalArgumentException("page");
        AuroraSettingsPage previous = pages.putIfAbsent(page.id(), page);
        if (previous != null) throw new IllegalStateException("Aurora settings page already registered: " + page.id());
        events.publish(new AuroraEvents.SettingsChanged(page.owner()));
    }

    @Override public Optional<AuroraSettingsPage> find(String id) {
        return Optional.ofNullable(pages.get(id));
    }

    @Override public List<AuroraSettingsPage> pages() {
        List<AuroraSettingsPage> snapshot = new ArrayList<AuroraSettingsPage>(pages.values());
        Collections.sort(snapshot, Comparator.comparingInt(AuroraSettingsPage::order).thenComparing(AuroraSettingsPage::title));
        return Collections.unmodifiableList(snapshot);
    }

    public void removeOwner(String owner) {
        for (AuroraSettingsPage page : new ArrayList<AuroraSettingsPage>(pages.values())) {
            if (page.owner().equals(owner)) pages.remove(page.id(), page);
        }
    }
}
