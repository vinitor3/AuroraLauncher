package com.aurora.skins.legacy;

import com.aurora.core.api.module.AuroraModule;
import com.aurora.core.api.module.AuroraModuleContext;
import com.aurora.core.api.module.AuroraModuleMetadata;
import com.aurora.core.api.ui.AuroraSettingsPage;
import com.aurora.core.api.ui.AuroraSettingsRegistry;
import com.aurora.skins.AuroraSkinsRuntime;
import net.minecraftforge.common.MinecraftForge;

public final class AuroraSkinsLegacyModule implements AuroraModule {
    private LegacySkinsUi ui;
    @Override public AuroraModuleMetadata metadata() {
        return new AuroraModuleMetadata("aurora_skins", "Skins", "0.2.1", "skin", "1.1.2");
    }
    @Override public void initialize(AuroraModuleContext context) throws Exception {
        System.setProperty("java.awt.headless", "false");
        AuroraSkinsRuntime runtime = AuroraSkinsRuntime.start();
        LegacySkinManager.initialize(runtime);
        ui = new LegacySkinsUi(runtime);
        MinecraftForge.EVENT_BUS.register(ui);
        context.services().previews().register("aurora_skins", new LegacySkinPreviewRenderer());
    }
    @Override public void registerSettings(AuroraSettingsRegistry settings) {
        settings.register(new AuroraSettingsPage("aurora_skins", "aurora_skins", "Skins",
            "Biblioteca local, favoritos, histórico e visualização 3D.", "skin", 100,
            context -> ui.open(context)));
    }
    @Override public void shutdown() {
        if (ui != null) MinecraftForge.EVENT_BUS.unregister(ui);
        try { com.aurora.core.api.Aurora.services().previews().unregister("aurora_skins"); } catch (Throwable ignored) { }
        LegacySkinManager.shutdown(); ui = null;
    }
}
