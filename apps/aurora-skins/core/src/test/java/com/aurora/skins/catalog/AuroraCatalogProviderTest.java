package com.aurora.skins.catalog;

import com.aurora.skins.api.SkinEntry;
import com.aurora.skins.api.SkinPage;
import com.aurora.skins.storage.AuroraSkinStore;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import javax.imageio.ImageIO;
import java.net.URI;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Base64;
import java.util.Collections;
import java.util.concurrent.TimeUnit;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class AuroraCatalogProviderTest {
    @TempDir Path temporary;

    @Test void themedFallbackCanBeSearchedAndInstalled() throws Exception {
        AuroraSkinStore store = new AuroraSkinStore(temporary.resolve("skins"));
        AuroraCatalogProvider provider = new AuroraCatalogProvider(store,
            "http://127.0.0.1:1/v1/skins/catalog");

        SkinPage page = provider.search("naruto", 0, 12).get(8, TimeUnit.SECONDS);

        assertFalse(page.entries().isEmpty());
        assertEquals("Ninja Solar", page.entries().get(0).name());
        SkinEntry installed = provider.materialize(page.entries().get(0)).get(8, TimeUnit.SECONDS);
        assertTrue(Files.isRegularFile(installed.file()));
        assertEquals(64, ImageIO.read(installed.file().toFile()).getWidth());
        assertTrue(store.list().stream().anyMatch(item -> item.contentHash().equals(installed.contentHash())));
    }

    @Test void parsesBoundedSameOriginCatalogProtocol() throws Exception {
        String hash = String.join("", Collections.nCopies(64, "a"));
        String payload = "AURORA_SKINS_CATALOG_V1\npage\t2\tmore\t1\n"
            + "sample\t" + encoded("Skin teste") + "\t" + encoded("Aurora Studio")
            + "\tslim\t" + hash + "\t" + encoded("/v1/skins/catalog/sample.png")
            + "\t" + encoded("anime,ninja") + "\n";

        SkinPage page = AuroraCatalogProvider.parse(payload,
            URI.create("https://aurora.example/v1/skins/catalog?page=2"));

        assertEquals(2, page.page());
        assertTrue(page.hasMore());
        assertEquals("https://aurora.example/v1/skins/catalog/sample.png", page.entries().get(0).source());
        assertEquals(2, page.entries().get(0).tags().size());
    }

    private static String encoded(String value) {
        return Base64.getUrlEncoder().withoutPadding().encodeToString(value.getBytes(StandardCharsets.UTF_8));
    }
}
