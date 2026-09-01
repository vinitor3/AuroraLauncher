package com.aurora.skins.image;

import org.junit.jupiter.api.Test;

import java.awt.image.BufferedImage;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertThrows;

final class SkinImageNormalizerTest {
    @Test void mirrorsLegacyLimbsIntoTheModernLayout() {
        BufferedImage legacy = new BufferedImage(64, 32, BufferedImage.TYPE_INT_ARGB);
        for (int y = 0; y < legacy.getHeight(); y++) {
            for (int x = 0; x < legacy.getWidth(); x++) {
                legacy.setRGB(x, y, 0xFF000000 | (y << 8) | x);
            }
        }

        BufferedImage normalized = SkinImageNormalizer.normalize(legacy);

        assertEquals(64, normalized.getWidth());
        assertEquals(64, normalized.getHeight());
        assertEquals(legacy.getRGB(4, 16), normalized.getRGB(23, 48));
        assertEquals(legacy.getRGB(7, 16), normalized.getRGB(20, 48));
        assertEquals(legacy.getRGB(44, 16), normalized.getRGB(39, 48));
        assertEquals(legacy.getRGB(47, 16), normalized.getRGB(36, 48));
        assertEquals(0, normalized.getRGB(0, 40));
    }

    @Test void leavesModernImagesUntouchedAndRejectsOtherDimensions() {
        BufferedImage modern = new BufferedImage(64, 64, BufferedImage.TYPE_INT_ARGB);
        assertSame(modern, SkinImageNormalizer.normalize(modern));
        assertThrows(IllegalArgumentException.class,
            () -> SkinImageNormalizer.normalize(new BufferedImage(32, 32, BufferedImage.TYPE_INT_ARGB)));
    }
}
