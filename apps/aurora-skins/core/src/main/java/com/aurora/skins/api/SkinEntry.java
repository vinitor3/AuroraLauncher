package com.aurora.skins.api;

import java.net.URI;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;

public final class SkinEntry {
    private static final List<String> RETIRED_CATALOG_IDS = Arrays.asList("aurora-voyager", "solar-ninja",
        "ki-guardian", "forest-ranger", "cyber-violet", "crimson-mage");
    private final String id;
    private final String name;
    private final String author;
    private final SkinModel model;
    private final String providerId;
    private final String source;
    private final String contentHash;
    private final Path file;
    private final List<String> tags;

    public SkinEntry(String id, String name, String author, SkinModel model,
                     String providerId, String source, String contentHash,
                     Path file, List<String> tags) {
        this.id = required(id, "id");
        this.name = required(name, "name");
        this.author = author == null ? "" : author.trim();
        this.model = model == null ? SkinModel.WIDE : model;
        this.providerId = required(providerId, "providerId");
        this.source = source == null ? "" : source.trim();
        this.contentHash = required(contentHash, "contentHash");
        this.file = file;
        this.tags = Collections.unmodifiableList(tags == null
            ? Collections.<String>emptyList() : new ArrayList<String>(tags));
    }

    public String id() { return id; }
    public String name() { return name; }
    public String author() { return author; }
    public SkinModel model() { return model; }
    public String providerId() { return providerId; }
    public String source() { return source; }
    public String contentHash() { return contentHash; }
    public Path file() { return file; }
    public List<String> tags() { return tags; }

    /** Old catalog metadata has a dedicated provider or a reserved ID and texture route. */
    public boolean isRetiredCatalogSkin() {
        if ("aurora-builtin".equals(providerId)) return true;
        if (!"aurora-online".equals(providerId) || !RETIRED_CATALOG_IDS.contains(id)) return false;
        try {
            return ("/v1/skins/catalog/" + id + ".png").equals(URI.create(source).getPath());
        } catch (IllegalArgumentException invalidSource) {
            return false;
        }
    }

    private static String required(String value, String name) {
        String trimmed = value == null ? "" : value.trim();
        if (trimmed.isEmpty()) throw new IllegalArgumentException(name);
        return trimmed;
    }
}
