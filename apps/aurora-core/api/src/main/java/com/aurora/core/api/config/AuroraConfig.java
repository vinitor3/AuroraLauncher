package com.aurora.core.api.config;

import java.io.IOException;
import java.util.Map;

public interface AuroraConfig {
    int schemaVersion();
    String getString(String key, String fallback);
    boolean getBoolean(String key, boolean fallback);
    int getInt(String key, int fallback);
    Map<String, Object> snapshot();
    void set(String key, Object value);
    void save() throws IOException;
}
