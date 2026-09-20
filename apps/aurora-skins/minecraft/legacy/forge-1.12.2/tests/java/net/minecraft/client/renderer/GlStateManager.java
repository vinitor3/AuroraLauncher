package net.minecraft.client.renderer;
import org.lwjgl.opengl.GL11;
/** SRG-only state facade, including the actual primitive overloads. */
public final class GlStateManager {
    public static void func_179126_j() { GL11.flags.put(2929, true); }
    public static void func_179097_i() { GL11.flags.put(2929, false); }
    public static void func_179147_l() { GL11.flags.put(3042, true); }
    public static void func_179084_k() { GL11.flags.put(3042, false); }
    public static void func_179089_o() { GL11.flags.put(2884, true); }
    public static void func_179129_p() { GL11.flags.put(2884, false); }
    public static void func_179145_e() { GL11.flags.put(2896, true); }
    public static void func_179140_f() { GL11.flags.put(2896, false); }
    public static void func_179142_g() { GL11.flags.put(2903, true); }
    public static void func_179119_h() { GL11.flags.put(2903, false); }
    public static void func_179091_B() { GL11.flags.put(32826, true); }
    public static void func_179101_C() { GL11.flags.put(32826, false); }
    public static void func_179098_w() { GL11.flags.put(3553, true); }
    public static void func_179090_x() { GL11.flags.put(3553, false); }
    public static void func_179085_a(int light) { GL11.flags.put(16384 + light, true); }
    public static void func_179122_b(int light) { GL11.flags.put(16384 + light, false); }
    public static void func_179132_a(boolean value) { GL11.flags.put(2930, value); }
    public static void func_179120_a(int sr, int dr, int sa, int da) { GL11.integers.put(32969, sr); GL11.integers.put(32968, dr); GL11.integers.put(32971, sa); GL11.integers.put(32970, da); }
    public static void func_179144_i(int texture) { GL11.integers.put(32873, texture); }
    public static void func_179103_j(int shade) { GL11.integers.put(2900, shade); }
    public static void func_179104_a(int face, int mode) { GL11.integers.put(2901, face); GL11.integers.put(2902, mode); }
    public static void func_179131_c(float r, float g, float b, float a) { GL11.color = new float[] {r,g,b,a}; }
    public static void func_179094_E() { GL11.matrixDepth++; }
    public static void func_179121_F() { if (--GL11.matrixDepth < 0) throw new AssertionError("matrix stack underflow"); }
    public static void func_179109_b(float x, float y, float z) { if (GL11.failTranslation) throw new IllegalStateException("injected GL failure"); }
    public static void func_179152_a(float x, float y, float z) { }
    public static void func_179114_b(float angle, float x, float y, float z) { }
}
