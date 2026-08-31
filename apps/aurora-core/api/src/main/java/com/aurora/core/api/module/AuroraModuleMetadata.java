package com.aurora.core.api.module;

import com.aurora.core.api.AuroraVersion;

public final class AuroraModuleMetadata {
    private final String id;
    private final String name;
    private final AuroraVersion version;
    private final String icon;
    private final AuroraVersion minimumCoreVersion;

    public AuroraModuleMetadata(String id, String name, String version, String icon, String minimumCoreVersion) {
        if (!AuroraModuleRegistry.isValidId(id)) throw new IllegalArgumentException("Invalid Aurora module id: " + id);
        if (name == null || name.trim().isEmpty() || name.length() > 80) throw new IllegalArgumentException("Invalid module name");
        this.id = id;
        this.name = name.trim();
        this.version = AuroraVersion.parse(version);
        this.icon = icon == null ? "module" : icon;
        this.minimumCoreVersion = AuroraVersion.parse(minimumCoreVersion);
    }

    public String id() { return id; }
    public String name() { return name; }
    public AuroraVersion version() { return version; }
    public String icon() { return icon; }
    public AuroraVersion minimumCoreVersion() { return minimumCoreVersion; }
}
