package com.aurora.skins.minecraft.fabric;

import com.aurora.skins.sync.SkinSyncServer;
import io.netty.buffer.Unpooled;
import net.fabricmc.api.ModInitializer;
import net.fabricmc.fabric.api.event.lifecycle.v1.ServerLifecycleEvents;
import net.fabricmc.fabric.api.networking.v1.ServerPlayConnectionEvents;
import net.fabricmc.fabric.api.networking.v1.ServerPlayNetworking;
import net.minecraft.network.FriendlyByteBuf;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerPlayer;

/** Server-safe relay: no Aurora Core, texture or client classes are loaded here. */
public final class SkinSyncFabricServer implements ModInitializer {
    static final ResourceLocation CHANNEL = new ResourceLocation("aurora_skins", "sync");
    static final int MAX_PAYLOAD = 24640;
    private static final SkinSyncServer RELAY = new SkinSyncServer();

    @Override public void onInitialize() {
        ServerPlayNetworking.registerGlobalReceiver(CHANNEL, (server, player, handler, buffer, responseSender) -> {
            byte[] payload = copyPayload(buffer);
            if (payload == null) return;
            server.execute(() -> {
                if (server.getPlayerList().getPlayer(player.getUUID()) == player) {
                    RELAY.receive(player.getUUID(), payload, System.currentTimeMillis(), sender(server));
                }
            });
        });
        ServerPlayConnectionEvents.DISCONNECT.register((handler, server) ->
            RELAY.disconnect(handler.player.getUUID(), sender(server)));
        ServerLifecycleEvents.SERVER_STOPPED.register(server -> RELAY.clear());
    }

    static byte[] copyPayload(FriendlyByteBuf buffer) {
        int length = buffer.readableBytes();
        if (length < 1 || length > MAX_PAYLOAD) return null;
        byte[] payload = new byte[length];
        buffer.readBytes(payload);
        return payload;
    }

    private static SkinSyncServer.Sender sender(MinecraftServer server) {
        return (target, payload) -> {
            ServerPlayer player = server.getPlayerList().getPlayer(target);
            if (player != null && payload.length <= MAX_PAYLOAD && ServerPlayNetworking.canSend(player, CHANNEL)) {
                ServerPlayNetworking.send(player, CHANNEL, new FriendlyByteBuf(Unpooled.wrappedBuffer(payload)));
            }
        };
    }
}
