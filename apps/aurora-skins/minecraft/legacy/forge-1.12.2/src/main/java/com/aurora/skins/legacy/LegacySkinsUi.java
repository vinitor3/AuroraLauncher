package com.aurora.skins.legacy;

import com.aurora.core.api.Aurora;
import com.aurora.core.api.ui.AuroraPlayerPreview;
import com.aurora.core.api.ui.AuroraUiContext;
import com.aurora.skins.AuroraSkinsRuntime;
import com.aurora.skins.api.SkinEntry;
import com.aurora.skins.api.SkinModel;
import com.aurora.skins.ui.AuroraSkinsLayout;
import com.aurora.skins.ui.AuroraTitleLayout;
import com.aurora.skins.ui.CatalogGridLayout;
import com.aurora.skins.ui.CatalogGridState;
import net.minecraftforge.client.event.GuiScreenEvent;
import net.minecraftforge.fml.common.eventhandler.SubscribeEvent;
import net.minecraftforge.fml.common.gameevent.TickEvent;

import javax.swing.JFileChooser;
import javax.swing.filechooser.FileNameExtensionFilter;
import java.lang.reflect.Constructor;
import java.lang.reflect.Method;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collections;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Map;

final class LegacySkinsUi {
    private static final int OPEN = 0xB001, CLOSE = 0xB002, IMPORT = 0xB003, PREVIOUS = 0xB004,
        NEXT = 0xB005, APPLY = 0xB006, FAVORITE = 0xB007, MODEL = 0xB008,
        LIBRARY = 0xB010, FAVORITES = 0xB011, HISTORY = 0xB012, EXPLORE = 0xB013;
    private final AuroraSkinsRuntime runtime;
    private final Map<Object, State> states = new IdentityHashMap<Object, State>();
    private final Map<Object, PreviewState> titlePreviews = new IdentityHashMap<Object, PreviewState>();
    private final Map<Object, AuroraTitleLayout> titleLayouts = new IdentityHashMap<Object, AuroraTitleLayout>();

    LegacySkinsUi(AuroraSkinsRuntime runtime) { this.runtime = runtime; }

    @SubscribeEvent public void onTick(TickEvent.ClientTickEvent event) {
        if (event.phase == TickEvent.Phase.END) {
            LegacyMinecraft.drainTasks();
            LegacySkinManager.applyToLocalPlayer();
            Object current = LegacyMinecraft.field(LegacyMinecraft.minecraft(), "currentScreen", "field_71462_r");
            java.util.Iterator<State> iterator = states.values().iterator();
            while (iterator.hasNext()) {
                State state = iterator.next();
                if (state.screen != current) { dispose(state); iterator.remove(); }
                else if (state.search != null) invokeAny(state.search, "updateCursorCounter", "func_146178_a");
            }
            titlePreviews.keySet().retainAll(Collections.singleton(current));
            titleLayouts.keySet().retainAll(Collections.singleton(current));
        }
    }

    @SubscribeEvent public void onRenderTick(TickEvent.RenderTickEvent event) {
        if (event.phase == TickEvent.Phase.START) LegacySkinManager.applyToLocalPlayer();
    }

    @SubscribeEvent public void onKeyboard(GuiScreenEvent.KeyboardInputEvent.Pre event) {
        State state = states.get(invoke(event, "getGui"));
        if (state == null) return;
        try {
            Class<?> keyboard = Class.forName("org.lwjgl.input.Keyboard");
            if (!Boolean.TRUE.equals(keyboard.getMethod("getEventKeyState").invoke(null))) return;
            char character = (Character)keyboard.getMethod("getEventCharacter").invoke(null);
            int key = (Integer)keyboard.getMethod("getEventKey").invoke(null);
            if (key == 1) { event.setCanceled(true); close(state); return; }
            if (state.search == null) return;
            Object handled = LegacyMinecraft.invoke(state.search, new String[] {"textboxKeyTyped", "func_146201_a"}, character, key);
            if (Boolean.TRUE.equals(handled)) {
                event.setCanceled(true);
                String query = String.valueOf(invokeAny(state.search, "getText", "func_146179_b"));
                if (!query.equals(state.query)) { state.query = query; state.selected = 0; state.request++;
                    if (state.view == 3) refreshCatalog(state, 0); else refresh(state);
                    rebuild(state.screen, state); }
            }
        } catch (ReflectiveOperationException ignored) { }
    }

