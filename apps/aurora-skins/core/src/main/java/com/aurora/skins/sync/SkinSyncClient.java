package com.aurora.skins.sync;

import com.aurora.skins.AuroraSkinsRuntime;
import com.aurora.skins.api.SkinEntry;
import com.aurora.skins.api.SkinModel;
import java.io.IOException;
import java.util.*;
import java.util.concurrent.*;
import java.util.function.Consumer;

/** Bounded asynchronous client. Skin traffic never blocks the render/network threads. */
public final class SkinSyncClient implements AutoCloseable, AuroraSkinsRuntime.Listener {
    public interface Listener {
        void apply(UUID player, byte[] png, SkinModel model);
        void remove(UUID player);
        void clear();
    }
    private final AuroraSkinsRuntime runtime;
    private final Listener listener;
    private final Executor clientThread;
    private final ThreadPoolExecutor io;
    private final ScheduledExecutorService timer;
    private final java.util.concurrent.atomic.AtomicInteger pendingClientActions = new java.util.concurrent.atomic.AtomicInteger();
    private final Set<UUID> players = new HashSet<UUID>();
    private final Map<UUID, String> fingerprints = new HashMap<UUID, String>();
    private volatile Consumer<byte[]> sender;
    private volatile long generation;
    private volatile boolean ready, closed;
    private volatile SkinEntry selected;
    private volatile long selection;
    private long sentSelection = -1, lastPublish, lastHello;
    private int helloAttempts;
    private boolean pulseQueued;

    public SkinSyncClient(AuroraSkinsRuntime runtime, Listener listener, Executor clientThread) {
        this.runtime = Objects.requireNonNull(runtime);
        this.listener = Objects.requireNonNull(listener);
        this.clientThread = Objects.requireNonNull(clientThread);
        io = new ThreadPoolExecutor(1, 1, 0, TimeUnit.MILLISECONDS, new ArrayBlockingQueue<Runnable>(512),
            runnable -> daemon(runnable, "Aurora-Skins-Sync"), new ThreadPoolExecutor.AbortPolicy());
        timer = Executors.newSingleThreadScheduledExecutor(runnable -> daemon(runnable, "Aurora-Skins-Sync-Timer"));
        selected = runtime.active(); runtime.addListener(this);
        timer.scheduleWithFixedDelay(this::queuePulse, 1, 1, TimeUnit.SECONDS);
    }
    private static Thread daemon(Runnable task, String name) { Thread thread = new Thread(task, name); thread.setDaemon(true); return thread; }

    public synchronized void connect(Consumer<byte[]> transport) {
        if (closed) return;
        disconnect();
        sender = Objects.requireNonNull(transport);
        final long session = generation;
        submit(() -> {
            if (session != generation) return;
            players.clear(); fingerprints.clear(); sentSelection = -1; lastPublish = 0; lastHello = 0; helloAttempts = 0;
            pulse(session);
        });
    }
    public synchronized void disconnect() {
        long session = ++generation; ready = false; sender = null;
        pulseQueued = false;
        io.getQueue().clear();
        clientThread.execute(() -> { if (session == generation) listener.clear(); });
    }

    public void receive(byte[] bytes) {
        if (closed || sender == null || bytes == null || bytes.length > SkinSyncProtocol.MAX_PACKET_BYTES) return;
        final long session = generation;
        final byte[] packet = bytes.clone();
        submit(() -> {
            if (session != generation || sender == null) return;
            try {
                SkinSyncProtocol.Message message = SkinSyncProtocol.decode(packet);
                if (message.kind == SkinSyncProtocol.HELLO) {
                    synchronized (this) {
                        if (session != generation || sender == null || ready) return;
                        ready = true;
                    }
                    pulse(session);
                } else if (ready && message.kind == SkinSyncProtocol.REMOVE) {
                    players.remove(message.player); fingerprints.remove(message.player);
                    dispatch(session, () -> listener.remove(message.player));
                } else if (ready && message.kind == SkinSyncProtocol.UPDATE) {
                    if (!players.contains(message.player) && players.size() >= SkinSyncProtocol.MAX_PLAYERS) return;
                    // A server is untrusted too; check before image decode and strip ancillary metadata.
                    byte[] png = SkinSyncProtocol.canonicalPng(message.png());
                    String fingerprint = SkinSyncProtocol.hash(png) + message.model.id();
                    if (fingerprint.equals(fingerprints.get(message.player))) return;
                    players.add(message.player); fingerprints.put(message.player, fingerprint);
                    dispatch(session, () -> listener.apply(message.player, png, message.model));
                }
            } catch (IOException | RuntimeException invalid) { }
        });
    }

    @Override public synchronized void activeSkinChanged(SkinEntry entry) { selected = entry; selection++; }
    private synchronized void queuePulse() {
        if (closed || sender == null || pulseQueued) return;
        pulseQueued = true;
        final long session = generation;
        if (!submit(() -> { try { pulse(session); } finally { synchronized (this) { pulseQueued = false; } } })) pulseQueued = false;
    }
    private void pulse(long session) {
        if (session != generation || sender == null || closed) return;
        long now = System.nanoTime() / 1000000L;
        if (!ready) {
            if (helloAttempts < 3 && (helloAttempts == 0 || now - lastHello >= 5000)) {
                helloAttempts++; lastHello = now; send(session, SkinSyncProtocol.hello());
            }
            return;
        }
        SkinEntry entry; long currentSelection;
        synchronized (this) { entry = selected; currentSelection = selection; }
        if (sentSelection == currentSelection || (lastPublish != 0 && now - lastPublish < 2200)) return;
        try {
            byte[] packet = entry == null ? SkinSyncProtocol.reset()
                : SkinSyncProtocol.publish(entry.model(), SkinSyncProtocol.readLocal(entry.file()));
            if (session != generation || currentSelection != selection) return;
            send(session, packet); sentSelection = currentSelection; lastPublish = now;
        } catch (IOException | RuntimeException unavailable) {
            // Keep rendering locally; retry at a bounded interval while the file is unavailable.
            lastPublish = now;
        }
    }
    private void send(long session, byte[] packet) {
        dispatch(session, () -> { Consumer<byte[]> current = sender; if (current != null) current.accept(packet); });
    }
    private void dispatch(long session, Runnable action) {
        // Also bound the render-thread backlog when an untrusted server floods updates while rendering stalls.
        if (pendingClientActions.incrementAndGet() > 512) {
            pendingClientActions.decrementAndGet();
            synchronized (this) { if (session == generation) disconnect(); }
            return;
        }
        try {
            clientThread.execute(() -> {
                try { if (!closed && session == generation) action.run(); }
                finally { pendingClientActions.decrementAndGet(); }
            });
        } catch (RuntimeException unavailable) {
            pendingClientActions.decrementAndGet();
        }
    }
    private boolean submit(Runnable action) {
        if (closed) return false;
        try { io.execute(action); return true; } catch (RejectedExecutionException full) { return false; }
    }
    @Override public synchronized void close() {
        if (closed) return;
        disconnect(); closed = true;
        runtime.removeListener(this); timer.shutdownNow(); io.shutdownNow();
    }
}
