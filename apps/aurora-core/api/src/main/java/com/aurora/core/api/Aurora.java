package com.aurora.core.api;

/** Stable entry point used by official Aurora modules. */
public final class Aurora {
    private static volatile AuroraServices services;

    private Aurora() { }

    public static boolean isAvailable() {
        return services != null;
    }

    public static AuroraServices services() {
        AuroraServices current = services;
        if (current == null) {
            throw new IllegalStateException("Aurora Core is not initialized");
        }
        return current;
    }

    /** Runtime hook. Modules must never replace the active service container. */
    public static synchronized void install(AuroraServices value) {
        if (value == null) throw new IllegalArgumentException("services");
        if (services != null && services != value) {
            throw new IllegalStateException("Aurora Core services are already installed");
        }
        services = value;
    }
}