    @SubscribeEvent public void onMouse(GuiScreenEvent.MouseInputEvent.Pre event) {
        Object screen = invoke(event, "getGui"); State state = states.get(screen);
        if (state == null) return;
        try {
            Class<?> mouse = Class.forName("org.lwjgl.input.Mouse");
            if (!Boolean.TRUE.equals(mouse.getMethod("getEventButtonState").invoke(null))) return;
            Object minecraft = LegacyMinecraft.minecraft();
            int x = (Integer)mouse.getMethod("getEventX").invoke(null) * width(screen)
                / Math.max(1, integer(LegacyMinecraft.field(minecraft, "displayWidth", "field_71443_c"), width(screen)));
            int y = height(screen) - (Integer)mouse.getMethod("getEventY").invoke(null) * height(screen)
                / Math.max(1, integer(LegacyMinecraft.field(minecraft, "displayHeight", "field_71440_d"), height(screen))) - 1;
            int button = (Integer)mouse.getMethod("getEventButton").invoke(null);
            if (state.search != null) LegacyMinecraft.invoke(state.search, new String[] {"mouseClicked", "func_146192_a"}, x, y, button);
            if (state.view == 3 && button == 0) {
                CatalogGridLayout gridLayout = CatalogGridLayout.calculate(AuroraSkinsLayout.calculate(width(screen), height(screen)));
                int index = gridLayout.hit(x, y);
                if (index >= 0) {
                    // Even an empty/loading card belongs to Skins: do not pass it to the host.
                    event.setCanceled(true);
                    if (!state.busy && state.grid.select(index)) rebuild(screen, state);
                } else if (state.grid.failed() && x >= gridLayout.statusX() && x < gridLayout.statusX() + gridLayout.statusWidth()
                    && y >= gridLayout.statusY() && y < gridLayout.statusY() + 10) {
                    event.setCanceled(true); refreshCatalog(state, state.grid.requestedPage()); rebuild(screen, state);
                }
            }
        } catch (ReflectiveOperationException ignored) { }
    }

    @SubscribeEvent public void onInit(GuiScreenEvent.InitGuiEvent.Post event) {
        Object screen = invoke(event, "getGui");
        if (screen == null) return;
        State state = states.get(screen);
        if (state != null) { state.search = null; rebuild(screen, state); return; }
        if (!isMainMenu(screen)) return;
        List<Object> buttons = list(invoke(event, "getButtonList"));
        List<AuroraTitleLayout.Rect> occupied = new ArrayList<AuroraTitleLayout.Rect>();
        for (Object existing : buttons) occupied.add(new AuroraTitleLayout.Rect(
            integer(LegacyMinecraft.field(existing, "x", "xPosition", "field_146128_h"), 0),
            integer(LegacyMinecraft.field(existing, "y", "yPosition", "field_146129_i"), 0),
            integer(LegacyMinecraft.field(existing, "width", "field_146120_f"), 200),
            integer(LegacyMinecraft.field(existing, "height", "field_146121_g"), 20)));
        AuroraTitleLayout layout = AuroraTitleLayout.calculate(width(screen), height(screen), occupied);
        titleLayouts.put(screen, layout);
        Object open = button(OPEN, layout.buttonX(), layout.buttonY(), layout.buttonWidth(), layout.buttonHeight(), "Skins");
        if (open != null) buttons.add(open);
    }

    @SubscribeEvent public void onAction(GuiScreenEvent.ActionPerformedEvent.Pre event) {
        final Object screen = invoke(event, "getGui");
        Object pressed = invoke(event, "getButton");
        int id = integer(LegacyMinecraft.field(pressed, "id", "field_146127_k"), -1);
        if (id == OPEN && titleLayouts.containsKey(screen) && !states.containsKey(screen)) {
            event.setCanceled(true);
            open(screen, new Runnable() { @Override public void run() { closeToVanilla(screen); }}); return;
        }
        State state = states.get(screen); if (state == null) return;
        if (!ownsAction(id)) return;
        // The host screen must never receive module IDs or replace the screen before Post.
        event.setCanceled(true);
        if (id == CLOSE) { close(state); return; }
        if (state.busy && (id == APPLY || id == FAVORITE || id == MODEL || id == IMPORT)) return;
        if (id == LIBRARY || id == FAVORITES || id == HISTORY || id == EXPLORE) {
            state.request++; state.view = id == FAVORITES ? 1 : id == HISTORY ? 2 : id == EXPLORE ? 3 : 0;
            runtime.catalogProvider().cancelSearch();
            clearGrid(state); state.previewRequest++; state.previewKey = ""; state.renderer.close();
            state.selected = 0;
            if (state.view == 3) refreshCatalog(state, 0); else refresh(state);
            rebuild(screen, state); return;
        }
        if (id == PREVIOUS || id == NEXT) {
            if (state.busy) return;
            if (state.view == 3) {
                if (id == PREVIOUS && state.grid.canPrevious()) refreshCatalog(state, state.grid.page() - 1);
                else if (id == NEXT && state.grid.canNext()) refreshCatalog(state, state.grid.page() + 1);
                rebuild(screen, state); return;
            }
            if (!state.entries.isEmpty()) {
                int amount = id == NEXT ? 1 : -1;
                state.selected = (state.selected + amount + state.entries.size()) % state.entries.size();
            }
            rebuild(screen, state); return;
        }
        if (id == APPLY) apply(state);
        else if (id == FAVORITE) favorite(state);
        else if (id == MODEL) toggleModel(state);
        else if (id == IMPORT) chooseFile(screen, state);
        rebuild(screen, state);
    }

