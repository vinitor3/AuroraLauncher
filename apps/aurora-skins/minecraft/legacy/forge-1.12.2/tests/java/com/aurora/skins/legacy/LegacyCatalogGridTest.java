package com.aurora.skins.legacy;

import com.aurora.skins.AuroraSkinsRuntime;
import com.aurora.skins.api.*;
import com.aurora.skins.ui.*;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.*;
import net.minecraft.client.model.ModelPlayer;
import net.minecraft.client.renderer.texture.*;
import net.minecraftforge.client.event.GuiScreenEvent;
import org.lwjgl.input.Mouse;
import org.lwjgl.opengl.GL11;

import javax.imageio.ImageIO;
import java.awt.image.BufferedImage;
import java.lang.reflect.*;
import java.nio.file.*;
import java.security.MessageDigest;
import java.util.*;
import java.util.concurrent.CompletableFuture;
import java.util.function.BooleanSupplier;

/** Adapter integration: real provider/cache and PNGs, SRG graphics/input fixtures; no HTTP/game. */
public final class LegacyCatalogGridTest {
    private static final int NEXT = 0xB005, PREVIOUS = 0xB004, APPLY = 0xB006;

    public static void main(String[] args) throws Exception {
        Path root = Paths.get(args[0]); Files.createDirectories(root);
        System.setProperty("aurora.skins.root", root.resolve("store").toString());
        System.clearProperty("aurora.profile.skinFile");
        AuroraSkinsRuntime runtime = AuroraSkinsRuntime.start();
        List<SkinEntry> entries = fixtures(root);
        SkinEntry equipped = runtime.store().importSkin(entries.get(0).file(), "Equipada", SkinModel.WIDE);
        runtime.apply(equipped);
        cachePage(runtime, "", new SkinPage(entries, 0, true));
        cachePage(runtime, "", new SkinPage(entries.subList(0, 11), 1, true));
        cachePage(runtime, "", new SkinPage(Collections.<SkinEntry>emptyList(), 2, true));
        cachePage(runtime, "", new SkinPage(entries.subList(11, 12), 3, false));
        GL11.reset(); ModelPlayer.failConstruction = false;
        Minecraft mc = Minecraft.func_71410_x(); TextureManager textures = mc.textures;
        LegacySkinsUi ui = new LegacySkinsUi(runtime); GuiMainMenu screen = open(ui);
        click(ui, screen, 0xB013);
        Object state = state(ui, screen); CatalogGridState grid = (CatalogGridState)get(state, "grid");
        await(() -> allReady(grid, 12));
        check(grid.selected() == null && !button(screen, APPLY).field_146124_l, "catalog starts without selection");
        check(!button(screen, PREVIOUS).field_146124_l && button(screen, NEXT).field_146124_l, "first page bounds");
        check(runtime.store().list().size() == 1 && runtime.store().history().size() == 1 && runtime.store().favorites().isEmpty(), "twelve previews never import/favorite/apply");
        check(runtime.active() == equipped, "preview preserves active entry");

        ModelPlayer.renderedParts = 0; ModelPlayer.renderedModels.clear(); textures.bound.clear();
        String glBefore = GL11.snapshot();
        ui.onDraw(new GuiScreenEvent.DrawScreenEvent.Post(screen));
        check(ModelPlayer.renderedParts == 12 * 12 && ModelPlayer.renderedModels.size() == 12,
            "twelve distinct player models each render all twelve body/wear parts");
        check(new HashSet<Object>(textures.bound).size() == 12 && textures.bound.size() == 12,
            "each visible player uses its own texture, not twelve renders of the equipped skin");
        Set<Integer> colors = new HashSet<Integer>(); int slim = 0;
        for (DynamicTexture texture : textures.live.values()) {
            check(texture.image.getHeight() == 64, "legacy 64x32 PNG normalized before upload");
            colors.add(texture.image.getRGB(0, 0));
        }
        for (ModelPlayer model : ModelPlayer.renderedModels) if (model.slim) slim++;
        check(colors.size() == 12 && slim == 6, "all fixture appearances stay distinct with six Slim and six Classic");
        check(glBefore.equals(GL11.snapshot()), "twelve-model frame restores OpenGL state");
        int uploads = textures.uploaded;
        for (int[] size : new int[][] {{320, 240}, {427, 240}, {854, 480}}) {
            screen.field_146294_l = size[0]; screen.field_146295_m = size[1];
            ui.onInit(new GuiScreenEvent.InitGuiEvent.Post(screen, screen.field_146292_n));
            ui.onDraw(new GuiScreenEvent.DrawScreenEvent.Post(screen));
            check(textures.uploaded == uploads && textures.live.size() == 12, "resize/fullscreen performs no new uploads or releases");
            CatalogGridLayout layout = CatalogGridLayout.calculate(AuroraSkinsLayout.calculate(size[0], size[1]));
            for (int index = 0; index < 12; index++) {
                GuiScreenEvent.MouseInputEvent.Pre mouse = cardClick(ui, screen, layout.slot(index));
                check(mouse.isCanceled() && grid.selectedIndex() == index, "physical mouse selects correct logical slot and consumes host click");
                check(runtime.active() == equipped, "selection never equips");
            }
        }
        // Applying is tied to the resolved preview that was clicked, even if selection changes before delivery.
        CatalogGridLayout layout = CatalogGridLayout.calculate(AuroraSkinsLayout.calculate(854, 480));
        cardClick(ui, screen, layout.slot(4)); click(ui, screen, APPLY); grid.select(7);
        await(() -> !booleanField(state, "busy"));
        check(runtime.active().contentHash().equals(entries.get(4).contentHash()), "Apply is bound to the clicked entry, not the latest index");
        check(grid.slot(7).entry() == entries.get(7), "materialization never replaces a newly selected card");

        click(ui, screen, NEXT);
        check(grid.loading() && grid.page() == 0 && grid.selected() == null && textures.live.isEmpty(), "next abandons all old cards but confirms page only with response");
        check(!button(screen, NEXT).field_146124_l && !button(screen, PREVIOUS).field_146124_l, "both page arrows disabled during fetch");
        await(() -> allReady(grid, 11));
        check(grid.page() == 1 && grid.slot(11).status() == CatalogGridState.Status.EMPTY && button(screen, NEXT).field_146124_l,
            "short page preserves empty slot and honors hasMore");
        cardClick(ui, screen, layout.slot(11));
        check(grid.selected() == null && !button(screen, APPLY).field_146124_l, "empty card cannot select/apply");
        click(ui, screen, NEXT); await(() -> !grid.loading());
        check(grid.page() == 2 && grid.canNext(), "empty provider page still allows next when hasMore");
        click(ui, screen, NEXT); await(() -> allReady(grid, 1));
        check(grid.page() == 3 && !button(screen, NEXT).field_146124_l, "hasMore false disables next");
        click(ui, screen, NEXT); check(grid.page() == 3 && !grid.loading(), "last page cannot wrap");
        click(ui, screen, PREVIOUS); await(() -> !grid.loading());
        check(grid.page() == 2, "previous traverses whole page, including empty page");
        click(ui, screen, 0xB002);
        check(textures.live.isEmpty(), "close releases every owned card texture");

        latePreview(ui, runtime, entries.get(8), false);
        latePreview(ui, runtime, entries.get(9), true);
        ui.shutdown();
        check(textures.live.isEmpty(), "repeat open/close leaves no preview textures");
        check(!new LegacySkinPreviewRenderer().renderCard(null, 100, 150, 40, new com.aurora.core.api.ui.AuroraPlayerPreview()),
            "missing card texture cannot render equipped/default fallback");
        System.out.println("PASS: 12 distinct full-body models/textures; Slim/Classic/64x32; physical click consumption; explicit identity-bound Apply; page batches/short+empty hasMore; resize zero uploads; stale tab/close cleanup.");
    }

