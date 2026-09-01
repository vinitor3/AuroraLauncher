package com.aurora.core.api.ui;

import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;

final class AuroraButtonLayoutTest {
    @Test void fillsTheMissingColumnAddedByAnotherMod() {
        List<AuroraButtonLayout.Rect> buttons = new ArrayList<AuroraButtonLayout.Rect>();
        buttons.add(new AuroraButtonLayout.Rect(85, 160, 150, 20));
        buttons.add(new AuroraButtonLayout.Rect(140, 208, 200, 20));

        AuroraButtonLayout.Slot slot = AuroraButtonLayout.findSlot(480, 250, buttons);

        assertEquals(245, slot.x());
        assertEquals(160, slot.y());
        assertEquals(150, slot.width());
    }

    @Test void createsAnEmptyRowAboveDoneWithoutOverlappingTwoColumns() {
        List<AuroraButtonLayout.Rect> buttons = new ArrayList<AuroraButtonLayout.Rect>();
        buttons.add(new AuroraButtonLayout.Rect(85, 160, 150, 20));
        buttons.add(new AuroraButtonLayout.Rect(245, 160, 150, 20));
        buttons.add(new AuroraButtonLayout.Rect(140, 208, 200, 20));

        AuroraButtonLayout.Slot slot = AuroraButtonLayout.findSlot(480, 250, buttons);

        assertEquals(140, slot.x());
        assertEquals(184, slot.y());
        assertEquals(200, slot.width());
        for (AuroraButtonLayout.Rect button : buttons) assertFalse(overlaps(slot, button));
    }

    private static boolean overlaps(AuroraButtonLayout.Slot slot, AuroraButtonLayout.Rect rectangle) {
        return slot.x() < rectangle.x() + rectangle.width()
            && slot.x() + slot.width() > rectangle.x()
            && slot.y() < rectangle.y() + rectangle.height()
            && slot.y() + slot.height() > rectangle.y();
    }
}
