package com.aurora.skins.minecraft.ui;

import com.aurora.core.api.Aurora;
import com.aurora.core.api.ui.AuroraPlayerPreview;
import com.aurora.skins.AuroraSkinsRuntime;
import com.aurora.skins.api.SkinEntry;
import com.aurora.skins.api.SkinModel;
import com.aurora.skins.minecraft.AuroraSkinsClient;
import com.aurora.skins.ui.AuroraSkinsLayout;
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

public final class AuroraSkinsScreen extends Screen {
    private static final int PANEL = 0xE61A112B;
    private static final int ACCENT = 0xFFB78CFF;
    private static final int TEXT = 0xFFF2EAFF;
    private static final int MUTED = 0xFFB9AACB;

    private enum View { LIBRARY, FAVORITES, HISTORY, EXPLORE }

    private final Screen parent;
    private final AuroraPlayerPreview preview = new AuroraPlayerPreview();
    private View view = View.LIBRARY;
    private List<SkinEntry> entries = Collections.emptyList();
    private int selectedIndex;
    private long lastFrame = System.nanoTime();
    private String status = "";
    private SkinModel importModel = SkinModel.WIDE;
    private EditBox search;
    private Button favoriteButton;
    private Button modelButton;
    private Button previousButton;
    private Button nextButton;
    private Button applyButton;
    private AuroraSkinsLayout layout;
    private int catalogPage;
    private int catalogRequest;
    private boolean catalogHasMore;
    private boolean busy;

    public AuroraSkinsScreen(Screen parent) {
        super(Component.translatable("aurora_skins.title"));
        this.parent = parent;
    }

    @Override protected void init() {
        layout = AuroraSkinsLayout.calculate(width, height);
        AuroraSkinsLayout.Slot library = layout.navigation(0);
        addRenderableWidget(Button.builder(Component.translatable("aurora_skins.library"),
            button -> selectView(View.LIBRARY)).bounds(library.x(), library.y(), library.width(), library.height()).build());
        AuroraSkinsLayout.Slot favorites = layout.navigation(1);
        addRenderableWidget(Button.builder(Component.translatable("aurora_skins.favorites"),
            button -> selectView(View.FAVORITES)).bounds(favorites.x(), favorites.y(), favorites.width(), favorites.height()).build());
        AuroraSkinsLayout.Slot history = layout.navigation(2);
        addRenderableWidget(Button.builder(Component.translatable("aurora_skins.history"),
            button -> selectView(View.HISTORY)).bounds(history.x(), history.y(), history.width(), history.height()).build());
        AuroraSkinsLayout.Slot explore = layout.navigation(3);
        addRenderableWidget(Button.builder(Component.translatable("aurora_skins.explore"),
            button -> selectView(View.EXPLORE)).bounds(explore.x(), explore.y(), explore.width(), explore.height()).build());
        AuroraSkinsLayout.Slot importSlot = layout.importButton();
        addRenderableWidget(Button.builder(Component.translatable("aurora_skins.import"),
            button -> chooseFile()).bounds(importSlot.x(), importSlot.y(), importSlot.width(), importSlot.height()).build());

        AuroraSkinsLayout.Slot searchSlot = layout.search();
        search = new EditBox(font, searchSlot.x(), searchSlot.y(), searchSlot.width(), searchSlot.height(),
            Component.translatable("aurora_skins.search"));
        search.setHint(Component.translatable("aurora_skins.search_hint"));
        search.setResponder(value -> {
            if (view == View.EXPLORE) catalogPage = 0;
            refresh();
        });
        addRenderableWidget(search);

        AuroraSkinsLayout.Slot previousSlot = layout.previous();
        previousButton = addRenderableWidget(Button.builder(Component.literal("‹"),
            button -> move(-1)).bounds(previousSlot.x(), previousSlot.y(), previousSlot.width(), previousSlot.height()).build());
        AuroraSkinsLayout.Slot nextSlot = layout.next();
        nextButton = addRenderableWidget(Button.builder(Component.literal("›"),
            button -> move(1)).bounds(nextSlot.x(), nextSlot.y(), nextSlot.width(), nextSlot.height()).build());
        AuroraSkinsLayout.Slot applySlot = layout.apply();
        applyButton = addRenderableWidget(Button.builder(Component.translatable("aurora_skins.apply"),
            button -> applySelected()).bounds(applySlot.x(), applySlot.y(), applySlot.width(), applySlot.height()).build());
        AuroraSkinsLayout.Slot favoriteSlot = layout.favorite();
        favoriteButton = addRenderableWidget(Button.builder(Component.translatable("aurora_skins.favorite"),
            button -> toggleFavorite()).bounds(favoriteSlot.x(), favoriteSlot.y(), favoriteSlot.width(), favoriteSlot.height()).build());
        AuroraSkinsLayout.Slot modelSlot = layout.model();
        modelButton = addRenderableWidget(Button.builder(Component.empty(),
            button -> toggleModel()).bounds(modelSlot.x(), modelSlot.y(), modelSlot.width(), modelSlot.height()).build());
        AuroraSkinsLayout.Slot doneSlot = layout.done();
        addRenderableWidget(Button.builder(Component.translatable("gui.done"), button -> onClose())
            .bounds(doneSlot.x(), doneSlot.y(), doneSlot.width(), doneSlot.height()).build());
        refresh();
    }

