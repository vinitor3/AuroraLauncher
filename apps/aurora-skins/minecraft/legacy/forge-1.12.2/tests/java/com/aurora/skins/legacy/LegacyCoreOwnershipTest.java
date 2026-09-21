package com.aurora.skins.legacy;

import com.aurora.core.api.module.AuroraModule;
import com.aurora.core.api.module.AuroraModuleMetadata;
import com.aurora.core.api.ui.AuroraSettingsPage;
import com.aurora.core.api.ui.AuroraSettingsRegistry;
import com.aurora.core.runtime.AuroraCorePlatform;
import com.aurora.core.runtime.AuroraCoreRuntime;
import com.aurora.core.runtime.AuroraLog;
import com.aurora.skins.AuroraSkinsRuntime;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiButton;
import net.minecraft.client.gui.GuiOptions;
import net.minecraftforge.client.event.GuiScreenEvent;
import net.minecraftforge.fml.common.gameevent.TickEvent;
import org.lwjgl.input.Keyboard;

import java.lang.reflect.Constructor;
import java.lang.reflect.Method;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.ArrayList;
import java.util.List;

/** Real Core -> Skins callbacks against source-compiled UIs and SRG Minecraft fixtures. */
public final class LegacyCoreOwnershipTest {
    private static final int CORE_OPEN = 0xA012, CORE_CLOSE = 0xA013, CORE_SKINS = 0xA100 + 90;
    private final GuiOptions host = new GuiOptions();
    private final Object core;
    private final LegacySkinsUi skins;
    private final List<String> errors = new ArrayList<String>();
    private boolean failOpen;

    private LegacyCoreOwnershipTest(LegacySkinsUi skins) throws Exception {
        this.skins = skins;
        Constructor<?> constructor = Class.forName("com.aurora.core.minecraft.legacy.LegacyAuroraUi").getDeclaredConstructor();
        constructor.setAccessible(true);
        core = constructor.newInstance();
    }

    public static void main(String[] args) throws Exception {
        final Path root = Paths.get(args[0]);
        System.setProperty("aurora.skins.root", root.resolve("skins").toString());
        System.clearProperty("aurora.profile.skinFile");
        System.clearProperty("aurora.session.nonce");
        System.clearProperty("aurora.ipc.port");
        LegacyCoreOwnershipTest test = new LegacyCoreOwnershipTest(new LegacySkinsUi(AuroraSkinsRuntime.start()));
        try (AuroraCoreRuntime runtime = AuroraCoreRuntime.start(test.platform(root))) {
            runtime.modules().register(new AuroraModule() {
                @Override public AuroraModuleMetadata metadata() {
                    return new AuroraModuleMetadata("aurora_skins", "Skins", "0.2.5", "skins", "1.1.3");
                }
                @Override public void registerSettings(AuroraSettingsRegistry settings) {
                    settings.register(new AuroraSettingsPage("aurora_skins", "aurora_skins", "Skins", "", "skins", 10, context -> {
                        if (test.failOpen) throw new IllegalStateException("injected opener failure");
                        check(context.nativeParentScreen() == test.host, "handoff must preserve host identity");
                        test.skins.open(context);
                    }));
                }
            });
            test.lifecycle();
        } finally {
            test.skins.shutdown();
            Minecraft.func_71410_x().field_71462_r = null;
        }
        System.out.println("PASS: real Core/Skins handoff, resize orders, stale click, Done/Escape, opener rollback, host departure.");
    }

