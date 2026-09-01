package com.aurora.skins.api;

import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

public final class SkinEntry {
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

    private static String required(String value, String name) {
        String trimmed = value == null ? "" : value.trim();
        if (trimmed.isEmpty()) throw new IllegalArgumentException(name);
        return trimmed;
    }
}