    private void ensureSearch(Object screen, State state) {
        AuroraSkinsLayout layout = AuroraSkinsLayout.calculate(width(screen), height(screen));
        if (state.search == null || state.screenWidth != width(screen) || state.screenHeight != height(screen)) {
            try {
                Object font = LegacyMinecraft.field(screen, "fontRenderer", "field_146289_q");
                Class<?> type = Class.forName("net.minecraft.client.gui.GuiTextField");
                AuroraSkinsLayout.Slot searchSlot = layout.search();
                state.search = type.getConstructor(int.class, Class.forName("net.minecraft.client.gui.FontRenderer"),
                    int.class, int.class, int.class, int.class).newInstance(0, font, searchSlot.x(), searchSlot.y(), searchSlot.width(), searchSlot.height());
                LegacyMinecraft.invoke(state.search, new String[] {"setMaxStringLength", "func_146203_f"}, 80);
                LegacyMinecraft.invoke(state.search, new String[] {"setText", "func_146180_a"}, state.query);
                state.screenWidth = width(screen); state.screenHeight = height(screen);
            } catch (ReflectiveOperationException ignored) { }
        }
    }

    @SubscribeEvent public void onDrawPre(GuiScreenEvent.DrawScreenEvent.Pre event) {
        if (states.containsKey(invoke(event, "getGui"))) event.setCanceled(true);
    }

    @SubscribeEvent public void onDraw(GuiScreenEvent.DrawScreenEvent.Post event) {
        Object screen = invoke(event, "getGui"); if (screen == null) return;
        int mouseX = integer(invoke(event, "getMouseX"), width(screen) / 2);
        int mouseY = integer(invoke(event, "getMouseY"), height(screen) / 2);
        State state = states.get(screen);
        if (state != null) {
            ensureSearch(screen, state);
            invokeAny(screen, "drawDefaultBackground", "func_146276_q_");
            drawPage(screen, state, mouseX, mouseY);
            Object partial = invoke(event, "getRenderPartialTicks");
            float partialTicks = partial instanceof Number ? ((Number)partial).floatValue() : 0F;
            for (Object button : list(LegacyMinecraft.field(screen, "buttonList", "field_146292_n"))) {
                LegacyMinecraft.invoke(button, new String[] {"drawButton", "func_191745_a"},
                    LegacyMinecraft.minecraft(), mouseX, mouseY, partialTicks);
            }
        }
        else if (isMainMenu(screen)) drawTitlePreview(screen, mouseX, mouseY);
    }

    void open(final AuroraUiContext context) {
        final Object screen = context.nativeParentScreen();
        if (screen == null) return;
        // Release the Core overlay through its public contract before taking over this host.
        context.close();
        open(screen, new Runnable() { @Override public void run() { context.openAuroraHome(); }});
    }

    private void open(Object screen, Runnable close) {
        if (screen == null) return;
        State previous = states.remove(screen); if (previous != null) dispose(previous);
        State state = new State(screen, close); states.put(screen, state); refresh(state); rebuild(screen, state);
    }

