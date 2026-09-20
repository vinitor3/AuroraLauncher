package com.aurora.core.api.ui;

/** Responsive geometry shared by the Minecraft-specific Aurora screens. */
public final class AuroraSettingsLayout {
    private final int panelLeft;
    private final int panelRight;
    private final int navigationX;
    private final int navigationWidth;
    private final int contentX;
    private final int contentWidth;
    private final int avatarX;
    private final int avatarY;

    private AuroraSettingsLayout(int panelLeft, int panelRight, int navigationX,
                                 int navigationWidth, int contentX, int contentWidth,
                                 int avatarX, int avatarY) {
        this.panelLeft = panelLeft;
        this.panelRight = panelRight;
        this.navigationX = navigationX;
        this.navigationWidth = navigationWidth;
        this.contentX = contentX;
        this.contentWidth = contentWidth;
        this.avatarX = avatarX;
        this.avatarY = avatarY;
    }

    public static AuroraSettingsLayout calculate(int screenWidth, int screenHeight) {
        int safeWidth = Math.max(320, screenWidth);
        int safeHeight = Math.max(200, screenHeight);
        int panelWidth = Math.min(500, safeWidth - 16);
        int panelLeft = (safeWidth - panelWidth) / 2;
        int panelRight = panelLeft + panelWidth;
        int navigationWidth = Math.min(154, Math.max(118, panelWidth / 3));
        int navigationX = panelLeft + 18;
        int avatarX = panelRight - Math.max(54, Math.min(72, panelWidth / 7));
        int contentX = navigationX + navigationWidth + 20;
        int contentRight = avatarX - 42;
        int contentWidth = Math.max(1, contentRight - contentX);
        int avatarY = safeHeight - 88;
        return new AuroraSettingsLayout(panelLeft, panelRight, navigationX, navigationWidth,
            contentX, contentWidth, avatarX, avatarY);
    }

    public int panelLeft() { return panelLeft; }
    public int panelRight() { return panelRight; }
    public int navigationX() { return navigationX; }
    public int navigationWidth() { return navigationWidth; }
    public int contentX() { return contentX; }
    public int contentWidth() { return contentWidth; }
    public int avatarX() { return avatarX; }
    public int avatarY() { return avatarY; }
}
