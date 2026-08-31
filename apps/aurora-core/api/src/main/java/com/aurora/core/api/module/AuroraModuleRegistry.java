package com.aurora.core.api.module;

import java.util.List;
import java.util.Optional;

public interface AuroraModuleRegistry {
    void register(AuroraModule module) throws AuroraModuleRegistrationException;
    Optional<AuroraModuleMetadata> find(String id);
    List<AuroraModuleMetadata> modules();
    boolean isInstalled(String id);

    static boolean isValidId(String value) {
        return value != null && value.matches("[a-z][a-z0-9_.-]{2,63}");
    }
}
