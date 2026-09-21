package com.aurora.skins.ui;

/** Twelve logical-GUI cards between the existing search and action controls. */
public final class CatalogGridLayout {
    private final AuroraSkinsLayout.Slot[] slots;
    private final int columns;
    private final int rows;
    private final int statusX;
    private final int statusY;
    private final int statusWidth;

    private CatalogGridLayout(AuroraSkinsLayout.Slot[] slots, int columns, int rows,
                              int statusX, int statusY, int statusWidth) {
        this.slots = slots;
        this.columns = columns;
        this.rows = rows;
        this.statusX = statusX;
        this.statusY = statusY;
        this.statusWidth = statusWidth;
    }

    public static CatalogGridLayout calculate(AuroraSkinsLayout screen) {
        int left = screen.compact() ? screen.navigation(0).x() : screen.search().x();
        int right = screen.search().right();
        int top = screen.search().bottom() + 4;
        int controlsTop = Math.min(screen.previous().y(), screen.favorite().y());
        int statusY = controlsTop - 11;
        int bottom = statusY - 3;
        int columns = bottom - top < 180 ? 6 : 4;
        int rows = CatalogGridState.PAGE_SIZE / columns;
        int gap = screen.compact() ? 3 : 6;
        int availableWidth = right - left - (columns - 1) * gap;
        int availableHeight = bottom - top - (rows - 1) * gap;
        AuroraSkinsLayout.Slot[] slots = new AuroraSkinsLayout.Slot[CatalogGridState.PAGE_SIZE];
        for (int index = 0; index < slots.length; index++) {
            int column = index % columns;
            int row = index / columns;
            int x = left + column * availableWidth / columns + column * gap;
            int y = top + row * availableHeight / rows + row * gap;
            int width = (column + 1) * availableWidth / columns - column * availableWidth / columns;
            int height = (row + 1) * availableHeight / rows - row * availableHeight / rows;
            slots[index] = new AuroraSkinsLayout.Slot(x, y, width, height);
        }
        return new CatalogGridLayout(slots, columns, rows, left, statusY, right - left);
    }

    public AuroraSkinsLayout.Slot slot(int index) { return slots[index]; }
    public int columns() { return columns; }
    public int rows() { return rows; }
    public int statusX() { return statusX; }
    public int statusY() { return statusY; }
    public int statusWidth() { return statusWidth; }
    public int modelX(int index) { return slots[index].x() + slots[index].width() / 2; }
    public int modelY(int index) {
        // The outer trouser layer extends a little below the nominal feet.
        return slots[index].bottom() - 3 - (int) Math.ceil(modelScale(index) * .02D);
    }

    /** Includes the outer skin layer and room for a fixed, slightly angled pose. */
    public int modelScale(int index) {
        AuroraSkinsLayout.Slot slot = slots[index];
        return Math.max(1, (int) Math.min((slot.height() - 6) / 2.14D, (slot.width() - 6) / 1.2D));
    }

    /** Uses exactly the drawn rectangles; gaps and the status row are not cards. */
    public int hit(double mouseX, double mouseY) {
        for (int index = 0; index < slots.length; index++) {
            AuroraSkinsLayout.Slot slot = slots[index];
            if (mouseX >= slot.x() && mouseX < slot.right()
                && mouseY >= slot.y() && mouseY < slot.bottom()) return index;
        }
        return -1;
    }
}
