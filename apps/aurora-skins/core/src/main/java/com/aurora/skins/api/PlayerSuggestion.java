package com.aurora.skins.api;

/** Public player search data from the Aurora Worker; never contains API credentials. */
public final class PlayerSuggestion {
    private final String name, uuid, provider, metadataJson;
    public PlayerSuggestion(String name, String uuid, String provider, String metadataJson) {
        this.name = name; this.uuid = uuid; this.provider = provider; this.metadataJson = metadataJson;
    }
    public String name() { return name; }
    public String uuid() { return uuid; }
    public String provider() { return provider; }
    public String metadataJson() { return metadataJson; }
    public String identifier() { return uuid.isEmpty() ? name : uuid; }
}
