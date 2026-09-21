package com.aurora.skins.ui;

import com.aurora.skins.api.SkinEntry;
import com.aurora.skins.api.SkinModel;
import com.aurora.skins.api.SkinPage;
import org.junit.jupiter.api.Test;

import java.nio.file.Paths;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;

import static org.junit.jupiter.api.Assertions.*;

final class CatalogGridStateTest {
    @Test void pagesAdvanceInBatchesOfTwelveWithoutWrappingOrInventingResults() {
        for (int total : new int[] {0, 1, 11, 12, 13, 24, 25}) {
            CatalogGridState grid = new CatalogGridState();
            int page = 0;
            int seen = 0;
            do {
                long token = grid.begin("", page);
                assertFalse(grid.canNext());
                assertFalse(grid.canPrevious());
                int count = Math.max(0, Math.min(12, total - page * 12));
                List<SkinEntry> entries = entries(page * 12, count);
                assertTrue(grid.accept(token, new SkinPage(entries, page, (page + 1) * 12 < total)));
                assertEquals(page, grid.page());
                assertEquals(page > 0, grid.canPrevious());
                assertNull(grid.selected());
                for (int slot = 0; slot < 12; slot++) {
                    if (slot < count) {
                        assertSame(entries.get(slot), grid.slot(slot).entry());
                        assertEquals(CatalogGridState.Status.LOADING, grid.slot(slot).status());
                        seen++;
                    } else {
                        assertNull(grid.slot(slot).entry());
                        assertEquals(CatalogGridState.Status.EMPTY, grid.slot(slot).status());
                    }
                }
                page++;
            } while (grid.canNext());
            assertEquals(total, seen);
            assertEquals(Math.max(1, (total + 11) / 12), page);
            assertFalse(grid.canNext());
            assertEquals(page - 1, grid.page());
            assertThrows(IllegalArgumentException.class, () -> grid.begin("", -1));
        }
    }

    @Test void hasMoreControlsNavigationEvenForShortAndEmptyPages() {
        CatalogGridState grid = new CatalogGridState();
        long first = grid.begin("filtered", 0);
        assertTrue(grid.accept(first, new SkinPage(entries(0, 1), 0, true)));
        assertTrue(grid.canNext());
        long second = grid.begin("filtered", 1);
        assertEquals(0, grid.page(), "do not relabel an old page before confirmation");
        assertTrue(grid.accept(second, new SkinPage(Collections.<SkinEntry>emptyList(), 1, true)));
        assertTrue(grid.canPrevious());
        assertTrue(grid.canNext());
        long last = grid.begin("filtered", 2);
        assertTrue(grid.accept(last, new SkinPage(entries(24, 12), 2, false)));
        assertFalse(grid.canNext(), "a full final page does not imply another page");
        assertEquals("entry-24", grid.slot(0).entry().id(), "response already comes paginated");
    }

    @Test void selectionRequiresCorrectReadyPreviewAndDoesNotFollowAnIndex() {
        CatalogGridState grid = new CatalogGridState();
        SkinEntry first = entry(1, SkinModel.WIDE, false);
        SkinEntry second = entry(2, SkinModel.SLIM, false);
        long token = grid.begin("", 0);
        assertTrue(grid.accept(token, new SkinPage(Arrays.asList(first, second), 0, false)));
        assertFalse(grid.select(0));
        assertFalse(grid.select(2));
        assertFalse(grid.select(-1));
        assertFalse(grid.select(12));
        assertFalse(grid.ready(token, 0, grid.slot(0).identity(), first), "remote entry alone is not a ready texture");
        ready(grid, token, 0);
        ready(grid, token, 1);
        assertNull(grid.selected(), "loading/rendering/hover do not select");
        assertTrue(grid.select(1));
        assertSame(second, grid.selected());
        assertEquals(SkinModel.SLIM, grid.selectedPreview().model());

        long refresh = grid.begin("", 0);
        assertNull(grid.selected(), "an in-flight refresh cannot apply an old texture");
        assertTrue(grid.accept(refresh, new SkinPage(Arrays.asList(second, first), 0, false)));
        ready(grid, refresh, 0);
        ready(grid, refresh, 1);
        assertSame(second, grid.selected());
        assertEquals(0, grid.selectedIndex());
        assertTrue(grid.isSelected(0));
        assertFalse(grid.isSelected(1));

        long removed = grid.begin("", 0);
        assertTrue(grid.accept(removed, new SkinPage(Collections.singletonList(first), 0, false)));
        ready(grid, removed, 0);
        assertNull(grid.selected());
        assertNull(grid.selectedIdentity());
    }