    private void selectView(View value) {
        catalogRequest++;
        view = value;
        selectedIndex = 0;
        if (view == View.EXPLORE) catalogPage = 0;
        refresh();
    }

    private void refresh() {
        if (view == View.EXPLORE) {
            refreshCatalog();
            return;
        }
        busy = false;
        try {
            AuroraSkinsRuntime runtime = AuroraSkinsClient.runtime();
            List<SkinEntry> source;
            if (view == View.FAVORITES) source = runtime.store().favorites();
            else if (view == View.HISTORY) source = runtime.store().history();
            else source = runtime.store().list();
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
            if (selectedIndex >= entries.size()) selectedIndex = Math.max(0, entries.size() - 1);
            if (entries.isEmpty()) {
                status = Component.translatable("aurora_skins.empty").getString();
            } else {
                status = "";
            }
            updateControls();
        } catch (Exception error) {
            entries = Collections.emptyList();
            status = Component.translatable("aurora_skins.read_error").getString();
            updateControls();
        }
    }

    private void refreshCatalog() {
        final int request = ++catalogRequest;
        final String query = search == null ? "" : search.getValue().trim();
        busy = true;
        status = Component.translatable("aurora_skins.catalog_loading").getString();
        updateControls();
        AuroraSkinsRuntime runtime = AuroraSkinsClient.runtime();
        runtime.catalogProvider().search(query, catalogPage, 12).whenComplete((page, error) ->
            minecraft.execute(() -> {
                if (request != catalogRequest || view != View.EXPLORE) return;
                busy = false;
                if (error != null || page == null) {
                    entries = Collections.emptyList();
                    catalogHasMore = false;
                    status = Component.translatable("aurora_skins.catalog_error").getString();
                } else {
                    entries = page.entries();
                    catalogPage = page.page();
                    catalogHasMore = page.hasMore();
                    selectedIndex = Math.min(selectedIndex, Math.max(0, entries.size() - 1));
                    if (entries.isEmpty()) {
                        status = Component.translatable("aurora_skins.catalog_empty").getString();
                    } else {
                        status = Component.translatable(runtime.catalogProvider().online()
                            ? "aurora_skins.catalog_online" : "aurora_skins.catalog_offline").getString();
                    }
                }
                updateControls();
            }));
    }

    private SkinEntry selected() {
        return entries.isEmpty() ? null : entries.get(Math.max(0, Math.min(selectedIndex, entries.size() - 1)));
    }

