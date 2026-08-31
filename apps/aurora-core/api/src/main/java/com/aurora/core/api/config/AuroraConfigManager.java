package com.aurora.core.api.config;

import java.io.IOException;
import java.util.Map;

public interface AuroraConfigManager {
    AuroraConfig open(String owner, int schemaVersion, Map<String, Object> defaults,
                      AuroraConfigMigrator migrator) throws IOException;
}
