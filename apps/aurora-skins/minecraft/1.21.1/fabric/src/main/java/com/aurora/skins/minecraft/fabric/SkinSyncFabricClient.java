package com.aurora.skins.minecraft.fabric;

import com.aurora.skins.minecraft.network.SkinSyncClientBridge;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.fabricmc.fabric.api.client.networking.v1.ClientPlayConnectionEvents;
import net.fabricmc.fabric.api.client.networking.v1.ClientPlayNetworking;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientPacketListener;

final class SkinSyncFabricClient {
    private static ClientPacketListener active;

    private SkinSyncFabricClient() { }

    static void initialize() {
        ClientPlayNetworking.registerGlobalReceiver(SkinSyncPayload.TYPE, (payload, context) -> {
            if (active != null && context.client().getConnection() == active
                    && context.responseSender() == ClientPlayNetworking.getSender() && payload.bytes().length > 0) {
                SkinSyncClientBridge.receive(payload.bytes());
            }
        });
        ClientTickEvents.END_CLIENT_TICK.register(SkinSyncFabricClient::connectWhenAvailable);
        ClientPlayConnectionEvents.DISCONNECT.register((handler, client) -> {
            if (active == null || active == handler) disconnect();
        });
    }

    private static void connectWhenAvailable(Minecraft client) {
        ClientPacketListener connection = client.getConnection();
        if (active != null && (connection != active || !ClientPlayNetworking.canSend(SkinSyncPayload.TYPE))) {
            disconnect();
        }
        if (connection == null || client.player == null || active == connection
                || !ClientPlayNetworking.canSend(SkinSyncPayload.TYPE)) return;
        active = connection;
        SkinSyncClientBridge.connected(bytes -> client.execute(() -> {
            if (active == connection && client.getConnection() == connection
                    && ClientPlayNetworking.canSend(SkinSyncPayload.TYPE)) {
                ClientPlayNetworking.send(new SkinSyncPayload(bytes));
            }
        }));
    }

    private static void disconnect() {
        active = null;
        SkinSyncClientBridge.disconnected();
    }
}
