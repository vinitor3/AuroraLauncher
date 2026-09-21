package com.aurora.skins.ui;

import org.junit.jupiter.api.Test;

import java.util.Arrays;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class CatalogGridLayoutTest {
    private static final int[][] SIZES = {
        {320, 240}, {427, 240}, {560, 260}, {854, 260}, {320, 480}, {854, 480}, {1920, 1080}, {3840, 2160}
    };

    @Test void twelveCardsAndWholeModelsFitWithoutCoveringControlsOrStatus() {
        for (int[] size : SIZES) {
            AuroraSkinsLayout screen = AuroraSkinsLayout.calculate(size[0], size[1]);
            CatalogGridLayout grid = CatalogGridLayout.calculate(screen);
            List<AuroraSkinsLayout.Slot> controls = Arrays.asList(
                screen.navigation(0), screen.navigation(1), screen.navigation(2), screen.navigation(3),
                screen.importButton(), screen.search(), screen.previous(), screen.next(), screen.apply(),
                screen.favorite(), screen.model(), screen.done(),
                new AuroraSkinsLayout.Slot(grid.statusX(), grid.statusY(), grid.statusWidth(), 9));
            assertEquals(12, grid.columns() * grid.rows());
            for (int index = 0; index < CatalogGridState.PAGE_SIZE; index++) {
                AuroraSkinsLayout.Slot card = grid.slot(index);
                assertTrue(card.x() >= 0 && card.y() >= 0);
                assertTrue(card.right() <= size[0] && card.bottom() <= size[1]);
                for (AuroraSkinsLayout.Slot control : controls) {
                    assertFalse(card.intersects(control), "card/control overlap at " + size[0] + "x" + size[1]);
                }
                for (int other = index + 1; other < CatalogGridState.PAGE_SIZE; other++) {
                    assertFalse(card.intersects(grid.slot(other)));
                }
                int scale = grid.modelScale(index);
                // A player is two model units tall; include wear layers and a
                // small fixed yaw with a conservative 2.1 by 1.2 unit envelope.
                assertTrue(grid.modelY(index) - 2.1D * scale >= card.y() + 2);
                assertTrue(grid.modelY(index) + .02D * scale < card.bottom() - 1);
                assertTrue(grid.modelX(index) - .6D * scale > card.x() + 1);
                assertTrue(grid.modelX(index) + .6D * scale < card.right() - 1);
                assertTrue(scale >= 8, "full body must remain visible at supported GUI sizes");
            }
        }
    }

    @Test void hitTestingUsesCardBoundsAndLeavesGapsAndStatusInert() {
        for (int[] size : SIZES) {
            CatalogGridLayout grid = CatalogGridLayout.calculate(AuroraSkinsLayout.calculate(size[0], size[1]));
            for (int index = 0; index < CatalogGridState.PAGE_SIZE; index++) {
                AuroraSkinsLayout.Slot card = grid.slot(index);
                assertEquals(index, grid.hit(card.x(), card.y()));
                assertEquals(index, grid.hit(card.right() - .01D, card.bottom() - .01D));
                assertEquals(index, grid.hit(grid.modelX(index), grid.modelY(index)));
                assertEquals(-1, grid.hit(card.right(), card.y()));
                assertEquals(-1, grid.hit(card.x(), card.bottom()));
            }
            assertEquals(-1, grid.hit(grid.statusX(), grid.statusY()));
            assertEquals(-1, grid.hit(-1, -1));
            assertEquals(-1, grid.hit(size[0], size[1]));
        }
    }

    @Test void compactHeightUsesTwoRowsAndResizeRestoresDefaultGrid() {
        CatalogGridLayout compact = CatalogGridLayout.calculate(AuroraSkinsLayout.calculate(320, 240));
        CatalogGridLayout wide = CatalogGridLayout.calculate(AuroraSkinsLayout.calculate(854, 480));
        assertEquals(6, compact.columns());
        assertEquals(2, compact.rows());
        assertEquals(4, wide.columns());
        assertEquals(3, wide.rows());
        assertTrue(compact.statusWidth() >= 270);
        assertTrue(wide.modelScale(0) > compact.modelScale(0));
    }
}
