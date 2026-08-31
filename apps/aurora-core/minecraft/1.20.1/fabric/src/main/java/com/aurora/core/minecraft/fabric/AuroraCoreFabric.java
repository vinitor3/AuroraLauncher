package com.aurora.core.minecraft.fabric;

import com.aurora.core.minecraft.AuroraCoreMod;
import net.fabricmc.api.ClientModInitializer;

public final class AuroraCoreFabric implements ClientModInitializer {
    @Override public void onInitializeClient() { AuroraCoreMod.initialize(); }
}
