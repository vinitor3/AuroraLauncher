package com.aurora.skins.sync;

import com.aurora.skins.AuroraSkinsRuntime;
import com.aurora.skins.api.SkinEntry;
import com.aurora.skins.api.SkinModel;
import com.aurora.skins.storage.AuroraSkinStore;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.lang.reflect.Constructor;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.CopyOnWriteArrayList;
import java.util.concurrent.Executor;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.function.BooleanSupplier;

import static org.junit.jupiter.api.Assertions.*;

class SkinSyncClientTest {
    @TempDir Path directory;

    private AuroraSkinsRuntime runtime(String name) throws Exception {
        // Isolate each simulated Minecraft process without mutating the runtime singleton or JVM properties.
        Constructor<AuroraSkinsRuntime> constructor = AuroraSkinsRuntime.class.getDeclaredConstructor(AuroraSkinStore.class);
        constructor.setAccessible(true);
        return constructor.newInstance(new AuroraSkinStore(directory.resolve(name)));
    }

    private SkinEntry skin(AuroraSkinsRuntime runtime, String name, SkinModel model, int color) throws Exception {
        Path file = directory.resolve(name + ".png");
        Files.write(file, SkinSyncServerTest.png(64, color));
        return runtime.store().importSkin(file, name, model);
    }

    @Test void twoClientsSeeEachOthersSkinsAndRapidChangesConvergeToFinalSelection() throws Exception {
        UUID alice = UUID.randomUUID(), bob = UUID.randomUUID();
        AuroraSkinsRuntime aliceRuntime = runtime("alice"), bobRuntime = runtime("bob");
        SkinEntry first = skin(aliceRuntime, "first", SkinModel.WIDE, 0xff126456);
        SkinEntry last = skin(aliceRuntime, "last", SkinModel.SLIM, 0xff654321);
        aliceRuntime.apply(first);
        bobRuntime.apply(skin(bobRuntime, "bob-skin", SkinModel.SLIM, 0xffb0b0b0));
        RecordingListener aliceView = new RecordingListener(), bobView = new RecordingListener();
        SkinSyncServer server = new SkinSyncServer();
        Map<UUID, SkinSyncClient> clients = new ConcurrentHashMap<UUID, SkinSyncClient>();
        SkinSyncServer.Sender relay = (id, packet) -> {
            SkinSyncClient client = clients.get(id);
            if (client != null) client.receive(packet);
        };
        try (SkinSyncClient aliceClient = new SkinSyncClient(aliceRuntime, aliceView, Runnable::run);
             SkinSyncClient bobClient = new SkinSyncClient(bobRuntime, bobView, Runnable::run)) {
            clients.put(alice, aliceClient); clients.put(bob, bobClient);
            aliceClient.connect(packet -> server.receive(alice, packet, System.nanoTime() / 1000000L, relay));
            await(() -> server.playerCount() == 1);
            bobClient.connect(packet -> server.receive(bob, packet, System.nanoTime() / 1000000L, relay));
            await(() -> aliceView.skins.containsKey(bob) && bobView.skins.containsKey(alice));
            assertFalse(aliceView.skins.containsKey(alice));
            assertFalse(bobView.skins.containsKey(bob));
            assertEquals(SkinModel.WIDE, bobView.skins.get(alice).model);
            assertEquals(SkinModel.SLIM, aliceView.skins.get(bob).model);

            aliceRuntime.apply(last);
            aliceRuntime.apply(first);
            aliceRuntime.apply(last);
            final String expected = SkinSyncProtocol.hash(SkinSyncProtocol.readLocal(last.file()));
            await(() -> {
                Appearance appearance = bobView.skins.get(alice);
                return appearance != null && appearance.model == SkinModel.SLIM && appearance.hash.equals(expected);
            });
            aliceClient.disconnect();
            server.disconnect(alice, relay);
            await(() -> !bobView.skins.containsKey(alice));
            assertTrue(aliceView.skins.isEmpty());
        }
    }

    @Test void queuedOldSessionUpdatesAndPublicationsCannotReachANewConnection() throws Exception {
        AuroraSkinsRuntime runtime = runtime("queued");
        runtime.apply(skin(runtime, "queued-skin", SkinModel.WIDE, 0xffabcdef));
        RecordingListener view = new RecordingListener();
        QueuedExecutor main = new QueuedExecutor();
        List<byte[]> firstWire = new CopyOnWriteArrayList<byte[]>(), secondWire = new CopyOnWriteArrayList<byte[]>();
        UUID other = UUID.randomUUID();
        try (SkinSyncClient client = new SkinSyncClient(runtime, view, main)) {
            client.connect(firstWire::add);
            main.runUntil(() -> !firstWire.isEmpty());
            assertEquals(SkinSyncProtocol.HELLO, SkinSyncProtocol.decode(firstWire.get(0)).kind);
            client.receive(SkinSyncProtocol.hello());
            // Hold a fully prepared outgoing publication on the main thread.
            Runnable stalePublish = main.take();
            client.receive(SkinSyncProtocol.update(other, SkinModel.SLIM, SkinSyncServerTest.png(64, 0xff111111)));
            Runnable staleUpdate = main.take();
            client.disconnect();
            client.connect(secondWire::add);
            stalePublish.run();
            staleUpdate.run();
            main.runUntil(() -> !secondWire.isEmpty());
            assertEquals(1, firstWire.size(), "Prepared skin packets must be fenced at the actual send");
            assertEquals(1, secondWire.size());
            assertEquals(SkinSyncProtocol.HELLO, SkinSyncProtocol.decode(secondWire.get(0)).kind);
            assertTrue(view.skins.isEmpty(), "The old server's queued texture must not repopulate the new session");
            assertEquals(0, view.applications.get());

            client.receive(SkinSyncProtocol.hello());
            main.runUntil(() -> secondWire.size() == 2);
            assertEquals(SkinSyncProtocol.PUBLISH, SkinSyncProtocol.decode(secondWire.get(1)).kind);
            client.receive(SkinSyncProtocol.update(other, SkinModel.WIDE, SkinSyncServerTest.png(64, 0xff222222)));
            main.runUntil(() -> view.skins.containsKey(other));
            assertEquals(1, view.applications.get());
        }
        main.drain();
        assertTrue(view.skins.isEmpty(), "Closing also clears previously published appearance state");
    }

