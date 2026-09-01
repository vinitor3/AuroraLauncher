package com.aurora.skins.storage;

import com.aurora.skins.api.SkinEntry;
import com.aurora.skins.api.SkinPage;
import com.aurora.skins.api.SkinProvider;

import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.concurrent.CompletableFuture;

public final class LocalSkinProvider implements SkinProvider {
    private final AuroraSkinStore store;
    public LocalSkinProvider(AuroraSkinStore store) { this.store = store; }
    @Override public String id() { return "local"; }

    @Override public CompletableFuture<SkinPage> search(final String query, final int page, final int pageSize) {
        return CompletableFuture.supplyAsync(() -> {
            try {
                String needle = query == null ? "" : query.trim().toLowerCase(Locale.ROOT);
                List<SkinEntry> matches = new ArrayList<SkinEntry>();
                for (SkinEntry entry : store.list()) {
                    String haystack = (entry.name() + " " + entry.author() + " " + entry.tags()).toLowerCase(Locale.ROOT);
                    if (needle.isEmpty() || haystack.contains(needle)) matches.add(entry);
                }
                int size = Math.max(1, Math.min(64, pageSize));
                int current = Math.max(0, page);
                int from = Math.min(matches.size(), current * size);
                int to = Math.min(matches.size(), from + size);
                return new SkinPage(matches.subList(from, to), current, to < matches.size());
            } catch (IOException error) {
                throw new IllegalStateException("Não foi possível ler a biblioteca local.", error);
            }
        });
    }
}
