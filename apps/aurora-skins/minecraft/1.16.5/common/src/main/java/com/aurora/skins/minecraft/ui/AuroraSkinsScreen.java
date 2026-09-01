package com.aurora.skins.minecraft.ui;

import com.aurora.core.api.Aurora;
import com.aurora.core.api.ui.AuroraPlayerPreview;
import com.aurora.skins.AuroraSkinsRuntime;
import com.aurora.skins.api.SkinEntry;
import com.aurora.skins.api.SkinModel;
import com.aurora.skins.minecraft.AuroraSkinsClient;
import com.aurora.skins.ui.AuroraSkinsLayout;
import com.mojang.blaze3d.vertex.PoseStack;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.components.EditBox;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.network.chat.TextComponent;
import net.minecraft.network.chat.TranslatableComponent;

import javax.swing.JFileChooser;
import javax.swing.filechooser.FileNameExtensionFilter;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Locale;

public final class AuroraSkinsScreen extends Screen {
    private static final int PANEL = 0xE61A112B, ACCENT = 0xFFB78CFF, TEXT = 0xFFF2EAFF, MUTED = 0xFFB9AACB;
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
    private Button favoriteButton, modelButton, previousButton, nextButton, applyButton;
    private AuroraSkinsLayout layout;

    public AuroraSkinsScreen(Screen parent) { super(new TranslatableComponent("aurora_skins.title")); this.parent = parent; }

    @Override protected void init() {
        layout = AuroraSkinsLayout.calculate(width, height);
        AuroraSkinsLayout.Slot slot = layout.navigation(0);
        addButton(new Button(slot.x(), slot.y(), slot.width(), slot.height(), new TranslatableComponent("aurora_skins.library"), b -> select(View.LIBRARY)));
        slot = layout.navigation(1);
        addButton(new Button(slot.x(), slot.y(), slot.width(), slot.height(), new TranslatableComponent("aurora_skins.favorites"), b -> select(View.FAVORITES)));
        slot = layout.navigation(2);
        addButton(new Button(slot.x(), slot.y(), slot.width(), slot.height(), new TranslatableComponent("aurora_skins.history"), b -> select(View.HISTORY)));
        slot = layout.navigation(3);
        addButton(new Button(slot.x(), slot.y(), slot.width(), slot.height(), new TranslatableComponent("aurora_skins.explore"), b -> select(View.EXPLORE)));
        slot = layout.importButton();
        addButton(new Button(slot.x(), slot.y(), slot.width(), slot.height(), new TranslatableComponent("aurora_skins.import"), b -> chooseFile()));
        slot = layout.search();
        search = new EditBox(font, slot.x(), slot.y(), slot.width(), slot.height(), new TranslatableComponent("aurora_skins.search"));
        search.setResponder(value -> refresh()); addWidget(search);
        slot = layout.previous();
        previousButton = addButton(new Button(slot.x(), slot.y(), slot.width(), slot.height(), new TextComponent("‹"), b -> move(-1)));
        slot = layout.next();
        nextButton = addButton(new Button(slot.x(), slot.y(), slot.width(), slot.height(), new TextComponent("›"), b -> move(1)));
        slot = layout.apply();
        applyButton = addButton(new Button(slot.x(), slot.y(), slot.width(), slot.height(), new TranslatableComponent("aurora_skins.apply"), b -> apply()));
        slot = layout.favorite();
        favoriteButton = addButton(new Button(slot.x(), slot.y(), slot.width(), slot.height(), new TranslatableComponent("aurora_skins.favorite"), b -> favorite()));
        slot = layout.model();
        modelButton = addButton(new Button(slot.x(), slot.y(), slot.width(), slot.height(), new TextComponent("Modelo"), b -> toggleModel()));
        slot = layout.done();
        addButton(new Button(slot.x(), slot.y(), slot.width(), slot.height(), new TranslatableComponent("gui.done"), b -> onClose()));
        refresh();
    }