    @Test void clientRejectsUntrustedPngAndDeduplicatesOnlyEqualPixelsAndModel() throws Exception {
        RecordingListener view = new RecordingListener();
        List<byte[]> wire = new CopyOnWriteArrayList<byte[]>();
        UUID remote = UUID.randomUUID();
        try (SkinSyncClient client = new SkinSyncClient(runtime("validation"), view, Runnable::run)) {
            client.connect(wire::add);
            await(() -> !wire.isEmpty());
            client.receive(SkinSyncProtocol.hello());
            await(() -> wire.size() >= 2);
            byte[] png = SkinSyncServerTest.png(64, 0xff123456);
            byte[] invalid = png.clone(); invalid[invalid.length - 1] ^= 1;
            client.receive(SkinSyncProtocol.update(remote, SkinModel.WIDE, invalid));
            client.receive(SkinSyncProtocol.update(remote, SkinModel.WIDE, png));
            client.receive(SkinSyncProtocol.update(remote, SkinModel.WIDE, png));
            client.receive(SkinSyncProtocol.update(remote, SkinModel.SLIM, png));
            // REMOVE is a FIFO barrier after all four image attempts on the single IO executor.
            client.receive(SkinSyncProtocol.remove(remote));
            await(() -> view.removals.get() == 1);
            assertEquals(2, view.applications.get());
            assertEquals(SkinModel.SLIM, view.history.get(1).model);
            assertTrue(view.skins.isEmpty());
        }
    }

    @Test void unsupportedServerKeepsLocalSelectionAndPublishesNothingBeforeHandshake() throws Exception {
        AuroraSkinsRuntime runtime = runtime("vanilla");
        SkinEntry local = skin(runtime, "local-only", SkinModel.SLIM, 0xff765432);
        runtime.apply(local);
        RecordingListener view = new RecordingListener();
        List<byte[]> wire = new CopyOnWriteArrayList<byte[]>();
        try (SkinSyncClient client = new SkinSyncClient(runtime, view, Runnable::run)) {
            client.connect(wire::add);
            await(() -> !wire.isEmpty());
            client.receive(SkinSyncProtocol.update(UUID.randomUUID(), SkinModel.WIDE, SkinSyncServerTest.png(64, 0)));
            // Switching without a handshake must keep using the local runtime alone.
            client.activeSkinChanged(local);
            Thread.sleep(1100);
            assertSame(local, runtime.active());
            assertTrue(view.skins.isEmpty());
            for (byte[] packet : wire) assertEquals(SkinSyncProtocol.HELLO, SkinSyncProtocol.decode(packet).kind);
        }
    }

    private static void await(BooleanSupplier condition) throws Exception {
        long deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(8);
        while (!condition.getAsBoolean() && System.nanoTime() < deadline) Thread.sleep(10);
        assertTrue(condition.getAsBoolean(), "Asynchronous skin operation did not finish within its bounded deadline");
    }

    private static final class Appearance {
        final String hash;
        final SkinModel model;
        Appearance(byte[] png, SkinModel model) { this.hash = SkinSyncProtocol.hash(png); this.model = model; }
    }

    private static final class RecordingListener implements SkinSyncClient.Listener {
        final Map<UUID, Appearance> skins = new ConcurrentHashMap<UUID, Appearance>();
        final List<Appearance> history = new CopyOnWriteArrayList<Appearance>();
        final AtomicInteger applications = new AtomicInteger(), removals = new AtomicInteger();
        @Override public void apply(UUID player, byte[] png, SkinModel model) {
            Appearance appearance = new Appearance(png, model);
            skins.put(player, appearance); history.add(appearance); applications.incrementAndGet();
        }
        @Override public void remove(UUID player) { skins.remove(player); removals.incrementAndGet(); }
        @Override public void clear() { skins.clear(); }
    }

    private static final class QueuedExecutor implements Executor {
        final BlockingQueue<Runnable> tasks = new LinkedBlockingQueue<Runnable>();
        @Override public void execute(Runnable command) { tasks.add(command); }
        Runnable take() throws Exception {
            Runnable task = tasks.poll(8, TimeUnit.SECONDS);
            assertNotNull(task, "Expected a queued client-thread action");
            return task;
        }
        void runUntil(BooleanSupplier condition) throws Exception {
            long deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(8);
            while (!condition.getAsBoolean() && System.nanoTime() < deadline) {
                Runnable task = tasks.poll(100, TimeUnit.MILLISECONDS);
                if (task != null) task.run();
            }
            assertTrue(condition.getAsBoolean());
        }
        void drain() { Runnable task; while ((task = tasks.poll()) != null) task.run(); }
    }
}
