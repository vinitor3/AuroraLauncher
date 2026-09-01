package com.aurora.core.runtime.ui;

import com.aurora.core.api.module.AuroraModuleRegistry;
import com.aurora.core.api.ui.AuroraPlayerPreview;
import com.aurora.core.api.ui.AuroraPreviewRegistry;
import com.aurora.core.api.ui.AuroraPreviewRenderer;
import com.aurora.core.runtime.AuroraLog;

import java.util.LinkedHashMap;
import java.util.Map;

public final class DefaultPreviewRegistry implements AuroraPreviewRegistry {
    private final Map<String, AuroraPreviewRenderer> renderers =
        new LinkedHashMap<String, AuroraPreviewRenderer>();
    private final AuroraLog log;

    public DefaultPreviewRegistry(AuroraLog log) { this.log = log; }

    @Override public synchronized void register(String owner, AuroraPreviewRenderer renderer) {
        if (!AuroraModuleRegistry.isValidId(owner)) throw new IllegalArgumentException("Invalid preview owner: " + owner);
        if (renderer == null) throw new IllegalArgumentException("renderer");
        renderers.put(owner, renderer);
    }

    @Override public synchronized void unregister(String owner) { renderers.remove(owner); }

    @Override public synchronized boolean render(Object nativeContext, int x, int y, int scale,
                                                 AuroraPlayerPreview pose) {
        for (Map.Entry<String, AuroraPreviewRenderer> entry : renderers.entrySet()) {
            try {
                if (entry.getValue().render(nativeContext, x, y, scale, pose)) return true;
            } catch (Throwable error) {
                log.warn("Preview renderer '" + entry.getKey() + "' failed; using the next fallback.");
            }
        }
        return false;
    }
}
