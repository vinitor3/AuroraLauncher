package com.aurora.skins.minecraft.fabric;

import com.aurora.skins.sync.SkinSyncServer;
import net.fabricmc.fabric.api.event.lifecycle.v1.ServerLifecycleEvents;
import net.fabricmc.fabric.api.networking.v1.PayloadTypeRegistry;
import net.fabricmc.fabric.api.networking.v1.ServerPlayConnectionEvents;
import net.fabricmc.fabric.api.networking.v1.ServerPlayNetworking;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerPlayer;

public final class SkinSyncFabricNetwork {
    private static final SkinSyncServer RELAY = new SkinSyncServer();

    private SkinSyncFabricNetwork() { }

    public static void initialize() {
        PayloadTypeRegistry.serverboundPlay().register(SkinSyncPayload.TYPE, SkinSyncPayload.CODEC);
        PayloadTypeRegistry.clientboundPlay().register(SkinSyncPayload.TYPE, SkinSyncPayload.CODEC);
        ServerPlayNetworking.registerGlobalReceiver(SkinSyncPayload.TYPE, (payload, context) -> {
            ServerPlayer player = context.player();
            MinecraftServer server = context.server();
            // Fabric dispatches typed play payloads on the server thread.
            if (server.getPlayerList().getPlayer(player.getUUID()) == player && payload.bytes().length > 0) {
                RELAY.receive(player.getUUID(), payload.bytes(), System.currentTimeMillis(), sender(server));
            }
        });
        ServerPlayConnectionEvents.DISCONNECT.register((handler, server) ->
                RELAY.disconnect(handler.player.getUUID(), sender(server)));
        ServerLifecycleEvents.SERVER_STOPPED.register(server -> RELAY.clear());
    }

    private static SkinSyncServer.Sender sender(MinecraftServer server) {
        return (target, bytes) -> {
            ServerPlayer player = server.getPlayerList().getPlayer(target);
            if (player != null && ServerPlayNetworking.canSend(player, SkinSyncPayload.TYPE)) {
                ServerPlayNetworking.send(player, new SkinSyncPayload(bytes));
            }
        };
    }
}
