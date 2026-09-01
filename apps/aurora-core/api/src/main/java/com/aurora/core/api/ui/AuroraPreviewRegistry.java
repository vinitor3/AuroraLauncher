package com.aurora.core.api.ui;

public interface AuroraPreviewRegistry {
    void register(String owner, AuroraPreviewRenderer renderer);
    void unregister(String owner);
    boolean render(Object nativeContext, int x, int y, int scale, AuroraPlayerPreview pose);
}
