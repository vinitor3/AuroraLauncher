package com.aurora.skins.minecraft.forge;

import com.aurora.skins.minecraft.AuroraSkinsBootstrap;
import com.aurora.skins.minecraft.network.SkinSyncClientBridge;
import net.minecraft.client.Minecraft;
import net.minecraft.network.Connection;
import net.minecraftforge.client.event.ClientPlayerNetworkEvent;
import net.minecraftforge.common.MinecraftForge;
import net.minecraftforge.event.TickEvent;

/** Referenced only through DistExecutor on the physical client. */
final class SkinSyncForgeClient {
    private static Connection active;

    private SkinSyncForgeClient() { }

    static void initialize() {
        AuroraSkinsBootstrap.initialize();
        SkinSyncForgeNetwork.clientReceiver = (connection, bytes) -> {
            if (connection == active) SkinSyncClientBridge.receive(bytes);
        };
        MinecraftForge.EVENT_BUS.addListener(SkinSyncForgeClient::tick);
        MinecraftForge.EVENT_BUS.addListener(SkinSyncForgeClient::logout);
    }

    private static void tick(TickEvent.ClientTickEvent.Post event) {
        Minecraft client = Minecraft.getInstance();
        Connection connection = client.getConnection() == null ? null : client.getConnection().getConnection();
        if (active != null && (active != connection || !SkinSyncForgeNetwork.CHANNEL.isRemotePresent(active))) {
            disconnect();
        }
        if (client.player == null || connection == null || active == connection
                || !SkinSyncForgeNetwork.CHANNEL.isRemotePresent(connection)) return;
        active = connection;
        SkinSyncClientBridge.connected(bytes -> client.execute(() -> {
            if (active == connection && connection.isConnected()
                    && SkinSyncForgeNetwork.CHANNEL.isRemotePresent(connection)) {
                SkinSyncForgeNetwork.send(connection, bytes);
            }
        }));
    }

    private static void logout(ClientPlayerNetworkEvent.LoggingOut event) {
        if (active == null || event.getConnection() == null || event.getConnection() == active) disconnect();
    }

    private static void disconnect() {
        active = null;
        SkinSyncClientBridge.disconnected();
    }
}