    private static void latePreview(LegacySkinsUi ui, AuroraSkinsRuntime runtime, SkinEntry source, boolean close) throws Exception {
        SkinEntry delayed = new SkinEntry("delayed-" + close, source.name(), "", source.model(), source.providerId(), source.source(), source.contentHash(), source.file(), source.tags());
        CompletableFuture<SkinEntry> completion = new CompletableFuture<SkinEntry>();
        @SuppressWarnings("unchecked") Map<String, CompletableFuture<SkinEntry>> pending = (Map<String, CompletableFuture<SkinEntry>>)get(runtime.catalogProvider(), "previews");
        String key = delayed.providerId() + "\n" + delayed.id() + "\n" + delayed.source() + "\n" + delayed.contentHash() + "\n" + delayed.model().id() + "\n" + delayed.file();
        synchronized (pending) { pending.put(key, completion); }
        cachePage(runtime, "", new SkinPage(Collections.singletonList(delayed), 0, false));
        GuiMainMenu screen = open(ui); click(ui, screen, 0xB013);
        CatalogGridState grid = (CatalogGridState)get(state(ui, screen), "grid");
        await(() -> !grid.loading());
        check(grid.slot(0).status() == CatalogGridState.Status.LOADING, "delayed preview remains neutral and unselectable");
        cardClick(ui, screen, CatalogGridLayout.calculate(AuroraSkinsLayout.calculate(854, 480)).slot(0));
        check(grid.selected() == null, "loading preview cannot select");
        int uploaded = Minecraft.func_71410_x().textures.uploaded;
        click(ui, screen, close ? 0xB002 : 0xB010);
        completion.complete(delayed); LegacyMinecraft.drainTasks();
        check(Minecraft.func_71410_x().textures.uploaded == uploaded && grid.slot(0).status() == CatalogGridState.Status.EMPTY,
            "late completion after tab/close cannot restore released texture");
        if (!close) click(ui, screen, 0xB002);
        synchronized (pending) { pending.remove(key); }
    }

