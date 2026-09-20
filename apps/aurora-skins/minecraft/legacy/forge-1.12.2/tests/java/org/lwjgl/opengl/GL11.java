package org.lwjgl.opengl;
import java.nio.FloatBuffer;
import java.util.*;
public final class GL11 {
    public static final Map<Integer, Boolean> flags = new TreeMap<Integer, Boolean>();
    public static final Map<Integer, Integer> integers = new TreeMap<Integer, Integer>();
    public static float[] color = {0.25F, 0.5F, 0.75F, 1F};
    public static int matrixDepth, attributeDepth;
    public static boolean failTranslation;
    public static void reset() {
        for (int key : new int[] {2929,3042,2884,2896,2903,32826,3553,16384,16385,2930}) flags.put(key, false);
        flags.put(2884, true); flags.put(16384, true);
        integers.put(32969, 1); integers.put(32968, 0); integers.put(32971, 1); integers.put(32970, 0);
        integers.put(32873, 7); integers.put(2900, 7425); integers.put(2901, 1028); integers.put(2902, 4608);
    }
    public static boolean glGetBoolean(int key) { return flags.get(key); }
    public static int glGetInteger(int key) { return integers.get(key); }
    public static void glGetFloat(int key, FloatBuffer target) { for (int i = 0; i < 4; i++) target.put(i, color[i]); }
    public static void glPushAttrib(int mask) { attributeDepth++; }
    public static void glPopAttrib() { if (--attributeDepth < 0) throw new AssertionError("attribute stack underflow"); }
    public static String snapshot() { return flags.toString() + integers + Arrays.toString(color) + matrixDepth + ":" + attributeDepth; }
}
