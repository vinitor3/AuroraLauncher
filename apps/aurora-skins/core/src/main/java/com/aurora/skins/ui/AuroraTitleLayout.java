package com.aurora.skins.ui;

import java.util.Collections;
import java.util.List;

/** Responsive title-screen panel that keeps the Skins button directly below the player. */
public final class AuroraTitleLayout {
    private static final int BUTTON_WIDTH = 96;
    private static final int BUTTON_HEIGHT = 20;
    private final int previewX;
    private final int previewY;
    private final int previewScale;
    private final int panelLeft;
    private final int panelTop;
    private final int panelRight;
    private final int panelBottom;
    private final int buttonX;
    private final int buttonY;

    private AuroraTitleLayout(int previewX, int previewY, int previewScale,
                              int panelLeft, int panelTop, int panelRight, int panelBottom,
                              int buttonX, int buttonY) {
        this.previewX = previewX;
        this.previewY = previewY;
        this.previewScale = previewScale;
        this.panelLeft = panelLeft;
        this.panelTop = panelTop;
        this.panelRight = panelRight;
        this.panelBottom = panelBottom;
        this.buttonX = buttonX;
        this.buttonY = buttonY;
    }

    public static AuroraTitleLayout calculate(int width, int height) {
        return calculate(width, height, Collections.<Rect>emptyList());
    }

    public static AuroraTitleLayout calculate(int width, int height, List<Rect> occupied) {
        int safeWidth = Math.max(240, width);
        int safeHeight = Math.max(180, height);
        int scale = safeWidth < 420 ? 34 : clamp(safeHeight / 5, 34, 52);
        int panelWidth = Math.max(BUTTON_WIDTH, scale + 34);
        int fullHeight = scale * 2 + BUTTON_HEIGHT + 26;
        int preferredCenter = safeWidth / 2 + Math.min(178, safeWidth / 3);
        int preferredLeft = clamp(preferredCenter - panelWidth / 2, 6, safeWidth - panelWidth - 6);
        int preferredTop = clamp((safeHeight - fullHeight) / 2, 24, safeHeight - fullHeight - 6);
        int[] lefts = unique(preferredLeft, safeWidth - panelWidth - 8, 8,
            clamp(safeWidth / 2 + 104, 6, safeWidth - panelWidth - 6),
            clamp(safeWidth / 2 - 104 - panelWidth, 6, safeWidth - panelWidth - 6));
        int[] tops = unique(preferredTop, 28, safeHeight - fullHeight - 8, 8);

        int bestLeft = preferredLeft;
        int bestTop = preferredTop;
        long bestScore = Long.MAX_VALUE;
        for (int left : lefts) {
            for (int top : tops) {
                Rect candidate = new Rect(left, top, panelWidth, fullHeight);
                long overlap = 0;
                if (occupied != null) for (Rect item : occupied) overlap += overlapArea(candidate, item.expand(4));
                long score = overlap * 10000L + Math.abs(left - preferredLeft) * 10L
                    + Math.abs(top - preferredTop);
                if (score < bestScore) {
                    bestScore = score;
                    bestLeft = left;
                    bestTop = top;
                }
            }
        }

        int previewX = bestLeft + panelWidth / 2;
        int previewY = bestTop + 8 + scale * 2;
        int buttonX = bestLeft + (panelWidth - BUTTON_WIDTH) / 2;
        int buttonY = previewY + 6;
        return new AuroraTitleLayout(previewX, previewY, scale, bestLeft, bestTop,
            bestLeft + panelWidth, previewY + 4, buttonX, buttonY);
    }

    public int previewX() { return previewX; }
    public int previewY() { return previewY; }
    public int previewScale() { return previewScale; }
    public int panelLeft() { return panelLeft; }
    public int panelTop() { return panelTop; }
    public int panelRight() { return panelRight; }
    public int panelBottom() { return panelBottom; }
    public int buttonX() { return buttonX; }
    public int buttonY() { return buttonY; }
    public int buttonWidth() { return BUTTON_WIDTH; }
    public int buttonHeight() { return BUTTON_HEIGHT; }

    private static long overlapArea(Rect first, Rect second) {
        int width = Math.max(0, Math.min(first.right(), second.right()) - Math.max(first.x, second.x));
        int height = Math.max(0, Math.min(first.bottom(), second.bottom()) - Math.max(first.y, second.y));
        return (long) width * height;
    }

    private static int[] unique(int... values) {
        int[] result = new int[values.length];
        int count = 0;
        for (int value : values) {
            boolean found = false;
            for (int index = 0; index < count; index++) if (result[index] == value) found = true;
            if (!found) result[count++] = value;
        }
        int[] compact = new int[count];
        System.arraycopy(result, 0, compact, 0, count);
        return compact;
    }

    private static int clamp(int value, int minimum, int maximum) {
        if (maximum < minimum) return minimum;
        return Math.max(minimum, Math.min(maximum, value));
    }

    public static final class Rect {
        private final int x;
        private final int y;
        private final int width;
        private final int height;

        public Rect(int x, int y, int width, int height) {
            this.x = x;
            this.y = y;
            this.width = Math.max(0, width);
            this.height = Math.max(0, height);
        }

        private int right() { return x + width; }
        private int bottom() { return y + height; }
        private Rect expand(int amount) {
            return new Rect(x - amount, y - amount, width + amount * 2, height + amount * 2);
        }
    }
}
