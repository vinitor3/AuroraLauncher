package com.aurora.skins.storage;

import com.aurora.skins.api.SkinEntry;
import com.aurora.skins.api.SkinModel;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import javax.imageio.ImageIO;
import java.awt.image.BufferedImage;
import java.nio.file.Path;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class AuroraSkinStoreTest {
    @TempDir Path temporary;

    @Test void importsDeduplicatesFavoritesAndTracksHistory() throws Exception {
        Path png = temporary.resolve("skin.png");
        ImageIO.write(new BufferedImage(64, 64, BufferedImage.TYPE_INT_ARGB), "png", png.toFile());
        AuroraSkinStore store = new AuroraSkinStore(temporary.resolve("library"));

        SkinEntry first = store.importSkin(png, "Minha skin", SkinModel.SLIM);
        SkinEntry second = store.importSkin(png, "Cópia", SkinModel.SLIM);
        store.setFavorite(first, true);
        store.setActive(first);

        assertEquals(first.contentHash(), second.contentHash());
        assertEquals(1, store.list().size());
        assertTrue(store.isFavorite(first));
        assertEquals(first.contentHash(), store.active().contentHash());
        assertEquals(first.contentHash(), store.history().get(0).contentHash());
    }

    @Test void rejectsImagesWithInvalidDimensions() throws Exception {
        Path png = temporary.resolve("invalid.png");
        ImageIO.write(new BufferedImage(32, 32, BufferedImage.TYPE_INT_ARGB), "png", png.toFile());
        AuroraSkinStore store = new AuroraSkinStore(temporary.resolve("library"));

        assertThrows(SkinValidationException.class,
            () -> store.importSkin(png, "Inválida", SkinModel.WIDE));
    }
}
