package net.minecraft.client.model;
public final class ModelPlayer {
    public static boolean failConstruction;
    // Actual 1.12.2 SRG names (MCP config 1.12.2), not the older 1.7 aliases.
    public final Part field_78116_c = new Part(), field_178720_f = new Part(), field_78115_e = new Part();
    public final Part field_178723_h = new Part(), field_178724_i = new Part(), field_178721_j = new Part(), field_178722_k = new Part();
    public final Part field_178730_v = new Part(), field_178732_b = new Part(), field_178734_a = new Part(), field_178731_d = new Part(), field_178733_c = new Part();
    public static int renderedParts;
    public static final class Part {
        public float field_78795_f, field_78796_g, field_78808_h;
        public void func_78785_a(float scale) { renderedParts++; }
    }
    public ModelPlayer(float size, boolean slim) { if (failConstruction) throw new IllegalStateException("injected constructor failure"); }
}
