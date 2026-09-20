package org.lwjgl.input;
public final class Keyboard {
    public static int key;
    public static char character;
    public static boolean getEventKeyState() { return true; }
    public static int getEventKey() { return key; }
    public static char getEventCharacter() { return character; }
}
