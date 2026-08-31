package com.aurora.core.api.ui;

import com.aurora.core.api.module.AuroraModuleRegistry;

public final class AuroraSettingsPage {
    private final String id;
    private final String owner;
    private final String title;
    private final String description;
    private final String icon;
    private final int order;
    private final AuroraPageOpener opener;

    public AuroraSettingsPage(String id, String owner, String title, String description,
                              String icon, int order, AuroraPageOpener opener) {
        if (!AuroraModuleRegistry.isValidId(id)) throw new IllegalArgumentException("Invalid page id: " + id);
        if (!AuroraModuleRegistry.isValidId(owner)) throw new IllegalArgumentException("Invalid page owner: " + owner);
        if (title == null || title.trim().isEmpty()) throw new IllegalArgumentException("Page title is required");
        if (opener == null) throw new IllegalArgumentException("Page opener is required");
        this.id = id;
        this.owner = owner;
        this.title = title.trim();
        this.description = description == null ? "" : description.trim();
        this.icon = icon == null ? "settings" : icon;
        this.order = order;
        this.opener = opener;
    }

    public String id() { return id; }
    public String owner() { return owner; }
    public String title() { return title; }
    public String description() { return description; }
    public String icon() { return icon; }
    public int order() { return order; }
    public AuroraPageOpener opener() { return opener; }
}
