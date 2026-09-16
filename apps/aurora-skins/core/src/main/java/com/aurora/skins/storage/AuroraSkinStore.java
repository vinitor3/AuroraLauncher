package com.aurora.skins.storage;

import com.aurora.skins.api.SkinEntry;
import com.aurora.skins.api.SkinModel;

import javax.imageio.ImageIO;
import java.awt.image.BufferedImage;
import java.io.BufferedInputStream;
import java.io.BufferedOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.ArrayList;
import java.util.Collections;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Locale;
import java.util.Properties;
import java.util.Set;

/** Content-addressed local library used when the Launcher/API is unavailable. */
public final class AuroraSkinStore {
    private static final long MAX_PNG_BYTES = 2L * 1024L * 1024L;
    private static final int HISTORY_LIMIT = 32;
    private final Path root;
    private final Path cache;
    private final Path metadata;

    public AuroraSkinStore(Path root) throws IOException {
        this.root = root.toAbsolutePath().normalize();
        this.cache = this.root.resolve("cache");
        this.metadata = this.root.resolve("metadata");
        Files.createDirectories(cache);
        Files.createDirectories(this.root.resolve("thumbnails"));
        Files.createDirectories(metadata);
        Files.createDirectories(this.root.resolve("favorites"));
        Files.createDirectories(this.root.resolve("history"));
    }

    public synchronized SkinEntry importSkin(Path source, String name, SkinModel model)
            throws IOException, SkinValidationException {
        validate(source);
        String hash = sha256(source);
        Path target = cache.resolve(hash + ".png");
        if (!Files.isRegularFile(target)) Files.copy(source, target, StandardCopyOption.REPLACE_EXISTING);
        SkinEntry entry = new SkinEntry(hash, safeName(name), "", model, "local",
            source.toAbsolutePath().normalize().toString(), hash, target,
            Collections.<String>emptyList());
        writeMetadata(entry);
        appendUnique(root.resolve("library.index"), hash, Integer.MAX_VALUE);
        return entry;
    }

    public synchronized SkinEntry importCatalogSkin(Path source, SkinEntry catalogEntry)
            throws IOException, SkinValidationException {
        SkinEntry entry = cacheCatalogSkin(source, catalogEntry);
        writeMetadata(entry);
        appendUnique(root.resolve("library.index"), entry.contentHash(), Integer.MAX_VALUE);
        return entry;
    }

    public synchronized SkinEntry cacheCatalogSkin(Path source, SkinEntry catalogEntry)
            throws IOException, SkinValidationException {
        if (catalogEntry == null) throw new SkinValidationException("Item de catálogo inválido.");
        validate(source);
        String hash = sha256(source);
        if (!hash.equals(catalogEntry.contentHash())) {
            throw new SkinValidationException("A skin baixada não corresponde ao catálogo Aurora.");
        }
        Path target = cache.resolve(hash + ".png");
        if (!Files.isRegularFile(target)) {
            Path temporary = cache.resolve(hash + ".png.tmp");
            Files.copy(source, temporary, StandardCopyOption.REPLACE_EXISTING);
            move(temporary, target);
        }
        SkinEntry entry = new SkinEntry(catalogEntry.id(), catalogEntry.name(), catalogEntry.author(),
            catalogEntry.model(), catalogEntry.providerId(), catalogEntry.source(), hash, target,
            catalogEntry.tags());
        // A preview owns only its returned entry and PNG cache, never saved library metadata.
        return entry;
    }

    public synchronized List<SkinEntry> list() throws IOException {
        List<SkinEntry> result = new ArrayList<SkinEntry>();
        for (String hash : readLines(root.resolve("library.index"))) {
            SkinEntry entry = readMetadata(hash);
            if (entry != null && Files.isRegularFile(entry.file())) result.add(entry);
        }
        return Collections.unmodifiableList(result);
    }

    public synchronized void setActive(SkinEntry entry) throws IOException {
        Properties properties = new Properties();
        properties.setProperty("hash", entry.contentHash());
        properties.setProperty("model", entry.model().id());
        writeProperties(root.resolve("active.properties"), properties);
        rememberHistory(entry);
    }

    public synchronized SkinEntry active() throws IOException {
        Properties properties = readProperties(root.resolve("active.properties"));
        return readMetadata(properties.getProperty("hash", ""));
    }

    public synchronized void setFavorite(SkinEntry entry, boolean favorite) throws IOException {
        Path file = root.resolve("favorites").resolve("favorites.index");
        LinkedHashSet<String> hashes = new LinkedHashSet<String>(readLines(file));
        if (favorite) hashes.add(entry.contentHash()); else hashes.remove(entry.contentHash());
        writeLines(file, new ArrayList<String>(hashes));
    }

    public synchronized boolean isFavorite(SkinEntry entry) throws IOException {
        return readLines(root.resolve("favorites").resolve("favorites.index"))
            .contains(entry.contentHash());
    }

    public synchronized List<SkinEntry> favorites() throws IOException {
        return entriesFor(readLines(root.resolve("favorites").resolve("favorites.index")));
    }

    public synchronized List<SkinEntry> history() throws IOException {
        return entriesFor(readLines(root.resolve("history").resolve("recent.index")));
    }

    public Path root() { return root; }

    private void rememberHistory(SkinEntry entry) throws IOException {
        Path file = root.resolve("history").resolve("recent.index");
        List<String> values = new ArrayList<String>(readLines(file));
        values.remove(entry.contentHash());
        values.add(0, entry.contentHash());
        if (values.size() > HISTORY_LIMIT) values = new ArrayList<String>(values.subList(0, HISTORY_LIMIT));
        writeLines(file, values);
    }

