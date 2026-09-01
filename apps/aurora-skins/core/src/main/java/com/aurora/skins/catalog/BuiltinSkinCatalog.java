package com.aurora.skins.catalog;

import com.aurora.skins.api.SkinEntry;
import com.aurora.skins.api.SkinModel;
import com.aurora.skins.api.SkinPage;
import com.aurora.skins.storage.AuroraSkinStore;

import javax.imageio.ImageIO;
import java.awt.image.BufferedImage;
import java.io.IOException;
import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.Locale;

/** Small original catalog that keeps Explore useful while the network is unavailable. */
final class BuiltinSkinCatalog {
    private static final List<Template> TEMPLATES = Arrays.asList(
        new Template("aurora-voyager", "Viajante Aurora", SkinModel.SLIM,
            0xFFDCC1A0, 0xFF241739, 0xFF7652B8, 0xFFB78CFF, 0xFF181125,
            "aurora", "cosmos", "espaço", "neon", "roxo"),
        new Template("solar-ninja", "Ninja Solar", SkinModel.SLIM,
            0xFFE7B78E, 0xFF19151B, 0xFFE86F22, 0xFFFFC34A, 0xFF202027,
            "anime", "ninja", "naruto", "shinobi", "laranja"),
        new Template("ki-guardian", "Guardião do Ki", SkinModel.WIDE,
            0xFFE5B186, 0xFF171B36, 0xFF3159AE, 0xFFF18B2A, 0xFF18213B,
            "anime", "dragon ball", "ki", "guerreiro", "azul"),
        new Template("forest-ranger", "Guardiã da Floresta", SkinModel.SLIM,
            0xFFD6A47F, 0xFF3B2A20, 0xFF355E3B, 0xFF79A85B, 0xFF25362A,
            "floresta", "ranger", "natureza", "verde", "aventura"),
        new Template("cyber-violet", "Cyber Violeta", SkinModel.SLIM,
            0xFFC99D83, 0xFF16131F, 0xFF4D2C78, 0xFF33D6C6, 0xFF191426,
            "cyber", "futurista", "neon", "violeta", "tecnologia"),
        new Template("crimson-mage", "Maga Carmesim", SkinModel.WIDE,
            0xFFDBA37F, 0xFF2A1721, 0xFF741F35, 0xFFE05268, 0xFF25151B,
            "maga", "fantasia", "carmesim", "vermelho", "aventura")
    );

    private final Path directory;
    private volatile List<SkinEntry> entries;

    BuiltinSkinCatalog(AuroraSkinStore store) {
        this.directory = store.root().resolve("catalog").resolve("builtin");
    }

    SkinPage search(String query, int page, int pageSize) throws IOException {
        List<SkinEntry> source = entries();
        String needle = normalize(query);
        List<SkinEntry> matches = new ArrayList<SkinEntry>();
        for (SkinEntry entry : source) {
            String haystack = normalize(entry.name() + " " + entry.author() + " " + entry.tags());
            if (needle.isEmpty() || haystack.contains(needle)) matches.add(entry);
        }
        int size = Math.max(1, Math.min(24, pageSize));
        int current = Math.max(0, page);
        int from = Math.min(matches.size(), current * size);
        int to = Math.min(matches.size(), from + size);
        return new SkinPage(matches.subList(from, to), current, to < matches.size());
    }

    private List<SkinEntry> entries() throws IOException {
        List<SkinEntry> current = entries;
        if (current != null) return current;
        synchronized (this) {
            if (entries != null) return entries;
            Files.createDirectories(directory);
            List<SkinEntry> generated = new ArrayList<SkinEntry>();
            for (Template template : TEMPLATES) {
                Path file = directory.resolve(template.id + ".png");
                if (!Files.isRegularFile(file)) writeSkin(file, template);
                generated.add(new SkinEntry(template.id, template.name, "Aurora Studio", template.model,
                    "aurora-builtin", "aurora://catalog/" + template.id, sha256(file), file,
                    Arrays.asList(template.tags)));
            }
            entries = Collections.unmodifiableList(generated);
            return entries;
        }
    }

