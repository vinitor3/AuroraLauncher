package com.aurora.skins.legacy;

import java.util.Map;

/** Overrides only the local player's SKIN slot; vanilla still loads capes/elytra. */
final class LegacyPlayerSkinOverride {
    private Object playerInfo, skinKey, originalTexture, originalModel, appliedTexture;
    private String appliedModel;
    private Map<Object, Object> textures;

    @SuppressWarnings("unchecked")
    boolean apply(Object info, Object key, Object location, String model) {
        if (info == null || key == null || location == null) return false;
        Object value = LegacyMinecraft.field(info, "playerTextures", "field_187107_a");
        if (!(value instanceof Map<?, ?>)) return false;
        Map<Object, Object> current = (Map<Object, Object>) value;
        if (info != playerInfo || current != textures) {
            clear();
            playerInfo = info; textures = current; skinKey = key;
            originalTexture = current.get(key);
            originalModel = LegacyMinecraft.field(info, "skinType", "field_178863_g");
        }
        // A vanilla download can finish after our first injection. Retain it for rollback.
        Object currentModel = LegacyMinecraft.field(info, "skinType", "field_178863_g");
        if (current.get(key) != appliedTexture) {
            originalTexture = current.get(key);
            originalModel = currentModel;
        } else if (appliedModel == null || !appliedModel.equals(currentModel)) originalModel = currentModel;
        String minecraftModel = "slim".equals(model) ? "slim" : "default";
        if (!LegacyMinecraft.setField(info, minecraftModel, "skinType", "field_178863_g")) return false;
        try {
            current.put(key, location);
            appliedTexture = location; appliedModel = minecraftModel;
            return true;
        } catch (RuntimeException error) {
            LegacyMinecraft.setField(info, currentModel, "skinType", "field_178863_g");
            return false;
        }
    }

    void clear() {
        if (textures != null && appliedTexture != null && textures.get(skinKey) == appliedTexture) {
            if (originalTexture == null) textures.remove(skinKey);
            else textures.put(skinKey, originalTexture);
            if (appliedModel != null && appliedModel.equals(LegacyMinecraft.field(playerInfo, "skinType", "field_178863_g"))) {
                LegacyMinecraft.setField(playerInfo, originalModel, "skinType", "field_178863_g");
            }
        }
        playerInfo = null; textures = null; skinKey = null;
        originalTexture = null; originalModel = null; appliedTexture = null; appliedModel = null;
    }
}
