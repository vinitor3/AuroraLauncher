package com.aurora.skins.sync;

import java.io.IOException;
import java.util.*;

/** Ephemeral relay. Connection identity is supplied exclusively by the loader, never by the packet. */
public final class SkinSyncServer {
    public interface Sender { void send(UUID target, byte[] payload); }
    private final Map<UUID, Peer> peers = new LinkedHashMap<UUID, Peer>();
    private static final class Peer {
        long lastHello, lastPublish = Long.MIN_VALUE;
        byte[] appearance, cape;
        boolean capes;
        long lastCape = Long.MIN_VALUE;
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
            if (peer != null && message.kind == SkinSyncProtocol.CAPE_HELLO) {
                if (peer.capes) return;
                peer.capes = true;
                send(sender, player, SkinSyncProtocol.capeHello());
                for (Map.Entry<UUID, Peer> other : peers.entrySet()) {
                    if (!other.getKey().equals(player) && other.getValue().cape != null) send(sender, player, other.getValue().cape);
                }
                return;
            }
            if (peer != null && peer.capes && message.kind == SkinSyncProtocol.CAPE_PUBLISH) {
                if (peer.lastCape != Long.MIN_VALUE && nowMillis - peer.lastCape < 2000) return;
                peer.lastCape = nowMillis;
                byte[] update = SkinSyncProtocol.capeUpdate(player, SkinSyncProtocol.canonicalCape(message.png()));
                if (!Arrays.equals(peer.cape, update)) {
                    peer.cape = update;
                    broadcastCape(player, update, sender);
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
        if (removed != null && removed.cape != null) {
            try { broadcastCape(player, SkinSyncProtocol.capeRemove(player), sender); }
            catch (IOException impossible) { throw new IllegalStateException(impossible); }
        }
        if (removed != null && removed.appearance != null) broadcast(player, SkinSyncProtocol.remove(player), sender);
    }
    public synchronized void clear() { peers.clear(); }
    public synchronized int playerCount() { return peers.size(); }
    private void broadcastCape(UUID except, byte[] packet, Sender sender) {
        for (Map.Entry<UUID, Peer> peer : peers.entrySet()) {
            if (!peer.getKey().equals(except) && peer.getValue().capes) send(sender, peer.getKey(), packet);
        }
    }
    private void broadcast(UUID except, byte[] packet, Sender sender) {
        for (UUID target : peers.keySet()) if (!target.equals(except)) send(sender, target, packet);
    }
    private static void send(Sender sender, UUID target, byte[] packet) {
        try { sender.send(target, packet.clone()); } catch (RuntimeException disconnected) { }
    }
}
