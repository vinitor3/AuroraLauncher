package com.aurora.skins.legacy;

import com.aurora.skins.AuroraSkinsRuntime;
import com.aurora.skins.api.SkinModel;
import com.aurora.skins.sync.SkinSyncClient;
import java.util.UUID;
import java.util.function.Consumer;

final class LegacySkinClientNetwork {
    private static SkinSyncClient sync;
    private static Consumer<byte[]> pending;
    private LegacySkinClientNetwork() { }
    static synchronized void initialize(AuroraSkinsRuntime runtime) {
        if(sync!=null) return;
        sync=new SkinSyncClient(runtime,new SkinSyncClient.Listener(){
            @Override public void apply(UUID id,byte[] png,SkinModel model){ LegacySkinManager.setRemoteSkin(id,png,model); }
            @Override public void cape(UUID id, byte[] png){ LegacySkinManager.setRemoteCape(id, png); }
            @Override public void remove(UUID id){ LegacySkinManager.removeRemoteSkin(id); }
            @Override public void clear(){ LegacySkinManager.clearRemoteSkins(); }
        },LegacyMinecraft::execute);
        if(pending!=null) sync.connect(pending);
    }
    static synchronized void connected(Consumer<byte[]> sender){ pending=sender; if(sync!=null) sync.connect(sender); }
    static synchronized void receive(byte[] packet){ if(sync!=null) sync.receive(packet); }
    static synchronized void disconnected(){ pending=null; if(sync!=null) sync.disconnect(); else LegacySkinManager.clearRemoteSkins(); }
    static synchronized void shutdown(){ pending=null; if(sync!=null) sync.close(); sync=null; }
}
