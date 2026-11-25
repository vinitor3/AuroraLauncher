package com.aurora.skins;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.nio.ByteBuffer;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;

/** Reads only the account's explicitly equipped cape, prepared by the launcher. */
public final class ProfileCape {
    private static final int MAX_BYTES = 5 * 1024 * 1024;

    private ProfileCape() { }

    public static byte[] readSelected() throws IOException {
        String selected = System.getProperty("aurora.profile.capeFile", "").trim();
        if (selected.isEmpty()) return null;
        Path path = Paths.get(selected);
        if (!path.isAbsolute() || !Files.isRegularFile(path)) return null;
        try (InputStream input = Files.newInputStream(path);
             ByteArrayOutputStream output = new ByteArrayOutputStream()) {
            byte[] block = new byte[8192];
            int count;
            while ((count = input.read(block)) != -1) {
                if (count > MAX_BYTES - output.size()) throw new IOException("Capa maior que 5 MB.");
                output.write(block, 0, count);
            }
            byte[] png = output.toByteArray();
            if (png.length < 33) throw new IOException("PNG de capa incompleto.");
            ByteBuffer header = ByteBuffer.wrap(png);
            if (header.getLong() != 0x89504e470d0a1a0aL || header.getInt() != 13
                    || header.getInt() != 0x49484452) throw new IOException("PNG de capa inválido.");
            int width = header.getInt(), height = header.getInt();
            if (width < 64 || width > 1024 || width % 64 != 0 || height != width / 2) {
                throw new IOException("Use uma textura de capa 64x32 ou um múltiplo até 1024x512.");
            }
            return png;
        }
    }
}