    private void lifecycle() throws Exception {
        Minecraft.func_71410_x().field_71462_r = host;
        coreEvent("onInit", init());
        GuiButton staleOpen = button(CORE_OPEN);
        check(click(staleOpen).isCanceled(), "Core opening must consume its button");
        ownership(true, false);
        check(click(button(CORE_SKINS)).isCanceled(), "page click must call the real Core opener");
        ownership(false, true);

        // Same-host ticks retain delegation. Forge rebuilds the list before InitGui.Post.
        coreEvent("onTick", new TickEvent.ClientTickEvent(TickEvent.Phase.END));
        for (boolean skinsFirst : new boolean[] {true, false}) {
            host.field_146294_l = skinsFirst ? 320 : 854;
            host.field_146295_m = skinsFirst ? 240 : 480;
            host.field_146292_n.clear();
            if (skinsFirst) skins.onInit(init());
            coreEvent("onInit", init());
            if (!skinsFirst) skins.onInit(init());
            check(host.field_146292_n.size() == 11 && !hasButton(CORE_OPEN), "resize must not inject Core controls into Skins");
            ownership(false, true);
        }
        check(!click(staleOpen).isCanceled(), "delegated host must ignore stale Core OPEN");
        ownership(false, true);
        check(host.field_146292_n.size() == 11, "stale Core click must not rebuild the host");

        click(button(0xB002)); // Skins Done -> context.openAuroraHome().
        ownership(true, false);
        click(button(CORE_SKINS));
        Keyboard.key = 1;
        GuiScreenEvent.KeyboardInputEvent.Pre escape = new GuiScreenEvent.KeyboardInputEvent.Pre(host);
        skins.onKeyboard(escape);
        check(escape.isCanceled(), "Skins Escape must return through its Core context");
        ownership(true, false);
        click(button(CORE_CLOSE));
        ownership(false, false);
        host.field_146292_n.clear();
        coreEvent("onInit", init());
        check(hasButton(CORE_OPEN), "return to vanilla must allow Core entry again");

        click(button(CORE_OPEN));
        check(errors.isEmpty(), "normal handoff must not log errors: " + errors);
        failOpen = true;
        click(button(CORE_SKINS));
        check(errors.size() == 1 && errors.get(0).contains("Settings page failed to open: aurora_skins"), "opener failure must be reported");
        ownership(true, false);
        failOpen = false;
        click(button(CORE_SKINS)); // Must not remain delegated after the failed opener.
        ownership(false, true);

        Minecraft.func_71410_x().field_71462_r = null;
        TickEvent.ClientTickEvent end = new TickEvent.ClientTickEvent(TickEvent.Phase.END);
        skins.onTick(end);
        coreEvent("onTick", end);
        ownership(false, false);
        Minecraft.func_71410_x().field_71462_r = host;
        host.field_146292_n.clear();
        coreEvent("onInit", init());
        check(hasButton(CORE_OPEN), "leaving the host must clear delegation before reuse");
    }

    private GuiScreenEvent.InitGuiEvent.Post init() {
        return new GuiScreenEvent.InitGuiEvent.Post(host, host.field_146292_n);
    }

    private GuiScreenEvent.ActionPerformedEvent.Pre click(GuiButton button) throws Exception {
        GuiScreenEvent.ActionPerformedEvent.Pre event = new GuiScreenEvent.ActionPerformedEvent.Pre(host, button);
        skins.onAction(event);
        // Match Forge's normal listeners: canceled events do not reach the next listener.
        if (!event.isCanceled()) coreEvent("onAction", event);
        return event;
    }

    private void ownership(boolean coreOwns, boolean skinsOwns) throws Exception {
        GuiScreenEvent.DrawScreenEvent.Pre coreDraw = new GuiScreenEvent.DrawScreenEvent.Pre(host);
        GuiScreenEvent.DrawScreenEvent.Pre skinsDraw = new GuiScreenEvent.DrawScreenEvent.Pre(host);
        coreEvent("onDrawPre", coreDraw);
        skins.onDrawPre(skinsDraw);
        check(coreDraw.isCanceled() == coreOwns && skinsDraw.isCanceled() == skinsOwns,
            "unexpected host ownership: Core=" + coreDraw.isCanceled() + ", Skins=" + skinsDraw.isCanceled());
    }

    private void coreEvent(String name, Object event) throws Exception {
        Method method = core.getClass().getDeclaredMethod(name, event.getClass());
        method.setAccessible(true);
        method.invoke(core, event);
    }

    private boolean hasButton(int id) {
        for (Object value : host.field_146292_n) if (((GuiButton)value).field_146127_k == id) return true;
        return false;
    }

    private GuiButton button(int id) {
        for (Object value : host.field_146292_n) if (((GuiButton)value).field_146127_k == id) return (GuiButton)value;
        throw new AssertionError("missing button " + id);
    }

    private AuroraCorePlatform platform(final Path root) {
        return new AuroraCorePlatform() {
            @Override public Path gameDirectory() { return root; }
            @Override public String minecraftVersion() { return "1.12.2"; }
            @Override public String loader() { return "forge"; }
            @Override public void runOnClient(Runnable action) { action.run(); }
            @Override public void openAuroraSettings(Object parent) { throw new AssertionError("unexpected platform navigation"); }
            @Override public void log(AuroraLog.Level level, String message, Throwable error) {
                if (level == AuroraLog.Level.ERROR) errors.add(message);
            }
        };
    }

    private static void check(boolean condition, String message) {
        if (!condition) throw new AssertionError(message);
    }
}
