package com.aurora.skins.ui;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertTrue;

final class AuroraTitleLayoutTest {
    @Test void keepsPreviewInsideCommonTitleScreens() {
        int[][] sizes = {{320, 240}, {480, 270}, {854, 480}, {1920, 1080}};
        for (int[] size : sizes) {
            AuroraTitleLayout layout = AuroraTitleLayout.calculate(size[0], size[1]);
            assertTrue(layout.previewX() > size[0] / 2);
            assertTrue(layout.previewX() + layout.previewScale() <= size[0]);
            assertTrue(layout.previewY() < size[1]);
            assertTrue(layout.previewScale() >= 34 && layout.previewScale() <= 52);
        }
    }
}
