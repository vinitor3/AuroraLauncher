package com.aurora.core.api.config;

import java.util.Map;

public interface AuroraConfigMigrator {
    Map<String, Object> migrate(int fromVersion, int toVersion, Map<String, Object> values) throws Exception;
}