    private void rebuild(Object screen, State state) {
        ensureSearch(screen, state);
        List<Object> buttons = list(LegacyMinecraft.field(screen, "buttonList", "field_146292_n")); buttons.clear();
        AuroraSkinsLayout layout = AuroraSkinsLayout.calculate(width(screen), height(screen));
        AuroraSkinsLayout.Slot slot = layout.navigation(0);
        add(buttons, button(LIBRARY, slot.x(), slot.y(), slot.width(), slot.height(), "Biblioteca"));
        slot = layout.navigation(1);
        add(buttons, button(FAVORITES, slot.x(), slot.y(), slot.width(), slot.height(), "Favoritos"));
        slot = layout.navigation(2);
        add(buttons, button(HISTORY, slot.x(), slot.y(), slot.width(), slot.height(), "Histórico"));
        slot = layout.navigation(3);
        add(buttons, button(EXPLORE, slot.x(), slot.y(), slot.width(), slot.height(), "Explorar"));
        slot = layout.importButton();
        add(buttons, button(IMPORT, slot.x(), slot.y(), slot.width(), slot.height(), "Importar PNG"));
        slot = layout.previous();
        add(buttons, button(PREVIOUS, slot.x(), slot.y(), slot.width(), slot.height(), "<"));
        slot = layout.next();
        add(buttons, button(NEXT, slot.x(), slot.y(), slot.width(), slot.height(), ">"));
        slot = layout.apply();
        add(buttons, button(APPLY, slot.x(), slot.y(), slot.width(), slot.height(), "Usar skin"));
        SkinEntry entry = selected(state); boolean favorite = false; if (entry != null) try { favorite = runtime.store().isFavorite(entry); } catch (Exception ignored) { }
        slot = layout.favorite();
        add(buttons, button(FAVORITE, slot.x(), slot.y(), slot.width(), slot.height(), favorite ? "Desfavoritar" : "Favoritar"));
        SkinModel model = entry == null ? state.importModel : entry.model();
        String modelLabel = model == SkinModel.SLIM ? "Slim" : "Clássico";
        slot = layout.model();
        add(buttons, button(MODEL, slot.x(), slot.y(), slot.width(), slot.height(), layout.compact() ? modelLabel : "Modelo: " + modelLabel));
        slot = layout.done();
        add(buttons, button(CLOSE, slot.x(), slot.y(), slot.width(), slot.height(), "Concluído"));
        for (Object button : buttons) {
            int id = integer(LegacyMinecraft.field(button, "id", "field_146127_k"), -1);
            boolean enabled = !state.busy || id == CLOSE || id == LIBRARY || id == FAVORITES || id == HISTORY || id == EXPLORE;
            if ((id == APPLY || id == FAVORITE) && entry == null) enabled = false;
            if (state.view == 3 && id == PREVIOUS) enabled = !state.busy && state.grid.canPrevious();
            if (state.view == 3 && id == NEXT) enabled = !state.busy && state.grid.canNext();
            LegacyMinecraft.setField(button, enabled, "enabled", "field_146124_l");
        }
    }

    private void refresh(State state) {
        if (state.view == 3) { refreshCatalog(state, state.grid.page()); return; }
        state.busy = false;
        try {
            state.entries = state.view == 1 ? runtime.store().favorites() : state.view == 2 ? runtime.store().history()
                : runtime.store().list();
            if (!state.query.trim().isEmpty()) {
                List<SkinEntry> filtered = new ArrayList<SkinEntry>();
                String query = state.query.trim().toLowerCase(java.util.Locale.ROOT);
                for (SkinEntry entry : state.entries) if (entry.name().toLowerCase(java.util.Locale.ROOT).contains(query)) filtered.add(entry);
                state.entries = filtered;
            }
            if (state.selected >= state.entries.size()) state.selected = Math.max(0, state.entries.size() - 1);
            state.status = state.entries.isEmpty() ? "Nenhuma skin aqui. Importe um PNG 64x64 ou 64x32." : "";
        } catch (Exception error) { state.entries = Collections.emptyList(); state.status = "Não foi possível ler a biblioteca local."; }
    }

