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
        search.setResponder(value -> refresh());
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
        view = value;
        selectedIndex = 0;
        refresh();
    }

    private void refresh() {
        try {
            AuroraSkinsRuntime runtime = AuroraSkinsClient.runtime();
            List<SkinEntry> source;
            if (view == View.FAVORITES) source = runtime.store().favorites();
            else if (view == View.HISTORY) source = runtime.store().history();
            else if (view == View.EXPLORE) source = Collections.emptyList();
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
            if (view == View.EXPLORE) {
                status = Component.translatable("aurora_skins.provider_unavailable").getString();
            } else if (entries.isEmpty()) {
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

    private SkinEntry selected() {
        return entries.isEmpty() ? null : entries.get(Math.max(0, Math.min(selectedIndex, entries.size() - 1)));
    }

    private void move(int direction) {
        if (entries.isEmpty()) return;
        selectedIndex = (selectedIndex + direction + entries.size()) % entries.size();
        updateControls();
    }

    private void applySelected() {
        SkinEntry entry = selected();
        if (entry == null) return;
        try {
            AuroraSkinsClient.runtime().apply(entry);
            status = Component.translatable("aurora_skins.applied", entry.name()).getString();
        } catch (Exception error) {
            status = Component.translatable("aurora_skins.apply_error").getString();
        }
    }

    private void toggleFavorite() {
        SkinEntry entry = selected();
        if (entry == null) return;
        try {
            AuroraSkinsRuntime runtime = AuroraSkinsClient.runtime();
            runtime.store().setFavorite(entry, !runtime.store().isFavorite(entry));
            if (view == View.FAVORITES) refresh(); else updateControls();
        } catch (Exception error) {
            status = Component.translatable("aurora_skins.favorite_error").getString();
        }
    }

    private void toggleModel() {
        SkinEntry entry = selected();
        SkinModel next = entry == null ? importModel : entry.model();
        next = next == SkinModel.SLIM ? SkinModel.WIDE : SkinModel.SLIM;
        importModel = next;
        if (entry != null) {
            try {
                SkinEntry updated = AuroraSkinsClient.runtime().store().importSkin(
                    entry.file(), entry.name(), next);
                AuroraSkinsClient.runtime().apply(updated);
                refresh();
            } catch (Exception error) {
                status = Component.translatable("aurora_skins.model_error").getString();
            }
        }
        updateControls();
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
        if (previousButton != null) previousButton.active = available && entries.size() > 1;
        if (nextButton != null) nextButton.active = available && entries.size() > 1;
        if (applyButton != null) applyButton.active = available;
        if (favoriteButton != null) {
            favoriteButton.active = available;
            boolean favorite = false;
            if (available) try { favorite = AuroraSkinsClient.runtime().store().isFavorite(entry); }
            catch (Exception ignored) { }
            favoriteButton.setMessage(Component.translatable(favorite
                ? "aurora_skins.unfavorite" : "aurora_skins.favorite"));
        }
        if (modelButton != null) {
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
            graphics.drawString(font, Component.literal((selectedIndex + 1) + " / " + entries.size()), textX, textY + 16, MUTED, false);
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
}
