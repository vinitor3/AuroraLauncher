package com.aurora.skins.ui;

/**
 * Version-neutral responsive layout for the Aurora Skins screen.
 *
 * <p>Minecraft can expose a very small logical GUI after GUI scaling. The
 * layout therefore switches from a sidebar to a two-column navigation grid
 * and computes every action button from the available width.</p>
 */
public final class AuroraSkinsLayout {
    private final int width;
    private final int height;
    private final boolean compact;
    private final Slot[] navigation;
    private final Slot importButton;
    private final Slot search;
    private final Slot previous;
    private final Slot next;
    private final Slot apply;
    private final Slot favorite;
    private final Slot model;
    private final Slot done;
    private final int panelLeft;
    private final int panelRight;
    private final int previewX;
    private final int previewY;
    private final int previewScale;
    private final int textX;
    private final int textY;
    private final int textWidth;

    private AuroraSkinsLayout(int width, int height, boolean compact, Slot[] navigation,
                              Slot importButton, Slot search, Slot previous, Slot next,
                              Slot apply, Slot favorite, Slot model, Slot done,
                              int panelLeft, int panelRight, int previewX, int previewY,
                              int previewScale, int textX, int textY, int textWidth) {
        this.width = width;
        this.height = height;
        this.compact = compact;
        this.navigation = navigation;
        this.importButton = importButton;
        this.search = search;
        this.previous = previous;
        this.next = next;
        this.apply = apply;
        this.favorite = favorite;
        this.model = model;
        this.done = done;
        this.panelLeft = panelLeft;
        this.panelRight = panelRight;
        this.previewX = previewX;
        this.previewY = previewY;
        this.previewScale = previewScale;
        this.textX = textX;
        this.textY = textY;
        this.textWidth = textWidth;
    }

