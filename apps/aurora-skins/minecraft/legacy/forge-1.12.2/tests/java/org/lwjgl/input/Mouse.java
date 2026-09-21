package org.lwjgl.input;

public final class Mouse {
    public static int x, y, button;
    public static boolean pressed = true;
    public static boolean getEventButtonState() { return pressed; }
    public static int getEventX() { return x; }
    public static int getEventY() { return y; }
    public static int getEventButton() { return button; }
}
