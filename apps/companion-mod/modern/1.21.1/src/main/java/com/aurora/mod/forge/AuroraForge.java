package com.aurora.mod.forge;

import com.aurora.mod.AuroraCompanion;
import net.minecraftforge.fml.common.Mod;

@Mod(AuroraCompanion.MOD_ID)
public final class AuroraForge {
    public AuroraForge() {
        System.setProperty("aurora.loader", "forge");
        AuroraCompanion.initialize();
    }
}
