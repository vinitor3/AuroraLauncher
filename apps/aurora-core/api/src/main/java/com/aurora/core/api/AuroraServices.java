package com.aurora.core.api;

import com.aurora.core.api.config.AuroraConfigManager;
import com.aurora.core.api.event.AuroraEventBus;
import com.aurora.core.api.ipc.AuroraIpc;
import com.aurora.core.api.module.AuroraModuleRegistry;
import com.aurora.core.api.session.AuroraSessionService;
import com.aurora.core.api.ui.AuroraSettingsRegistry;

public interface AuroraServices {
    String coreVersion();
    AuroraModuleRegistry modules();
    AuroraSettingsRegistry settings();
    AuroraEventBus events();
    AuroraSessionService sessions();
    AuroraIpc ipc();
    AuroraConfigManager configs();
}
