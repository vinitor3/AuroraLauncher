package com.aurora.core.minecraft.legacy;

import com.aurora.core.api.Aurora;
import com.aurora.core.api.ui.AuroraPlayerPreview;
import com.aurora.core.api.ui.AuroraButtonLayout;
import com.aurora.core.api.ui.AuroraSettingsLayout;
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
            List<AuroraButtonLayout.Rect> occupied = new ArrayList<AuroraButtonLayout.Rect>();
            for (Object existing : buttons) {
                occupied.add(new AuroraButtonLayout.Rect(
                    integer(field(existing, "x", "xPosition", "field_146128_h"), 0),
                    integer(field(existing, "y", "yPosition", "field_146129_i"), 0),
                    integer(field(existing, "width", "field_146120_f"), 200),
                    integer(field(existing, "height", "field_146121_g"), 20)));
            }
            AuroraButtonLayout.Slot slot = AuroraButtonLayout.findSlot(width(screen), height(screen), occupied);
            Object button = button(OPEN_BUTTON, slot.x(), slot.y(), slot.width(), slot.height(), "Opções Aurora");
            if (button != null) buttons.add(button);
        }
    }

    @SubscribeEvent public void onDrawBackground(GuiScreenEvent.BackgroundDrawnEvent event) {
        Object screen = invoke(event, "getGui");
        if (!states.containsKey(screen)) return;
        AuroraSettingsLayout layout = AuroraSettingsLayout.calculate(width(screen), height(screen));
        drawRect(layout.panelLeft(), 40, layout.panelRight(), height(screen) - 40, 0xD91A112B);
        drawRect(layout.panelLeft(), 40, layout.panelLeft() + 3, height(screen) - 40, 0xFFB78CFF);
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
        AuroraSettingsLayout layout = AuroraSettingsLayout.calculate(width(screen), height(screen));
        drawText(screen, "AURORA", width(screen) / 2 - 18, 18, 0xF2EAFF);
        drawText(screen, "Módulos", layout.navigationX(), 48, 0xB9AACB);
        drawText(screen, fitText(screen, Aurora.services().ipc().isConnected()
            ? "Launcher conectado" : "Launcher desconectado", layout.contentWidth()),
            layout.contentX(), 58, 0xB9AACB);
        drawText(screen, fitText(screen, "Módulos instalados: "
            + Aurora.services().modules().modules().size(), layout.contentWidth()),
            layout.contentX(), 76, 0xB9AACB);
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
        AuroraSettingsLayout layout = AuroraSettingsLayout.calculate(width(screen), height(screen));
        List<AuroraSettingsPage> pages = Aurora.services().settings().pages();
        int x = layout.navigationX();
        for (int index = 0; index < pages.size() && index < 8; index++) {
            AuroraSettingsPage page = pages.get(index);
            int id = PAGE_BUTTON_START + index;
            Object pageButton = button(id, x, 64 + index * 24, layout.navigationWidth(), 20, page.title());
            if (pageButton != null) {
                buttons.add(pageButton);
                state.pages.put(id, page);
            }
        }
        AuroraSettingsPage skins = findSkinsPage();
        if (skins != null) {
            int id = PAGE_BUTTON_START + 90;
            Object skinButton = button(id, layout.avatarX() - 47, height(screen) - 76, 94, 20, "Skins");
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
            AuroraSettingsLayout layout = AuroraSettingsLayout.calculate(width(screen), height(screen));
            int x = layout.avatarX(), y = layout.avatarY();
            state.preview.aim(mouseX, mouseY, x, y - 70, Math.max(160, width(screen) / 2), Math.max(120, height(screen)));
            long now = System.nanoTime();
            state.preview.update((now - state.lastFrame) / 1_000_000_000.0F);
            state.lastFrame = now;
            Object minecraft = LegacyMinecraftPlatform.minecraft();
            Object player = LegacyMinecraftPlatform.field(minecraft, "player", "field_71439_g");
            int scale = Math.min(55, Math.max(34, height(screen) / 5));
            if (player == null) {
                Aurora.services().previews().render(screen, x, y, scale, state.preview);
                return;
            }
            Class<?> inventory = Class.forName("net.minecraft.client.gui.inventory.GuiInventory");
            for (Method method : inventory.getMethods()) {
                if (!(method.getName().equals("drawEntityOnScreen") || method.getName().equals("func_147046_a"))
                    || method.getParameterTypes().length != 6) continue;
                method.invoke(null, x, y, scale,
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

    private static String fitText(Object screen, String value, int maxWidth) {
        Object font = LegacyMinecraftPlatform.field(screen, "fontRenderer", "field_146289_q");
        if (font == null) return value;
        String shortened = value;
        while (!shortened.isEmpty() && stringWidth(font, shortened + "…") > maxWidth) {
            shortened = shortened.substring(0, shortened.length() - 1);
        }
        return shortened.equals(value) ? value : shortened + "…";
    }

    private static int stringWidth(Object font, String value) {
        for (String name : new String[] { "getStringWidth", "func_78256_a" }) {
            try {
                Method method = font.getClass().getMethod(name, String.class);
                Object result = method.invoke(font, value);
                if (result instanceof Number) return ((Number) result).intValue();
            } catch (ReflectiveOperationException ignored) { }
        }
        return value.length() * 6;
    }

    private static void drawRect(int left, int top, int right, int bottom, int color) {
        try {
            Class<?> gui = Class.forName("net.minecraft.client.gui.Gui");
            for (String name : new String[] { "drawRect", "func_73734_a" }) {
                try {
                    Method method = gui.getDeclaredMethod(name, int.class, int.class, int.class, int.class, int.class);
                    method.setAccessible(true);
                    method.invoke(null, left, top, right, bottom, color);
                    return;
                } catch (ReflectiveOperationException ignored) { }
            }
        } catch (ClassNotFoundException ignored) { }
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
