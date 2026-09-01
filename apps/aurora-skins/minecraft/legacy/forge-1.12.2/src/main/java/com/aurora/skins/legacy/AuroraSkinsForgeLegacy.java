package com.aurora.skins.legacy;

import com.aurora.core.api.Aurora;
import net.minecraftforge.fml.common.Mod;

@Mod(modid = "aurora_skins", name = "Aurora Skins", version = "0.2.1",
    clientSideOnly = true, dependencies = "required-after:aurora_core@[1.1.2,)")
public final class AuroraSkinsForgeLegacy {
    public AuroraSkinsForgeLegacy() {
        try {
            if (Aurora.isAvailable() && !Aurora.services().modules().isInstalled("aurora_skins")) {
                Aurora.services().modules().register(new AuroraSkinsLegacyModule());
            }
        } catch (Throwable error) {
            System.err.println("[Aurora Skins] Inicialização legada adiada: " + error.getMessage());
        }
    }
}
