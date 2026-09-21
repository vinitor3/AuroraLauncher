package net.minecraft.client.model;
public final class ModelPlayer {
    public static boolean failConstruction;
    public static final java.util.Set<ModelPlayer> renderedModels = java.util.Collections.newSetFromMap(new java.util.IdentityHashMap<ModelPlayer, Boolean>());
    public final boolean slim;
    // Actual 1.12.2 SRG names (MCP config 1.12.2), not the older 1.7 aliases.
    public final Part field_78116_c = new Part(this), field_178720_f = new Part(this), field_78115_e = new Part(this);
    public final Part field_178723_h = new Part(this), field_178724_i = new Part(this), field_178721_j = new Part(this), field_178722_k = new Part(this);
    public final Part field_178730_v = new Part(this), field_178732_b = new Part(this), field_178734_a = new Part(this), field_178731_d = new Part(this), field_178733_c = new Part(this);
    public static int renderedParts;
    public static final class Part {
        private final ModelPlayer owner;
        Part(ModelPlayer owner) { this.owner = owner; }
        public float field_78795_f, field_78796_g, field_78808_h;
        public void func_78785_a(float scale) { renderedParts++; renderedModels.add(owner); }
    }
    public ModelPlayer(float size, boolean slim) { this.slim = slim; if (failConstruction) throw new IllegalStateException("injected constructor failure"); }
}
