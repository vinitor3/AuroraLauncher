package com.aurora.skins.legacy;

import java.util.Map;

final class LegacyCapeOverride {
    private Map<Object, Object> textures;
    private Object key, original, applied;

    @SuppressWarnings("unchecked")
    void apply(Object info, Object capeKey, Object location) {
        Object value = LegacyMinecraft.field(info, "playerTextures", "field_187107_a");
        if (!(value instanceof Map<?, ?>) || location == null) { clear(); return; }
        if (textures != value) {
            clear();
            textures = (Map<Object, Object>) value;
            key = capeKey;
        }
        if (applied == null || textures.get(key) != applied) original = textures.get(key);
        textures.put(key, location);
        applied = location;
    }

    void clear() {
        if (textures != null && applied != null && textures.get(key) == applied) {
            if (original == null) textures.remove(key);
            else textures.put(key, original);
        }
        textures = null;
        key = original = applied = null;
    }
}
