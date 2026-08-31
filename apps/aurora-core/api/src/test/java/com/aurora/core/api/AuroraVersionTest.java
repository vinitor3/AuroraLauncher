package com.aurora.core.api;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class AuroraVersionTest {
    @Test void ordersStableAndPrereleaseVersions() {
        assertTrue(AuroraVersion.parse("1.1.0").isAtLeast("1.0.9"));
        assertTrue(AuroraVersion.parse("1.0.0").compareTo(AuroraVersion.parse("1.0.0-rc.1")) > 0);
        assertFalse(AuroraVersion.parse("0.9.9").isAtLeast("1.0.0"));
    }

    @Test void rejectsNonSemanticVersions() {
        assertThrows(IllegalArgumentException.class, () -> AuroraVersion.parse("1.0"));
        assertThrows(IllegalArgumentException.class, () -> AuroraVersion.parse("01.0.0"));
    }
}
