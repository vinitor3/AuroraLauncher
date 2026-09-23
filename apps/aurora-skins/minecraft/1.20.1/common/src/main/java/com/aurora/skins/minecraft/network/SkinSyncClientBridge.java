package com.aurora.skins.minecraft.network;

import com.aurora.skins.AuroraSkinsRuntime;
import com.aurora.skins.api.SkinModel;
import com.aurora.skins.minecraft.appearance.AuroraSkinManager;
import com.aurora.skins.sync.SkinSyncClient;
import net.minecraft.client.Minecraft;
import java.util.UUID;
import java.util.function.Consumer;

/** Client-only lifecycle boundary: dedicated-server adapters never resolve this class. */
public final class SkinSyncClientBridge {
    private static SkinSyncClient sync;
    private static Consumer<byte[]> pending;
    private SkinSyncClientBridge() { }
    public static synchronized void initialize(AuroraSkinsRuntime runtime) {
        if (sync != null) return;
        sync = new SkinSyncClient(runtime, new SkinSyncClient.Listener() {
            @Override public void apply(UUID player, byte[] png, SkinModel model) { AuroraSkinManager.setRemoteSkin(player, png, model); }
            @Override public void remove(UUID player) { AuroraSkinManager.removeRemoteSkin(player); }
            @Override public void clear() { AuroraSkinManager.clearRemoteSkins(); }
        }, action -> Minecraft.getInstance().execute(action));
        if (pending != null) sync.connect(pending);
    }
    public static synchronized void connected(Consumer<byte[]> sender) {
        pending = sender;
        if (sync != null) sync.connect(sender);
    }
    public static synchronized void receive(byte[] packet) { if (sync != null) sync.receive(packet); }
    public static synchronized void disconnected() {
        pending = null;
        if (sync != null) sync.disconnect();
        else AuroraSkinManager.clearRemoteSkins();
    }
    public static synchronized void shutdown() {
        pending = null;
        if (sync != null) sync.close();
        sync = null;
    }
}
