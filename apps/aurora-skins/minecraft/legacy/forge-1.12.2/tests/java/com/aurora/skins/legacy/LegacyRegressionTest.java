package com.aurora.skins.legacy;

import com.aurora.core.api.ui.AuroraPlayerPreview;
import com.aurora.skins.AuroraSkinsRuntime;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.*;
import net.minecraftforge.client.event.GuiScreenEvent;
import net.minecraftforge.fml.common.gameevent.TickEvent;
import org.lwjgl.input.Keyboard;
import org.lwjgl.opengl.GL11;

import java.lang.reflect.Field;
import java.util.*;
import java.util.concurrent.atomic.AtomicBoolean;

/** Behavioral regression checks with SRG-only Minecraft fixtures; no game or network required. */
public final class LegacyRegressionTest {
    public static void main(String[] args) throws Exception {
        System.setProperty("aurora.skins.root", args[0]);
        reflectionAndScheduling();
        playerSkin();
        screenLifecycle();
        glRestoration();
        System.out.println("PASS: overloads, client scheduling, SRG skin injection/rollback, UI ownership/resize/search/Escape, GL success/failure.");
    }

    private static void reflectionAndScheduling() throws Exception {
        UUID id = UUID.randomUUID();
        check(id.equals(LegacyMinecraft.invoke(new Overloads(), new String[] {"wrong", "lookup"}, id)), "incompatible overload must be skipped");
        check("srg".equals(LegacyMinecraft.invoke(new Overloads(), new String[] {"missing", "func_test"})), "SRG fallback");
        AtomicBoolean called = new AtomicBoolean();
        Thread worker = new Thread(() -> LegacyMinecraft.execute(() -> called.set(true)));
        worker.start(); worker.join();
        check(!called.get(), "scheduler failure must not execute GUI/GL on the worker");
        LegacyMinecraft.drainTasks();
        check(called.get(), "client tick drains fallback task");
    }

    private static void playerSkin() {
        LegacyPlayerSkinOverride override = new LegacyPlayerSkinOverride();
        SrgInfo info = new SrgInfo(); Object original = new Object(), custom = new Object(), cape = new Object();
        info.field_187107_a.put("SKIN", original); info.field_187107_a.put("CAPE", cape);
        check(override.apply(info, "SKIN", custom, "classic"), "map injection");
        check(info.field_187107_a.get("SKIN") == custom && info.field_187107_a.get("CAPE") == cape, "only SKIN overridden");
        check("default".equals(info.field_178863_g), "classic translates to Minecraft default");
        Object lateVanilla = new Object(); info.field_187107_a.put("SKIN", lateVanilla); info.field_178863_g = "slim";
        check(override.apply(info, "SKIN", custom, "classic"), "late vanilla callback is repaired");
        override.clear();
        check(info.field_187107_a.get("SKIN") == lateVanilla && "slim".equals(info.field_178863_g), "rollback retains late vanilla result");
        override.apply(info, "SKIN", custom, "classic");
        Object lateClassic = new Object(); info.field_187107_a.put("SKIN", lateClassic); info.field_178863_g = "default";
        override.apply(info, "SKIN", custom, "classic");
        override.clear();
        check(info.field_187107_a.get("SKIN") == lateClassic && "default".equals(info.field_178863_g), "late texture/model are retained as a pair even when model matches override");
        info.field_178863_g = "slim";
        override.apply(info, "SKIN", custom, "classic");
        info.field_187107_a.put("SKIN", lateClassic); info.field_178863_g = "default";
        override.clear();
        check(info.field_187107_a.get("SKIN") == lateClassic && "default".equals(info.field_178863_g), "clear leaves a newer externally owned texture/model untouched");
        NamedInfo development = new NamedInfo();
        check(override.apply(development, "SKIN", custom, "slim"), "MCP field names work too");
        check("slim".equals(development.skinType), "slim model");
        override.apply(info, "SKIN", custom, "classic");
        check(!development.playerTextures.containsKey("SKIN") && development.skinType == null, "switching player info restores old owner");
        override.clear();
        check(!override.apply(new Object(), "SKIN", custom, "classic"), "missing map fails safely");
        NamedInfo immutable = new NamedInfo(); immutable.playerTextures = Collections.emptyMap();
        check(!override.apply(immutable, "SKIN", custom, "classic") && immutable.skinType == null, "failed mutation restores model");
        override.clear();
    }

