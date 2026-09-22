package com.aurora.skins.minecraft.forge;

import com.aurora.skins.minecraft.AuroraSkinsBootstrap;
import com.aurora.skins.minecraft.network.SkinSyncClientBridge;
import io.netty.buffer.Unpooled;
import net.minecraft.client.Minecraft;
import net.minecraft.network.Connection;
import net.minecraft.network.FriendlyByteBuf;
import net.minecraft.network.protocol.game.ServerboundCustomPayloadPacket;
import net.minecraftforge.client.event.ClientPlayerNetworkEvent;
import net.minecraftforge.common.MinecraftForge;

/** Loaded exclusively by the physical client entrypoint. */
final class SkinSyncForgeClient {
    private static Connection connected;

    private SkinSyncForgeClient() { }

    static void initialize() {
        AuroraSkinsBootstrap.initialize();
        SkinSyncForgeNetwork.clientReceiver = (connection, payload) -> {
            if (connected == connection && current(connection)) SkinSyncClientBridge.receive(payload);
        };
        SkinSyncForgeNetwork.clientRegistration = SkinSyncForgeClient::connect;
        MinecraftForge.EVENT_BUS.addListener(SkinSyncForgeClient::login);
        MinecraftForge.EVENT_BUS.addListener(SkinSyncForgeClient::logout);
    }

    private static boolean current(Connection connection) {
        Minecraft client = Minecraft.getInstance();
        return client.getConnection() != null && client.getConnection().getConnection() == connection;
    }

    private static void connect(Connection connection) {
        Minecraft client = Minecraft.getInstance();
        if (!current(connection) || client.player == null) return;
        if (!SkinSyncForgeNetwork.CHANNEL.isRemotePresent(connection)) {
            if (connected == connection) disconnect();
            return;
        }
        if (connected == connection) return;
        disconnect();
        connected = connection;
        SkinSyncClientBridge.connected(payload -> client.execute(() -> {
            if (connected == connection && current(connection) && payload.length <= SkinSyncForgeNetwork.MAX_PAYLOAD) {
                connection.send(new ServerboundCustomPayloadPacket(SkinSyncForgeNetwork.ID,
                    new FriendlyByteBuf(Unpooled.wrappedBuffer(payload))));
            }
        }));
    }

    private static void disconnect() {
        connected = null;
        SkinSyncClientBridge.disconnected();
    }

    private static void login(ClientPlayerNetworkEvent.LoggingIn event) { connect(event.getConnection()); }
    private static void logout(ClientPlayerNetworkEvent.LoggingOut event) { disconnect(); }
}