    private void move(int direction) {
        if (entries.isEmpty()) return;
        if (view == View.EXPLORE) {
            if (direction > 0 && selectedIndex == entries.size() - 1 && catalogHasMore) {
                catalogPage++;
                selectedIndex = 0;
                refreshCatalog();
                return;
            }
            if (direction < 0 && selectedIndex == 0 && catalogPage > 0) {
                catalogPage--;
                selectedIndex = 0;
                refreshCatalog();
                return;
            }
        }
        selectedIndex = (selectedIndex + direction + entries.size()) % entries.size();
        updateControls();
    }

    private void applySelected() {
        SkinEntry entry = selected();
        if (entry == null) return;
        withInstalled(entry, "aurora_skins.apply_error", installed -> {
            AuroraSkinsClient.runtime().apply(installed);
            replaceSelected(installed);
            status = Component.translatable("aurora_skins.applied", installed.name()).getString();
        });
    }

    private void toggleFavorite() {
        SkinEntry entry = selected();
        if (entry == null) return;
        withInstalled(entry, "aurora_skins.favorite_error", installed -> {
            AuroraSkinsRuntime runtime = AuroraSkinsClient.runtime();
            runtime.store().setFavorite(installed, !runtime.store().isFavorite(installed));
            replaceSelected(installed);
            status = "";
            if (view == View.FAVORITES) refresh();
        });
    }

    private void toggleModel() {
        SkinEntry entry = selected();
        SkinModel next = entry == null ? importModel : entry.model();
        next = next == SkinModel.SLIM ? SkinModel.WIDE : SkinModel.SLIM;
        importModel = next;
        if (entry != null) {
            final SkinModel selectedModel = next;
            withInstalled(entry, "aurora_skins.model_error", installed -> {
                SkinEntry updated = AuroraSkinsClient.runtime().store().importSkin(
                    installed.file(), installed.name(), selectedModel);
                AuroraSkinsClient.runtime().apply(updated);
                replaceSelected(updated);
                status = "";
            });
        }
        updateControls();
    }

    private void withInstalled(SkinEntry entry, String errorKey, InstalledAction action) {
        busy = true;
        status = Component.translatable("aurora_skins.downloading").getString();
        updateControls();
        AuroraSkinsClient.runtime().materialize(entry).whenComplete((installed, error) ->
            minecraft.execute(() -> {
                try {
                    if (error != null || installed == null) throw new IllegalStateException();
                    action.run(installed);
                } catch (Exception failure) {
                    status = Component.translatable(errorKey).getString();
                } finally {
                    busy = false;
                    updateControls();
                }
            }));
    }

    private void replaceSelected(SkinEntry installed) {
        if (entries.isEmpty()) return;
        List<SkinEntry> updated = new ArrayList<SkinEntry>(entries);
        updated.set(Math.max(0, Math.min(selectedIndex, updated.size() - 1)), installed);
        entries = updated;
    }

    private void chooseFile() {
        status = Component.translatable("aurora_skins.chooser_opening").getString();
        Thread chooserThread = new Thread(() -> {
            try {
                JFileChooser chooser = new JFileChooser();
                chooser.setDialogTitle(Component.translatable("aurora_skins.import").getString());
                chooser.setFileFilter(new FileNameExtensionFilter("PNG 64x64 / 64x32", "png"));
                if (chooser.showOpenDialog(null) != JFileChooser.APPROVE_OPTION) return;
                Path selectedFile = chooser.getSelectedFile().toPath();
                minecraft.execute(() -> importFile(selectedFile));
            } catch (Throwable error) {
                minecraft.execute(() -> status = Component.translatable("aurora_skins.chooser_error").getString());
            }
        }, "Aurora-Skins-FileChooser");
        chooserThread.setDaemon(true);
        chooserThread.start();
    }

    private void importFile(Path file) {
        try {
            String fileName = file.getFileName() == null ? "Skin importada" : file.getFileName().toString();
            int dot = fileName.lastIndexOf('.');
            if (dot > 0) fileName = fileName.substring(0, dot);
            SkinEntry entry = AuroraSkinsClient.runtime().store().importSkin(file, fileName, importModel);
            AuroraSkinsClient.runtime().apply(entry);
            view = View.LIBRARY;
            refresh();
            for (int index = 0; index < entries.size(); index++) {
                if (entries.get(index).contentHash().equals(entry.contentHash())) selectedIndex = index;
            }
            status = Component.translatable("aurora_skins.imported", entry.name()).getString();
            updateControls();
        } catch (Exception error) {
            status = error.getMessage() == null
                ? Component.translatable("aurora_skins.import_error").getString() : error.getMessage();
        }
    }

