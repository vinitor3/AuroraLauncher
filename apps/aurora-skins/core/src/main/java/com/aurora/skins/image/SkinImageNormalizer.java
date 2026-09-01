package com.aurora.skins.image;

import java.awt.Graphics2D;
import java.awt.image.BufferedImage;

/** Converts the legacy 64x32 skin layout into Minecraft's complete 64x64 layout. */
public final class SkinImageNormalizer {
    private SkinImageNormalizer() { }

    public static BufferedImage normalize(BufferedImage source) {
        if (source == null || source.getWidth() != 64
                || (source.getHeight() != 32 && source.getHeight() != 64)) {
            throw new IllegalArgumentException("Skin precisa ter 64x64 ou 64x32 pixels");
        }
        if (source.getHeight() == 64) return source;

        BufferedImage target = new BufferedImage(64, 64, BufferedImage.TYPE_INT_ARGB);
        Graphics2D graphics = target.createGraphics();
        try {
            graphics.drawImage(source, 0, 0, null);
        } finally {
            graphics.dispose();
        }

        // Same mirroring used by the vanilla skin texture normalizer. Legacy
        // files contain only the right arm/leg; the missing left limbs are
        // reconstructed into their modern slots.
        copyRect(target, 4, 16, 16, 32, 4, 4, true, false);
        copyRect(target, 8, 16, 16, 32, 4, 4, true, false);
        copyRect(target, 0, 20, 24, 32, 4, 12, true, false);
        copyRect(target, 4, 20, 16, 32, 4, 12, true, false);
        copyRect(target, 8, 20, 8, 32, 4, 12, true, false);
        copyRect(target, 12, 20, 16, 32, 4, 12, true, false);
        copyRect(target, 44, 16, -8, 32, 4, 4, true, false);
        copyRect(target, 48, 16, -8, 32, 4, 4, true, false);
        copyRect(target, 40, 20, 0, 32, 4, 12, true, false);
        copyRect(target, 44, 20, -8, 32, 4, 12, true, false);
        copyRect(target, 48, 20, -16, 32, 4, 12, true, false);
        copyRect(target, 52, 20, -8, 32, 4, 12, true, false);
        return target;
    }

    private static void copyRect(BufferedImage image, int sourceX, int sourceY,
                                 int translateX, int translateY, int width, int height,
                                 boolean mirrorX, boolean mirrorY) {
        int[] pixels = new int[width * height];
        image.getRGB(sourceX, sourceY, width, height, pixels, 0, width);
        for (int y = 0; y < height; y++) {
            for (int x = 0; x < width; x++) {
                int destinationX = sourceX + translateX + (mirrorX ? width - 1 - x : x);
                int destinationY = sourceY + translateY + (mirrorY ? height - 1 - y : y);
                image.setRGB(destinationX, destinationY, pixels[y * width + x]);
            }
        }
    }
}
