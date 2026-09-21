package com.aurora.skins.ui;

import com.aurora.skins.api.SkinEntry;
import com.aurora.skins.api.SkinPage;

import java.util.HashSet;
import java.util.Set;

/**
 * Screen-owned catalog state, independent of Minecraft and persistent skin data.
 * Call from the UI thread, including delivery of asynchronous completions.
 * Texture ownership belongs to the adapter: READY means it has registered the
 * correct preview texture, not merely that the catalog returned an entry.
 */
public final class CatalogGridState {
    public static final int PAGE_SIZE = 12;

    public enum Status { EMPTY, LOADING, READY, FAILED }

    private final Slot[] slots = new Slot[PAGE_SIZE];
    private long generation;
    private boolean active;
    private boolean loading;
    private boolean failed;
    private boolean confirmed;
    private boolean hasMore;
    private String query = "";
    private int page;
    private int requestedPage;
    private String selectedIdentity;

    public CatalogGridState() { clearSlots(); }

    /** Clears old cards immediately; the displayed page changes only on accept. */
    public long begin(String nextQuery, int nextPage) {
        if (nextPage < 0) throw new IllegalArgumentException("page must be non-negative");
        String normalized = nextQuery == null ? "" : nextQuery.trim();
        if (!normalized.equals(query) || nextPage != requestedPage) selectedIdentity = null;
        query = normalized;
        requestedPage = nextPage;
        active = true;
        loading = true;
        failed = false;
        clearSlots();
        return ++generation;
    }

    /** The response is already paginated. Never apply another page-size offset. */
    public boolean accept(long token, SkinPage response) {
        if (!isCurrent(token) || !loading) return false;
        if (response == null || response.page() != requestedPage) {
            fail(token);
            return false;
        }
        Set<String> identities = new HashSet<String>();
        int index = 0;
        for (SkinEntry entry : response.entries()) {
            if (entry == null) continue;
            String identity = identity(entry);
            if (!identities.add(identity)) continue;
            slots[index++] = new Slot(entry, identity);
            if (index == PAGE_SIZE) break;
        }
        if (indexOf(selectedIdentity) < 0) selectedIdentity = null;
        page = response.page();
        hasMore = response.hasMore();
        confirmed = true;
        loading = false;
        failed = false;
        return true;
    }

    public boolean fail(long token) {
        if (!isCurrent(token) || !loading) return false;
        loading = false;
        failed = true;
        hasMore = false;
        selectedIdentity = null;
        clearSlots();
        return true;
    }

    /** Invalidates pending page/preview work when leaving the tab or screen. */
    public void invalidate() {
        ++generation;
        active = false;
        loading = false;
        failed = false;
        confirmed = false;
        hasMore = false;
        selectedIdentity = null;
        clearSlots();
    }

    public boolean ready(long token, int index, String expectedIdentity, SkinEntry preview) {
        if (!matches(token, index, expectedIdentity) || preview == null || preview.file() == null
            || slots[index].status != Status.LOADING) return false;
        slots[index].preview = preview;
        slots[index].status = Status.READY;
        return true;
    }

    /** May also mark a render failure after upload; invisible cards cannot apply. */
    public boolean previewFailed(long token, int index, String expectedIdentity) {
        if (!matches(token, index, expectedIdentity)) return false;
        Slot slot = slots[index];
        slot.preview = null;
        slot.status = Status.FAILED;
        if (slot.identity.equals(selectedIdentity)) selectedIdentity = null;
        return true;
    }

    /**
     * Updates the target of an explicit Model action, without moving selection
     * to whatever entry now occupies its old index. The adapter reloads this
     * returned slot and releases its previous texture.
     */
    public int replaceEntry(long token, String originalIdentity, SkinEntry replacement) {
        if (!isCurrent(token) || loading || replacement == null) return -1;
        int index = indexOf(originalIdentity);
        if (index < 0) return -1;
        String replacementIdentity = identity(replacement);
        int duplicate = indexOf(replacementIdentity);
        if (duplicate >= 0 && duplicate != index) return -1;
        slots[index] = new Slot(replacement, replacementIdentity);
        if (originalIdentity.equals(selectedIdentity)) selectedIdentity = replacementIdentity;
        return index;
    }

    public boolean select(int index) {
        if (!active || loading || index < 0 || index >= PAGE_SIZE
            || slots[index].status != Status.READY) return false;
        selectedIdentity = slots[index].identity;
        return true;
    }

    public int selectedIndex() {
        int index = indexOf(selectedIdentity);
        return index >= 0 && slots[index].status == Status.READY ? index : -1;
    }

    public SkinEntry selected() {
        int index = selectedIndex();
        return index < 0 ? null : slots[index].entry;
    }

    public SkinEntry selectedPreview() {
        int index = selectedIndex();
        return index < 0 ? null : slots[index].preview;
    }

    public boolean isSelected(int index) { return index >= 0 && index == selectedIndex(); }
    public String selectedIdentity() { return selectedIdentity; }
    public Slot slot(int index) { return slots[index]; }
    public long generation() { return generation; }
    public boolean isCurrent(long token) { return active && token == generation; }
    public int page() { return page; }
    public int requestedPage() { return requestedPage; }
    public String query() { return query; }
    public boolean loading() { return loading; }
    public boolean failed() { return failed; }
    public boolean hasMore() { return hasMore; }
    public boolean canPrevious() { return active && confirmed && !loading && !failed && page > 0; }
    public boolean canNext() { return active && confirmed && !loading && !failed && hasMore; }

    private boolean matches(long token, int index, String expectedIdentity) {
        return isCurrent(token) && !loading && index >= 0 && index < PAGE_SIZE
            && expectedIdentity != null && expectedIdentity.equals(slots[index].identity);
    }

    public int indexOf(String identity) {
        if (identity == null) return -1;
        for (int index = 0; index < PAGE_SIZE; index++) {
            if (identity.equals(slots[index].identity)) return index;
        }
        return -1;
    }

    /** Includes model and origin as well as entry identity; slot position is irrelevant. */
    public static String identity(SkinEntry entry) {
        if (entry == null) return "";
        return component(entry.providerId()) + component(entry.id()) + component(entry.contentHash())
            + component(entry.source()) + component(entry.model().id());
    }

    private static String component(String value) { return value.length() + ":" + value; }

    private void clearSlots() {
        for (int index = 0; index < PAGE_SIZE; index++) slots[index] = new Slot();
    }

    public static final class Slot {
        private final SkinEntry entry;
        private final String identity;
        private SkinEntry preview;
        private Status status;

        private Slot() {
            entry = null;
            identity = null;
            status = Status.EMPTY;
        }

        private Slot(SkinEntry entry, String identity) {
            this.entry = entry;
            this.identity = identity;
            status = Status.LOADING;
        }

        public SkinEntry entry() { return entry; }
        public String identity() { return identity; }
        public SkinEntry preview() { return preview; }
        public Status status() { return status; }
    }
}