    private void select(View next) { view = next; selectedIndex = 0; refresh(); }
    private void refresh() {
        try {
            AuroraSkinsRuntime runtime = AuroraSkinsClient.runtime();
            List<SkinEntry> source = view == View.FAVORITES ? runtime.store().favorites()
                : view == View.HISTORY ? runtime.store().history()
                : view == View.EXPLORE ? Collections.<SkinEntry>emptyList() : runtime.store().list();
            String query = search == null ? "" : search.getValue().trim().toLowerCase(Locale.ROOT);
            if (query.isEmpty()) entries = source;
            else { List<SkinEntry> filtered = new ArrayList<SkinEntry>(); for (SkinEntry entry : source)
                if (entry.name().toLowerCase(Locale.ROOT).contains(query)) filtered.add(entry); entries = filtered; }
            if (selectedIndex >= entries.size()) selectedIndex = Math.max(0, entries.size() - 1);
            status = view == View.EXPLORE ? new TranslatableComponent("aurora_skins.provider_unavailable").getString()
                : entries.isEmpty() ? new TranslatableComponent("aurora_skins.empty").getString() : "";
        } catch (Exception error) { entries = Collections.emptyList(); status = new TranslatableComponent("aurora_skins.read_error").getString(); }
        updateControls();
    }
    private SkinEntry selected() { return entries.isEmpty() ? null : entries.get(selectedIndex); }
    private void move(int amount) { if (!entries.isEmpty()) selectedIndex = (selectedIndex + amount + entries.size()) % entries.size(); updateControls(); }
    private void apply() { SkinEntry entry = selected(); if (entry == null) return; try {
        AuroraSkinsClient.runtime().apply(entry); status = new TranslatableComponent("aurora_skins.applied", entry.name()).getString();
    } catch (Exception error) { status = new TranslatableComponent("aurora_skins.apply_error").getString(); } }
    private void favorite() { SkinEntry entry = selected(); if (entry == null) return; try {
        AuroraSkinsRuntime runtime = AuroraSkinsClient.runtime(); runtime.store().setFavorite(entry, !runtime.store().isFavorite(entry));
        if (view == View.FAVORITES) refresh(); else updateControls();
    } catch (Exception error) { status = new TranslatableComponent("aurora_skins.favorite_error").getString(); } }
    private void toggleModel() {
        SkinEntry entry = selected(); SkinModel model = entry == null ? importModel : entry.model(); importModel = model == SkinModel.SLIM ? SkinModel.WIDE : SkinModel.SLIM;
        if (entry != null) try { SkinEntry updated = AuroraSkinsClient.runtime().store().importSkin(entry.file(), entry.name(), importModel);
            AuroraSkinsClient.runtime().apply(updated); refresh(); } catch (Exception error) { status = new TranslatableComponent("aurora_skins.model_error").getString(); }
        updateControls();
    }
    private void chooseFile() {
        status = new TranslatableComponent("aurora_skins.chooser_opening").getString(); Thread thread = new Thread(() -> { try {
            JFileChooser chooser = new JFileChooser(); chooser.setFileFilter(new FileNameExtensionFilter("PNG 64x64 / 64x32", "png"));
            if (chooser.showOpenDialog(null) == JFileChooser.APPROVE_OPTION) { Path path = chooser.getSelectedFile().toPath(); minecraft.execute(() -> importFile(path)); }
        } catch (Throwable error) { minecraft.execute(() -> status = new TranslatableComponent("aurora_skins.chooser_error").getString()); } }, "Aurora-Skins-FileChooser");
        thread.setDaemon(true); thread.start();
    }
    private void importFile(Path file) { try {
        String name = file.getFileName() == null ? "Skin importada" : file.getFileName().toString().replaceFirst("(?i)\\.png$", "");
        SkinEntry entry = AuroraSkinsClient.runtime().store().importSkin(file, name, importModel); AuroraSkinsClient.runtime().apply(entry);
        view = View.LIBRARY; refresh(); for (int index = 0; index < entries.size(); index++) if (entries.get(index).contentHash().equals(entry.contentHash())) selectedIndex = index;
        status = new TranslatableComponent("aurora_skins.imported", entry.name()).getString(); updateControls();
    } catch (Exception error) { status = error.getMessage() == null ? new TranslatableComponent("aurora_skins.import_error").getString() : error.getMessage(); } }
    private void updateControls() {
        SkinEntry entry = selected(); boolean active = entry != null;
        if (previousButton != null) previousButton.active = active && entries.size() > 1; if (nextButton != null) nextButton.active = active && entries.size() > 1;
        if (applyButton != null) applyButton.active = active; if (favoriteButton != null) { favoriteButton.active = active; boolean favorite = false;
            if (active) try { favorite = AuroraSkinsClient.runtime().store().isFavorite(entry); } catch (Exception ignored) { }
            favoriteButton.setMessage(new TranslatableComponent(favorite ? "aurora_skins.unfavorite" : "aurora_skins.favorite")); }
        if (modelButton != null) {
            String label = (active ? entry.model() : importModel) == SkinModel.SLIM ? "Slim" : "Clássico";
            modelButton.setMessage(new TextComponent(layout != null && layout.compact() ? label : "Modelo: " + label));
        }
    }
    @Override public void render(PoseStack pose, int mouseX, int mouseY, float partialTick) {
        AuroraSkinsLayout current = layout == null ? AuroraSkinsLayout.calculate(width, height) : layout;
        renderBackground(pose); fill(pose, current.panelLeft(), current.panelTop(), current.panelRight(), current.panelBottom(), PANEL);
        fill(pose, current.panelLeft(), current.panelTop(), current.panelLeft() + 3, current.panelBottom(), ACCENT);
        drawCenteredString(pose, font, title, width / 2, 14, TEXT); search.render(pose, mouseX, mouseY, partialTick);
        int previewX = current.previewX(), previewY = current.previewY(), scale = current.previewScale();
        preview.aim(mouseX, mouseY, previewX, previewY - scale, 180, 150); long now = System.nanoTime(); preview.update((now - lastFrame) / 1_000_000_000.0F); lastFrame = now;
        Aurora.services().previews().render(pose, previewX, previewY, scale, preview); SkinEntry entry = selected(); int textX = current.textX(), textY = current.textY();
        if (entry != null) { drawString(pose, font, fit(entry.name(), current.textWidth()), textX, textY, TEXT); drawString(pose, font, (selectedIndex + 1) + " / " + entries.size(), textX, textY + 16, MUTED); }
        if (!status.isEmpty()) drawString(pose, font, fit(status, current.textWidth()), textX, textY + 40, MUTED); super.render(pose, mouseX, mouseY, partialTick);
    }
    private String fit(String value, int maxWidth) { if (font.width(value) <= maxWidth) return value; String shortened = value;
        while (!shortened.isEmpty() && font.width(shortened + "…") > maxWidth) shortened = shortened.substring(0, shortened.length() - 1); return shortened + "…"; }
    @Override public void onClose() { minecraft.setScreen(parent); }
    @Override public boolean isPauseScreen() { return false; }
}
