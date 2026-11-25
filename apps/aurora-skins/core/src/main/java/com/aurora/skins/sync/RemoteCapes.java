package com.aurora.skins.sync;

import java.util.HashMap;
import java.util.Map;
import java.util.UUID;
import java.util.function.Consumer;

/** Client-thread texture owners. Callers supply only 64x32 PNGs canonicalized on the sync worker. */
public final class RemoteCapes<T> {
    public interface Register<T> { T create(UUID player, byte[] png) throws Exception; }
    private final Map<UUID, T> textures = new HashMap<>();
    private final Register<T> register;
    private final Consumer<T> release;

    public RemoteCapes(Register<T> register, Consumer<T> release) {
        this.register = register;
        this.release = release;
    }
    public T get(UUID player) { return textures.get(player); }
    public void update(UUID player, byte[] pixels) {
        if (player == null) return;
        T previous = textures.remove(player);
        if (previous != null) release.accept(previous);
        if (pixels == null || pixels.length > SkinSyncProtocol.MAX_PNG_BYTES || textures.size() >= SkinSyncProtocol.MAX_PLAYERS) return;
        try {
            T next = register.create(player, pixels);
            if (next != null) textures.put(player, next);
        } catch (Exception unavailable) { /* Keep the player's vanilla cape. */ }
    }
    public void clear() {
        for (T texture : textures.values()) release.accept(texture);
        textures.clear();
    }
}
