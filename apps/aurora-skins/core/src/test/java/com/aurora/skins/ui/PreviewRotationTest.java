package com.aurora.skins.ui;

import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

final class PreviewRotationTest {
    @Test void modelFrontPointsTowardCursorInsteadOfAway() {
        // Model front points along -Z. Positive GUI Y points down.
        float yaw = PreviewRotation.headYaw(38F, 12F)
            + (float)Math.toRadians(PreviewRotation.bodyYaw(12F));
        assertTrue(-Math.sin(yaw) > 0, "cursor right must rotate the face right");
        assertTrue(Math.sin(PreviewRotation.headPitch(-25F)) > 0,
            "cursor down produces negative shared pitch and must tilt the face down");
        assertTrue(Math.sin(PreviewRotation.headPitch(25F)) < 0, "cursor up must tilt the face up");
        assertEquals(0F, PreviewRotation.headYaw(0F, 0F));
    }
}
