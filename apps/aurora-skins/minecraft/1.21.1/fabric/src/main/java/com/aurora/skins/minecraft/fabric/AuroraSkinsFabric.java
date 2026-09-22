package com.aurora.skins.minecraft.fabric;
import com.aurora.skins.minecraft.AuroraSkinsBootstrap;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.loader.api.FabricLoader;
public final class AuroraSkinsFabric implements ClientModInitializer {
    @Override public void onInitializeClient() {
        if (!FabricLoader.getInstance().isModLoaded("aurora_core")) {
            throw new IllegalStateException("Aurora Skins requires Aurora Core 1.1.3 or newer on the client. Dedicated servers only need Aurora Skins.");
        }
        AuroraSkinsBootstrap.initialize();
        SkinSyncFabricClient.initialize();
    }
}
