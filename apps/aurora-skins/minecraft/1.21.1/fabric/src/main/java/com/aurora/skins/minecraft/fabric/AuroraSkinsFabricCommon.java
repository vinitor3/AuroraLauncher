package com.aurora.skins.minecraft.fabric;

import net.fabricmc.api.ModInitializer;

/** Loads on dedicated servers without resolving any client or Aurora Core class. */
public final class AuroraSkinsFabricCommon implements ModInitializer {
    @Override public void onInitialize() { SkinSyncFabricNetwork.initialize(); }
}
