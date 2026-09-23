package com.aurora.skins.sync;

import com.aurora.skins.api.SkinModel;
import com.aurora.skins.image.SkinImageNormalizer;
import javax.imageio.ImageIO;
import java.awt.image.BufferedImage;
import java.io.*;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.util.UUID;
import java.util.zip.CRC32;

/** Loader-independent, bounded protocol. Publications never contain a claimed player UUID. */
public final class SkinSyncProtocol {
    public static final int VERSION = 1;
    public static final int MAX_PNG_BYTES = 24576;
    public static final int MAX_PACKET_BYTES = 24640;
    public static final int MAX_PLAYERS = 256;
    public static final int HELLO = 0, PUBLISH = 1, UPDATE = 2, REMOVE = 3, RESET = 4;
    private static final byte[] PNG = {(byte)137,80,78,71,13,10,26,10};
    private SkinSyncProtocol() { }

    public static final class Message {
        public final int kind;
        public final UUID player;
        public final SkinModel model;
        private final byte[] png;
        private Message(int kind, UUID player, SkinModel model, byte[] png) {
            this.kind = kind; this.player = player; this.model = model; this.png = png;
        }
        public byte[] png() { return png == null ? null : png.clone(); }
    }

    public static byte[] hello() { return new byte[] {VERSION, HELLO}; }
    public static byte[] reset() { return new byte[] {VERSION, RESET}; }
    public static byte[] publish(SkinModel model, byte[] png) throws IOException { return encode(PUBLISH, null, model, png); }
    public static byte[] update(UUID player, SkinModel model, byte[] png) throws IOException { return encode(UPDATE, player, model, png); }
    public static byte[] remove(UUID player) {
        try { return encode(REMOVE, player, null, null); }
        catch (IOException impossible) { throw new IllegalStateException(impossible); }
    }

    private static byte[] encode(int kind, UUID player, SkinModel model, byte[] png) throws IOException {
        ByteArrayOutputStream bytes = new ByteArrayOutputStream();
        DataOutputStream out = new DataOutputStream(bytes);
        out.writeByte(VERSION); out.writeByte(kind);
        if (kind == UPDATE || kind == REMOVE) {
            if (player == null) throw new IOException("Missing player");
            out.writeLong(player.getMostSignificantBits()); out.writeLong(player.getLeastSignificantBits());
        }
        if (kind == PUBLISH || kind == UPDATE) {
            if (png == null || png.length > MAX_PNG_BYTES || png.length < 67) throw new IOException("Invalid skin size");
            out.writeByte(model == SkinModel.SLIM ? 1 : 0); out.writeInt(png.length); out.write(png);
        }
        return bytes.toByteArray();
    }

    public static Message decode(byte[] packet) throws IOException {
        if (packet == null || packet.length < 2 || packet.length > MAX_PACKET_BYTES) throw new IOException("Invalid packet size");
        DataInputStream in = new DataInputStream(new ByteArrayInputStream(packet));
        if (in.readUnsignedByte() != VERSION) throw new IOException("Unsupported skin protocol");
        int kind = in.readUnsignedByte();
        if (kind > RESET) throw new IOException("Unknown skin message");
        UUID player = kind == UPDATE || kind == REMOVE ? new UUID(in.readLong(), in.readLong()) : null;
        SkinModel model = null; byte[] png = null;
        if (kind == PUBLISH || kind == UPDATE) {
            int value = in.readUnsignedByte();
            if (value > 1) throw new IOException("Invalid skin model");
            model = value == 1 ? SkinModel.SLIM : SkinModel.WIDE;
            int length = in.readInt();
            if (length < 67 || length > MAX_PNG_BYTES || length != in.available()) throw new IOException("Invalid skin length");
            png = new byte[length]; in.readFully(png);
        }
        if (in.available() != 0) throw new IOException("Trailing skin data");
        return new Message(kind, player, model, png);
    }

