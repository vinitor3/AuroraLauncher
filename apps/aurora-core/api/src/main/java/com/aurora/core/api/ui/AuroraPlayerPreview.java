package com.aurora.core.api.ui;

/** Version-neutral, smoothed pose used by each Minecraft renderer adapter. */
public final class AuroraPlayerPreview {
    private float headYaw;
    private float headPitch;
    private float bodyYaw;
    private float targetHeadYaw;
    private float targetHeadPitch;
    private float targetBodyYaw;

    public void aim(float mouseX, float mouseY, float centerX, float centerY, float width, float height) {
        float safeWidth = Math.max(1.0F, width);
        float safeHeight = Math.max(1.0F, height);
        float horizontal = clamp((mouseX - centerX) / (safeWidth * 0.5F), -1.0F, 1.0F);
        float vertical = clamp((mouseY - centerY) / (safeHeight * 0.5F), -1.0F, 1.0F);
        targetHeadYaw = horizontal * 38.0F;
        targetBodyYaw = horizontal * 12.0F;
        targetHeadPitch = vertical * 25.0F;
    }

    public void update(float deltaSeconds) {
        float delta = clamp(deltaSeconds, 0.0F, 0.25F);
        float headFactor = 1.0F - (float) Math.exp(-12.0F * delta);
        float bodyFactor = 1.0F - (float) Math.exp(-7.0F * delta);
        headYaw += (targetHeadYaw - headYaw) * headFactor;
        headPitch += (targetHeadPitch - headPitch) * headFactor;
        bodyYaw += (targetBodyYaw - bodyYaw) * bodyFactor;
    }

    public float headYaw() { return headYaw; }
    public float headPitch() { return headPitch; }
    public float bodyYaw() { return bodyYaw; }

    private static float clamp(float value, float minimum, float maximum) {
        return Math.max(minimum, Math.min(maximum, value));
    }
}
