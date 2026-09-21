package com.aurora.skins.catalog;

import com.aurora.skins.api.SkinEntry;
import com.aurora.skins.api.SkinModel;
import com.aurora.skins.storage.AuroraSkinStore;
import com.sun.net.httpserver.HttpServer;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import javax.imageio.ImageIO;
import java.awt.image.BufferedImage;
import java.io.ByteArrayOutputStream;
import java.net.InetSocketAddress;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.util.Collections;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;
import static org.junit.jupiter.api.Assertions.*;

final class CatalogPreviewCacheTest {
    @TempDir Path temporary;

    @Test void concurrentModelsShareDownloadAndDiskCacheWithoutChangingSavedAppearance() throws Exception {
        BufferedImage image = new BufferedImage(64, 32, BufferedImage.TYPE_INT_ARGB);
        image.setRGB(10, 10, 0xffbb2266);
        ByteArrayOutputStream output = new ByteArrayOutputStream();
        ImageIO.write(image, "PNG", output);
        byte[] png = output.toByteArray();
        StringBuilder hash = new StringBuilder();
        for (byte value : MessageDigest.getInstance("SHA-256").digest(png)) hash.append(String.format("%02x", value & 255));
        Path source = temporary.resolve("fixture.png");
        Files.write(source, png);
        AuroraSkinStore store = new AuroraSkinStore(temporary.resolve("store"));
        SkinEntry active = store.importSkin(source, "Saved classic", SkinModel.WIDE);
        store.setActive(active);
        store.setFavorite(active, true);
        byte[] metadata = Files.readAllBytes(store.root().resolve("metadata").resolve(hash + ".properties"));

        // Separate store forces a real PNG request before checking persisted reuse.
        AuroraSkinStore downloadStore = new AuroraSkinStore(temporary.resolve("download-store"));
        AtomicInteger requests = new AtomicInteger();
        CountDownLatch requested = new CountDownLatch(1);
        CountDownLatch release = new CountDownLatch(1);
        HttpServer server = HttpServer.create(new InetSocketAddress("127.0.0.1", 0), 0);
        server.createContext("/skin.png", exchange -> {
            requests.incrementAndGet(); requested.countDown();
            try {
                release.await(5, TimeUnit.SECONDS);
                exchange.sendResponseHeaders(200, png.length);
                exchange.getResponseBody().write(png);
            } catch (InterruptedException error) { Thread.currentThread().interrupt(); }
            finally { exchange.close(); }
        });
        server.start();
        try {
            String endpoint = "http://127.0.0.1:" + server.getAddress().getPort();
            SkinEntry wide = entry("wide", SkinModel.WIDE, hash.toString(), endpoint);
            SkinEntry slim = entry("slim", SkinModel.SLIM, hash.toString(), endpoint);
            AuroraCatalogProvider provider = new AuroraCatalogProvider(downloadStore, endpoint + "/catalog");
            CompletableFuture<SkinEntry> first = provider.preview(wide);
            assertTrue(requested.await(3, TimeUnit.SECONDS));
            CompletableFuture<SkinEntry> abandoned = provider.preview(wide);
            CompletableFuture<SkinEntry> otherModel = provider.preview(slim);
            abandoned.cancel(false);
            release.countDown();
            SkinEntry loadedWide = first.get(5, TimeUnit.SECONDS);
            SkinEntry loadedSlim = otherModel.get(5, TimeUnit.SECONDS);
            assertEquals(1, requests.get());
            assertEquals(loadedWide.file(), loadedSlim.file());
            assertEquals(SkinModel.WIDE, loadedWide.model());
            assertEquals(SkinModel.SLIM, loadedSlim.model());
            assertEquals("slim", loadedSlim.id());
            assertEquals(1, requests.get());
            assertTrue(downloadStore.list().isEmpty());
            assertNull(downloadStore.active());
            assertTrue(downloadStore.favorites().isEmpty());
            assertTrue(downloadStore.history().isEmpty());
            provider.preview(wide).get(3, TimeUnit.SECONDS);
            new AuroraCatalogProvider(downloadStore, endpoint + "/catalog").preview(slim).get(3, TimeUnit.SECONDS);
            assertEquals(1, requests.get(), "Reopening a provider reuses validated disk PNG");

            new AuroraCatalogProvider(store, endpoint + "/catalog").preview(slim).get(3, TimeUnit.SECONDS);
            assertArrayEquals(metadata, Files.readAllBytes(store.root().resolve("metadata").resolve(hash + ".properties")));
            assertEquals(SkinModel.WIDE, store.active().model());
            assertEquals("Saved classic", store.list().get(0).name());
            assertEquals(1, store.favorites().size());
            assertEquals(1, store.history().size());
            assertEquals(1, requests.get());
            Files.write(downloadStore.root().resolve("cache").resolve(hash + ".png"), new byte[] {1, 2, 3});
            SkinEntry repaired = new AuroraCatalogProvider(downloadStore, endpoint + "/catalog").preview(slim).get(3, TimeUnit.SECONDS);
            assertArrayEquals(png, Files.readAllBytes(repaired.file()));
            assertEquals(2, requests.get(), "Corrupt disk cache recovers by one validated download");
        } finally { release.countDown(); server.stop(0); }
    }

    private static SkinEntry entry(String id, SkinModel model, String hash, String endpoint) {
        return new SkinEntry(id, id, "fixture", model, "aurora-online", endpoint + "/skin.png", hash,
            null, Collections.<String>emptyList());
    }
}
