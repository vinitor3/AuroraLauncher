package com.aurora.skins.minecraft.ui;

import com.aurora.core.api.ui.AuroraPlayerPreview;
import com.aurora.skins.AuroraSkinsRuntime;
import com.aurora.skins.api.SkinEntry;
import com.aurora.skins.api.SkinModel;
import com.aurora.skins.minecraft.AuroraSkinsClient;
import com.aurora.skins.ui.AuroraSkinsLayout;
import com.aurora.skins.ui.CatalogGridLayout;
import com.aurora.skins.ui.CatalogGridState;
import net.minecraft.client.gui.GuiGraphics;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.components.EditBox;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.network.chat.Component;
import javax.swing.JFileChooser;
import javax.swing.filechooser.FileNameExtensionFilter;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Locale;
import java.util.concurrent.CompletableFuture;

public final class AuroraSkinsScreen extends Screen {
    private static final int TEXT = 0xFFF2EAFF, MUTED = 0xFFB9AACB;
    private enum View { LIBRARY, FAVORITES, HISTORY, EXPLORE }
    private final Screen parent;
    private final AuroraPlayerPreview preview = new AuroraPlayerPreview();
    private final MinecraftSkinPreviewRenderer selectionPreview = new MinecraftSkinPreviewRenderer();
    private final CatalogGridState catalog = new CatalogGridState();
    private final MinecraftSkinPreviewRenderer[] cardPreviews = new MinecraftSkinPreviewRenderer[CatalogGridState.PAGE_SIZE];
    private final List<CompletableFuture<?>> cardRequests = new ArrayList<CompletableFuture<?>>();
    private String previewKey = "";
    private int previewRequest, actionRequest;
    private View view = View.LIBRARY;
    private List<SkinEntry> entries = Collections.emptyList();
    private int selectedIndex;
    private long lastFrame = System.nanoTime();
    private String status = "";
    private SkinModel importModel = SkinModel.WIDE;
    private EditBox search;
    private Button favoriteButton, modelButton, previousButton, nextButton, applyButton;
    private AuroraSkinsLayout layout;
    private CatalogGridLayout gridLayout;
    private boolean busy, initialized, closed;

    public AuroraSkinsScreen(Screen parent) { super(translated("aurora_skins.title")); this.parent = parent; }

    @Override protected void init() {
        String query = search == null ? "" : search.getValue();
        layout = AuroraSkinsLayout.calculate(width, height);
        gridLayout = CatalogGridLayout.calculate(layout);
        addControl(layout.navigation(0), translated("aurora_skins.library"), button -> selectView(View.LIBRARY));
        addControl(layout.navigation(1), translated("aurora_skins.favorites"), button -> selectView(View.FAVORITES));
        addControl(layout.navigation(2), translated("aurora_skins.history"), button -> selectView(View.HISTORY));
        addControl(layout.navigation(3), translated("aurora_skins.explore"), button -> selectView(View.EXPLORE));
        addControl(layout.importButton(), translated("aurora_skins.import"), button -> chooseFile());
        AuroraSkinsLayout.Slot slot = layout.search();
        search = new EditBox(font, slot.x(), slot.y(), slot.width(), slot.height(), translated("aurora_skins.search"));
        search.setMaxLength(80);
        // Resize rebuilds widgets only; preserve the query, page and live textures.
        search.setValue(query);
        search.setResponder(value -> { invalidateAction(); if (view == View.EXPLORE) refreshCatalog(0); else refresh(); });
        addRenderableWidget(search);
        previousButton = addControl(layout.previous(), literal("‹"), button -> move(-1));
        nextButton = addControl(layout.next(), literal("›"), button -> move(1));
        applyButton = addControl(layout.apply(), translated("aurora_skins.apply"), button -> applySelected());
        favoriteButton = addControl(layout.favorite(), translated("aurora_skins.favorite"), button -> toggleFavorite());
        modelButton = addControl(layout.model(), literal("Modelo"), button -> toggleModel());
        addControl(layout.done(), translated("gui.done"), button -> onClose());
        if (!initialized || closed) { initialized = true; closed = false; refresh(); }
        else updateControls();
    }

