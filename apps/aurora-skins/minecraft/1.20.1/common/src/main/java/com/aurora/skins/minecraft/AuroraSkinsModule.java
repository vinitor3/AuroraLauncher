package com.aurora.skins.minecraft;

import com.aurora.core.api.module.AuroraModule;
import com.aurora.core.api.module.AuroraModuleContext;
import com.aurora.core.api.module.AuroraModuleMetadata;
import com.aurora.core.api.ui.AuroraSettingsPage;
import com.aurora.core.api.ui.AuroraSettingsRegistry;

/** Service-loaded official module. Aurora Core owns its lifecycle. */
public final class AuroraSkinsModule implements AuroraModule {
    @Override public AuroraModuleMetadata metadata() {
        return new AuroraModuleMetadata("aurora_skins", "Skins", "0.1.0", "skin", "1.1.0");
    }

    @Override public void initialize(AuroraModuleContext context) throws Exception {
        AuroraSkinsClient.initialize(context.services());
    }

    @Override public void registerSettings(AuroraSettingsRegistry settings) {
        settings.register(new AuroraSettingsPage(
            "aurora_skins", "aurora_skins", "Skins",
            "Biblioteca local, favoritos, histórico e visualização 3D.",
            "skin", 100, context -> AuroraSkinsClient.open(context.nativeParentScreen())));
    }

    @Override public void shutdown() {
        AuroraSkinsClient.shutdown();
    }
}
