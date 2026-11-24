package com.aurora.skins.catalog;

import com.aurora.skins.api.SkinEntry;
import com.aurora.skins.api.SkinModel;
import com.aurora.skins.api.SkinPage;
import com.aurora.skins.storage.AuroraSkinStore;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import javax.imageio.ImageIO;
import java.awt.image.BufferedImage;
import java.net.URI;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Base64;
import java.util.Collections;
import java.util.Properties;
import java.io.OutputStream;
import java.util.concurrent.TimeUnit;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;

final class AuroraCatalogProviderTest {
    @TempDir Path temporary;

    @Test void offlineSearchDoesNotSeedStudioSkinsOrRestoreLegacySelections() throws Exception {
        AuroraSkinStore store = new AuroraSkinStore(temporary.resolve("skins"));
        AuroraCatalogProvider provider = new AuroraCatalogProvider(store,
            "http://127.0.0.1:1/v1/skins/catalog");

        SkinPage page = provider.search("naruto", 0, 12).get(8, TimeUnit.SECONDS);

        assertTrue(page.entries().isEmpty());
        assertFalse(provider.online());
        assertFalse(Files.exists(store.root().resolve("catalog/builtin")));
        Path png = temporary.resolve("solar-ninja.png");
        ImageIO.write(new BufferedImage(64, 64, BufferedImage.TYPE_INT_ARGB), "png", png.toFile());
        SkinEntry original = store.importSkin(png, "Ninja Solar", SkinModel.SLIM);
        store.setFavorite(original, true);
        store.setActive(original);
        Properties old = new Properties();
        old.setProperty("id", "solar-ninja");
        old.setProperty("provider", "aurora-builtin");
        old.setProperty("source", "aurora://catalog/solar-ninja");
        try (OutputStream file = Files.newOutputStream(store.root().resolve("metadata/" + original.contentHash() + ".properties"))) {
            old.store(file, null);
        }
        assertTrue(store.list().isEmpty());
        assertTrue(store.favorites().isEmpty());
        assertTrue(store.history().isEmpty());
        assertNull(store.active());
        SkinEntry retired = new SkinEntry("solar-ninja", "Ninja Solar", "", SkinModel.SLIM, "aurora-online",
            "https://aurora.example/v1/skins/catalog/solar-ninja.png", original.contentHash(), original.file(), original.tags());
        assertNull(store.cachedCatalogSkin(retired));
        assertThrows(java.util.concurrent.ExecutionException.class, () -> provider.materialize(retired).get(2, TimeUnit.SECONDS));
        // Importing the very same PNG oneself is still supported.
        assertEquals("local", store.importSkin(png, "Ninja Solar", SkinModel.SLIM).providerId());
        assertEquals(1, store.list().size());
    }

    @Test void parsesBoundedSameOriginCatalogProtocol() throws Exception {
        String hash = String.join("", Collections.nCopies(64, "a"));
        String payload = "AURORA_SKINS_CATALOG_V1\npage\t2\tmore\t1\n"
            + "sample\t" + encoded("Skin teste") + "\t" + encoded("Aurora Studio")
            + "\tslim\t" + hash + "\t" + encoded("/v1/skins/catalog/sample.png")
            + "\t" + encoded("anime,ninja") + "\n"
            + "solar-ninja\t" + encoded("Renamed") + "\t" + encoded("Renamed")
            + "\tclassic\t" + hash + "\t" + encoded("/v1/skins/catalog/solar-ninja.png") + "\t\n";

        SkinPage page = AuroraCatalogProvider.parse(payload,
            URI.create("https://aurora.example/v1/skins/catalog?page=2"));

        assertEquals(2, page.page());
        assertTrue(page.hasMore());
        assertEquals(1, page.entries().size());
        assertEquals("https://aurora.example/v1/skins/catalog/sample.png", page.entries().get(0).source());
        assertEquals(2, page.entries().get(0).tags().size());
    }

    private static String encoded(String value) {
        return Base64.getUrlEncoder().withoutPadding().encodeToString(value.getBytes(StandardCharsets.UTF_8));
    }
}