    private Button addControl(AuroraSkinsLayout.Slot slot, Component message, Button.OnPress action) {
        return addRenderableWidget(Button.builder(message, action).bounds(slot.x(), slot.y(), slot.width(), slot.height()).build());
    }
    private static Component translated(String key, Object... arguments) { return Component.translatable(key, arguments); }
    private static Component literal(String value) { return Component.literal(value); }

    private void selectView(View value) {
        invalidateAction();
        catalog.invalidate();
        releaseCards();
        clearSelectionPreview();
        AuroraSkinsClient.runtime().catalogProvider().cancelSearch();
        view = value; selectedIndex = 0;
        if (view == View.EXPLORE) refreshCatalog(0); else refresh();
    }
    private void refresh() {
        if (view == View.EXPLORE) { refreshCatalog(catalog.page()); return; }
        try {
            AuroraSkinsRuntime runtime = AuroraSkinsClient.runtime();
            List<SkinEntry> source = view == View.FAVORITES ? runtime.store().favorites()
                : view == View.HISTORY ? runtime.store().history() : runtime.store().list();
            String query = search == null ? "" : search.getValue().trim().toLowerCase(Locale.ROOT);
            if (query.isEmpty()) entries = source;
            else {
                List<SkinEntry> filtered = new ArrayList<SkinEntry>();
                for (SkinEntry entry : source) {
                    if (entry.name().toLowerCase(Locale.ROOT).contains(query)
                        || entry.author().toLowerCase(Locale.ROOT).contains(query)) filtered.add(entry);
                }
                entries = filtered;
            }
            selectedIndex = Math.max(0, Math.min(selectedIndex, entries.size() - 1));
            status = entries.isEmpty() ? translated("aurora_skins.empty").getString() : "";
        } catch (Exception error) { entries = Collections.emptyList(); status = translated("aurora_skins.read_error").getString(); }
        updateControls();
    }
    private void refreshCatalog(int pageIndex) {
        invalidateAction();
        final String query = search == null ? "" : search.getValue().trim();
        final long request = catalog.begin(query, pageIndex);
        releaseCards(); clearSelectionPreview();
        status = translated("aurora_skins.catalog_loading").getString(); updateControls();
        AuroraSkinsRuntime runtime = AuroraSkinsClient.runtime();
        runtime.catalogProvider().searchDebounced(query, pageIndex, CatalogGridState.PAGE_SIZE)
            .whenComplete((page, error) -> minecraft.execute(() -> {
                if (closed || view != View.EXPLORE || !catalog.isCurrent(request) || minecraft.screen != this) return;
                if (error != null || page == null) {
                    catalog.fail(request); status = "Erro no catálogo. Clique aqui para tentar novamente.";
                } else if (catalog.accept(request, page)) {
                    status = page.entries().isEmpty() ? translated("aurora_skins.catalog_empty").getString()
                        : translated(runtime.catalogProvider().online()
                            ? "aurora_skins.catalog_online" : "aurora_skins.catalog_offline").getString();
                    for (int index = 0; index < CatalogGridState.PAGE_SIZE; index++) loadCard(request, index);
                }
                updateControls();
            }));
    }
    private void loadCard(final long request, final int index) {
        final CatalogGridState.Slot slot = catalog.slot(index);
        final SkinEntry entry = slot.entry();
        if (entry == null) return;
        final String identity = slot.identity();
        CompletableFuture<SkinEntry> future = AuroraSkinsClient.runtime().catalogProvider().preview(entry);
        cardRequests.add(future);
        future.whenComplete((prepared, error) -> minecraft.execute(() -> {
            cardRequests.remove(future);
            if (closed || view != View.EXPLORE || minecraft.screen != this || !catalog.isCurrent(request)
                || !identity.equals(catalog.slot(index).identity())) return;
            if (error != null || prepared == null) { failCard(request, index, identity); return; }
            MinecraftSkinPreviewRenderer renderer = new MinecraftSkinPreviewRenderer();
            try {
                renderer.setSkin(prepared);
                if (!renderer.isReady() || !catalog.ready(request, index, identity, prepared)) {
                    renderer.close(); failCard(request, index, identity); return;
                }
                if (cardPreviews[index] != null) cardPreviews[index].close();
                cardPreviews[index] = renderer;
            } catch (Exception failure) { renderer.close(); failCard(request, index, identity); }
            updateControls();
        }));
    }
    private void failCard(long request, int index, String identity) {
        if (catalog.previewFailed(request, index, identity)) {
            if (cardPreviews[index] != null) cardPreviews[index].close();
            cardPreviews[index] = null;
            status = "Algumas prévias falharam. Clique aqui para tentar novamente.";
            updateControls();
        }
    }
    private void releaseCards() {
        // Cancel only this screen's deliveries; the provider owns shared downloads.
        CompletableFuture<?>[] pendingRequests = cardRequests.toArray(new CompletableFuture<?>[0]);
        cardRequests.clear();
        for (CompletableFuture<?> pending : pendingRequests) pending.cancel(false);
        for (int index = 0; index < cardPreviews.length; index++) {
            if (cardPreviews[index] != null) cardPreviews[index].close();
            cardPreviews[index] = null;
        }
    }
    private boolean hasFailedCard() {
        for (int index = 0; index < CatalogGridState.PAGE_SIZE; index++)
            if (catalog.slot(index).status() == CatalogGridState.Status.FAILED) return true;
        return false;
    }
    private SkinEntry selected() {
        if (view == View.EXPLORE) return catalog.selectedPreview();
        return entries.isEmpty() ? null : entries.get(Math.max(0, Math.min(selectedIndex, entries.size() - 1)));
    }
    private void move(int direction) {
        if (busy) return;
        if (view == View.EXPLORE) {
            if (direction < 0 && catalog.canPrevious()) refreshCatalog(catalog.page() - 1);
            else if (direction > 0 && catalog.canNext()) refreshCatalog(catalog.page() + 1);
            return;
        }
        if (entries.isEmpty()) return;
        selectedIndex = (selectedIndex + direction + entries.size()) % entries.size(); updateControls();
    }
    private void applySelected() {
        final SkinEntry entry = selected(); if (entry == null) return;
        withInstalled(entry, "aurora_skins.apply_error", installed -> {
            AuroraSkinsClient.runtime().apply(installed); replaceLocalEntry(entry, installed);
            status = translated("aurora_skins.applied", installed.name()).getString();
        });
    }
    private void toggleFavorite() {
        final SkinEntry entry = selected(); if (entry == null) return;
        withInstalled(entry, "aurora_skins.favorite_error", installed -> {
            AuroraSkinsRuntime runtime = AuroraSkinsClient.runtime();
            runtime.store().setFavorite(installed, !runtime.store().isFavorite(installed));
            replaceLocalEntry(entry, installed); status = "";
            if (view == View.FAVORITES) refresh();
        });
    }
    private void toggleModel() {
        final SkinEntry entry = selected();
        SkinModel current = entry == null ? importModel : entry.model();
        final SkinModel next = current == SkinModel.SLIM ? SkinModel.WIDE : SkinModel.SLIM;
        importModel = next;
        final String originalIdentity = view == View.EXPLORE ? catalog.selectedIdentity() : "";
        final long request = catalog.generation();
        if (entry != null) withInstalled(entry, "aurora_skins.model_error", installed -> {
            SkinEntry updated = AuroraSkinsClient.runtime().store().importSkin(installed.file(), installed.name(), next);
            AuroraSkinsClient.runtime().apply(updated);
            if (view == View.EXPLORE) {
                int index = catalog.replaceEntry(request, originalIdentity, updated);
                if (index >= 0) {
                    if (cardPreviews[index] != null) cardPreviews[index].close();
                    cardPreviews[index] = null; loadCard(request, index);
                }
            } else replaceLocalEntry(entry, updated);
            status = "";
        });
        updateControls();
    }
    private void withInstalled(SkinEntry entry, String errorKey, InstalledAction action) {
        if (busy) return;
        final int request = ++actionRequest;
        busy = true; status = translated("aurora_skins.downloading").getString(); updateControls();
        // Captured entry/action: no completion writes to the newly selected index.
        AuroraSkinsClient.runtime().materialize(entry).whenComplete((installed, error) -> minecraft.execute(() -> {
            if (request != actionRequest || closed || minecraft.screen != this) return;
            try {
                if (error != null || installed == null) throw new IllegalStateException();
                action.run(installed);
            } catch (Exception failure) { status = translated(errorKey).getString(); }
            finally { busy = false; updateControls(); }
        }));
    }
    private void replaceLocalEntry(SkinEntry original, SkinEntry installed) {
        if (view == View.EXPLORE) return;
        String identity = CatalogGridState.identity(original);
        List<SkinEntry> updated = new ArrayList<SkinEntry>(entries);
        for (int index = 0; index < updated.size(); index++)
            if (identity.equals(CatalogGridState.identity(updated.get(index)))) { updated.set(index, installed); break; }
        entries = updated;
    }
    private void invalidateAction() { actionRequest++; busy = false; }
    private void chooseFile() {
        invalidateAction();
        final int request = actionRequest;
        final SkinModel model = importModel;
        status = translated("aurora_skins.chooser_opening").getString();
        updateControls();
        Thread thread = new Thread(() -> {
            try {
                JFileChooser chooser = new JFileChooser();
                chooser.setDialogTitle(translated("aurora_skins.import").getString());
                chooser.setFileFilter(new FileNameExtensionFilter("PNG 64x64 / 64x32", "png"));
                if (chooser.showOpenDialog(null) != JFileChooser.APPROVE_OPTION) return;
                Path file = chooser.getSelectedFile().toPath();
                minecraft.execute(() -> { if (!closed && request == actionRequest && minecraft.screen == this) importFile(file, model); });
            } catch (Throwable error) {
                minecraft.execute(() -> { if (!closed && request == actionRequest && minecraft.screen == this)
                    status = translated("aurora_skins.chooser_error").getString(); });
            }
        }, "Aurora-Skins-FileChooser");
        thread.setDaemon(true); thread.start();
    }
    private void importFile(Path file, SkinModel model) {
        try {
            String name = file.getFileName() == null ? "Skin importada" : file.getFileName().toString();
            int dot = name.lastIndexOf('.'); if (dot > 0) name = name.substring(0, dot);
            SkinEntry entry = AuroraSkinsClient.runtime().store().importSkin(file, name, model);
            AuroraSkinsClient.runtime().apply(entry); selectView(View.LIBRARY);
            for (int index = 0; index < entries.size(); index++)
                if (entries.get(index).contentHash().equals(entry.contentHash())) selectedIndex = index;
            status = translated("aurora_skins.imported", entry.name()).getString(); updateControls();
        } catch (Exception error) { status = translated("aurora_skins.import_error").getString(); }
    }
    private void clearSelectionPreview() { previewRequest++; previewKey = ""; selectionPreview.close(); }
    private void refreshPreview() {
        if (view == View.EXPLORE) return;
        final SkinEntry entry = selected();
        String key = entry == null ? "" : CatalogGridState.identity(entry);
        if (previewKey.equals(key)) return;
        previewKey = key;
        final int request = ++previewRequest;
        selectionPreview.close();
        if (entry == null) return;
        AuroraSkinsClient.runtime().catalogProvider().preview(entry).whenComplete((prepared, error) -> minecraft.execute(() -> {
            if (closed || request != previewRequest || view == View.EXPLORE || minecraft.screen != this) return;
            if (error != null || prepared == null) { status = "Não foi possível carregar a prévia."; return; }
            try { selectionPreview.setSkin(prepared); }
            catch (Exception failure) { status = "Não foi possível carregar a prévia."; }
        }));
    }
    private void updateControls() {
        if (!busy) refreshPreview();
        SkinEntry entry = selected(); boolean available = entry != null;
        boolean blocked = busy || view == View.EXPLORE && catalog.loading();
        if (previousButton != null) previousButton.active = !busy && (view == View.EXPLORE ? catalog.canPrevious() : entries.size() > 1);
        if (nextButton != null) nextButton.active = !busy && (view == View.EXPLORE ? catalog.canNext() : entries.size() > 1);
        if (applyButton != null) applyButton.active = !blocked && available;
        if (favoriteButton != null) {
            favoriteButton.active = !blocked && available;
            boolean favorite = false;
            if (available) try { favorite = AuroraSkinsClient.runtime().store().isFavorite(entry); } catch (Exception ignored) { }
            favoriteButton.setMessage(translated(favorite ? "aurora_skins.unfavorite" : "aurora_skins.favorite"));
        }
        if (modelButton != null) {
            modelButton.active = !blocked;
            String label = (available ? entry.model() : importModel) == SkinModel.SLIM ? "Slim" : "Clássico";
            modelButton.setMessage(literal(layout != null && layout.compact() ? label : "Modelo: " + label));
        }
    }
    @Override public boolean mouseClicked(double mouseX, double mouseY, int button) {
        if (view == View.EXPLORE && button == 0 && gridLayout != null) {
            int index = gridLayout.hit(mouseX, mouseY);
            if (index >= 0) { if (catalog.select(index)) updateControls(); return true; }
            if (!busy && !catalog.loading() && (catalog.failed() || hasFailedCard())
                && mouseX >= gridLayout.statusX() && mouseX < gridLayout.statusX() + gridLayout.statusWidth()
                && mouseY >= gridLayout.statusY() && mouseY < gridLayout.statusY() + 10) {
                refreshCatalog(catalog.failed() ? catalog.requestedPage() : catalog.page()); return true;
            }
        }
        return super.mouseClicked(mouseX, mouseY, button);
    }
    @Override public void render(GuiGraphics graphics, int mouseX, int mouseY, float partialTick) {
        fillUi(graphics, 0, 0, width, height, 0xE6101018);
        centeredUi(graphics, title, width / 2, 14, TEXT);
        if (view == View.EXPLORE) renderCatalog(graphics, mouseX, mouseY);
        else renderSelection(graphics, mouseX, mouseY);
        super.render(graphics, mouseX, mouseY, partialTick);
    }
    private void renderCatalog(GuiGraphics graphics, int mouseX, int mouseY) {
        if (!layout.compact()) textUi(graphics, "Tema, #tag ou @jogador / @UUID", layout.search().x(), layout.search().y() - 10, MUTED);
        int hovered = gridLayout.hit(mouseX, mouseY);
        for (int index = 0; index < CatalogGridState.PAGE_SIZE; index++) {
            AuroraSkinsLayout.Slot bounds = gridLayout.slot(index);
            CatalogGridState.Slot slot = catalog.slot(index);
            boolean ready = slot.status() == CatalogGridState.Status.READY;
            int border = catalog.isSelected(index) ? 0xFFFFD35B : ready && hovered == index ? 0xFF9A8DB0 : 0xFF48404F;
            fillUi(graphics, bounds.x(), bounds.y(), bounds.right(), bounds.bottom(), border);
            fillUi(graphics, bounds.x() + 1, bounds.y() + 1, bounds.right() - 1, bounds.bottom() - 1, 0xFF24212A);
            MinecraftSkinPreviewRenderer renderer = cardPreviews[index];
            if (ready && renderer != null) {
                if (!renderer.renderCard(graphics, gridLayout.modelX(index), gridLayout.modelY(index), gridLayout.modelScale(index),
                        bounds.x() + 1, bounds.y() + 1, bounds.right() - 1, bounds.bottom() - 1))
                    failCard(catalog.generation(), index, slot.identity());
            } else if (slot.status() == CatalogGridState.Status.LOADING || slot.status() == CatalogGridState.Status.FAILED) {
                // Neutral marker; never render the equipped/default player for an unresolved entry.
                int x = bounds.x() + bounds.width() / 2, y = bounds.y() + bounds.height() / 2;
                fillUi(graphics, x - 3, y - 1, x + 3, y + 1,
                    slot.status() == CatalogGridState.Status.FAILED ? 0xFFB76B71 : 0xFF77717F);
            }
        }
        String message = "Página " + (catalog.page() + 1);
        if (catalog.failed()) message += " · Erro — clique para tentar novamente";
        else if (hasFailedCard()) message += " · Prévia falhou — clique para tentar";
        else if (!status.isEmpty()) message += " · " + status;
        textUi(graphics, fit(message, gridLayout.statusWidth()), gridLayout.statusX(), gridLayout.statusY(), MUTED);
    }
    private void renderSelection(GuiGraphics graphics, int mouseX, int mouseY) {
        int x = layout.previewX(), y = layout.previewY(), scale = layout.previewScale();
        preview.aim(mouseX, mouseY, x, y - scale, 180, 150);
        long now = System.nanoTime(); preview.update((now - lastFrame) / 1_000_000_000.0F); lastFrame = now;
        selectionPreview.render(graphics, x, y, scale, preview);
        SkinEntry entry = selected(); int textX = layout.textX(), textY = layout.textY();
        if (entry != null) {
            textUi(graphics, fit(entry.name(), layout.textWidth()), textX, textY, TEXT);
            textUi(graphics, (selectedIndex + 1) + " / " + entries.size(), textX, textY + 16, MUTED);
            textUi(graphics, entry.model() == SkinModel.SLIM ? "Slim" : "Clássico", textX, textY + 32, MUTED);
        }
        if (!status.isEmpty()) textUi(graphics, fit(status, layout.textWidth()), textX, textY + 54, MUTED);
    }
    private void fillUi(GuiGraphics graphics, int left, int top, int right, int bottom, int color) { graphics.fill(left, top, right, bottom, color); }
    private void textUi(GuiGraphics graphics, String value, int x, int y, int color) { graphics.drawString(font, value, x, y, color, false); }
    private void centeredUi(GuiGraphics graphics, Component value, int x, int y, int color) { graphics.drawCenteredString(font, value, x, y, color); }

    // Shared with 1.21: Screen.render otherwise blurs over cards drawn above.
    // No @Override: the four-argument hook does not exist in 1.20.1.
    public void renderBackground(GuiGraphics graphics, int mouseX, int mouseY, float partialTick) { }

    private String fit(String value, int maxWidth) {
        if (font.width(value) <= maxWidth) return value;
        String shortened = value;
        while (!shortened.isEmpty() && font.width(shortened + "…") > maxWidth) shortened = shortened.substring(0, shortened.length() - 1);
        return shortened + "…";
    }
    private void closeResources() {
        if (closed) return;
        closed = true; invalidateAction(); catalog.invalidate(); releaseCards(); clearSelectionPreview();
        AuroraSkinsClient.runtime().catalogProvider().cancelSearch();
    }
    @Override public void onClose() { closeResources(); minecraft.setScreen(parent); }
    @Override public void removed() { closeResources(); super.removed(); }
    @Override public boolean isPauseScreen() { return false; }
    private interface InstalledAction { void run(SkinEntry entry) throws Exception; }
}
