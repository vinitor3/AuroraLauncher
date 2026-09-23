package com.aurora.skins.legacy;

import com.aurora.skins.sync.SkinSyncProtocol;
import com.aurora.skins.sync.SkinSyncServer;
import io.netty.buffer.ByteBuf;
import io.netty.buffer.Unpooled;
import net.minecraftforge.common.MinecraftForge;
import net.minecraftforge.fml.common.FMLCommonHandler;
import net.minecraftforge.fml.common.eventhandler.SubscribeEvent;
import net.minecraftforge.fml.common.eventhandler.Event;
import net.minecraftforge.fml.common.gameevent.PlayerEvent;
import net.minecraftforge.fml.common.network.FMLNetworkEvent;
import net.minecraftforge.fml.relauncher.Side;
import java.util.Collection;
import java.util.LinkedHashSet;
import java.util.Set;
import java.util.UUID;

/** Raw Forge channel; reflection keeps the legacy build independent of obfuscated Minecraft symbols. */
public final class LegacySkinNetwork {
    private static final String CHANNEL = "aurora_skins:sync";
    private static final SkinSyncServer RELAY = new SkinSyncServer();
    private static Object channel;
    // Accessed only on the client thread. REGISTER can precede Forge's connected event.
    private static final Set<Object> registeredConnections = new LinkedHashSet<Object>();
    private static Object clientConnection, syncingConnection;
    private LegacySkinNetwork() { }
    static void initialize() {
        if (channel != null) return;
        try {
            Object registry = Class.forName("net.minecraftforge.fml.common.network.NetworkRegistry").getField("INSTANCE").get(null);
            channel = LegacyMinecraft.invoke(registry, new String[] {"newEventDrivenChannel"}, CHANNEL);
            if (channel == null) throw new IllegalStateException("Skin network channel unavailable");
            LegacySkinNetwork events = new LegacySkinNetwork();
            LegacyMinecraft.invoke(channel, new String[] {"register"}, events);
            MinecraftForge.EVENT_BUS.register(events);
        } catch (ReflectiveOperationException error) { throw new IllegalStateException("Skin network unavailable",error); }
    }
    @SubscribeEvent public void serverPacket(FMLNetworkEvent.ServerCustomPacketEvent event) {
        Object packet=LegacyMinecraft.invoke(event,new String[]{"getPacket"});
        byte[] data=bytes(packet); if(data==null) return;
        Object handler=LegacyMinecraft.invoke(event,new String[]{"getHandler"});
        Object player=LegacyMinecraft.field(handler,"player","field_147369_b");
        Object id=LegacyMinecraft.invoke(player,new String[]{"getUniqueID","func_110124_au"});
        if(!(id instanceof UUID)) return;
        Object server=server(); if(server==null) return;
        LegacyMinecraft.invoke(server,new String[]{"addScheduledTask","func_152344_a"},(Runnable)()->{
            // Ignore a packet queued before this connection left or was replaced.
            if(findPlayer(server,(UUID)id)!=player) return;
            RELAY.receive((UUID)id,data,System.nanoTime()/1000000L,(target,payload)->sendTo(server,target,payload));
        });
    }
    @SubscribeEvent public void clientPacket(FMLNetworkEvent.ClientCustomPacketEvent event) {
        byte[] data=bytes(LegacyMinecraft.invoke(event,new String[]{"getPacket"}));
        Object manager=LegacyMinecraft.invoke(event,new String[]{"getManager"});
        if(data!=null) LegacyMinecraft.execute(()->{
            if(manager==syncingConnection && manager==currentClientManager()) LegacySkinClientNetwork.receive(data);
        });
    }
    @SubscribeEvent public void registration(Event event) {
        if (!event.getClass().getName().equals("net.minecraftforge.fml.common.network.FMLNetworkEvent$CustomPacketRegistrationEvent")) return;
        if(LegacyMinecraft.invoke(event,new String[]{"getSide"})!=Side.CLIENT) return;
        Object names=LegacyMinecraft.invoke(event,new String[]{"getRegistrations"});
        if(!(names instanceof Collection<?>) || !((Collection<?>)names).contains(CHANNEL)) return;
        Object manager=LegacyMinecraft.invoke(event,new String[]{"getManager"});
        Object operation=LegacyMinecraft.invoke(event,new String[]{"getOperation"});
        LegacyMinecraft.execute(()->{
            if(manager==null) return;
            if("REGISTER".equals(operation)) {
                if(!open(manager)) return;
                if(registeredConnections.size()>=4) registeredConnections.remove(registeredConnections.iterator().next());
                registeredConnections.add(manager);
                connectClient(manager);
            } else if("UNREGISTER".equals(operation)) {
                registeredConnections.remove(manager);
                if(syncingConnection==manager) { syncingConnection=null; LegacySkinClientNetwork.disconnected(); }
            }
        });
    }
    @SubscribeEvent public void connected(FMLNetworkEvent.ClientConnectedToServerEvent event) {
        Object manager=LegacyMinecraft.invoke(event,new String[]{"getManager"});
        LegacyMinecraft.execute(()->{
            if(manager==null || !open(manager)) return;
            if(clientConnection!=manager) {
                clientConnection=manager; syncingConnection=null; LegacySkinClientNetwork.disconnected();
            }
            connectClient(manager);
        });
    }
    @SubscribeEvent public void disconnected(FMLNetworkEvent.ClientDisconnectionFromServerEvent event) {
        Object manager=LegacyMinecraft.invoke(event,new String[]{"getManager"});
        LegacyMinecraft.execute(()->{
            registeredConnections.remove(manager);
            if(clientConnection==manager) {
                clientConnection=null; syncingConnection=null; LegacySkinClientNetwork.disconnected();
            }
        });
    }
    private static void connectClient(Object manager) {
        if(clientConnection!=manager || syncingConnection==manager || !registeredConnections.contains(manager)) return;
        syncingConnection=manager;
        LegacySkinClientNetwork.connected(data->{
            if(syncingConnection==manager && manager==currentClientManager() && open(manager)) sendToServer(data);
        });
    }
    private static boolean open(Object manager) {
        return Boolean.TRUE.equals(LegacyMinecraft.invoke(manager,new String[]{"isChannelOpen","func_150724_d"}));
    }
    private static Object currentClientManager() {
        Object handler=LegacyMinecraft.invoke(LegacyMinecraft.minecraft(),new String[]{"getConnection","func_147114_u"});
        return LegacyMinecraft.invoke(handler,new String[]{"getNetworkManager","func_147298_b"});
    }
    @SubscribeEvent public void logout(PlayerEvent.PlayerLoggedOutEvent event) {
        Object player=LegacyMinecraft.field(event,"player");
        Object id=LegacyMinecraft.invoke(player,new String[]{"getUniqueID","func_110124_au"});
        Object server=server();
        if(id instanceof UUID && server!=null) RELAY.disconnect((UUID)id,(target,payload)->sendTo(server,target,payload));
    }
    static void clear() { RELAY.clear(); }
    private static Object server() { return LegacyMinecraft.invoke(FMLCommonHandler.instance(),new String[]{"getMinecraftServerInstance"}); }
    private static Object findPlayer(Object server,UUID id) {
        Object players=LegacyMinecraft.invoke(server,new String[]{"getPlayerList","func_184103_al"});
        return LegacyMinecraft.invoke(players,new String[]{"getPlayerByUUID","func_177451_a"},id);
    }
    private static void sendTo(Object server,UUID id,byte[] data) {
        Object player=findPlayer(server,id); if(player==null) return;
        Object packet=packet(data); if(packet!=null) LegacyMinecraft.invoke(channel,new String[]{"sendTo"},packet,player);
    }
    private static void sendToServer(byte[] data) {
        Object packet=packet(data); if(packet!=null) LegacyMinecraft.invoke(channel,new String[]{"sendToServer"},packet);
    }
    private static Object packet(byte[] data) {
        ByteBuf buffer=Unpooled.wrappedBuffer(data);
        try {
            Class<?> type=Class.forName("net.minecraft.network.PacketBuffer");
            Object wrapped=type.getConstructor(ByteBuf.class).newInstance(buffer);
            return Class.forName("net.minecraftforge.fml.common.network.internal.FMLProxyPacket").getConstructor(type,String.class).newInstance(wrapped,CHANNEL);
        } catch (ReflectiveOperationException error) { buffer.release(); return null; }
    }
    private static byte[] bytes(Object packet) {
        Object payload=LegacyMinecraft.invoke(packet,new String[]{"payload"});
        if(!(payload instanceof ByteBuf)) return null;
        ByteBuf buffer=(ByteBuf)payload;
        if(buffer.readableBytes()<2 || buffer.readableBytes()>SkinSyncProtocol.MAX_PACKET_BYTES) return null;
        byte[] bytes=new byte[buffer.readableBytes()]; buffer.getBytes(buffer.readerIndex(),bytes); return bytes;
    }
}
