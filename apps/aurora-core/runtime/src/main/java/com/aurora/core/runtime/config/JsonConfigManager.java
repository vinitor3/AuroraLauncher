package com.aurora.core.runtime.config;

import com.aurora.core.api.config.AuroraConfig;
import com.aurora.core.api.config.AuroraConfigManager;
import com.aurora.core.api.config.AuroraConfigMigrator;
import com.aurora.core.api.event.AuroraEventBus;
import com.aurora.core.api.event.AuroraEvents;
import com.aurora.core.api.module.AuroraModuleRegistry;
import com.aurora.core.runtime.AuroraLog;
import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParseException;
import com.google.gson.reflect.TypeToken;

import java.io.IOException;
import java.io.Reader;
import java.io.Writer;
import java.lang.reflect.Type;
import java.nio.charset.StandardCharsets;
import java.nio.file.AtomicMoveNotSupportedException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.nio.file.StandardOpenOption;
import java.time.Instant;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.Map;

public final class JsonConfigManager implements AuroraConfigManager {
    private static final Type VALUES_TYPE = new TypeToken<LinkedHashMap<String, Object>>() { }.getType();
    private final Path configDirectory;
    private final AuroraEventBus events;
    private final AuroraLog log;
    private final Gson gson = new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create();

    public JsonConfigManager(Path configDirectory, AuroraEventBus events, AuroraLog log) {
        this.configDirectory = configDirectory;
        this.events = events;
        this.log = log;
    }

    @Override public AuroraConfig open(String owner, int schemaVersion, Map<String, Object> defaults,
                                      AuroraConfigMigrator migrator) throws IOException {
        if (!AuroraModuleRegistry.isValidId(owner)) throw new IllegalArgumentException("Invalid config owner: " + owner);
        if (schemaVersion < 1) throw new IllegalArgumentException("schemaVersion");
        Files.createDirectories(configDirectory);
        Path path = configDirectory.resolve(owner.equals("aurora_core") ? "core.json" : owner + ".json");
        Map<String, Object> safeDefaults = sanitizeMap(defaults == null
            ? Collections.<String, Object>emptyMap() : defaults);
        if (!Files.exists(path)) return new JsonConfig(owner, path, schemaVersion, safeDefaults, this);

        ConfigDocument document;
        try {
            document = read(path);
        } catch (IOException error) {
            Path preserved = path.resolveSibling(path.getFileName() + ".invalid-" + System.currentTimeMillis() + ".backup");
            Files.copy(path, preserved, StandardCopyOption.COPY_ATTRIBUTES);
            log.warn("Invalid config for " + owner + " was preserved as " + preserved.getFileName() + ".");
            return new JsonConfig(owner, path, schemaVersion, safeDefaults, this);
        }

        Map<String, Object> values = new LinkedHashMap<String, Object>(safeDefaults);
        values.putAll(document.values);
        if (document.schemaVersion > schemaVersion) {
            throw new IOException("Config " + owner + " uses newer schema " + document.schemaVersion);
        }
        if (document.schemaVersion < schemaVersion) {
            Path backup = path.resolveSibling(path.getFileName() + ".v" + document.schemaVersion + ".backup.json");
            Files.copy(path, backup, StandardCopyOption.REPLACE_EXISTING, StandardCopyOption.COPY_ATTRIBUTES);
            if (migrator == null) throw new IOException("Config " + owner + " requires a migration");
            try {
                values = sanitizeMap(migrator.migrate(document.schemaVersion, schemaVersion,
                    new LinkedHashMap<String, Object>(values)));
            } catch (Exception error) {
                throw new IOException("Config migration failed for " + owner + "; original and backup were preserved", error);
            }
            JsonConfig migrated = new JsonConfig(owner, path, schemaVersion, values, this);
            migrated.save();
            log.info("Config for " + owner + " migrated from schema " + document.schemaVersion
                + " to " + schemaVersion + ".");
            return migrated;
        }
        return new JsonConfig(owner, path, schemaVersion, values, this);
    }

