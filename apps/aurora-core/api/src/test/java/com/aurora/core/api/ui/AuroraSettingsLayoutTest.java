package com.aurora.core.api.ui;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class AuroraSettingsLayoutTest {
    @Test void statusNeverInvadesTheAvatarColumnAtSmallGuiWidths() {
        AuroraSettingsLayout layout = AuroraSettingsLayout.calculate(478, 252);
        assertTrue(layout.contentWidth() >= 120);
        assertTrue(layout.contentX() + layout.contentWidth() <= layout.avatarX() - 42);
        assertTrue(layout.avatarX() < layout.panelRight());
    }

    @Test void panelRemainsCenteredOnWideScreens() {
        AuroraSettingsLayout layout = AuroraSettingsLayout.calculate(1280, 720);
        assertEquals(500, layout.panelRight() - layout.panelLeft());
        assertEquals(390, layout.panelLeft());
    }

    @Test void minimumGuiWidthDoesNotForceTextOverTheAvatar() {
        AuroraSettingsLayout layout = AuroraSettingsLayout.calculate(320, 240);
        assertTrue(layout.contentWidth() > 0);
        assertTrue(layout.contentX() + layout.contentWidth() <= layout.avatarX() - 42);
    }
}
