package com.aurora.skins.ui;

/** Converts the shared mouse pose to the model's negative-Z-facing GUI coordinates. */
public final class PreviewRotation {
    private PreviewRotation() { }
    public static float bodyYaw(float yaw) { return -yaw; }
    public static float headYaw(float head, float body) { return radians(body - head); }
    public static float headPitch(float pitch) { return radians(-pitch); }
    private static float radians(float degrees) { return degrees * ((float)Math.PI / 180F); }
}
