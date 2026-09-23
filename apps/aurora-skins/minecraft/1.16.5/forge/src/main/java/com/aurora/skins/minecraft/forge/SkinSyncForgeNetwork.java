package com.aurora.skins.minecraft.forge;

import com.aurora.skins.sync.SkinSyncServer;
import io.netty.buffer.Unpooled;
import java.util.function.BiConsumer;
import java.util.function.Consumer;
import net.minecraft.network.Connection;
import net.minecraft.network.FriendlyByteBuf;
import net.minecraft.network.protocol.game.ClientboundCustomPayloadPacket;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerPlayer;
import net.minecraftforge.common.MinecraftForge;
import net.minecraftforge.event.entity.player.PlayerEvent;
import net.minecraftforge.fml.event.server.FMLServerStoppedEvent;
import net.minecraftforge.fml.network.NetworkDirection;
import net.minecraftforge.fml.network.NetworkEvent;
import net.minecraftforge.fml.network.NetworkRegistry;
import net.minecraftforge.fml.network.event.EventNetworkChannel;

/** Raw custom payloads keep the Fabric and Forge wire format identical. */
final class SkinSyncForgeNetwork {
    static final ResourceLocation ID = new ResourceLocation("aurora_skins", "sync");
    static final int MAX_PAYLOAD = 24640;
    static final EventNetworkChannel CHANNEL = NetworkRegistry.newEventChannel(ID, () -> "1", version -> true, version -> true);
    private static final SkinSyncServer RELAY = new SkinSyncServer();
    static BiConsumer<Connection, byte[]> clientReceiver = (connection, payload) -> { };
    static Consumer<Connection> clientRegistration = connection -> { };

    private SkinSyncForgeNetwork() { }

    static void initialize() {
        CHANNEL.addListener(SkinSyncForgeNetwork::fromClient);
        CHANNEL.addListener(SkinSyncForgeNetwork::fromServer);
        CHANNEL.addListener(SkinSyncForgeNetwork::registrationChanged);
        MinecraftForge.EVENT_BUS.addListener(SkinSyncForgeNetwork::logout);
        MinecraftForge.EVENT_BUS.addListener(SkinSyncForgeNetwork::stopped);
    }

    private static void fromClient(NetworkEvent.ClientCustomPayloadEvent event) {
        NetworkEvent.Context context = event.getSource().get();
        if (context.getDirection() != NetworkDirection.PLAY_TO_SERVER) return;
        context.setPacketHandled(true);
        byte[] payload = copyPayload(event.getPayload());
        ServerPlayer player = context.getSender();
        if (payload == null || player == null) return;
        context.enqueueWork(() -> {
            MinecraftServer server = player.getServer();
            if (server != null && server.getPlayerList().getPlayer(player.getUUID()) == player) {
                RELAY.receive(player.getUUID(), payload, System.currentTimeMillis(), sender(server));
            }
        });
    }

    private static void fromServer(NetworkEvent.ServerCustomPayloadEvent event) {
        NetworkEvent.Context context = event.getSource().get();
        if (context.getDirection() != NetworkDirection.PLAY_TO_CLIENT) return;
        context.setPacketHandled(true);
        byte[] payload = copyPayload(event.getPayload());
        if (payload != null) context.enqueueWork(() -> clientReceiver.accept(context.getNetworkManager(), payload));
    }

    private static void registrationChanged(NetworkEvent.ChannelRegistrationChangeEvent event) {
        NetworkEvent.Context context = event.getSource().get();
        if (context.getDirection().getReceptionSide().isClient()) {
            context.enqueueWork(() -> clientRegistration.accept(context.getNetworkManager()));
        }
    }

    private static byte[] copyPayload(FriendlyByteBuf buffer) {
        if (buffer == null) return null;
        int length = buffer.readableBytes();
        if (length < 1 || length > MAX_PAYLOAD) return null;
        byte[] payload = new byte[length];
        buffer.readBytes(payload);
        return payload;
    }

    private static SkinSyncServer.Sender sender(MinecraftServer server) {
        return (target, payload) -> {
            ServerPlayer player = server.getPlayerList().getPlayer(target);
            if (player != null && payload.length <= MAX_PAYLOAD) {
                player.connection.send(new ClientboundCustomPayloadPacket(ID, new FriendlyByteBuf(Unpooled.wrappedBuffer(payload))));
            }
        };
    }

    private static void logout(PlayerEvent.PlayerLoggedOutEvent event) {
        if (!(event.getPlayer() instanceof ServerPlayer)) return;
        MinecraftServer server = event.getPlayer().getServer();
        if (server != null) RELAY.disconnect(event.getPlayer().getUUID(), sender(server));
    }

    private static void stopped(FMLServerStoppedEvent event) { RELAY.clear(); }
}
