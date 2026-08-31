package com.aurora.mod.forge;

import com.aurora.mod.AuroraCompanion;
import net.minecraftforge.fml.common.Mod;

@Mod(modid = AuroraCompanion.MOD_ID, name = "Aurora Companion", version = "0.2.0",
    clientSideOnly = true, dependencies = "required-after:aurora_core@[1.0.0,)")
public final class AuroraForgeLegacy {
    public AuroraForgeLegacy() { AuroraCompanion.initialize(); }
}
