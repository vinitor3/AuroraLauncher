package com.aurora.skins.minecraft.fabric;

import com.aurora.skins.minecraft.network.SkinSyncClientBridge;
import io.netty.buffer.Unpooled;
import net.fabricmc.fabric.api.client.networking.v1.C2SPlayChannelEvents;
import net.fabricmc.fabric.api.client.networking.v1.ClientPlayConnectionEvents;
import net.fabricmc.fabric.api.client.networking.v1.ClientPlayNetworking;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientPacketListener;
import net.minecraft.network.FriendlyByteBuf;

final class SkinSyncFabricClient {
    private static ClientPacketListener connected;

    private SkinSyncFabricClient() { }

    static void initialize() {
        ClientPlayNetworking.registerGlobalReceiver(SkinSyncFabricServer.CHANNEL, (client, handler, buffer, responseSender) -> {
            byte[] payload = SkinSyncFabricServer.copyPayload(buffer);
            if (payload != null) client.execute(() -> {
                if (client.getConnection() == handler && connected == handler) SkinSyncClientBridge.receive(payload);
            });
        });
        ClientPlayConnectionEvents.JOIN.register((handler, sender, client) -> connect(handler));
        C2SPlayChannelEvents.REGISTER.register((handler, sender, client, channels) -> {
            if (channels.contains(SkinSyncFabricServer.CHANNEL)) client.execute(() -> connect(handler));
        });
        C2SPlayChannelEvents.UNREGISTER.register((handler, sender, client, channels) -> {
            if (channels.contains(SkinSyncFabricServer.CHANNEL)) client.execute(() -> disconnect(handler));
        });
        ClientPlayConnectionEvents.DISCONNECT.register((handler, client) -> disconnect(handler));
    }

    private static void connect(ClientPacketListener handler) {
        Minecraft client = Minecraft.getInstance();
        if (client.getConnection() != handler || client.player == null || connected == handler
                || !ClientPlayNetworking.canSend(SkinSyncFabricServer.CHANNEL)) return;
        SkinSyncClientBridge.disconnected();
        connected = handler;
        SkinSyncClientBridge.connected(payload -> client.execute(() -> {
            if (connected == handler && client.getConnection() == handler && payload.length <= SkinSyncFabricServer.MAX_PAYLOAD
                    && ClientPlayNetworking.canSend(SkinSyncFabricServer.CHANNEL)) {
                ClientPlayNetworking.send(SkinSyncFabricServer.CHANNEL, new FriendlyByteBuf(Unpooled.wrappedBuffer(payload)));
            }
        }));
    }

    private static void disconnect(ClientPacketListener handler) {
        if (connected != null && connected != handler) return;
        connected = null;
        SkinSyncClientBridge.disconnected();
    }
}
