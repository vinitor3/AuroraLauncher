package com.aurora.mod;

import com.aurora.core.api.module.AuroraModule;
import com.aurora.core.api.module.AuroraModuleMetadata;
import com.aurora.core.api.ui.AuroraSettingsPage;
import com.aurora.core.api.ui.AuroraSettingsRegistry;

final class AuroraCompanionModule implements AuroraModule {
    private final AuroraIpcClient ipc;

    AuroraCompanionModule(AuroraIpcClient ipc) { this.ipc = ipc; }

    @Override public AuroraModuleMetadata metadata() {
        return new AuroraModuleMetadata("aurora_companion", "Assistente", "0.2.0", "assistant", "1.0.0");
    }

    @Override public void registerSettings(AuroraSettingsRegistry settings) {
        settings.register(new AuroraSettingsPage(
            "aurora_assistant", "aurora_companion", "Assistente",
            "Converse com o Aurora e configure a integração no jogo.",
            "assistant", 200, context -> AuroraAssistantOverlay.toggle(ipc)));
    }

    @Override public void shutdown() { AuroraCompanion.shutdown(); }
}
