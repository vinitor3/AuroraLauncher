package com.aurora.skins.api;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

public final class SkinPage {
    private final List<SkinEntry> entries;
    private final int page;
    private final boolean hasMore;

    public SkinPage(List<SkinEntry> entries, int page, boolean hasMore) {
        this.entries = Collections.unmodifiableList(new ArrayList<SkinEntry>(entries));
        this.page = page;
        this.hasMore = hasMore;
    }

    public List<SkinEntry> entries() { return entries; }
    public int page() { return page; }
    public boolean hasMore() { return hasMore; }
}
