package com.aurora.mod.fabric;

import com.aurora.mod.AuroraCompanion;
import net.fabricmc.api.ClientModInitializer;

public final class AuroraFabric implements ClientModInitializer {
    @Override public void onInitializeClient() {
        System.setProperty("aurora.loader", "fabric");
        AuroraCompanion.initialize();
    }
}