    private void updateControls() {
        SkinEntry entry = selected();
        boolean available = entry != null;
        if (previousButton != null) previousButton.active = !busy && available
            && (entries.size() > 1 || view == View.EXPLORE && catalogPage > 0);
        if (nextButton != null) nextButton.active = !busy && available
            && (entries.size() > 1 || view == View.EXPLORE && catalogHasMore);
        if (applyButton != null) applyButton.active = !busy && available;
        if (favoriteButton != null) {
            favoriteButton.active = !busy && available;
            boolean favorite = false;
            if (available) try { favorite = AuroraSkinsClient.runtime().store().isFavorite(entry); }
            catch (Exception ignored) { }
            favoriteButton.setMessage(Component.translatable(favorite
                ? "aurora_skins.unfavorite" : "aurora_skins.favorite"));
        }
        if (modelButton != null) {
            modelButton.active = !busy;
            SkinModel current = available ? entry.model() : importModel;
            String label = current == SkinModel.SLIM ? "Slim" : "Clássico";
            modelButton.setMessage(Component.literal(layout != null && layout.compact() ? label : "Modelo: " + label));
        }
    }

    @Override public void render(GuiGraphics graphics, int mouseX, int mouseY, float partialTick) {
        graphics.fill(0, 0, width, height, 0xD0100818);
        AuroraSkinsLayout current = layout == null ? AuroraSkinsLayout.calculate(width, height) : layout;
        graphics.fill(current.panelLeft(), current.panelTop(), current.panelRight(), current.panelBottom(), PANEL);
        graphics.fill(current.panelLeft(), current.panelTop(), current.panelLeft() + 3, current.panelBottom(), ACCENT);
        graphics.drawCenteredString(font, title, width / 2, 14, TEXT);

        int previewX = current.previewX();
        int previewY = current.previewY();
        int scale = current.previewScale();
        preview.aim(mouseX, mouseY, previewX, previewY - scale, 180.0F, 150.0F);
        long now = System.nanoTime();
        preview.update((now - lastFrame) / 1_000_000_000.0F);
        lastFrame = now;
        Aurora.services().previews().render(graphics, previewX, previewY, scale, preview);

        SkinEntry entry = selected();
        int textX = current.textX();
        int textY = current.textY();
        if (entry != null) {
            graphics.drawString(font, fit(entry.name(), current.textWidth()), textX, textY, TEXT, false);
            String position = view == View.EXPLORE
                ? "Página " + (catalogPage + 1) + " · " + (selectedIndex + 1) + " / " + entries.size()
                : (selectedIndex + 1) + " / " + entries.size();
            graphics.drawString(font, Component.literal(position), textX, textY + 16, MUTED, false);
            graphics.drawString(font, Component.literal(entry.model() == SkinModel.SLIM ? "Slim" : "Clássico"), textX, textY + 32, MUTED, false);
        }
        if (!status.isEmpty()) graphics.drawString(font, fit(status, current.textWidth()), textX, textY + 54, MUTED, false);
        super.render(graphics, mouseX, mouseY, partialTick);
    }

    private String fit(String value, int maxWidth) {
        if (font.width(value) <= maxWidth) return value;
        String shortened = value;
        while (!shortened.isEmpty() && font.width(shortened + "…") > maxWidth) {
            shortened = shortened.substring(0, shortened.length() - 1);
        }
        return shortened + "…";
    }

    @Override public void onClose() { minecraft.setScreen(parent); }
    @Override public boolean isPauseScreen() { return false; }

    private interface InstalledAction { void run(SkinEntry entry) throws Exception; }
}