    /** Check dimensions and chunk lengths before an image decoder can allocate from untrusted input. */
    private static void validatePng(byte[] bytes, int maximum) throws IOException {
        if (bytes == null || bytes.length < 67 || bytes.length > maximum) throw new IOException("Invalid PNG size");
        for (int i = 0; i < PNG.length; i++) if (bytes[i] != PNG[i]) throw new IOException("Invalid PNG signature");
        DataInputStream in = new DataInputStream(new ByteArrayInputStream(bytes, 8, bytes.length - 8));
        boolean first = true, data = false, end = false;
        while (in.available() > 0) {
            int length = in.readInt();
            if (length < 0 || length > in.available() - 8) throw new IOException("Invalid PNG chunk");
            byte[] type = new byte[4]; in.readFully(type);
            byte[] chunk = new byte[length]; in.readFully(chunk);
            long supplied = Integer.toUnsignedLong(in.readInt());
            CRC32 crc = new CRC32(); crc.update(type); crc.update(chunk);
            if (crc.getValue() != supplied) throw new IOException("Invalid PNG checksum");
            String name = new String(type, java.nio.charset.StandardCharsets.US_ASCII);
            if (first) {
                if (!"IHDR".equals(name) || length != 13) throw new IOException("Missing PNG header");
                DataInputStream dimensions = new DataInputStream(new ByteArrayInputStream(chunk));
                int width = dimensions.readInt(), height = dimensions.readInt();
                if (width != 64 || (height != 32 && height != 64)) throw new IOException("Invalid skin dimensions");
            } else if ("IHDR".equals(name)) throw new IOException("Duplicate PNG header");
            if ("IDAT".equals(name)) data = true;
            if ("IEND".equals(name)) {
                if (length != 0 || !data || in.available() != 0) throw new IOException("Invalid PNG end");
                end = true;
            }
            first = false;
        }
        if (!end) throw new IOException("Incomplete PNG");
    }

    /** Re-encode only pixels, stripping metadata and enforcing the wire budget. Safe on a headless server. */
    public static byte[] canonicalPng(byte[] bytes) throws IOException {
        validatePng(bytes, 2 * 1024 * 1024);
        BufferedImage decoded = ImageIO.read(new ByteArrayInputStream(bytes));
        if (decoded == null) throw new IOException("Invalid skin image");
        BufferedImage normalized = SkinImageNormalizer.normalize(decoded);
        BufferedImage pixels = new BufferedImage(64, 64, BufferedImage.TYPE_INT_ARGB);
        pixels.setRGB(0, 0, 64, 64, normalized.getRGB(0, 0, 64, 64, null, 0, 64), 0, 64);
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        if (!ImageIO.write(pixels, "png", out)) throw new IOException("PNG encoder unavailable");
        byte[] result = out.toByteArray();
        if (result.length > MAX_PNG_BYTES) throw new IOException("Skin exceeds network budget");
        return result;
    }

    public static byte[] readLocal(Path file) throws IOException {
        if (file == null) throw new IOException("Missing skin file");
        try (InputStream in = Files.newInputStream(file); ByteArrayOutputStream out = new ByteArrayOutputStream()) {
            byte[] buffer = new byte[8192]; int count;
            while ((count = in.read(buffer)) != -1) {
                if (out.size() + count > 2 * 1024 * 1024) throw new IOException("Skin exceeds file budget");
                out.write(buffer, 0, count);
            }
            return canonicalPng(out.toByteArray());
        }
    }

    public static String hash(byte[] bytes) {
        try {
            byte[] digest = MessageDigest.getInstance("SHA-256").digest(bytes);
            StringBuilder result = new StringBuilder(64);
            for (byte value : digest) result.append(String.format(java.util.Locale.ROOT, "%02x", value & 255));
            return result.toString();
        } catch (java.security.NoSuchAlgorithmException impossible) { throw new IllegalStateException(impossible); }
    }
}