    private static List<SkinEntry> fixtures(Path root) throws Exception {
        List<SkinEntry> entries = new ArrayList<SkinEntry>();
        for (int index = 0; index < 12; index++) {
            BufferedImage image = new BufferedImage(64, index == 0 ? 32 : 64, BufferedImage.TYPE_INT_ARGB);
            for (int y = 0; y < image.getHeight(); y++) for (int x = 0; x < 64; x++) image.setRGB(x, y, 0xFF101010 + index * 0x000A0B0C);
            Path file = root.resolve("skin-" + index + ".png"); ImageIO.write(image, "png", file.toFile());
            StringBuilder hash = new StringBuilder();
            for (byte value : MessageDigest.getInstance("SHA-256").digest(Files.readAllBytes(file))) hash.append(String.format(Locale.ROOT, "%02x", value & 255));
            entries.add(new SkinEntry("fixture-" + index, "Fixture " + index, "", index % 2 == 0 ? SkinModel.WIDE : SkinModel.SLIM,
                "fixture", "fixture:" + index, hash.toString(), file, Collections.<String>emptyList()));
        }
        return entries;
    }

    private static void cachePage(AuroraSkinsRuntime runtime, String query, SkinPage page) throws Exception {
        Constructor<?> constructor = Class.forName("com.aurora.skins.catalog.AuroraCatalogProvider$CacheEntry").getDeclaredConstructor(SkinPage.class, long.class);
        constructor.setAccessible(true);
        @SuppressWarnings("unchecked") Map<String, Object> cache = (Map<String, Object>)get(runtime.catalogProvider(), "cache");
        cache.put(query + "\n" + page.page() + "\n12", constructor.newInstance(page, System.currentTimeMillis()));
    }
    private static GuiMainMenu open(LegacySkinsUi ui) {
        GuiMainMenu screen = new GuiMainMenu(); Minecraft.func_71410_x().field_71462_r = screen;
        ui.onInit(new GuiScreenEvent.InitGuiEvent.Post(screen, screen.field_146292_n)); click(ui, screen, 0xB001); return screen;
    }
    private static void click(LegacySkinsUi ui, GuiScreen screen, int id) { ui.onAction(new GuiScreenEvent.ActionPerformedEvent.Pre(screen, button(screen, id))); }
    private static GuiScreenEvent.MouseInputEvent.Pre cardClick(LegacySkinsUi ui, GuiScreen screen, AuroraSkinsLayout.Slot slot) {
        Minecraft mc = Minecraft.func_71410_x(); int x = slot.x() + slot.width() / 2, y = slot.y() + slot.height() / 2;
        Mouse.x = (int)Math.ceil((double)x * mc.field_71443_c / screen.field_146294_l);
        Mouse.y = (int)Math.ceil((double)(screen.field_146295_m - y - 1) * mc.field_71440_d / screen.field_146295_m);
        Mouse.button = 0; Mouse.pressed = true;
        GuiScreenEvent.MouseInputEvent.Pre event = new GuiScreenEvent.MouseInputEvent.Pre(screen); ui.onMouse(event); return event;
    }
    private static Object state(LegacySkinsUi ui, GuiScreen screen) throws Exception { return ((Map<?, ?>)get(ui, "states")).get(screen); }
    private static Object get(Object target, String name) throws Exception { Field field = target.getClass().getDeclaredField(name); field.setAccessible(true); return field.get(target); }
    private static boolean booleanField(Object target, String name) { try { return (Boolean)get(target, name); } catch (Exception error) { throw new AssertionError(error); } }
    private static GuiButton button(GuiScreen screen, int id) { for (Object item : screen.field_146292_n) if (((GuiButton)item).field_146127_k == id) return (GuiButton)item; throw new AssertionError("missing button " + id); }
    private static boolean allReady(CatalogGridState grid, int count) { if (grid.loading()) return false; for (int i = 0; i < count; i++) if (grid.slot(i).status() != CatalogGridState.Status.READY) return false; return true; }
    private static void await(BooleanSupplier condition) throws Exception { long deadline = System.nanoTime() + 5_000_000_000L; while (!condition.getAsBoolean() && System.nanoTime() < deadline) { LegacyMinecraft.drainTasks(); Thread.sleep(5); } check(condition.getAsBoolean(), "async fixture delivery timed out"); }
    private static void check(boolean condition, String message) { if (!condition) throw new AssertionError(message); }
}
