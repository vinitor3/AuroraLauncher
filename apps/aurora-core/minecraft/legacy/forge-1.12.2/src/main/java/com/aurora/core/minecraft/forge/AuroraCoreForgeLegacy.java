package com.aurora.core.minecraft.forge;
import com.aurora.core.minecraft.legacy.AuroraCoreLegacy;
import net.minecraftforge.fml.common.Mod;
@Mod(modid = "aurora_core", name = "Aurora Core", version = "1.1.5", clientSideOnly = true)
public final class AuroraCoreForgeLegacy {
    public AuroraCoreForgeLegacy() { AuroraCoreLegacy.initialize(); }
}
