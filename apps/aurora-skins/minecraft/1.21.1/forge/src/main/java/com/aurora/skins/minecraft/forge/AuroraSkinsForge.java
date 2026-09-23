package com.aurora.skins.minecraft.forge;
import net.minecraftforge.api.distmarker.Dist;
import net.minecraftforge.fml.DistExecutor;
import net.minecraftforge.fml.common.Mod;
@Mod("aurora_skins")
public final class AuroraSkinsForge {
    public AuroraSkinsForge() {
        SkinSyncForgeNetwork.initialize();
        DistExecutor.safeRunWhenOn(Dist.CLIENT, () -> SkinSyncForgeClient::initialize);
    }
}
