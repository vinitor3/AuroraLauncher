package com.aurora.skins.legacy;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.FloatBuffer;

/** Restores both OpenGL state and GlStateManager's cache after a GUI preview. */
final class LegacyPreviewGlState implements AutoCloseable {
    private static final int[] FLAGS = {2929, 3042, 2884, 2896, 2903, 32826, 3553};
    private static final String[][] ENABLE = {
        {"enableDepth", "func_179126_j"}, {"enableBlend", "func_179147_l"},
        {"enableCull", "func_179089_o"}, {"enableLighting", "func_179145_e"},
        {"enableColorMaterial", "func_179142_g"}, {"enableRescaleNormal", "func_179091_B"},
        {"enableTexture2D", "func_179098_w"}
    };
    private static final String[][] DISABLE = {
        {"disableDepth", "func_179097_i"}, {"disableBlend", "func_179084_k"},
        {"disableCull", "func_179129_p"}, {"disableLighting", "func_179140_f"},
        {"disableColorMaterial", "func_179119_h"}, {"disableRescaleNormal", "func_179101_C"},
        {"disableTexture2D", "func_179090_x"}
    };
    private final boolean[] enabled = new boolean[FLAGS.length];
    private final boolean light0, light1, depthMask;
    private final int sourceRgb, destRgb, sourceAlpha, destAlpha, texture, shade, materialFace, materialMode;
    private final FloatBuffer color = ByteBuffer.allocateDirect(64).order(ByteOrder.nativeOrder()).asFloatBuffer();
    private boolean matrixPushed;

    LegacyPreviewGlState() throws Exception {
        for (int i = 0; i < FLAGS.length; i++) enabled[i] = flag(FLAGS[i]);
        light0 = flag(16384); light1 = flag(16385); depthMask = flag(2930);
        sourceRgb = integer(32969); destRgb = integer(32968);
        sourceAlpha = integer(32971); destAlpha = integer(32970);
        texture = integer(32873); shade = integer(2900);
        materialFace = integer(2901); materialMode = integer(2902);
        gl("glGetFloat", new Class<?>[] {int.class, FloatBuffer.class}, 2816, color);
        // Save light parameters and current normal too. Cached state is restored explicitly below.
        gl("glPushAttrib", new Class<?>[] {int.class}, 0x0041);
    }

    void pushMatrix() throws Exception {
        state(new String[] {"pushMatrix", "func_179094_E"}); matrixPushed = true;
    }

    @Override public void close() throws Exception {
        try {
            if (matrixPushed) state(new String[] {"popMatrix", "func_179121_F"});
            for (int i = 0; i < FLAGS.length; i++) state(enabled[i] ? ENABLE[i] : DISABLE[i]);
            state(light0 ? new String[] {"enableLight", "func_179085_a"} : new String[] {"disableLight", "func_179122_b"}, 0);
            state(light1 ? new String[] {"enableLight", "func_179085_a"} : new String[] {"disableLight", "func_179122_b"}, 1);
            state(new String[] {"depthMask", "func_179132_a"}, depthMask);
            state(new String[] {"tryBlendFuncSeparate", "func_179120_a"}, sourceRgb, destRgb, sourceAlpha, destAlpha);
            state(new String[] {"bindTexture", "func_179144_i"}, texture);
            state(new String[] {"shadeModel", "func_179103_j"}, shade);
            state(new String[] {"colorMaterial", "func_179104_a"}, materialFace, materialMode);
            state(new String[] {"color", "func_179131_c"}, color.get(0), color.get(1), color.get(2), color.get(3));
        } finally {
            gl("glPopAttrib", new Class<?>[0]);
        }
    }

    private static boolean flag(int name) throws Exception { return (Boolean)gl("glGetBoolean", new Class<?>[] {int.class}, name); }
    private static int integer(int name) throws Exception { return (Integer)gl("glGetInteger", new Class<?>[] {int.class}, name); }
    private static Object gl(String name, Class<?>[] types, Object... args) throws Exception {
        return Class.forName("org.lwjgl.opengl.GL11").getMethod(name, types).invoke(null, args);
    }
    private static void state(String[] names, Object... args) throws Exception {
        LegacySkinPreviewRenderer.staticInvoke("net.minecraft.client.renderer.GlStateManager", names, args);
    }
}
