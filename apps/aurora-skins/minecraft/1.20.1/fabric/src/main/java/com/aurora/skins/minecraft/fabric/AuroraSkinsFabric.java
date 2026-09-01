package com.aurora.skins.minecraft.fabric;

import com.aurora.skins.minecraft.AuroraSkinsBootstrap;
import net.fabricmc.api.ClientModInitializer;

public final class AuroraSkinsFabric implements ClientModInitializer {
    @Override public void onInitializeClient() { AuroraSkinsBootstrap.initialize(); }
}
