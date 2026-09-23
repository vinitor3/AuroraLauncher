package com.aurora.skins.sync;

import java.io.IOException;
import java.util.*;

/** Ephemeral relay. Connection identity is supplied exclusively by the loader, never by the packet. */
public final class SkinSyncServer {
    public interface Sender { void send(UUID target, byte[] payload); }
    private final Map<UUID, Peer> peers = new LinkedHashMap<UUID, Peer>();
    private static final class Peer {
        long lastHello, lastPublish = Long.MIN_VALUE;
        byte[] appearance;
        String fingerprint;
    }

    public synchronized void receive(UUID player, byte[] payload, long nowMillis, Sender sender) {
        if (player == null || sender == null || payload == null || payload.length > SkinSyncProtocol.MAX_PACKET_BYTES) return;
        try {
            SkinSyncProtocol.Message message = SkinSyncProtocol.decode(payload);
            Peer peer = peers.get(player);
            if (message.kind == SkinSyncProtocol.HELLO) {
                if (peer != null && nowMillis - peer.lastHello < 5000) return;
                if (peer == null) {
                    if (peers.size() >= SkinSyncProtocol.MAX_PLAYERS) return;
                    peer = new Peer(); peers.put(player, peer);
                }
                peer.lastHello = nowMillis;
                send(sender, player, SkinSyncProtocol.hello());
                for (Map.Entry<UUID, Peer> entry : peers.entrySet()) {
                    if (!entry.getKey().equals(player) && entry.getValue().appearance != null) send(sender, player, entry.getValue().appearance);
                }
                return;
            }
            if (peer == null || (message.kind != SkinSyncProtocol.PUBLISH && message.kind != SkinSyncProtocol.RESET)) return;
            // Charge invalid PNG attempts too. One tiny image per player every two seconds at most.
            if (peer.lastPublish != Long.MIN_VALUE && nowMillis - peer.lastPublish < 2000) return;
            peer.lastPublish = nowMillis;
            if (message.kind == SkinSyncProtocol.RESET) {
                if (peer.appearance != null) {
                    peer.appearance = null; peer.fingerprint = null;
                    broadcast(player, SkinSyncProtocol.remove(player), sender);
                }
                return;
            }
            byte[] png = SkinSyncProtocol.canonicalPng(message.png());
            String fingerprint = SkinSyncProtocol.hash(png) + message.model.id();
            if (fingerprint.equals(peer.fingerprint)) return;
            peer.appearance = SkinSyncProtocol.update(player, message.model, png);
            peer.fingerprint = fingerprint;
            broadcast(player, peer.appearance, sender);
        } catch (IOException | RuntimeException invalid) {
            // Malformed or unsupported optional skin data must not disconnect a player.
        }
    }

    public synchronized void disconnect(UUID player, Sender sender) {
        Peer removed = peers.remove(player);
        if (removed != null && removed.appearance != null) broadcast(player, SkinSyncProtocol.remove(player), sender);
    }
    public synchronized void clear() { peers.clear(); }
    public synchronized int playerCount() { return peers.size(); }
    private void broadcast(UUID except, byte[] packet, Sender sender) {
        for (UUID target : peers.keySet()) if (!target.equals(except)) send(sender, target, packet);
    }
    private static void send(Sender sender, UUID target, byte[] packet) {
        try { sender.send(target, packet.clone()); } catch (RuntimeException disconnected) { }
    }
}
