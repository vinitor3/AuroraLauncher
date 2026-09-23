package com.aurora.skins.minecraft.forge;

import com.aurora.skins.sync.SkinSyncServer;
import io.netty.buffer.Unpooled;
import java.util.function.BiConsumer;
import net.minecraft.network.Connection;
import net.minecraft.network.ConnectionProtocol;
import net.minecraft.network.FriendlyByteBuf;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerPlayer;
import net.minecraftforge.common.MinecraftForge;
import net.minecraftforge.event.entity.player.PlayerEvent;
import net.minecraftforge.event.network.CustomPayloadEvent;
import net.minecraftforge.event.server.ServerStoppedEvent;
import net.minecraftforge.network.ChannelBuilder;
import net.minecraftforge.network.EventNetworkChannel;

public final class SkinSyncForgeNetwork {
    private static final int MAX_BYTES = 24640;
    private static final SkinSyncServer RELAY = new SkinSyncServer();
    static final EventNetworkChannel CHANNEL = ChannelBuilder
            .named(ResourceLocation.fromNamespaceAndPath("aurora_skins", "sync"))
            .networkProtocolVersion(1).optional().eventNetworkChannel();
    // The dedicated server never resolves Minecraft's client classes.
    static BiConsumer<Connection, byte[]> clientReceiver = (connection, bytes) -> { };

    private SkinSyncForgeNetwork() { }

    public static void initialize() {
        CHANNEL.addListener(SkinSyncForgeNetwork::receive);
        MinecraftForge.EVENT_BUS.addListener(SkinSyncForgeNetwork::logout);
        MinecraftForge.EVENT_BUS.addListener(SkinSyncForgeNetwork::stopped);
    }

    private static void receive(CustomPayloadEvent event) {
        CustomPayloadEvent.Context context = event.getSource();
        context.setPacketHandled(true);
        FriendlyByteBuf buffer = event.getPayload();
        if (context.getConnection().getProtocol() != ConnectionProtocol.PLAY || buffer == null
                || buffer.readableBytes() < 1 || buffer.readableBytes() > MAX_BYTES) return;
        byte[] bytes = new byte[buffer.readableBytes()];
        buffer.readBytes(bytes);
        context.enqueueWork(() -> {
            if (!context.getConnection().isConnected()) return;
            if (context.isServerSide()) {
                ServerPlayer player = context.getSender();
                if (player == null) return;
                MinecraftServer server = player.getServer();
                if (server != null && server.getPlayerList().getPlayer(player.getUUID()) == player) {
                    RELAY.receive(player.getUUID(), bytes, System.currentTimeMillis(), sender(server));
                }
            } else {
                clientReceiver.accept(context.getConnection(), bytes);
            }
        });
    }

    static void send(Connection connection, byte[] bytes) {
        if (bytes.length < 1 || bytes.length > MAX_BYTES || !connection.isConnected()) return;
        // EventNetworkChannel writes this buffer verbatim, matching Fabric's payload codec.
        CHANNEL.send(new FriendlyByteBuf(Unpooled.wrappedBuffer(bytes)), connection);
    }

    private static SkinSyncServer.Sender sender(MinecraftServer server) {
        return (target, bytes) -> {
            ServerPlayer player = server.getPlayerList().getPlayer(target);
            if (player != null && CHANNEL.isRemotePresent(player.connection.getConnection())) {
                send(player.connection.getConnection(), bytes);
            }
        };
    }

    private static void logout(PlayerEvent.PlayerLoggedOutEvent event) {
        if (event.getEntity() instanceof ServerPlayer player && player.getServer() != null) {
            RELAY.disconnect(player.getUUID(), sender(player.getServer()));
        }
    }

    private static void stopped(ServerStoppedEvent event) { RELAY.clear(); }
}