    private void refreshCatalog(final State state, int pageIndex) {
        final int request = ++state.request;
        final long generation = state.grid.begin(state.query, pageIndex);
        closeCardRenderers(state);
        state.previewRequest++; state.previewKey = ""; state.renderer.close();
        state.entries = Collections.emptyList();
        state.busy = true; state.status = "Buscando no catálogo Aurora...";
        runtime.catalogProvider().searchDebounced(state.query, pageIndex, CatalogGridState.PAGE_SIZE).whenComplete((page, error) ->
            LegacyMinecraft.execute(new Runnable() { @Override public void run() {
                if (request != state.request || !currentGrid(state, generation)) return;
                state.busy = false;
                if (error != null || page == null) {
                    state.grid.fail(generation);
                    state.status = "Falha. Clique aqui para repetir.";
                } else {
                    if (!state.grid.accept(generation, page)) {
                        state.status = "Falha. Clique aqui para repetir."; rebuild(state.screen, state); return;
                    }
                    state.status = page.entries().isEmpty() ? "Nenhuma skin encontrada." : "";
                    for (int index = 0; index < CatalogGridState.PAGE_SIZE; index++) loadCard(state, generation, index);
                }
                rebuild(state.screen, state);
            }}));
    }

    private boolean currentGrid(State state, long generation) {
        return state.view == 3 && activeState(state) && state.grid.isCurrent(generation);
    }
    private boolean activeState(State state) {
        return states.get(state.screen) == state
            && LegacyMinecraft.field(LegacyMinecraft.minecraft(), "currentScreen", "field_71462_r") == state.screen;
    }

    private void loadCard(final State state, final long generation, final int index) {
        final SkinEntry entry = state.grid.slot(index).entry();
        if (entry == null) return;
        final String identity = state.grid.slot(index).identity();
        runtime.catalogProvider().preview(entry).whenComplete((preview, error) -> LegacyMinecraft.execute(() -> {
            if (!currentGrid(state, generation) || !identity.equals(state.grid.slot(index).identity())) return;
            try {
                if (error != null || preview == null) throw new IllegalStateException();
                state.cardRenderers[index].setSkin(preview);
                state.grid.ready(generation, index, identity, preview);
            } catch (Exception failure) {
                state.cardRenderers[index].close();
                state.grid.previewFailed(generation, index, identity);
                state.status = "Uma prévia está indisponível.";
            }
            rebuild(state.screen, state);
        }));
    }

    private static void closeCardRenderers(State state) {
        for (LegacySkinPreviewRenderer renderer : state.cardRenderers) renderer.close();
    }
    private static void clearGrid(State state) { state.grid.invalidate(); closeCardRenderers(state); }

    private SkinEntry selected(State state) {
        if (state.view == 3) return state.grid.selectedPreview();
        return state.selected < 0 || state.selected >= state.entries.size() ? null : state.entries.get(state.selected);
    }
    private SkinEntry selectedSource(State state) { return state.view == 3 ? state.grid.selected() : selected(state); }
    private void refreshPreview(final State state) {
        SkinEntry entry = selected(state);
        String key = entry == null ? "" : entry.id() + ":" + entry.contentHash() + ":" + entry.model().id();
        if (key.equals(state.previewKey)) return;
        state.previewKey = key; final int request = ++state.previewRequest; state.renderer.close();
        if (entry == null) return;
        runtime.catalogProvider().preview(entry).whenComplete((installed, error) -> LegacyMinecraft.execute(() -> {
            if (request != state.previewRequest || !activeState(state)) return;
            if (error != null || installed == null) { state.status = "Prévia indisponível."; return; }
            try { state.renderer.setSkin(installed); } catch (Exception failure) { state.status = "Prévia indisponível."; }
        }));
    }
    private void apply(final State state) { final SkinEntry entry = selected(state); if (entry == null) return;
        final SkinEntry original = selectedSource(state);
        withInstalled(state, entry, "Não foi possível aplicar esta skin.", new InstalledAction() {
            @Override public void run(SkinEntry installed) throws Exception {
                runtime.apply(installed); if (state.view != 3) replaceSelected(state, original, installed);
                state.status = "Skin aplicada: " + installed.name();
            }
        });
    }
    private void favorite(final State state) { final SkinEntry entry = selected(state); if (entry == null) return;
        final SkinEntry original = selectedSource(state);
        withInstalled(state, entry, "Não foi possível atualizar os favoritos.", new InstalledAction() {
            @Override public void run(SkinEntry installed) throws Exception {
                runtime.store().setFavorite(installed, !runtime.store().isFavorite(installed));
                if (state.view != 3) replaceSelected(state, original, installed);
                state.status = ""; if (state.view == 1) refresh(state);
            }
        });
    }
    private void toggleModel(State state) { final SkinEntry entry = selected(state); SkinModel current = entry == null ? state.importModel : entry.model(); state.importModel = current == SkinModel.SLIM ? SkinModel.WIDE : SkinModel.SLIM;
        if (entry != null) {
            final SkinEntry original = selectedSource(state);
            final SkinModel model = state.importModel; final State target = state;
            withInstalled(state, entry, "Não foi possível alterar o modelo.", new InstalledAction() {
                @Override public void run(SkinEntry installed) throws Exception {
                    SkinEntry updated = runtime.store().importSkin(installed.file(), installed.name(), model);
                    runtime.apply(updated); replaceSelected(target, original, updated); target.status = "";
                }
            });
        }
    }

