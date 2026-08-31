package com.aurora.core.runtime.ui;

import com.aurora.core.api.ui.AuroraSettingsPage;
import com.aurora.core.api.ui.AuroraSettingsRegistry;
import java.util.List;
import java.util.Optional;

public final class ScopedSettingsRegistry implements AuroraSettingsRegistry {
    private final String owner;
    private final AuroraSettingsRegistry delegate;

    public ScopedSettingsRegistry(String owner, AuroraSettingsRegistry delegate) {
        this.owner = owner;
        this.delegate = delegate;
    }

    @Override public void register(AuroraSettingsPage page) {
        if (!owner.equals(page.owner())) {
            throw new IllegalArgumentException("Module " + owner + " cannot register a page owned by " + page.owner());
        }
        delegate.register(page);
    }

    @Override public Optional<AuroraSettingsPage> find(String id) { return delegate.find(id); }
    @Override public List<AuroraSettingsPage> pages() { return delegate.pages(); }
}
