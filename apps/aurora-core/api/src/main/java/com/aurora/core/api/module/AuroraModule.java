package com.aurora.core.api.module;

import com.aurora.core.api.event.AuroraEventBus;
import com.aurora.core.api.ui.AuroraSettingsRegistry;

public interface AuroraModule {
    AuroraModuleMetadata metadata();

    default void initialize(AuroraModuleContext context) throws Exception { }
    default void registerSettings(AuroraSettingsRegistry settings) throws Exception { }
    default void registerEvents(AuroraEventBus events) throws Exception { }
    default void shutdown() throws Exception { }
}
