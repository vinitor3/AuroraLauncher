package com.aurora.skins.minecraft;

import com.aurora.core.api.Aurora;

/** Loader fallback when a mod loader does not expose service resources early enough. */
public final class AuroraSkinsBootstrap {
    private AuroraSkinsBootstrap() { }

    public static void initialize() {
        try {
            if (Aurora.isAvailable() && !Aurora.services().modules().isInstalled("aurora_skins")) {
                Aurora.services().modules().register(new AuroraSkinsModule());
            }
        } catch (Throwable error) {
            System.err.println("[Aurora Skins] Inicialização adiada: " + error.getMessage());
        }
    }
}