    private void withInstalled(final State state, SkinEntry entry, final String errorMessage, final InstalledAction action) {
        final int request = ++state.request;
        state.busy = true; state.status = "Preparando e verificando esta skin...";
        runtime.materialize(entry).whenComplete((installed, error) -> LegacyMinecraft.execute(new Runnable() {
            @Override public void run() {
                if (request != state.request || !activeState(state)) return;
                try {
                    if (error != null || installed == null) throw new IllegalStateException();
                    action.run(installed);
                } catch (Exception failure) { state.status = errorMessage; }
                finally { state.busy = false; rebuild(state.screen, state); }
            }
        }));
    }

    private void replaceSelected(State state, SkinEntry original, SkinEntry installed) {
        String identity = CatalogGridState.identity(original);
        if (state.view == 3) {
            long generation = state.grid.generation();
            int index = state.grid.replaceEntry(generation, identity, installed);
            if (index >= 0) { state.cardRenderers[index].close(); loadCard(state, generation, index); }
            return;
        }
        List<SkinEntry> updated = new ArrayList<SkinEntry>(state.entries);
        for (int index = 0; index < updated.size(); index++) {
            if (identity.equals(CatalogGridState.identity(updated.get(index)))) { updated.set(index, installed); break; }
        }
        state.entries = updated;
    }

    private void chooseFile(final Object screen, final State state) {
        final int request = ++state.request;
        state.busy = true; state.status = "Abrindo o seletor de arquivo...";
        Thread thread = new Thread(new Runnable() { @Override public void run() { try {
            JFileChooser chooser = new JFileChooser(); chooser.setFileFilter(new FileNameExtensionFilter("PNG 64x64 / 64x32", "png"));
            final Path path = chooser.showOpenDialog(null) == JFileChooser.APPROVE_OPTION ? chooser.getSelectedFile().toPath() : null;
            LegacyMinecraft.execute(() -> {
                if (request != state.request || !activeState(state)) return;
                state.busy = false; state.status = "";
                if (path != null) importFile(screen, state, path);
                rebuild(screen, state);
            });
        } catch (Throwable error) { LegacyMinecraft.execute(() -> {
            if (request != state.request || !activeState(state)) return;
            state.busy = false; state.status = "Não foi possível abrir o seletor de arquivo."; rebuild(screen, state);
        }); } }}, "Aurora-Skins-FileChooser");
        thread.setDaemon(true); thread.start();
    }
    private void importFile(Object screen, State state, Path file) { try {
        String name = file.getFileName() == null ? "Skin importada" : file.getFileName().toString().replaceFirst("(?i)\\.png$", "");
        SkinEntry entry = runtime.store().importSkin(file, name, state.importModel); runtime.apply(entry); clearGrid(state); state.view = 0; refresh(state);
        for (int index = 0; index < state.entries.size(); index++) if (state.entries.get(index).contentHash().equals(entry.contentHash())) state.selected = index;
        state.status = "Skin importada: " + entry.name(); rebuild(screen, state);
    } catch (Exception error) { state.status = error.getMessage() == null ? "Não foi possível importar esta skin." : error.getMessage(); } }

    private void drawPage(Object screen, State state, int mouseX, int mouseY) {
        AuroraSkinsLayout layout = AuroraSkinsLayout.calculate(width(screen), height(screen));
        drawText(screen, "AURORA SKINS", width(screen) / 2 - 36, 15, 0xF2EAFF);
        if (state.search != null) invokeAny(state.search, "drawTextBox", "func_146194_f");
        if (state.view == 3 && !layout.compact()) drawText(screen, "Tema, #tag ou @jogador / @UUID", layout.search().x(), layout.search().y() - 10, 0xB9AACB);
        if (state.view == 3) { drawCatalog(screen, state, layout, mouseX, mouseY); return; }
        if (!state.busy) refreshPreview(state);
        SkinEntry entry = selected(state); int textX = layout.textX(), textY = layout.textY();
        if (entry != null) {
            drawText(screen, fit(screen, entry.name(), layout.textWidth()), textX, textY, 0xF2EAFF);
            String position = (state.selected + 1) + " / " + state.entries.size();
            drawText(screen, position, textX, textY + 16, 0xB9AACB);
        }
        if (!state.status.isEmpty()) drawText(screen, fit(screen, state.status, layout.textWidth()), textX, textY + 40, 0xB9AACB);
        renderPreview(screen, state.preview, mouseX, mouseY, layout.previewX(), layout.previewY(), layout.previewScale(), state);
    }

