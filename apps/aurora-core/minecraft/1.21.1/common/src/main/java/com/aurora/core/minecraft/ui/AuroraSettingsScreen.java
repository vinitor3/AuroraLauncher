package com.aurora.core.minecraft.ui;

import com.aurora.core.api.Aurora;
import com.aurora.core.api.module.AuroraModuleMetadata;
import com.aurora.core.api.session.AuroraSession;
import com.aurora.core.api.ui.AuroraPlayerPreview;
import com.aurora.core.api.ui.AuroraSettingsPage;
import com.aurora.core.api.ui.AuroraSettingsLayout;
import com.aurora.core.api.ui.AuroraUiContext;
import com.aurora.core.runtime.AuroraCoreRuntime;
import net.minecraft.client.gui.GuiGraphics;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.components.Tooltip;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.inventory.InventoryScreen;
import net.minecraft.network.chat.Component;
import java.util.List;

public final class AuroraSettingsScreen extends Screen {
    private static final int PANEL = 0xD91A112B, ACCENT = 0xFFB78CFF, TEXT = 0xFFF2EAFF, MUTED = 0xFFB9AACB;
    private final Screen parent;
    private final AuroraPlayerPreview preview = new AuroraPlayerPreview();
    private long lastFrame = System.nanoTime();
    private boolean avatarAvailable = true;

    public AuroraSettingsScreen(Screen parent) { super(Component.translatable("aurora_core.title")); this.parent = parent; }

    @Override protected void init() {
        AuroraSettingsLayout layout = AuroraSettingsLayout.calculate(width, height);
        int navigationX = layout.navigationX();
        List<AuroraSettingsPage> pages = Aurora.services().settings().pages();
        int visible = Math.min(pages.size(), Math.max(1, (height - 110) / 24));
        for (int index = 0; index < visible; index++) {
            final AuroraSettingsPage page = pages.get(index);
            addRenderableWidget(Button.builder(Component.literal(icon(page.icon()) + "  " + page.title()), button -> openPage(page))
                .bounds(navigationX, 64 + index * 24, layout.navigationWidth(), 20).build());
        }
        AuroraSettingsPage skins = findSkinsPage();
        if (skins != null) {
            Button button = Button.builder(Component.translatable("aurora_core.change_skin"), ignored -> openPage(skins))
                .bounds(layout.avatarX() - 46, height - 76, 92, 20).build();
            button.setTooltip(Tooltip.create(Component.translatable("aurora_core.change_skin.tooltip")));
            addRenderableWidget(button);
        }
        addRenderableWidget(Button.builder(Component.translatable("gui.done"), ignored -> onClose())
            .bounds(width / 2 - 75, height - 30, 150, 20).build());
    }

    private AuroraSettingsPage findSkinsPage() {
        if (!Aurora.services().modules().isInstalled("aurora_skins")) return null;
        java.util.Optional<AuroraSettingsPage> exact = Aurora.services().settings().find("aurora_skins");
        if (exact.isPresent()) return exact.get();
        for (AuroraSettingsPage page : Aurora.services().settings().pages()) if ("aurora_skins".equals(page.owner())) return page;
        return null;
    }

    private void openPage(final AuroraSettingsPage page) {
        if ("aurora_general".equals(page.id())) return;
        try {
            page.opener().open(new AuroraUiContext() {
                @Override public void openAuroraHome() { minecraft.setScreen(new AuroraSettingsScreen(parent)); }
                @Override public void close() { minecraft.setScreen(parent); }
                @Override public Object nativeParentScreen() { return AuroraSettingsScreen.this; }
            });
        } catch (Throwable error) { AuroraCoreRuntime.instance().log().error("Settings page failed to open: " + page.id(), error); }
    }

    @Override public void render(GuiGraphics graphics, int mouseX, int mouseY, float partialTick) {
        super.renderBackground(graphics, mouseX, mouseY, partialTick);
        AuroraSettingsLayout layout = AuroraSettingsLayout.calculate(width, height);
        int left = layout.panelLeft(), right = layout.panelRight();
        graphics.drawCenteredString(font, title, width / 2, 18, TEXT);
        graphics.drawString(font, Component.translatable("aurora_core.modules"), left + 18, 48, MUTED, false);
        int avatarX = layout.avatarX(), avatarY = layout.avatarY();
        preview.aim(mouseX, mouseY, avatarX, avatarY - 74, Math.max(160, width / 2), Math.max(120, height));
        long now = System.nanoTime();
        preview.update((now - lastFrame) / 1_000_000_000.0F);
        lastFrame = now;
        renderAvatar(graphics, avatarX, avatarY);
        renderStatus(graphics, layout.contentX(), 58, layout.contentWidth());
        super.render(graphics, mouseX, mouseY, partialTick);
    }

    private void renderAvatar(GuiGraphics graphics, int x, int y) {
        if (avatarAvailable && minecraft.player != null) {
            try {
                int scale = Math.min(58, Math.max(36, height / 5));
                InventoryScreen.renderEntityInInventoryFollowsMouse(graphics, x - 70, y - 130, x + 70, y,
                    scale, 0.0F, -preview.headYaw() * 0.65F, preview.headPitch() * 0.7F, minecraft.player);
                return;
            } catch (Throwable error) {
                avatarAvailable = false;
                AuroraCoreRuntime.instance().log().error("3D avatar failed; settings remain available.", error);
            }
        }
        if (avatarAvailable && Aurora.services().previews().render(graphics, x, y,
                Math.min(58, Math.max(36, height / 5)), preview)) return;
        graphics.drawCenteredString(font, Component.translatable("aurora_core.avatar_unavailable"), x, y - 58, MUTED);
    }

    // Screen.render calls this again in 1.21; only the initial pass may blur.
    @Override public void renderBackground(GuiGraphics graphics, int mouseX, int mouseY, float partialTick) { }

    private void renderStatus(GuiGraphics graphics, int x, int y, int maxWidth) {
        AuroraSession session = Aurora.services().sessions().current();
        graphics.drawString(font, fit(Component.translatable("aurora_core.welcome").getString(), maxWidth), x, y, TEXT, false);
        graphics.drawString(font, fit(session.username().isEmpty()
            ? Component.translatable("aurora_core.session.offline").getString() : session.username(), maxWidth), x, y + 18, MUTED, false);
        graphics.drawString(font, fit(Component.translatable("aurora_core.launcher", Component.translatable(
            Aurora.services().ipc().isConnected() ? "aurora_core.connected" : "aurora_core.disconnected")).getString(), maxWidth), x, y + 36, MUTED, false);
        graphics.drawString(font, fit(Component.translatable("aurora_core.installed_modules",
            Aurora.services().modules().modules().size()).getString(), maxWidth), x, y + 54, MUTED, false);
        int moduleY = y + 78;
        for (AuroraModuleMetadata module : Aurora.services().modules().modules()) {
            if (moduleY > height - 118) break;
            graphics.drawString(font, fit("• " + module.name() + "  " + module.version(), maxWidth), x, moduleY, TEXT, false);
            moduleY += 14;
        }
    }

    private String fit(String value, int maxWidth) {
        if (font.width(value) <= maxWidth) return value;
        String shortened = value;
        while (!shortened.isEmpty() && font.width(shortened + "…") > maxWidth) {
            shortened = shortened.substring(0, shortened.length() - 1);
        }
        return shortened + "…";
    }

    private static String icon(String value) {
        if ("skin".equals(value)) return "◇"; if ("audio".equals(value)) return "♪"; return "•";
    }
    @Override public void onClose() { minecraft.setScreen(parent); }
    @Override public boolean isPauseScreen() { return false; }
}
