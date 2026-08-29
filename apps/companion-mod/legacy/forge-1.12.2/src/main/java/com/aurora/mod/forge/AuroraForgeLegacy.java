package com.aurora.mod.forge;

import com.aurora.mod.AuroraCompanion;
import net.minecraftforge.fml.common.Mod;

@Mod(modid = AuroraCompanion.MOD_ID, name = "Aurora Companion", version = "0.1.0", clientSideOnly = true)
public final class AuroraForgeLegacy {
    public AuroraForgeLegacy() { AuroraCompanion.initialize(); }
}
