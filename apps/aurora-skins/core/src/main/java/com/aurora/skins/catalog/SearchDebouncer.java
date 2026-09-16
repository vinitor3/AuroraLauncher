package com.aurora.skins.catalog;

import java.util.concurrent.CompletableFuture;
import java.util.concurrent.Executors;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.ScheduledFuture;
import java.util.concurrent.TimeUnit;
import java.util.function.Supplier;

/** One per screen/provider. Cancelled queries never overwrite newer search results. */
public final class SearchDebouncer {
    private static final ScheduledExecutorService TIMER = Executors.newSingleThreadScheduledExecutor(r -> {
        Thread thread = new Thread(r, "Aurora-Skins-Debounce"); thread.setDaemon(true); return thread;
    });
    private ScheduledFuture<?> task;
    private CompletableFuture<?> pending;
    public synchronized <T> CompletableFuture<T> submit(Supplier<CompletableFuture<T>> operation) {
        cancel();
        final CompletableFuture<T> result = new CompletableFuture<T>();
        pending = result;
        task = TIMER.schedule(() -> {
            if (result.isCancelled()) return;
            try { operation.get().whenComplete((value, error) -> {
                if (error != null) result.completeExceptionally(error); else result.complete(value);
            }); } catch (Exception error) { result.completeExceptionally(error); }
        }, 250, TimeUnit.MILLISECONDS);
        return result;
    }
    public synchronized void cancel() {
        if (task != null) task.cancel(false);
        if (pending != null) pending.cancel(false);
        task = null; pending = null;
    }
}
