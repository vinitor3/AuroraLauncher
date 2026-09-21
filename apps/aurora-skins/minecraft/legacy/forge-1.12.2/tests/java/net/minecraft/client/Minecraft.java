package net.minecraft.client;

import net.minecraft.client.gui.GuiScreen;

/** SRG-only fixture catches development-name assumptions. */
public final class Minecraft {
    private static final Minecraft INSTANCE = new Minecraft();
    public Object field_71462_r;
    public int displayed;
    public int field_71443_c = 1708, field_71440_d = 960;
    public final net.minecraft.client.renderer.texture.TextureManager textures = new net.minecraft.client.renderer.texture.TextureManager();
    public static Minecraft func_71410_x() { return INSTANCE; }
    public net.minecraft.client.renderer.texture.TextureManager func_110434_K() { return textures; }
    public void func_147108_a(GuiScreen screen) { field_71462_r = screen; displayed++; }
    // No scheduler: exercise the client-tick fallback without an OpenGL thread.
}
