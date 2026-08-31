package com.aurora.core.runtime.config;

import com.aurora.core.api.config.AuroraConfig;
import java.io.IOException;
import java.nio.file.Path;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.Map;

final class JsonConfig implements AuroraConfig {
    private final String owner;
    private final Path path;
    private final int schemaVersion;
    private final Map<String, Object> values;
    private final JsonConfigManager manager;

    JsonConfig(String owner, Path path, int schemaVersion, Map<String, Object> values, JsonConfigManager manager) {
        this.owner = owner;
        this.path = path;
        this.schemaVersion = schemaVersion;
        this.values = new LinkedHashMap<String, Object>(values);
        this.manager = manager;
    }

    @Override public int schemaVersion() { return schemaVersion; }
    @Override public synchronized String getString(String key, String fallback) {
        Object value = values.get(key);
        return value instanceof String ? (String) value : fallback;
    }
    @Override public synchronized boolean getBoolean(String key, boolean fallback) {
        Object value = values.get(key);
        return value instanceof Boolean ? (Boolean) value : fallback;
    }
    @Override public synchronized int getInt(String key, int fallback) {
        Object value = values.get(key);
        return value instanceof Number ? ((Number) value).intValue() : fallback;
    }
    @Override public synchronized Map<String, Object> snapshot() {
        return Collections.unmodifiableMap(new LinkedHashMap<String, Object>(values));
    }
    @Override public synchronized void set(String key, Object value) {
        if (key == null || key.isEmpty() || key.length() > 128) throw new IllegalArgumentException("config key");
        if (value == null) values.remove(key);
        else values.put(key, value);
    }
    @Override public void save() throws IOException { manager.save(this); }

    String owner() { return owner; }
    Path path() { return path; }
}