    private List<SkinEntry> entriesFor(List<String> hashes) throws IOException {
        List<SkinEntry> result = new ArrayList<SkinEntry>();
        for (String hash : hashes) {
            SkinEntry entry = readMetadata(hash);
            if (entry != null && Files.isRegularFile(entry.file())) result.add(entry);
        }
        return Collections.unmodifiableList(result);
    }

    private void validate(Path source) throws IOException, SkinValidationException {
        if (source == null || !Files.isRegularFile(source)) throw new SkinValidationException("Arquivo de skin não encontrado.");
        long size = Files.size(source);
        if (size < 67 || size > MAX_PNG_BYTES) throw new SkinValidationException("A skin deve ser um PNG de até 2 MB.");
        byte[] signature = new byte[8];
        try (InputStream input = new BufferedInputStream(Files.newInputStream(source))) {
            if (input.read(signature) != signature.length) throw new SkinValidationException("PNG incompleto.");
        }
        byte[] expected = new byte[] {(byte) 0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A};
        for (int index = 0; index < expected.length; index++) {
            if (signature[index] != expected[index]) throw new SkinValidationException("O arquivo selecionado não é um PNG válido.");
        }
        BufferedImage image = ImageIO.read(source.toFile());
        if (image == null) throw new SkinValidationException("Não foi possível decodificar a skin.");
        if (image.getWidth() != 64 || (image.getHeight() != 64 && image.getHeight() != 32)) {
            throw new SkinValidationException("A skin precisa ter 64x64 ou 64x32 pixels.");
        }
    }

    private void writeMetadata(SkinEntry entry) throws IOException {
        Properties properties = new Properties();
        properties.setProperty("id", entry.id());
        properties.setProperty("name", entry.name());
        properties.setProperty("author", entry.author());
        properties.setProperty("model", entry.model().id());
        properties.setProperty("provider", entry.providerId());
        properties.setProperty("source", entry.source());
        properties.setProperty("hash", entry.contentHash());
        properties.setProperty("tags", String.join(",", entry.tags()));
        writeProperties(metadata.resolve(entry.contentHash() + ".properties"), properties);
    }

    private SkinEntry readMetadata(String hash) throws IOException {
        if (!hash.matches("[0-9a-f]{64}")) return null;
        Properties properties = readProperties(metadata.resolve(hash + ".properties"));
        if (properties.isEmpty()) return null;
        return new SkinEntry(properties.getProperty("id", hash), properties.getProperty("name", "Skin"),
            properties.getProperty("author", ""), SkinModel.fromId(properties.getProperty("model")),
            properties.getProperty("provider", "local"), properties.getProperty("source", ""), hash,
            cache.resolve(hash + ".png"), tags(properties.getProperty("tags", "")));
    }

    private static List<String> tags(String value) {
        if (value == null || value.trim().isEmpty()) return Collections.emptyList();
        List<String> tags = new ArrayList<String>();
        for (String tag : value.split(",")) if (!tag.trim().isEmpty()) tags.add(tag.trim());
        return tags;
    }

    private static String safeName(String value) {
        String trimmed = value == null ? "" : value.trim();
        if (trimmed.isEmpty()) return "Skin importada";
        return trimmed.length() > 64 ? trimmed.substring(0, 64) : trimmed;
    }

    private static String sha256(Path file) throws IOException {
        try {
            MessageDigest digest = MessageDigest.getInstance("SHA-256");
            byte[] buffer = new byte[8192];
            try (InputStream input = new BufferedInputStream(Files.newInputStream(file))) {
                int read;
                while ((read = input.read(buffer)) >= 0) if (read > 0) digest.update(buffer, 0, read);
            }
            StringBuilder result = new StringBuilder(64);
            for (byte value : digest.digest()) result.append(String.format(Locale.ROOT, "%02x", value & 0xff));
            return result.toString();
        } catch (NoSuchAlgorithmException impossible) {
            throw new IllegalStateException(impossible);
        }
    }

    private static void appendUnique(Path file, String value, int limit) throws IOException {
        List<String> values = new ArrayList<String>(readLines(file));
        if (!values.contains(value)) values.add(value);
        if (values.size() > limit) values = new ArrayList<String>(values.subList(values.size() - limit, values.size()));
        writeLines(file, values);
    }

    private static List<String> readLines(Path file) throws IOException {
        if (!Files.isRegularFile(file)) return Collections.emptyList();
        List<String> result = new ArrayList<String>();
        for (String line : Files.readAllLines(file, StandardCharsets.UTF_8)) {
            String value = line.trim();
            if (value.matches("[0-9a-f]{64}")) result.add(value);
        }
        return result;
    }

    private static void writeLines(Path file, List<String> values) throws IOException {
        Files.createDirectories(file.getParent());
        Path temporary = file.resolveSibling(file.getFileName() + ".tmp");
        Files.write(temporary, values, StandardCharsets.UTF_8);
        move(temporary, file);
    }

    private static Properties readProperties(Path file) throws IOException {
        Properties properties = new Properties();
        if (!Files.isRegularFile(file)) return properties;
        try (InputStream input = new BufferedInputStream(Files.newInputStream(file))) { properties.load(input); }
        return properties;
    }

    private static void writeProperties(Path file, Properties properties) throws IOException {
        Files.createDirectories(file.getParent());
        Path temporary = file.resolveSibling(file.getFileName() + ".tmp");
        try (OutputStream output = new BufferedOutputStream(Files.newOutputStream(temporary))) {
            properties.store(output, "Aurora Skins");
        }
        move(temporary, file);
    }

    private static void move(Path source, Path target) throws IOException {
        try {
            Files.move(source, target, StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
        } catch (IOException unsupported) {
            Files.move(source, target, StandardCopyOption.REPLACE_EXISTING);
        }
    }
}
