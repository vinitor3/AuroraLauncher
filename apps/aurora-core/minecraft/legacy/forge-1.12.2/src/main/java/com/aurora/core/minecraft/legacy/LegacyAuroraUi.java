package com.aurora.core.minecraft.legacy;

import com.aurora.core.api.Aurora;
import com.aurora.core.api.ui.AuroraPlayerPreview;
import com.aurora.core.api.ui.AuroraSettingsPage;
import com.aurora.core.api.ui.AuroraUiContext;
import com.aurora.core.runtime.AuroraCoreRuntime;
import net.minecraftforge.client.event.GuiScreenEvent;
import net.minecraftforge.fml.common.eventhandler.SubscribeEvent;

import java.lang.reflect.Constructor;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Map;

final class LegacyAuroraUi {
    private static final int OPEN_BUTTON = 0xA012;
    private static final int CLOSE_BUTTON = 0xA013;
    private static final int PAGE_BUTTON_START = 0xA100;
    private final Map<Object, State> states = new IdentityHashMap<Object, State>();

    @SubscribeEvent public void onInit(GuiScreenEvent.InitGuiEvent.Post event) {
        Object screen = invoke(event, "getGui");
        if (screen == null || !screen.getClass().getName().equals("net.minecraft.client.gui.GuiOptions")) return;
        List<Object> buttons = list(invoke(event, "getButtonList"));
        if (!states.containsKey(screen)) {
            Object button = button(OPEN_BUTTON, width(screen) / 2 - 100, height(screen) - 52, 200, 20, "Opções Aurora");
            if (button != null) buttons.add(button);
        }
    }

    @SubscribeEvent public void onAction(GuiScreenEvent.ActionPerformedEvent.Post event) {
        Object screen = invoke(event, "getGui");
        Object pressed = invoke(event, "getButton");
        int id = integer(field(pressed, "id", "field_146127_k"), -1);
        if (id == OPEN_BUTTON) {
            open(screen);
            return;
        }
        State state = states.get(screen);
        if (state == null) return;
        if (id == CLOSE_BUTTON) {
            states.remove(screen);
            invokeAny(screen, "initGui", "func_73866_w_");
            return;
        }
        AuroraSettingsPage page = state.pages.get(id);
        if (page != null && !"aurora_general".equals(page.id())) openPage(screen, page);
    }

    @SubscribeEvent public void onDraw(GuiScreenEvent.DrawScreenEvent.Post event) {
        Object screen = invoke(event, "getGui");
        State state = states.get(screen);
        if (state == null) return;
        int mouseX = integer(invoke(event, "getMouseX"), width(screen) / 2);
        int mouseY = integer(invoke(event, "getMouseY"), height(screen) / 2);
        drawText(screen, "AURORA", width(screen) / 2 - 18, 18, 0xF2EAFF);
        drawText(screen, "Módulos", Math.max(18, width(screen) / 2 - 220), 48, 0xB9AACB);
        drawText(screen, Aurora.services().ipc().isConnected() ? "Launcher conectado" : "Launcher desconectado",
            width(screen) / 2 + 62, 52, 0xB9AACB);
        renderAvatar(state, screen, mouseX, mouseY);
    }

    synchronized void open(Object screen) {
        if (screen == null) {
            Object minecraft = LegacyMinecraftPlatform.minecraft();
            screen = LegacyMinecraftPlatform.field(minecraft, "currentScreen", "field_71462_r");
        }
        if (screen == null) return;
        List<Object> buttons = list(LegacyMinecraftPlatform.field(screen, "buttonList", "field_146292_n"));
        buttons.clear();
        State state = new State();
        List<AuroraSettingsPage> pages = Aurora.services().settings().pages();
        int x = Math.max(18, width(screen) / 2 - 220);
        for (int index = 0; index < pages.size() && index < 8; index++) {
            AuroraSettingsPage page = pages.get(index);
            int id = PAGE_BUTTON_START + index;
            Object pageButton = button(id, x, 64 + index * 24, 150, 20, page.title());
            if (pageButton != null) {
                buttons.add(pageButton);
                state.pages.put(id, page);
            }
        }
        AuroraSettingsPage skins = findSkinsPage();
        if (skins != null) {
            int id = PAGE_BUTTON_START + 90;
            Object skinButton = button(id, width(screen) / 2 + 52, height(screen) - 76, 94, 20, "Skins");
            if (skinButton != null) {
                buttons.add(skinButton);
                state.pages.put(id, skins);
            }
        }
        Object close = button(CLOSE_BUTTON, width(screen) / 2 - 75, height(screen) - 30, 150, 20, "Concluído");
        if (close != null) buttons.add(close);
        states.put(screen, state);
    }