    public static AuroraSkinsLayout calculate(int screenWidth, int screenHeight) {
        int width = Math.max(240, screenWidth);
        int height = Math.max(180, screenHeight);
        boolean compact = width < 560 || height < 260;
        int margin = compact ? 12 : 18;
        int panelLeft = margin;
        int panelRight = width - margin;
        int innerLeft = panelLeft + (compact ? 8 : 10);
        int innerRight = panelRight - (compact ? 8 : 10);
        int innerWidth = innerRight - innerLeft;
        Slot[] navigation = new Slot[4];
        Slot importButton;
        Slot search;
        int textX;
        int textY;

        if (compact) {
            int gap = 6;
            int columnWidth = (innerWidth - gap) / 2;
            for (int index = 0; index < navigation.length; index++) {
                int column = index % 2;
                int row = index / 2;
                navigation[index] = new Slot(innerLeft + column * (columnWidth + gap),
                    42 + row * 24, columnWidth, 20);
            }
            importButton = new Slot(innerLeft, 90, columnWidth, 20);
            search = new Slot(innerLeft + columnWidth + gap, 90, columnWidth, 20);
            textX = innerLeft;
            textY = 120;
        } else {
            int navigationWidth = clamp(width / 5, 104, 132);
            for (int index = 0; index < navigation.length; index++) {
                navigation[index] = new Slot(innerLeft, 54 + index * 24, navigationWidth, 20);
            }
            importButton = new Slot(innerLeft, 166, navigationWidth, 20);
            int contentLeft = innerLeft + navigationWidth + 18;
            search = new Slot(contentLeft, 54, Math.max(92, innerRight - contentLeft), 20);
            textX = contentLeft;
            textY = 88;
        }

        Slot done = new Slot((width - Math.min(150, width - 32)) / 2,
            height - 30, Math.min(150, width - 32), 20);
        int controlsY = height - 58;
        Slot previous;
        Slot next;
        Slot apply;
        Slot favorite;
        Slot model;
        int controlsLeft = compact ? innerLeft : textX;
        int controlsRight = innerRight;
        int available = controlsRight - controlsLeft;

        if (available < 244) {
            int firstGap = 4;
            int arrowWidth = 28;
            int applyWidth = Math.max(72, available - arrowWidth * 2 - firstGap * 2);
            previous = new Slot(controlsLeft, controlsY - 24, arrowWidth, 20);
            next = new Slot(previous.right() + firstGap, controlsY - 24, arrowWidth, 20);
            apply = new Slot(next.right() + firstGap, controlsY - 24, applyWidth, 20);
            int secondWidth = (available - firstGap) / 2;
            favorite = new Slot(controlsLeft, controlsY, secondWidth, 20);
            model = new Slot(favorite.right() + firstGap, controlsY,
                controlsRight - favorite.right() - firstGap, 20);
        } else {
            int gap = 4;
            int arrowWidth = compact ? 24 : 28;
            int applyWidth = compact ? clamp(width / 5, 58, 72) : 92;
            int favoriteWidth = compact ? clamp(width / 4, 70, 88) : 94;
            int modelWidth = available - arrowWidth * 2 - applyWidth - favoriteWidth - gap * 4;
            previous = new Slot(controlsLeft, controlsY, arrowWidth, 20);
            next = new Slot(previous.right() + gap, controlsY, arrowWidth, 20);
            apply = new Slot(next.right() + gap, controlsY, applyWidth, 20);
            favorite = new Slot(apply.right() + gap, controlsY, favoriteWidth, 20);
            model = new Slot(favorite.right() + gap, controlsY, modelWidth, 20);
        }

        int previewScale = compact ? clamp(height / 6, 30, 42) : clamp(height / 5, 38, 54);
        int previewX = compact ? innerRight - 44 : Math.min(innerRight - 44,
            Math.max(width / 2 + 82, width - 98));
        int upperControlsY = Math.min(previous.y(), favorite.y());
        int previewY = compact ? upperControlsY - 7 : height - 72;
        // The full model is two scale units tall. Keep its head below the
        // search/navigation rows even with Minecraft's large GUI scaling.
        int previewTop = search.bottom() + 4;
        previewScale = Math.max(12, Math.min(previewScale, (previewY - previewTop) / 2));
        int textWidth = Math.max(60, previewX - textX - 48);

        return new AuroraSkinsLayout(width, height, compact, navigation, importButton,
            search, previous, next, apply, favorite, model, done, panelLeft, panelRight,
            previewX, previewY, previewScale, textX, textY, textWidth);
    }

    public boolean compact() { return compact; }
    public Slot navigation(int index) { return navigation[index]; }
    public Slot importButton() { return importButton; }
    public Slot search() { return search; }
    public Slot previous() { return previous; }
    public Slot next() { return next; }
    public Slot apply() { return apply; }
    public Slot favorite() { return favorite; }
    public Slot model() { return model; }
    public Slot done() { return done; }
    public int panelLeft() { return panelLeft; }
    public int panelRight() { return panelRight; }
    public int panelTop() { return 34; }
    public int panelBottom() { return height - 36; }
    public int previewX() { return previewX; }
    public int previewY() { return previewY; }
    public int previewScale() { return previewScale; }
    public int textX() { return textX; }
    public int textY() { return textY; }
    public int textWidth() { return textWidth; }
    public int width() { return width; }
    public int height() { return height; }

    private static int clamp(int value, int minimum, int maximum) {
        return Math.max(minimum, Math.min(maximum, value));
    }

    public static final class Slot {
        private final int x;
        private final int y;
        private final int width;
        private final int height;

        public Slot(int x, int y, int width, int height) {
            this.x = x;
            this.y = y;
            this.width = Math.max(1, width);
            this.height = Math.max(1, height);
        }

        public int x() { return x; }
        public int y() { return y; }
        public int width() { return width; }
        public int height() { return height; }
        public int right() { return x + width; }
        public int bottom() { return y + height; }

        public boolean intersects(Slot other) {
            return x < other.right() && right() > other.x
                && y < other.bottom() && bottom() > other.y;
        }
    }
}
