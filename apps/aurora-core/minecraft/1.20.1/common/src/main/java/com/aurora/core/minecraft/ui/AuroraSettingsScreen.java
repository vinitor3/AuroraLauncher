package com.aurora.core.minecraft.ui;

import com.aurora.core.api.Aurora;
import com.aurora.core.api.module.AuroraModuleMetadata;
import com.aurora.core.api.session.AuroraSession;
import com.aurora.core.api.ui.AuroraPlayerPreview;
import com.aurora.core.api.ui.AuroraSettingsPage;
import com.aurora.core.api.ui.AuroraUiContext;
import com.aurora.core.runtime.AuroraCoreRuntime;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphics;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.components.Tooltip;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.inventory.InventoryScreen;
import net.minecraft.network.chat.Component;

import java.util.List;

public final class AuroraSettingsScreen extends Screen {
    private static final int PANEL = 0xD91A112B;
    private static final int ACCENT = 0xFFB78CFF;
    private static final int TEXT = 0xFFF2EAFF;
    private static final int MUTED = 0xFFB9AACB;
    private final Screen parent;
    private final AuroraPlayerPreview preview = new AuroraPlayerPreview();
    private long lastFrame = System.nanoTime();
    private boolean avatarAvailable = true;

    public AuroraSettingsScreen(Screen parent) {
        super(Component.translatable("aurora_core.title"));
        this.parent = parent;
    }

    @Override protected void init() {
        int navigationX = Math.max(18, width / 2 - 230);
        int navigationY = 64;
        List<AuroraSettingsPage> pages = Aurora.services().settings().pages();
        int visible = Math.min(pages.size(), Math.max(1, (height - 110) / 24));
        for (int index = 0; index < visible; index++) {
            final AuroraSettingsPage page = pages.get(index);
            addRenderableWidget(Button.builder(Component.literal(icon(page.icon()) + "  " + page.title()),
                button -> openPage(page)).bounds(navigationX, navigationY + index * 24, 154, 20).build());
        }

        AuroraSettingsPage skins = findSkinsPage();
        if (skins != null) {
            Button skinButton = Button.builder(Component.translatable("aurora_core.change_skin"),
                button -> openPage(skins)).bounds(width / 2 + 55, height - 76, 92, 20).build();
            skinButton.setTooltip(Tooltip.create(Component.translatable("aurora_core.change_skin.tooltip")));
            addRenderableWidget(skinButton);
        }
        addRenderableWidget(Button.builder(Component.translatable("gui.done"), button -> onClose())
            .bounds(width / 2 - 75, height - 30, 150, 20).build());
    }

    private AuroraSettingsPage findSkinsPage() {
        if (!Aurora.services().modules().isInstalled("aurora_skins")) return null;
        java.util.Optional<AuroraSettingsPage> exact = Aurora.services().settings().find("aurora_skins");
        if (exact.isPresent()) return exact.get();
        for (AuroraSettingsPage page : Aurora.services().settings().pages()) {
            if ("aurora_skins".equals(page.owner())) return page;
        }
        return null;
    }

    private void openPage(AuroraSettingsPage page) {
        if ("aurora_general".equals(page.id())) return;
        try {
            page.opener().open(new AuroraUiContext() {
                @Override public void openAuroraHome() { minecraft.setScreen(new AuroraSettingsScreen(parent)); }
                @Override public void close() { minecraft.setScreen(parent); }
                @Override public Object nativeParentScreen() { return AuroraSettingsScreen.this; }
            });
        } catch (Throwable error) {
            AuroraCoreRuntime.instance().log().error("Settings page failed to open: " + page.id(), error);
        }
    }

    @Override public void render(GuiGraphics graphics, int mouseX, int mouseY, float partialTick) {
        renderBackground(graphics);
        int left = Math.max(8, width / 2 - 250);
        int right = Math.min(width - 8, width / 2 + 250);
        graphics.fill(left, 40, right, height - 40, PANEL);
        graphics.fill(left, 40, left + 3, height - 40, ACCENT);
        graphics.drawCenteredString(font, title, width / 2, 18, TEXT);
        graphics.drawString(font, Component.translatable("aurora_core.modules"), left + 18, 48, MUTED, false);

        int avatarX = width / 2 + 102;
        int avatarY = height - 88;
        preview.aim(mouseX, mouseY, avatarX, avatarY - 74, Math.max(160, width / 2), Math.max(120, height));
        long now = System.nanoTime();
        preview.update((now - lastFrame) / 1_000_000_000.0F);
        lastFrame = now;
        renderAvatar(graphics, avatarX, avatarY);
        renderStatus(graphics, left + 184, 58);
        super.render(graphics, mouseX, mouseY, partialTick);
    }

    private void renderAvatar(GuiGraphics graphics, int x, int y) {
        if (avatarAvailable && minecraft.player != null) {
            try {
                InventoryScreen.renderEntityInInventoryFollowsMouse(
                    graphics, x, y, Math.min(58, Math.max(36, height / 5)),
                    -preview.headYaw() * 0.65F, preview.headPitch() * 0.7F, minecraft.player);
                return;
            } catch (Throwable error) {
                avatarAvailable = false;
                AuroraCoreRuntime.instance().log().error("3D avatar failed; settings remain available.", error);
            }
        }
        graphics.fill(x - 28, y - 112, x + 28, y, 0x553F2C5E);
        graphics.drawCenteredString(font, Component.translatable("aurora_core.avatar_unavailable"), x, y - 58, MUTED);
    }

    private void renderStatus(GuiGraphics graphics, int x, int y) {
        AuroraSession session = Aurora.services().sessions().current();
        graphics.drawString(font, Component.translatable("aurora_core.welcome"), x, y, TEXT, false);
        String profile = session.username().isEmpty()
            ? Component.translatable("aurora_core.session.offline").getString()
            : session.username();
        graphics.drawString(font, profile, x, y + 18, MUTED, false);
        graphics.drawString(font, Component.translatable("aurora_core.launcher",
            Aurora.services().ipc().isConnected()
                ? Component.translatable("aurora_core.connected")
                : Component.translatable("aurora_core.disconnected")), x, y + 36, MUTED, false);
        graphics.drawString(font, Component.translatable("aurora_core.installed_modules",
            Aurora.services().modules().modules().size()), x, y + 54, MUTED, false);
        int moduleY = y + 78;
        for (AuroraModuleMetadata module : Aurora.services().modules().modules()) {
            if (moduleY > height - 118) break;
            graphics.drawString(font, "• " + module.name() + "  " + module.version(), x, moduleY, TEXT, false);
            moduleY += 14;
        }
    }

    private static String icon(String value) {
        if ("skin".equals(value)) return "◇";
        if ("assistant".equals(value)) return "✦";
        if ("audio".equals(value)) return "♪";
        return "•";
    }

    @Override public void onClose() { minecraft.setScreen(parent); }
    @Override public boolean isPauseScreen() { return false; }
}
