package com.aurora.core.api.ui;

/** Renderer supplied by an appearance module for title screens without a player entity. */
public interface AuroraPreviewRenderer {
    boolean render(Object nativeContext, int x, int y, int scale, AuroraPlayerPreview pose);
}
