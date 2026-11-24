package com.aurora.skins.legacy;

import com.aurora.core.api.Aurora;
import net.minecraftforge.fml.common.Mod;
import net.minecraftforge.fml.common.FMLCommonHandler;
import net.minecraftforge.fml.common.event.FMLPreInitializationEvent;
import net.minecraftforge.fml.common.event.FMLServerStoppedEvent;
import net.minecraftforge.fml.common.Loader;

@Mod(modid = "aurora_skins", name = "Aurora Skins", version = "0.3.3",
    acceptableRemoteVersions = "*", dependencies = "after:aurora_core@[1.1.3,)")
public final class AuroraSkinsForgeLegacy {
    @Mod.EventHandler public void initialize(FMLPreInitializationEvent event) {
        LegacySkinNetwork.initialize();
        if (!FMLCommonHandler.instance().getSide().isClient()) return;
        if (!Loader.isModLoaded("aurora_core")) throw new IllegalStateException("Aurora Skins requer Aurora Core 1.1.3+ no cliente.");
        Client.initialize();
    }
    /** Keep Core-linked bytecode out of the class verified on a dedicated server. */
    private static final class Client {
        private static void initialize() {
            try {
                if (Aurora.isAvailable() && !Aurora.services().modules().isInstalled("aurora_skins")) {
                    Aurora.services().modules().register(new AuroraSkinsLegacyModule());
                }
            } catch (Throwable error) {
                System.err.println("[Aurora Skins] Inicialização legada adiada: " + error.getMessage());
            }
        }
    }
    @Mod.EventHandler public void stopped(FMLServerStoppedEvent event) { LegacySkinNetwork.clear(); }
}