    @Test void pageAndQueryChangesClearSelectionAndInvalidateOutOfOrderWork() {
        CatalogGridState grid = new CatalogGridState();
        long original = grid.begin("", 0);
        assertTrue(grid.accept(original, new SkinPage(entries(0, 12), 0, true)));
        String originalIdentity = grid.slot(0).identity();
        ready(grid, original, 0);
        assertTrue(grid.select(0));
        long nextPage = grid.begin("", 1);
        assertNull(grid.selectedIdentity());
        assertNull(grid.slot(0).entry());
        assertFalse(grid.ready(original, 0, originalIdentity, entry(0, SkinModel.WIDE, true)));

        long queryA = grid.begin("a", 0);
        long queryAB = grid.begin("ab", 0);
        assertFalse(grid.accept(nextPage, new SkinPage(entries(12, 12), 1, true)));
        assertFalse(grid.fail(queryA));
        assertFalse(grid.accept(queryA, new SkinPage(entries(30, 1), 0, false)));
        assertTrue(grid.accept(queryAB, new SkinPage(entries(40, 1), 0, false)));
        assertEquals("entry-40", grid.slot(0).entry().id());
        assertEquals("ab", grid.query());
        assertEquals(0, grid.page());
        assertFalse(grid.accept(queryAB, new SkinPage(entries(50, 1), 0, false)), "one response per request");
    }

    @Test void failureKeepsConfirmedPageButClearsCardsAndAllowsRetryOfRequestedPage() {
        CatalogGridState grid = new CatalogGridState();
        long first = grid.begin("", 0);
        assertTrue(grid.accept(first, new SkinPage(entries(0, 12), 0, true)));
        ready(grid, first, 0);
        assertTrue(grid.select(0));
        long next = grid.begin("", 1);
        assertTrue(grid.fail(next));
        assertEquals(0, grid.page());
        assertEquals(1, grid.requestedPage());
        assertTrue(grid.failed());
        assertFalse(grid.loading());
        assertNull(grid.selected());
        assertFalse(grid.canNext());
        assertFalse(grid.canPrevious());
        for (int index = 0; index < 12; index++) assertNull(grid.slot(index).entry());
        long retry = grid.begin(grid.query(), grid.requestedPage());
        assertTrue(grid.accept(retry, new SkinPage(entries(12, 1), 1, false)));
        assertEquals(1, grid.page());
        assertFalse(grid.failed());
        assertTrue(grid.canPrevious());
    }

    @Test void wrongPageResponseDoesNotMislabelCards() {
        CatalogGridState grid = new CatalogGridState();
        long token = grid.begin("", 1);
        assertFalse(grid.accept(token, new SkinPage(entries(0, 12), 0, true)));
        assertTrue(grid.failed());
        assertNull(grid.slot(0).entry());
    }

    @Test void failedPreviewIsIsolatedAndCannotKeepAnInvisibleSelection() {
        CatalogGridState grid = new CatalogGridState();
        long token = grid.begin("", 0);
        assertTrue(grid.accept(token, new SkinPage(entries(0, 12), 0, false)));
        ready(grid, token, 0);
        ready(grid, token, 1);
        assertTrue(grid.select(0));
        assertTrue(grid.previewFailed(token, 0, grid.slot(0).identity()));
        assertFalse(grid.select(0));
        assertNull(grid.selected());
        assertEquals(CatalogGridState.Status.FAILED, grid.slot(0).status());
        assertEquals(CatalogGridState.Status.READY, grid.slot(1).status());
        assertTrue(grid.select(1));
    }

    @Test void modelReplacementRejectsPreviewForFormerEntryAndKeepsExplicitActionTarget() {
        CatalogGridState grid = new CatalogGridState();
        long token = grid.begin("", 0);
        assertTrue(grid.accept(token, new SkinPage(entries(0, 2), 0, false)));
        ready(grid, token, 0);
        ready(grid, token, 1);
        assertTrue(grid.select(0));
        SkinEntry actionTarget = grid.selected();
        String originalIdentity = grid.selectedIdentity();
        assertTrue(grid.select(1));
        assertEquals("entry-0", actionTarget.id(), "captured explicit action is independent of later selection");
        SkinEntry slim = entry(0, SkinModel.SLIM, false);
        assertEquals(0, grid.replaceEntry(token, originalIdentity, slim));
        assertEquals("entry-1", grid.selected().id(), "finishing an action must not steal selection");
        assertFalse(grid.ready(token, 0, originalIdentity, entry(0, SkinModel.WIDE, true)));
        assertFalse(grid.previewFailed(token, 0, originalIdentity));
        ready(grid, token, 0);
        assertTrue(grid.select(0));
        assertEquals(SkinModel.SLIM, grid.selectedPreview().model());
        String slimIdentity = grid.selectedIdentity();
        assertEquals(0, grid.replaceEntry(token, slimIdentity, actionTarget));
        ready(grid, token, 0);
        assertEquals(SkinModel.WIDE, grid.selected().model());
        assertEquals(0, grid.selectedIndex());
    }