    private void drawCatalog(Object screen, State state, AuroraSkinsLayout layout, int mouseX, int mouseY) {
        CatalogGridLayout gridLayout = CatalogGridLayout.calculate(layout);
        boolean changed = false;
        for (int index = 0; index < CatalogGridState.PAGE_SIZE; index++) {
            AuroraSkinsLayout.Slot bounds = gridLayout.slot(index);
            CatalogGridState.Slot slot = state.grid.slot(index);
            boolean ready = slot.status() == CatalogGridState.Status.READY;
            int border = state.grid.isSelected(index) ? 0xFFFFCE55
                : ready && gridLayout.hit(mouseX, mouseY) == index ? 0xFFAAA4B4 : 0xFF48444F;
            drawRect(bounds.x(), bounds.y(), bounds.right(), bounds.bottom(), border);
            drawRect(bounds.x() + 1, bounds.y() + 1, bounds.right() - 1, bounds.bottom() - 1, 0xFF242229);
            if (ready) {
                if (!state.cardRenderers[index].renderCard(screen, gridLayout.modelX(index), gridLayout.modelY(index),
                    gridLayout.modelScale(index), state.cardPose)) {
                    state.grid.previewFailed(state.grid.generation(), index, slot.identity());
                    state.cardRenderers[index].close(); state.status = "Uma prévia está indisponível."; changed = true;
                }
            } else if (slot.status() != CatalogGridState.Status.EMPTY) {
                int centerX = (bounds.x() + bounds.right()) / 2, centerY = (bounds.y() + bounds.bottom()) / 2;
                int marker = slot.status() == CatalogGridState.Status.FAILED ? 0xFF915E5E : 0xFF65616C;
                drawRect(centerX - 3, centerY - 1, centerX + 3, centerY + 1, marker);
            }
        }
        String page = "Página " + (state.grid.page() + 1);
        String status = state.status.isEmpty() ? page : page + " · " + state.status;
        drawText(screen, fit(screen, status, gridLayout.statusWidth()), gridLayout.statusX(), gridLayout.statusY(), 0xB9AACB);
        if (changed) rebuild(screen, state);
    }

    private void drawTitlePreview(Object screen, int mouseX, int mouseY) {
        PreviewState state = titlePreviews.get(screen); if (state == null) { state = new PreviewState(); titlePreviews.put(screen, state); }
        AuroraTitleLayout layout = titleLayouts.get(screen);
        if (layout == null) layout = AuroraTitleLayout.calculate(width(screen), height(screen));
        int x = layout.previewX(), y = layout.previewY(), scale = layout.previewScale();
        renderPreview(screen, state.preview, mouseX, mouseY, x, y, scale, state);
    }

    private void renderPreview(Object screen, AuroraPlayerPreview preview, int mouseX, int mouseY, int x, int y, int scale, PreviewState state) {
        preview.aim(mouseX, mouseY, x, y - scale, 180, 150); long now = System.nanoTime(); preview.update((now - state.lastFrame) / 1_000_000_000F); state.lastFrame = now;
        if (state instanceof State) ((State)state).renderer.render(screen, x, y, scale, preview);
        else Aurora.services().previews().render(screen, x, y, scale, preview);
    }

