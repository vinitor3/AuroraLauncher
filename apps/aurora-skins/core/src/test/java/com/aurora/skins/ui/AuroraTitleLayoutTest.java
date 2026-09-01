package com.aurora.skins.ui;

import org.junit.jupiter.api.Test;

import java.util.Arrays;

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
            assertTrue(layout.buttonY() > layout.previewY());
            assertTrue(layout.buttonY() + layout.buttonHeight() <= size[1]);
            assertTrue(layout.buttonX() >= 0);
            assertTrue(layout.buttonX() + layout.buttonWidth() <= size[0]);
            assertTrue(layout.panelBottom() > layout.buttonY() + layout.buttonHeight());
            assertTrue(layout.containsButton(
                layout.buttonX() + layout.buttonWidth() / 2.0,
                layout.buttonY() + layout.buttonHeight() / 2.0));
            assertTrue(!layout.containsButton(layout.buttonX() - 1, layout.buttonY()));
        }
    }

    @Test void movesTheWholePlayerPanelAwayFromAddedButtons() {
        AuroraTitleLayout baseline = AuroraTitleLayout.calculate(854, 480);
        AuroraTitleLayout shifted = AuroraTitleLayout.calculate(854, 480, Arrays.asList(
            new AuroraTitleLayout.Rect(baseline.panelLeft(), baseline.panelTop(),
                baseline.panelRight() - baseline.panelLeft(),
                baseline.buttonY() + baseline.buttonHeight() - baseline.panelTop())
        ));
        assertTrue(shifted.previewX() != baseline.previewX() || shifted.previewY() != baseline.previewY());
        assertTrue(shifted.buttonX() + shifted.buttonWidth() / 2 == shifted.previewX());
    }
}