    private static void screenLifecycle() throws Exception {
        AuroraSkinsRuntime runtime = AuroraSkinsRuntime.start();
        LegacySkinsUi ui = new LegacySkinsUi(runtime);
        GuiMainMenu screen = new GuiMainMenu(); Minecraft mc = Minecraft.func_71410_x(); mc.field_71462_r = screen;
        ui.onInit(new GuiScreenEvent.InitGuiEvent.Post(screen, screen.field_146292_n));
        GuiButton open = button(screen, 0xB001);
        GuiScreenEvent.ActionPerformedEvent.Pre action = new GuiScreenEvent.ActionPerformedEvent.Pre(screen, open);
        ui.onAction(action);
        check(action.isCanceled(), "module click must not reach vanilla actionPerformed");
        Object state = ((Map<?, ?>)get(ui, "states")).get(screen);
        check(get(state, "search") instanceof GuiTextField, "search exists without any BackgroundDrawnEvent");
        int created = GuiTextField.created;
        screen.field_146294_l = 320; screen.field_146295_m = 240; screen.field_146292_n.clear();
        ui.onInit(new GuiScreenEvent.InitGuiEvent.Post(screen, screen.field_146292_n));
        check(GuiTextField.created > created && screen.field_146292_n.size() == 11, "resize reconstructs module controls");
        GuiScreenEvent.DrawScreenEvent.Pre draw = new GuiScreenEvent.DrawScreenEvent.Pre(screen); ui.onDrawPre(draw);
        check(draw.isCanceled(), "host controls/title must not draw over module");
        Keyboard.key = 30; Keyboard.character = 'a';
        GuiScreenEvent.KeyboardInputEvent.Pre key = new GuiScreenEvent.KeyboardInputEvent.Pre(screen); ui.onKeyboard(key);
        check(key.isCanceled() && "a".equals(get(state, "query")), "SRG search input updates query");
        ui.onAction(new GuiScreenEvent.ActionPerformedEvent.Pre(screen, button(screen, 0xB013)));
        ui.onAction(new GuiScreenEvent.ActionPerformedEvent.Pre(screen, button(screen, 0xB010)));
        LegacyMinecraft.drainTasks();
        check(((Integer)get(state, "view")) == 0 && !((Boolean)get(state, "busy")), "canceling pending catalog search keeps library available");
        Keyboard.key = 1;
        key = new GuiScreenEvent.KeyboardInputEvent.Pre(screen); ui.onKeyboard(key);
        check(key.isCanceled() && mc.displayed == 1 && ((Map<?, ?>)get(ui, "states")).isEmpty(), "Escape closes overlay through displayGuiScreen");
        screen.field_146292_n.clear(); ui.onInit(new GuiScreenEvent.InitGuiEvent.Post(screen, screen.field_146292_n));
        check(button(screen, 0xB001) != null, "Skins button returns after Forge reinitialization");
        ui.onAction(new GuiScreenEvent.ActionPerformedEvent.Pre(screen, button(screen, 0xB001)));
        mc.field_71462_r = null; ui.onTick(new TickEvent.ClientTickEvent(TickEvent.Phase.END));
        check(((Map<?, ?>)get(ui, "states")).isEmpty(), "leaving host releases overlay");
        ui.shutdown();
    }

    private static void glRestoration() throws Exception {
        GL11.reset(); String before = GL11.snapshot();
        LegacySkinPreviewRenderer renderer = new LegacySkinPreviewRenderer();
        Field texture = LegacySkinPreviewRenderer.class.getDeclaredField("previewTexture"); texture.setAccessible(true); texture.set(renderer, new Object());
        check(renderer.render(null, 100, 150, 40, new AuroraPlayerPreview()), "SRG preview renders");
        check(net.minecraft.client.model.ModelPlayer.renderedParts == 12, "all twelve body/wear parts render using real 1.12 SRG fields");
        check(before.equals(GL11.snapshot()), "successful preview restores GL flags, blend, material, color, binding and stacks");
        GL11.failTranslation = true;
        check(!renderer.render(null, 100, 150, 40, new AuroraPlayerPreview()), "injected render failure is contained");
        check(before.equals(GL11.snapshot()), "failed preview also restores GL state");
        GL11.failTranslation = false;
        net.minecraft.client.model.ModelPlayer.failConstruction = true;
        renderer = new LegacySkinPreviewRenderer(); texture.set(renderer, new Object());
        check(!renderer.render(null, 100, 150, 40, new AuroraPlayerPreview()), "failure before push is contained");
        check(GL11.matrixDepth == 0 && GL11.attributeDepth == 0, "no stack underflow before push");
    }

    private static GuiButton button(GuiScreen screen, int id) {
        for (Object item : screen.field_146292_n) if (((GuiButton)item).field_146127_k == id) return (GuiButton)item;
        throw new AssertionError("missing button " + id);
    }
    private static Object get(Object target, String name) throws Exception { Field field = target.getClass().getDeclaredField(name); field.setAccessible(true); return field.get(target); }
    private static void check(boolean condition, String message) { if (!condition) throw new AssertionError(message); }
    public static final class Overloads {
        public String wrong(String value) { throw new AssertionError("wrong overload invoked"); }
        public UUID lookup(UUID value) { return value; }
        public String lookup(String value) { throw new AssertionError("wrong overload invoked"); }
        public String func_test() { return "srg"; }
    }
    public static final class SrgInfo { public final Map<Object, Object> field_187107_a = new HashMap<Object, Object>(); public String field_178863_g = "slim"; }
    public static final class NamedInfo { public Map<Object, Object> playerTextures = new HashMap<Object, Object>(); public String skinType; }
}
