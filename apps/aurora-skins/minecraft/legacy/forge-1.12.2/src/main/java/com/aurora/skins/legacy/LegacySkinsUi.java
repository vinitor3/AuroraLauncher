package com.aurora.skins.legacy;

import com.aurora.core.api.Aurora;
import com.aurora.core.api.ui.AuroraPlayerPreview;
import com.aurora.core.api.ui.AuroraUiContext;
import com.aurora.skins.AuroraSkinsRuntime;
import com.aurora.skins.api.SkinEntry;
import com.aurora.skins.api.SkinModel;
import com.aurora.skins.ui.AuroraSkinsLayout;
import com.aurora.skins.ui.AuroraTitleLayout;
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
        if (event.phase == TickEvent.Phase.END) LegacySkinManager.applyToLocalPlayer();
    }

    @SubscribeEvent public void onInit(GuiScreenEvent.InitGuiEvent.Post event) {
        Object screen = invoke(event, "getGui");
        if (screen == null || !isMainMenu(screen) || states.containsKey(screen)) return;
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

    @SubscribeEvent public void onAction(GuiScreenEvent.ActionPerformedEvent.Post event) {
        final Object screen = invoke(event, "getGui");
        Object pressed = invoke(event, "getButton");
        int id = integer(LegacyMinecraft.field(pressed, "id", "field_146127_k"), -1);
        if (id == OPEN) { open(screen, new Runnable() { @Override public void run() { closeToVanilla(screen); }}); return; }
        State state = states.get(screen); if (state == null) return;
        if (id == CLOSE) { states.remove(screen); state.close.run(); return; }
        if (id == LIBRARY || id == FAVORITES || id == HISTORY || id == EXPLORE) {
            state.request++; state.view = id == FAVORITES ? 1 : id == HISTORY ? 2 : id == EXPLORE ? 3 : 0;
            state.selected = 0; if (state.view == 3) state.catalogPage = 0; refresh(state); rebuild(screen, state); return;
        }
        if (id == PREVIOUS || id == NEXT) {
            if (!state.entries.isEmpty()) {
                int amount = id == NEXT ? 1 : -1;
                if (state.view == 3 && amount > 0 && state.selected == state.entries.size() - 1 && state.catalogHasMore) {
                    state.catalogPage++; state.selected = 0; refresh(state);
                } else if (state.view == 3 && amount < 0 && state.selected == 0 && state.catalogPage > 0) {
                    state.catalogPage--; state.selected = 0; refresh(state);
                } else state.selected = (state.selected + amount + state.entries.size()) % state.entries.size();
            }
            rebuild(screen, state); return;
        }
        if (id == APPLY) apply(state);
        else if (id == FAVORITE) favorite(state);
        else if (id == MODEL) toggleModel(state);
        else if (id == IMPORT) chooseFile(screen, state);
        rebuild(screen, state);
    }

    @SubscribeEvent public void onBackground(GuiScreenEvent.BackgroundDrawnEvent event) {
        Object screen = invoke(event, "getGui"); if (!states.containsKey(screen)) return;
        AuroraSkinsLayout layout = AuroraSkinsLayout.calculate(width(screen), height(screen));
        drawRect(layout.panelLeft(), layout.panelTop(), layout.panelRight(), layout.panelBottom(), 0xE61A112B);
        drawRect(layout.panelLeft(), layout.panelTop(), layout.panelLeft() + 3, layout.panelBottom(), 0xFFB78CFF);
    }

    @SubscribeEvent public void onDraw(GuiScreenEvent.DrawScreenEvent.Post event) {
        Object screen = invoke(event, "getGui"); if (screen == null) return;
        int mouseX = integer(invoke(event, "getMouseX"), width(screen) / 2);
        int mouseY = integer(invoke(event, "getMouseY"), height(screen) / 2);
        State state = states.get(screen);
        if (state != null) drawPage(screen, state, mouseX, mouseY);
        else if (isMainMenu(screen)) drawTitlePreview(screen, mouseX, mouseY);
    }

    void open(final AuroraUiContext context) {
        final Object screen = context.nativeParentScreen();
        open(screen, new Runnable() { @Override public void run() { context.openAuroraHome(); }});
    }

    private void open(Object screen, Runnable close) {
        if (screen == null) return;
        State state = new State(screen, close); states.put(screen, state); refresh(state); rebuild(screen, state);
    }

    private void rebuild(Object screen, State state) {
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
    }

    private void refresh(State state) {
        if (state.view == 3) { refreshCatalog(state); return; }
        state.busy = false;
        try {
            state.entries = state.view == 1 ? runtime.store().favorites() : state.view == 2 ? runtime.store().history()
                : runtime.store().list();
            if (state.selected >= state.entries.size()) state.selected = Math.max(0, state.entries.size() - 1);
            state.status = state.entries.isEmpty() ? "Nenhuma skin aqui. Importe um PNG 64x64 ou 64x32." : "";
        } catch (Exception error) { state.entries = Collections.emptyList(); state.status = "Não foi possível ler a biblioteca local."; }
    }

    private void refreshCatalog(final State state) {
        final int request = ++state.request;
        state.busy = true; state.status = "Buscando no catálogo Aurora...";
        runtime.catalogProvider().search("", state.catalogPage, 12).whenComplete((page, error) ->
            LegacyMinecraft.execute(new Runnable() { @Override public void run() {
                if (request != state.request || state.view != 3) return;
                state.busy = false;
                if (error != null || page == null) {
                    state.entries = Collections.emptyList(); state.catalogHasMore = false;
                    state.status = "Não foi possível abrir o catálogo Aurora.";
                } else {
                    state.entries = page.entries(); state.catalogPage = page.page(); state.catalogHasMore = page.hasMore();
                    state.selected = Math.min(state.selected, Math.max(0, state.entries.size() - 1));
                    state.status = state.entries.isEmpty() ? "Nenhuma skin encontrada."
                        : runtime.catalogProvider().online() ? "Catálogo Aurora online e sincronizado."
                        : "Catálogo Aurora pronto no modo offline.";
                }
                rebuild(state.screen, state);
            }}));
    }

    private SkinEntry selected(State state) { return state.entries.isEmpty() ? null : state.entries.get(state.selected); }
    private void apply(final State state) { SkinEntry entry = selected(state); if (entry == null) return;
        withInstalled(state, entry, "Não foi possível aplicar esta skin.", new InstalledAction() {
            @Override public void run(SkinEntry installed) throws Exception {
                runtime.apply(installed); replaceSelected(state, installed); state.status = "Skin aplicada: " + installed.name();
            }
        });
    }
    private void favorite(final State state) { SkinEntry entry = selected(state); if (entry == null) return;
        withInstalled(state, entry, "Não foi possível atualizar os favoritos.", new InstalledAction() {
            @Override public void run(SkinEntry installed) throws Exception {
                runtime.store().setFavorite(installed, !runtime.store().isFavorite(installed)); replaceSelected(state, installed);
                state.status = ""; if (state.view == 1) refresh(state);
            }
        });
    }
    private void toggleModel(State state) { SkinEntry entry = selected(state); SkinModel current = entry == null ? state.importModel : entry.model(); state.importModel = current == SkinModel.SLIM ? SkinModel.WIDE : SkinModel.SLIM;
        if (entry != null) {
            final SkinModel model = state.importModel; final State target = state;
            withInstalled(state, entry, "Não foi possível alterar o modelo.", new InstalledAction() {
                @Override public void run(SkinEntry installed) throws Exception {
                    SkinEntry updated = runtime.store().importSkin(installed.file(), installed.name(), model);
                    runtime.apply(updated); replaceSelected(target, updated); target.status = "";
                }
            });
        }
    }

    private void withInstalled(final State state, SkinEntry entry, final String errorMessage, final InstalledAction action) {
        state.busy = true; state.status = "Preparando e verificando esta skin...";
        runtime.materialize(entry).whenComplete((installed, error) -> LegacyMinecraft.execute(new Runnable() {
            @Override public void run() {
                try {
                    if (error != null || installed == null) throw new IllegalStateException();
                    action.run(installed);
                } catch (Exception failure) { state.status = errorMessage; }
                finally { state.busy = false; rebuild(state.screen, state); }
            }
        }));
    }

    private static void replaceSelected(State state, SkinEntry installed) {
        if (state.entries.isEmpty()) return;
        List<SkinEntry> updated = new ArrayList<SkinEntry>(state.entries);
        updated.set(Math.max(0, Math.min(state.selected, updated.size() - 1)), installed); state.entries = updated;
    }

    private void chooseFile(final Object screen, final State state) {
        state.status = "Abrindo o seletor de arquivo...";
        Thread thread = new Thread(new Runnable() { @Override public void run() { try {
            JFileChooser chooser = new JFileChooser(); chooser.setFileFilter(new FileNameExtensionFilter("PNG 64x64 / 64x32", "png"));
            if (chooser.showOpenDialog(null) != JFileChooser.APPROVE_OPTION) return; final Path path = chooser.getSelectedFile().toPath();
            LegacyMinecraft.execute(new Runnable() { @Override public void run() { importFile(screen, state, path); }});
        } catch (Throwable error) { state.status = "Não foi possível abrir o seletor de arquivo."; } }}, "Aurora-Skins-FileChooser");
        thread.setDaemon(true); thread.start();
    }
    private void importFile(Object screen, State state, Path file) { try {
        String name = file.getFileName() == null ? "Skin importada" : file.getFileName().toString().replaceFirst("(?i)\\.png$", "");
        SkinEntry entry = runtime.store().importSkin(file, name, state.importModel); runtime.apply(entry); state.view = 0; refresh(state);
        for (int index = 0; index < state.entries.size(); index++) if (state.entries.get(index).contentHash().equals(entry.contentHash())) state.selected = index;
        state.status = "Skin importada: " + entry.name(); rebuild(screen, state);
    } catch (Exception error) { state.status = error.getMessage() == null ? "Não foi possível importar esta skin." : error.getMessage(); } }

    private void drawPage(Object screen, State state, int mouseX, int mouseY) {
        AuroraSkinsLayout layout = AuroraSkinsLayout.calculate(width(screen), height(screen));
        drawText(screen, "AURORA SKINS", width(screen) / 2 - 36, 15, 0xF2EAFF);
        SkinEntry entry = selected(state); int textX = layout.textX(), textY = layout.textY();
        if (entry != null) {
            drawText(screen, fit(screen, entry.name(), layout.textWidth()), textX, textY, 0xF2EAFF);
            String position = state.view == 3 ? "Página " + (state.catalogPage + 1) + " · "
                + (state.selected + 1) + " / " + state.entries.size() : (state.selected + 1) + " / " + state.entries.size();
            drawText(screen, position, textX, textY + 16, 0xB9AACB);
        }
        if (!state.status.isEmpty()) drawText(screen, fit(screen, state.status, layout.textWidth()), textX, textY + 40, 0xB9AACB);
        renderPreview(screen, state.preview, mouseX, mouseY, layout.previewX(), layout.previewY(), layout.previewScale(), state);
    }

    private void drawTitlePreview(Object screen, int mouseX, int mouseY) {
        PreviewState state = titlePreviews.get(screen); if (state == null) { state = new PreviewState(); titlePreviews.put(screen, state); }
        AuroraTitleLayout layout = titleLayouts.get(screen);
        if (layout == null) layout = AuroraTitleLayout.calculate(width(screen), height(screen));
        int x = layout.previewX(), y = layout.previewY(), scale = layout.previewScale();
        drawRect(layout.panelLeft(), layout.panelTop(), layout.panelRight(), layout.panelBottom(), 0x661A112B);
        drawRect(layout.panelLeft(), layout.panelTop(), layout.panelLeft() + 3, layout.panelBottom(), 0xFFB78CFF);
        renderPreview(screen, state.preview, mouseX, mouseY, x, y, scale, state);
    }

    private void renderPreview(Object screen, AuroraPlayerPreview preview, int mouseX, int mouseY, int x, int y, int scale, PreviewState state) {
        preview.aim(mouseX, mouseY, x, y - scale, 180, 150); long now = System.nanoTime(); preview.update((now - state.lastFrame) / 1_000_000_000F); state.lastFrame = now;
        Aurora.services().previews().render(screen, x, y, scale, preview);
    }

    private void closeToVanilla(Object screen) { states.remove(screen); invokeAny(screen, "initGui", "func_73866_w_"); }
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
        int selected, view, catalogPage, request; boolean catalogHasMore, busy;
        String status = ""; SkinModel importModel = SkinModel.WIDE;
        State(Object screen, Runnable close) { this.screen = screen; this.close = close; }
    }
    private interface InstalledAction { void run(SkinEntry entry) throws Exception; }
}
