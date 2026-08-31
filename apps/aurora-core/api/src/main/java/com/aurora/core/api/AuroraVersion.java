package com.aurora.core.api;

import java.util.regex.Matcher;
import java.util.regex.Pattern;

/** Small SemVer value object with no external dependency. */
public final class AuroraVersion implements Comparable<AuroraVersion> {
    private static final Pattern PATTERN = Pattern.compile("^(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)(?:-([0-9A-Za-z.-]+))?$");
    private final int major;
    private final int minor;
    private final int patch;
    private final String prerelease;

    private AuroraVersion(int major, int minor, int patch, String prerelease) {
        this.major = major;
        this.minor = minor;
        this.patch = patch;
        this.prerelease = prerelease;
    }

    public static AuroraVersion parse(String value) {
        Matcher matcher = PATTERN.matcher(value == null ? "" : value.trim());
        if (!matcher.matches()) throw new IllegalArgumentException("Invalid semantic version: " + value);
        return new AuroraVersion(
            Integer.parseInt(matcher.group(1)),
            Integer.parseInt(matcher.group(2)),
            Integer.parseInt(matcher.group(3)),
            matcher.group(4));
    }

    public boolean isAtLeast(String minimum) {
        return compareTo(parse(minimum)) >= 0;
    }

    @Override public int compareTo(AuroraVersion other) {
        int result = Integer.compare(major, other.major);
        if (result == 0) result = Integer.compare(minor, other.minor);
        if (result == 0) result = Integer.compare(patch, other.patch);
        if (result != 0) return result;
        if (prerelease == null) return other.prerelease == null ? 0 : 1;
        if (other.prerelease == null) return -1;
        return prerelease.compareTo(other.prerelease);
    }

    @Override public String toString() {
        return major + "." + minor + "." + patch + (prerelease == null ? "" : "-" + prerelease);
    }
}
