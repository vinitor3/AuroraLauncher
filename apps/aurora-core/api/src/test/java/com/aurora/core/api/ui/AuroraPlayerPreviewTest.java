package com.aurora.core.api.ui;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class AuroraPlayerPreviewTest {
    @Test void clampsTargetsAndMovesSmoothly() {
        AuroraPlayerPreview preview = new AuroraPlayerPreview();
        preview.aim(10_000.0F, -10_000.0F, 100.0F, 100.0F, 80.0F, 120.0F);
        preview.update(1.0F / 60.0F);

        assertTrue(preview.headYaw() > 0.0F && preview.headYaw() < 38.0F);
        assertTrue(preview.headPitch() < 0.0F && preview.headPitch() > -25.0F);
        assertTrue(preview.bodyYaw() > 0.0F && preview.bodyYaw() < 12.0F);

        for (int frame = 0; frame < 300; frame++) preview.update(1.0F / 60.0F);
        assertEquals(38.0F, preview.headYaw(), 0.01F);
        assertEquals(-25.0F, preview.headPitch(), 0.01F);
        assertEquals(12.0F, preview.bodyYaw(), 0.01F);
    }
}
