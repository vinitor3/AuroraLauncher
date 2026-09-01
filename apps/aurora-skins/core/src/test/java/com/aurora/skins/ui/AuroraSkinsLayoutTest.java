package com.aurora.skins.ui;

import org.junit.jupiter.api.Test;

import java.util.Arrays;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class AuroraSkinsLayoutTest {
    @Test void keepsEveryControlInsideCompactAndWideScreens() {
        for (int[] size : new int[][] {{320, 240}, {427, 240}, {854, 480}, {1920, 1080}}) {
            AuroraSkinsLayout layout = AuroraSkinsLayout.calculate(size[0], size[1]);
            List<AuroraSkinsLayout.Slot> controls = Arrays.asList(
                layout.navigation(0), layout.navigation(1), layout.navigation(2), layout.navigation(3),
                layout.importButton(), layout.search(), layout.previous(), layout.next(), layout.apply(),
                layout.favorite(), layout.model(), layout.done());
            for (AuroraSkinsLayout.Slot control : controls) {
                assertTrue(control.x() >= 0 && control.y() >= 0, "negative control position");
                assertTrue(control.right() <= size[0], "control beyond right edge at " + size[0]);
                assertTrue(control.bottom() <= size[1], "control beyond bottom edge at " + size[1]);
            }
            assertTrue(layout.previewX() > 0 && layout.previewX() < size[0]);
            assertTrue(layout.previewY() > layout.panelTop() && layout.previewY() < layout.panelBottom());
        }
    }

    @Test void actionButtonsNeverOverlap() {
        for (int width : new int[] {240, 320, 480, 560, 854, 1920}) {
            AuroraSkinsLayout layout = AuroraSkinsLayout.calculate(width, 480);
            List<AuroraSkinsLayout.Slot> actions = Arrays.asList(
                layout.previous(), layout.next(), layout.apply(), layout.favorite(), layout.model());
            for (int left = 0; left < actions.size(); left++) {
                for (int right = left + 1; right < actions.size(); right++) {
                    assertFalse(actions.get(left).intersects(actions.get(right)),
                        "actions overlap at width " + width);
                }
            }
            for (AuroraSkinsLayout.Slot action : actions) {
                assertFalse(action.intersects(layout.done()), "action overlaps Done at width " + width);
            }
        }
    }

    @Test void switchesToCompactNavigationWhenSpaceIsLimited() {
        assertTrue(AuroraSkinsLayout.calculate(320, 240).compact());
        assertFalse(AuroraSkinsLayout.calculate(854, 480).compact());
    }
}
