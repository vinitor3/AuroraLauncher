package com.aurora.core.api.ui;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/** Finds a free, vanilla-aligned slot after other mods added their widgets. */
public final class AuroraButtonLayout {
    private static final int BUTTON_HEIGHT = 20;
    private static final int ROW_STEP = 24;

    private AuroraButtonLayout() { }

    public static Slot findSlot(int screenWidth, int screenHeight, List<Rect> existing) {
        int safeWidth = Math.max(120, screenWidth);
        int safeHeight = Math.max(90, screenHeight);
        List<Rect> occupied = existing == null
            ? Collections.<Rect>emptyList() : new ArrayList<Rect>(existing);

        Rect bottom = bottomCenteredButton(safeWidth, occupied);
        int bottomY = bottom == null ? safeHeight - 27 : bottom.y();
        int halfGap = safeWidth >= 340 ? 5 : 3;
        int columnWidth = Math.min(150, Math.max(80, (safeWidth - 30 - halfGap * 2) / 2));
        int leftX = safeWidth / 2 - halfGap - columnWidth;
        int rightX = safeWidth / 2 + halfGap;

        // Complete a row left with one free column by another mod.
        List<Integer> rows = distinctRows(occupied, bottomY);
        for (Integer row : rows) {
            Rect left = new Rect(leftX, row.intValue(), columnWidth, BUTTON_HEIGHT);
            Rect right = new Rect(rightX, row.intValue(), columnWidth, BUTTON_HEIGHT);
            boolean leftFree = isFree(left, occupied, 2);
            boolean rightFree = isFree(right, occupied, 2);
            if (leftFree != rightFree) return Slot.from(leftFree ? left : right);
        }

        // Otherwise use the closest empty row above the bottom/Done button.
        int centeredWidth = Math.min(200, safeWidth - 20);
        int centeredX = (safeWidth - centeredWidth) / 2;
        for (int y = bottomY - ROW_STEP; y >= 32; y -= ROW_STEP) {
            Rect candidate = new Rect(centeredX, y, centeredWidth, BUTTON_HEIGHT);
            if (isFree(candidate, occupied, 2)) return Slot.from(candidate);
        }

        // Irregular modded screens do not always follow the 24 px grid.
        for (int y = Math.min(bottomY - BUTTON_HEIGHT - 4, safeHeight - BUTTON_HEIGHT - 8);
             y >= 28; y--) {
            Rect candidate = new Rect(centeredX, y, centeredWidth, BUTTON_HEIGHT);
            if (isFree(candidate, occupied, 2)) return Slot.from(candidate);
        }

        return new Slot(centeredX, Math.max(28, safeHeight - 52), centeredWidth, BUTTON_HEIGHT);
    }

    private static Rect bottomCenteredButton(int screenWidth, List<Rect> occupied) {
        Rect result = null;
        int center = screenWidth / 2;
        for (Rect rectangle : occupied) {
            int rectangleCenter = rectangle.x() + rectangle.width() / 2;
            if (rectangle.width() < 90
                    || Math.abs(rectangleCenter - center) > Math.max(36, screenWidth / 5)) continue;
            if (result == null || rectangle.y() > result.y()) result = rectangle;
        }
        return result;
    }

    private static List<Integer> distinctRows(List<Rect> occupied, int bottomY) {
        List<Integer> rows = new ArrayList<Integer>();
        for (Rect rectangle : occupied) {
            if (rectangle.y() >= bottomY || rectangle.height() < 16 || rectangle.height() > 24) continue;
            boolean known = false;
            for (Integer row : rows) {
                if (Math.abs(row.intValue() - rectangle.y()) <= 2) {
                    known = true;
                    break;
                }
            }
            if (!known) rows.add(Integer.valueOf(rectangle.y()));
        }
        Collections.sort(rows, Collections.reverseOrder());
        return rows;
    }

    private static boolean isFree(Rect candidate, List<Rect> occupied, int padding) {
        for (Rect rectangle : occupied) {
            if (candidate.intersects(rectangle, padding)) return false;
        }
        return true;
    }

    public static final class Slot {
        private final int x;
        private final int y;
        private final int width;
        private final int height;

        public Slot(int x, int y, int width, int height) {
            this.x = x;
            this.y = y;
            this.width = width;
            this.height = height;
        }

        private static Slot from(Rect rectangle) {
            return new Slot(rectangle.x(), rectangle.y(), rectangle.width(), rectangle.height());
        }

        public int x() { return x; }
        public int y() { return y; }
        public int width() { return width; }
        public int height() { return height; }
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

        public int x() { return x; }
        public int y() { return y; }
        public int width() { return width; }
        public int height() { return height; }

        private boolean intersects(Rect other, int padding) {
            return x < other.x + other.width + padding
                && x + width + padding > other.x
                && y < other.y + other.height + padding
                && y + height + padding > other.y;
        }
    }
}