    private static void writeSkin(Path target, Template template) throws IOException {
        BufferedImage image = new BufferedImage(64, 64, BufferedImage.TYPE_INT_ARGB);
        paintHead(image, template);
        paintBody(image, template);
        paintRightArm(image, template);
        paintRightLeg(image, template);
        paintLeftArm(image, template);
        paintLeftLeg(image, template);
        Path temporary = target.resolveSibling(target.getFileName() + ".tmp");
        if (!ImageIO.write(image, "png", temporary.toFile())) throw new IOException("Codificador PNG indisponível.");
        try {
            Files.move(temporary, target, StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
        } catch (IOException unsupported) {
            Files.move(temporary, target, StandardCopyOption.REPLACE_EXISTING);
        }
    }

    private static void paintHead(BufferedImage image, Template value) {
        rect(image, 0, 0, 32, 16, value.skin);
        rect(image, 0, 8, 8, 8, value.hair);
        rect(image, 8, 0, 8, 8, value.hair);
        rect(image, 16, 8, 16, 8, value.hair);
        rect(image, 8, 8, 8, 2, value.hair);
        pixel(image, 10, 11, value.accent);
        pixel(image, 13, 11, value.accent);
        pixel(image, 11, 14, 0xFF7B4A42);
        pixel(image, 12, 14, 0xFF7B4A42);
    }

    private static void paintBody(BufferedImage image, Template value) {
        rect(image, 16, 16, 24, 16, value.primary);
        rect(image, 20, 20, 8, 12, value.primary);
        rect(image, 23, 20, 2, 12, value.accent);
        rect(image, 20, 29, 8, 3, value.dark);
    }

    private static void paintRightArm(BufferedImage image, Template value) {
        rect(image, 40, 16, 16, 16, value.primary);
        rect(image, 44, 20, 4, 4, value.primary);
        rect(image, 44, 24, 4, 8, value.skin);
        rect(image, 44, 24, 4, 2, value.accent);
    }

    private static void paintLeftArm(BufferedImage image, Template value) {
        rect(image, 32, 48, 16, 16, value.primary);
        rect(image, 36, 52, 4, 4, value.primary);
        rect(image, 36, 56, 4, 8, value.skin);
        rect(image, 36, 56, 4, 2, value.accent);
    }

    private static void paintRightLeg(BufferedImage image, Template value) {
        rect(image, 0, 16, 16, 16, value.dark);
        rect(image, 4, 20, 4, 9, value.dark);
        rect(image, 4, 29, 4, 3, value.accent);
    }

    private static void paintLeftLeg(BufferedImage image, Template value) {
        rect(image, 16, 48, 16, 16, value.dark);
        rect(image, 20, 52, 4, 9, value.dark);
        rect(image, 20, 61, 4, 3, value.accent);
    }

    private static void rect(BufferedImage image, int x, int y, int width, int height, int color) {
        for (int row = y; row < y + height; row++) {
            for (int column = x; column < x + width; column++) image.setRGB(column, row, color);
        }
    }

    private static void pixel(BufferedImage image, int x, int y, int color) { image.setRGB(x, y, color); }

    private static String normalize(String value) {
        String text = value == null ? "" : value.toLowerCase(Locale.ROOT);
        return text.replace('á', 'a').replace('à', 'a').replace('â', 'a').replace('ã', 'a')
            .replace('é', 'e').replace('ê', 'e').replace('í', 'i').replace('ó', 'o')
            .replace('ô', 'o').replace('õ', 'o').replace('ú', 'u').replace('ç', 'c').trim();
    }

    private static String sha256(Path file) throws IOException {
        try {
            MessageDigest digest = MessageDigest.getInstance("SHA-256");
            byte[] buffer = new byte[8192];
            try (InputStream input = Files.newInputStream(file)) {
                int read;
                while ((read = input.read(buffer)) >= 0) if (read > 0) digest.update(buffer, 0, read);
            }
            StringBuilder result = new StringBuilder(64);
            for (byte item : digest.digest()) result.append(String.format(Locale.ROOT, "%02x", item & 0xff));
            return result.toString();
        } catch (NoSuchAlgorithmException impossible) {
            throw new IllegalStateException(impossible);
        }
    }

    private static final class Template {
        private final String id;
        private final String name;
        private final SkinModel model;
        private final int skin;
        private final int hair;
        private final int primary;
        private final int accent;
        private final int dark;
        private final String[] tags;

        private Template(String id, String name, SkinModel model, int skin, int hair,
                         int primary, int accent, int dark, String... tags) {
            this.id = id;
            this.name = name;
            this.model = model;
            this.skin = skin;
            this.hair = hair;
            this.primary = primary;
            this.accent = accent;
            this.dark = dark;
            this.tags = tags;
        }
    }
}
