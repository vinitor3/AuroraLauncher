package com.aurora.skins.api;

import java.util.concurrent.CompletableFuture;

/** Pluggable, paginated provider. Implementations must never block the render thread. */
public interface SkinProvider {
    String id();
    CompletableFuture<SkinPage> search(String query, int page, int pageSize);
}
