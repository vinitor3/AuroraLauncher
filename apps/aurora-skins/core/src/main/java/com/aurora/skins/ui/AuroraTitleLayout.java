package com.aurora.skins.ui;

/** Responsive placement for the title-screen player preview. */
public final class AuroraTitleLayout {
    private final int previewX;
    private final int previewY;
    private final int previewScale;

    private AuroraTitleLayout(int previewX, int previewY, int previewScale) {
        this.previewX = previewX;
        this.previewY = previewY;
        this.previewScale = previewScale;
    }

    public static AuroraTitleLayout calculate(int width, int height) {
        int safeWidth = Math.max(240, width);
        int safeHeight = Math.max(180, height);
        int scale = clamp(safeHeight / 5, 34, 52);
        int preferredRight = safeWidth / 2 + Math.min(178, safeWidth / 3);
        int x = clamp(preferredRight, safeWidth / 2 + 112, safeWidth - scale - 14);
        if (safeWidth < 390) x = safeWidth - scale - 10;
        int y = clamp(safeHeight - 42, 132, safeHeight - 30);
        return new AuroraTitleLayout(x, y, scale);
    }

    public int previewX() { return previewX; }
    public int previewY() { return previewY; }
    public int previewScale() { return previewScale; }

    private static int clamp(int value, int minimum, int maximum) {
        return Math.max(minimum, Math.min(maximum, value));
    }
}