    private void dispose(State state) { state.request++; state.previewRequest++; state.renderer.close(); clearGrid(state); runtime.catalogProvider().cancelSearch(); }
    private void close(State state) { states.remove(state.screen); dispose(state); state.close.run(); }
    void shutdown() { for (State state : states.values()) dispose(state); states.clear(); titlePreviews.clear(); titleLayouts.clear(); }
    private static boolean ownsAction(int id) { return id == CLOSE || id == IMPORT || id == PREVIOUS || id == NEXT
        || id == APPLY || id == FAVORITE || id == MODEL || id == LIBRARY || id == FAVORITES || id == HISTORY || id == EXPLORE; }
    private void closeToVanilla(Object screen) {
        // displayGuiScreen runs setWorldAndResolution and Forge InitGui events, restoring all mod buttons.
        LegacyMinecraft.invoke(LegacyMinecraft.minecraft(), new String[] {"displayGuiScreen", "func_147108_a"}, screen);
    }
    private static boolean isMainMenu(Object screen) { return screen.getClass().getName().equals("net.minecraft.client.gui.GuiMainMenu"); }
    private static Object button(int id, int x, int y, int width, int height, String label) { try {
        Class<?> type = Class.forName("net.minecraft.client.gui.GuiButton"); Constructor<?> constructor = type.getConstructor(int.class,int.class,int.class,int.class,int.class,String.class);
        return constructor.newInstance(id,x,y,width,height,label); } catch (ReflectiveOperationException error) { return null; } }
    private static void add(List<Object> buttons, Object button) { if (button != null) buttons.add(button); }
    private static void drawText(Object screen, String text, int x, int y, int color) { Object font = LegacyMinecraft.field(screen, "fontRenderer", "field_146289_q"); if (font == null) return;
        for (String name : new String[] {"drawStringWithShadow", "func_175063_a"}) try { Method method = font.getClass().getMethod(name,String.class,float.class,float.class,int.class); method.invoke(font,text,(float)x,(float)y,color); return; }
        catch (ReflectiveOperationException ignored) { } }
    private static String fit(Object screen, String value, int maxWidth) { Object font = LegacyMinecraft.field(screen,"fontRenderer","field_146289_q"); String shortened = value;
        while (!shortened.isEmpty() && stringWidth(font, shortened + "…") > maxWidth) shortened = shortened.substring(0, shortened.length()-1); return shortened.equals(value) ? value : shortened + "…"; }
    private static int stringWidth(Object font, String value) { if (font != null) for (String name : new String[] {"getStringWidth","func_78256_a"}) try { Object result = font.getClass().getMethod(name,String.class).invoke(font,value); if (result instanceof Number) return ((Number)result).intValue(); }
        catch (ReflectiveOperationException ignored) { } return value.length()*6; }
    private static void drawRect(int left,int top,int right,int bottom,int color) { try { Class<?> gui=Class.forName("net.minecraft.client.gui.Gui"); for(String name:new String[]{"drawRect","func_73734_a"}) try {
        Method method=gui.getDeclaredMethod(name,int.class,int.class,int.class,int.class,int.class); method.setAccessible(true); method.invoke(null,left,top,right,bottom,color); return; } catch(ReflectiveOperationException ignored){} } catch(ClassNotFoundException ignored){} }
    private static int width(Object screen) { return integer(LegacyMinecraft.field(screen,"width","field_146294_l"),854); }
    private static int height(Object screen) { return integer(LegacyMinecraft.field(screen,"height","field_146295_m"),480); }
    private static int integer(Object value,int fallback){return value instanceof Number?((Number)value).intValue():fallback;}
    @SuppressWarnings("unchecked") private static List<Object> list(Object value){return value instanceof List<?>?(List<Object>)value:new ArrayList<Object>();}
    private static Object invoke(Object target,String method){if(target==null)return null;try{return target.getClass().getMethod(method).invoke(target);}catch(ReflectiveOperationException ignored){return null;}}
    private static Object invokeAny(Object target,String...names){return LegacyMinecraft.invoke(target,names);}

    private static class PreviewState { final AuroraPlayerPreview preview = new AuroraPlayerPreview(); long lastFrame = System.nanoTime(); }
    private static final class State extends PreviewState {
        final Object screen; final Runnable close; List<SkinEntry> entries = Collections.emptyList();
        int selected, view, request; boolean busy;
        String status = ""; SkinModel importModel = SkinModel.WIDE;
        String query = "", previewKey = ""; Object search;
        int screenWidth, screenHeight, previewRequest;
        final LegacySkinPreviewRenderer renderer = new LegacySkinPreviewRenderer();
        final CatalogGridState grid = new CatalogGridState();
        final LegacySkinPreviewRenderer[] cardRenderers = new LegacySkinPreviewRenderer[CatalogGridState.PAGE_SIZE];
        final AuroraPlayerPreview cardPose = new AuroraPlayerPreview();
        State(Object screen, Runnable close) {
            this.screen = screen; this.close = close;
            for (int index = 0; index < cardRenderers.length; index++) cardRenderers[index] = new LegacySkinPreviewRenderer();
        }
    }
    private interface InstalledAction { void run(SkinEntry entry) throws Exception; }
}