    private AuroraSettingsPage findSkinsPage() {
        if (!Aurora.services().modules().isInstalled("aurora_skins")) return null;
        for (AuroraSettingsPage page : Aurora.services().settings().pages()) {
            if ("aurora_skins".equals(page.id()) || "aurora_skins".equals(page.owner())) return page;
        }
        return null;
    }

    private void openPage(final Object screen, final AuroraSettingsPage page) {
        try {
            page.opener().open(new AuroraUiContext() {
                @Override public void openAuroraHome() { open(screen); }
                @Override public void close() { states.remove(screen); }
                @Override public Object nativeParentScreen() { return screen; }
            });
        } catch (Throwable error) {
            AuroraCoreRuntime.instance().log().error("Settings page failed to open: " + page.id(), error);
        }
    }

    private void renderAvatar(State state, Object screen, int mouseX, int mouseY) {
        try {
            int x = width(screen) / 2 + 100, y = height(screen) - 88;
            state.preview.aim(mouseX, mouseY, x, y - 70, Math.max(160, width(screen) / 2), Math.max(120, height(screen)));
            long now = System.nanoTime();
            state.preview.update((now - state.lastFrame) / 1_000_000_000.0F);
            state.lastFrame = now;
            Object minecraft = LegacyMinecraftPlatform.minecraft();
            Object player = LegacyMinecraftPlatform.field(minecraft, "player", "field_71439_g");
            if (player == null) return;
            Class<?> inventory = Class.forName("net.minecraft.client.gui.inventory.GuiInventory");
            for (Method method : inventory.getMethods()) {
                if (!(method.getName().equals("drawEntityOnScreen") || method.getName().equals("func_147046_a"))
                    || method.getParameterTypes().length != 6) continue;
                method.invoke(null, x, y, Math.min(55, Math.max(34, height(screen) / 5)),
                    -state.preview.headYaw() * 0.65F, state.preview.headPitch() * 0.7F, player);
                return;
            }
        } catch (Throwable error) {
            if (!state.avatarFailed) {
                state.avatarFailed = true;
                AuroraCoreRuntime.instance().log().error("3D avatar failed; settings remain available.", error);
            }
        }
    }

    private static Object button(int id, int x, int y, int width, int height, String label) {
        try {
            Class<?> type = Class.forName("net.minecraft.client.gui.GuiButton");
            Constructor<?> constructor = type.getConstructor(int.class, int.class, int.class, int.class, int.class, String.class);
            return constructor.newInstance(id, x, y, width, height, label);
        } catch (ReflectiveOperationException error) { return null; }
    }

    private static void drawText(Object screen, String text, int x, int y, int color) {
        Object font = LegacyMinecraftPlatform.field(screen, "fontRenderer", "field_146289_q");
        if (font == null) return;
        for (String name : new String[] { "drawStringWithShadow", "func_175063_a" }) {
            try {
                Method method = font.getClass().getMethod(name, String.class, float.class, float.class, int.class);
                method.invoke(font, text, (float) x, (float) y, color);
                return;
            } catch (ReflectiveOperationException ignored) { }
        }
    }

    private static int width(Object screen) { return integer(LegacyMinecraftPlatform.field(screen, "width", "field_146294_l"), 854); }
    private static int height(Object screen) { return integer(LegacyMinecraftPlatform.field(screen, "height", "field_146295_m"), 480); }
    private static int integer(Object value, int fallback) { return value instanceof Number ? ((Number) value).intValue() : fallback; }
    @SuppressWarnings("unchecked")
    private static List<Object> list(Object value) { return value instanceof List<?> ? (List<Object>) value : new ArrayList<Object>(); }

    private static Object field(Object target, String... names) { return LegacyMinecraftPlatform.field(target, names); }
    private static Object invoke(Object target, String method) {
        if (target == null) return null;
        try { return target.getClass().getMethod(method).invoke(target); } catch (ReflectiveOperationException ignored) { return null; }
    }
    private static Object invokeAny(Object target, String... names) {
        if (target == null) return null;
        for (String name : names) {
            try { Method method = target.getClass().getMethod(name); method.setAccessible(true); return method.invoke(target); }
            catch (ReflectiveOperationException ignored) { }
        }
        return null;
    }

    private static final class State {
        private final Map<Integer, AuroraSettingsPage> pages = new java.util.LinkedHashMap<Integer, AuroraSettingsPage>();
        private final AuroraPlayerPreview preview = new AuroraPlayerPreview();
        private long lastFrame = System.nanoTime();
        private boolean avatarFailed;
    }
}