    @Test void identitySeparatesModelsOriginsAndProvidersAndRejectsDuplicateSlots() {
        SkinEntry wide = entry(1, SkinModel.WIDE, false);
        SkinEntry slim = entry(1, SkinModel.SLIM, false);
        SkinEntry otherSource = new SkinEntry(wide.id(), wide.name(), "", wide.model(), wide.providerId(),
            "https://example.invalid/another.png", wide.contentHash(), null, null);
        SkinEntry otherProvider = new SkinEntry(wide.id(), wide.name(), "", wide.model(), "another",
            wide.source(), wide.contentHash(), null, null);
        assertNotEquals(CatalogGridState.identity(wide), CatalogGridState.identity(slim));
        assertNotEquals(CatalogGridState.identity(wide), CatalogGridState.identity(otherSource));
        assertNotEquals(CatalogGridState.identity(wide), CatalogGridState.identity(otherProvider));
        CatalogGridState grid = new CatalogGridState();
        long token = grid.begin("", 0);
        assertTrue(grid.accept(token, new SkinPage(Arrays.asList(wide, wide, slim), 0, false)));
        assertSame(wide, grid.slot(0).entry());
        assertSame(slim, grid.slot(1).entry());
        assertNull(grid.slot(2).entry());
    }

    @Test void tabExitOrCloseRejectsLateDownloadsWhileResizeDoesNotInvalidateSelection() {
        CatalogGridState grid = new CatalogGridState();
        long token = grid.begin("", 0);
        assertTrue(grid.accept(token, new SkinPage(entries(0, 12), 0, false)));
        String identity = grid.slot(0).identity();
        ready(grid, token, 0);
        assertTrue(grid.select(0));
        SkinEntry selected = grid.selected();
        CatalogGridLayout.calculate(AuroraSkinsLayout.calculate(320, 240));
        CatalogGridLayout.calculate(AuroraSkinsLayout.calculate(854, 480));
        assertSame(selected, grid.selected());
        assertTrue(grid.isCurrent(token));
        grid.invalidate();
        assertFalse(grid.isCurrent(token));
        assertFalse(grid.ready(token, 0, identity, entry(0, SkinModel.WIDE, true)));
        assertFalse(grid.previewFailed(token, 0, identity));
        assertFalse(grid.replaceEntry(token, identity, selected) >= 0);
        assertFalse(grid.select(0));
        assertNull(grid.selected());
        assertFalse(grid.canNext());
        assertFalse(grid.canPrevious());
        long reopened = grid.begin("", 0);
        assertTrue(grid.accept(reopened, new SkinPage(entries(0, 1), 0, false)));
        ready(grid, reopened, 0);
        assertNull(grid.selected(), "entering catalog starts without a selection");
    }

    private static void ready(CatalogGridState grid, long token, int index) {
        SkinEntry entry = grid.slot(index).entry();
        SkinEntry prepared = new SkinEntry(entry.id(), entry.name(), entry.author(), entry.model(), entry.providerId(),
            entry.source(), entry.contentHash(), Paths.get("preview-" + entry.id() + ".png"), entry.tags());
        assertTrue(grid.ready(token, index, grid.slot(index).identity(), prepared));
    }

    private static List<SkinEntry> entries(int first, int count) {
        List<SkinEntry> entries = new ArrayList<SkinEntry>();
        for (int index = first; index < first + count; index++) entries.add(entry(index, SkinModel.WIDE, false));
        return entries;
    }

    private static SkinEntry entry(int index, SkinModel model, boolean prepared) {
        return new SkinEntry("entry-" + index, "Skin " + index, "", model, "catalog",
            "https://example.invalid/skin-" + index + ".png", "hash-" + index,
            prepared ? Paths.get("skin-" + index + ".png") : null, Collections.<String>emptyList());
    }
}