    private ConfigDocument read(Path path) throws IOException {
        try (Reader reader = Files.newBufferedReader(path, StandardCharsets.UTF_8)) {
            JsonObject root = gson.fromJson(reader, JsonObject.class);
            if (root == null || !root.has("schemaVersion") || !root.get("schemaVersion").isJsonPrimitive()) {
                throw new IOException("schemaVersion missing");
            }
            int version = root.get("schemaVersion").getAsInt();
            JsonElement valuesElement = root.get("values");
            if (version < 1 || valuesElement == null || !valuesElement.isJsonObject()) {
                throw new IOException("invalid config document");
            }
            Map<String, Object> values = gson.fromJson(valuesElement, VALUES_TYPE);
            return new ConfigDocument(version, sanitizeMap(values));
        } catch (JsonParseException | IllegalStateException | NumberFormatException error) {
            throw new IOException("invalid JSON config", error);
        }
    }

    synchronized void save(JsonConfig config) throws IOException {
        Files.createDirectories(configDirectory);
        JsonObject root = new JsonObject();
        root.addProperty("schemaVersion", config.schemaVersion());
        root.addProperty("updatedAt", Instant.now().toString());
        root.add("values", gson.toJsonTree(config.snapshot()));
        Path temporary = config.path().resolveSibling(config.path().getFileName() + ".aurora-writing");
        try (Writer writer = Files.newBufferedWriter(temporary, StandardCharsets.UTF_8,
                StandardOpenOption.CREATE, StandardOpenOption.TRUNCATE_EXISTING, StandardOpenOption.WRITE)) {
            gson.toJson(root, writer);
        }
        try {
            Files.move(temporary, config.path(), StandardCopyOption.REPLACE_EXISTING, StandardCopyOption.ATOMIC_MOVE);
        } catch (AtomicMoveNotSupportedException ignored) {
            Files.move(temporary, config.path(), StandardCopyOption.REPLACE_EXISTING);
        }
        events.publish(new AuroraEvents.SettingsChanged(config.owner()));
    }

    private static Map<String, Object> sanitizeMap(Map<String, Object> input) {
        Map<String, Object> result = new LinkedHashMap<String, Object>();
        if (input == null) return result;
        for (Map.Entry<String, Object> entry : input.entrySet()) {
            String key = entry.getKey();
            if (key == null || key.isEmpty() || key.length() > 128) continue;
            Object value = sanitizeValue(entry.getValue(), 0);
            if (value != Unsupported.VALUE) result.put(key, value);
        }
        return result;
    }

    private static Object sanitizeValue(Object value, int depth) {
        if (depth > 8) return Unsupported.VALUE;
        if (value == null || value instanceof String || value instanceof Boolean) return value;
        if (value instanceof Number) {
            double number = ((Number) value).doubleValue();
            return Double.isInfinite(number) || Double.isNaN(number) ? Unsupported.VALUE : value;
        }
        if (value instanceof Map<?, ?>) {
            Map<String, Object> map = new LinkedHashMap<String, Object>();
            for (Map.Entry<?, ?> entry : ((Map<?, ?>) value).entrySet()) {
                if (!(entry.getKey() instanceof String)) continue;
                Object nested = sanitizeValue(entry.getValue(), depth + 1);
                if (nested != Unsupported.VALUE) map.put((String) entry.getKey(), nested);
            }
            return map;
        }
        if (value instanceof Iterable<?>) {
            java.util.List<Object> list = new java.util.ArrayList<Object>();
            for (Object item : (Iterable<?>) value) {
                Object nested = sanitizeValue(item, depth + 1);
                if (nested != Unsupported.VALUE) list.add(nested);
            }
            return list;
        }
        return Unsupported.VALUE;
    }

    private enum Unsupported { VALUE }

    private static final class ConfigDocument {
        private final int schemaVersion;
        private final Map<String, Object> values;
        private ConfigDocument(int schemaVersion, Map<String, Object> values) {
            this.schemaVersion = schemaVersion;
            this.values = values;
        }
    }
}
