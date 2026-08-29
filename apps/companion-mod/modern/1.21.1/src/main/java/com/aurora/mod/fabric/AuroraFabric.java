package com.aurora.mod.fabric;

import com.aurora.mod.AuroraCompanion;
import net.fabricmc.api.ModInitializer;

public final class AuroraFabric implements ModInitializer {
    @Override public void onInitialize() {
        System.setProperty("aurora.loader", "fabric");
        AuroraCompanion.initialize();
    }
}
