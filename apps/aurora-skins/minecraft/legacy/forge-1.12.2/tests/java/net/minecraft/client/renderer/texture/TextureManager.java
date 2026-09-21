package net.minecraft.client.renderer.texture;

import java.util.*;

/** SRG-only texture fixture tracks independent uploads/binds/deletes. */
public final class TextureManager {
    public final Map<Object, DynamicTexture> live = new IdentityHashMap<Object, DynamicTexture>();
    public final List<Object> bound = new ArrayList<Object>();
    public int uploaded, deleted;
    public Object func_110578_a(String name, DynamicTexture texture) {
        Object location = new Object(); live.put(location, texture); uploaded++; return location;
    }
    public void func_110577_a(Object texture) { bound.add(texture); }
    public void func_147645_c(Object texture) { if (live.remove(texture) != null) deleted++; }
}
