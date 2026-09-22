package com.aurora.skins.minecraft.fabric;

import com.aurora.skins.minecraft.AuroraSkinsBootstrap;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.loader.api.FabricLoader;
import net.fabricmc.loader.api.Version;
import net.fabricmc.loader.api.VersionParsingException;

public final class AuroraSkinsFabric implements ClientModInitializer {
    @Override public void onInitializeClient() {
        if (!hasCompatibleCore()) {
            throw new IllegalStateException("Aurora Skins precisa do Aurora Core 1.1.3 ou superior no cliente.");
        }
        AuroraSkinsBootstrap.initialize();
        SkinSyncFabricClient.initialize();
    }

    private static boolean hasCompatibleCore() {
        try {
            Version minimum = Version.parse("1.1.3");
            return FabricLoader.getInstance().getModContainer("aurora_core")
                .map(core -> core.getMetadata().getVersion().compareTo(minimum) >= 0).orElse(false);
        } catch (VersionParsingException impossible) {
            throw new IllegalStateException(impossible);
        }
    }
}
