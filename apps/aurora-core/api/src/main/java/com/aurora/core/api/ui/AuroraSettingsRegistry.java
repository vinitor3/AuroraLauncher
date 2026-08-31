package com.aurora.core.api.ui;

import java.util.List;
import java.util.Optional;

public interface AuroraSettingsRegistry {
    void register(AuroraSettingsPage page);
    Optional<AuroraSettingsPage> find(String id);
    List<AuroraSettingsPage> pages();
}
