package com.aurora.core.minecraft.ui;

import com.aurora.core.api.Aurora;
import com.aurora.core.api.module.AuroraModuleMetadata;
import com.aurora.core.api.session.AuroraSession;
import com.aurora.core.api.ui.AuroraPlayerPreview;
import com.aurora.core.api.ui.AuroraSettingsPage;
import com.aurora.core.api.ui.AuroraSettingsLayout;
import com.aurora.core.api.ui.AuroraUiContext;
import com.aurora.core.runtime.AuroraCoreRuntime;
import com.mojang.blaze3d.vertex.PoseStack;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.inventory.InventoryScreen;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.TextComponent;
import net.minecraft.network.chat.TranslatableComponent;
import java.util.List;

public final class AuroraSettingsScreen extends Screen {
    private static final int PANEL = 0xD91A112B, ACCENT = 0xFFB78CFF, TEXT = 0xFFF2EAFF, MUTED = 0xFFB9AACB;
    private final Screen parent;
    private final AuroraPlayerPreview preview = new AuroraPlayerPreview();
    private long lastFrame = System.nanoTime();
    private boolean avatarAvailable = true;
    private Button skinButton;

    public AuroraSettingsScreen(Screen parent) {
        super(new TranslatableComponent("aurora_core.title"));
        this.parent = parent;
    }

    @Override protected void init() {
        AuroraSettingsLayout layout = AuroraSettingsLayout.calculate(width, height);
        int navigationX = layout.navigationX();
        List<AuroraSettingsPage> pages = Aurora.services().settings().pages();
        int visible = Math.min(pages.size(), Math.max(1, (height - 110) / 24));
        for (int index = 0; index < visible; index++) {
            final AuroraSettingsPage page = pages.get(index);
            addButton(new Button(navigationX, 64 + index * 24, layout.navigationWidth(), 20,
                new TextComponent(icon(page.icon()) + "  " + page.title()), button -> openPage(page)));
        }
        final AuroraSettingsPage skins = findSkinsPage();
        if (skins != null) {
            skinButton = addButton(new Button(layout.avatarX() - 46, height - 76, 92, 20,
                new TranslatableComponent("aurora_core.change_skin"), button -> openPage(skins)));
        }
        addButton(new Button(width / 2 - 75, height - 30, 150, 20,
            new TranslatableComponent("gui.done"), button -> onClose()));
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
        } catch (Throwable error) {
            AuroraCoreRuntime.instance().log().error("Settings page failed to open: " + page.id(), error);
        }
    }

    @Override public void render(PoseStack pose, int mouseX, int mouseY, float partialTick) {
        renderBackground(pose);
        AuroraSettingsLayout layout = AuroraSettingsLayout.calculate(width, height);
        int left = layout.panelLeft(), right = layout.panelRight();
        drawCenteredString(pose, font, title, width / 2, 18, TEXT);
        drawString(pose, font, new TranslatableComponent("aurora_core.modules"), left + 18, 48, MUTED);

        int avatarX = layout.avatarX(), avatarY = layout.avatarY();
        preview.aim(mouseX, mouseY, avatarX, avatarY - 74, Math.max(160, width / 2), Math.max(120, height));
        long now = System.nanoTime();
        preview.update((now - lastFrame) / 1_000_000_000.0F);
        lastFrame = now;
        renderAvatar(pose, avatarX, avatarY);
        renderStatus(pose, layout.contentX(), 58, layout.contentWidth());
        super.render(pose, mouseX, mouseY, partialTick);
        if (skinButton != null && skinButton.isHovered()) {
            renderTooltip(pose, new TranslatableComponent("aurora_core.change_skin.tooltip"), mouseX, mouseY);
        }
    }

    private void renderAvatar(PoseStack pose, int x, int y) {
        if (avatarAvailable && minecraft.player != null) {
            try {
                InventoryScreen.renderEntityInInventory(x, y, Math.min(58, Math.max(36, height / 5)),
                    -preview.headYaw() * 0.65F, preview.headPitch() * 0.7F, minecraft.player);
                return;
            } catch (Throwable error) {
                avatarAvailable = false;
                AuroraCoreRuntime.instance().log().error("3D avatar failed; settings remain available.", error);
            }
        }
        if (avatarAvailable && Aurora.services().previews().render(pose, x, y,
                Math.min(58, Math.max(36, height / 5)), preview)) return;
        drawCenteredString(pose, font, new TranslatableComponent("aurora_core.avatar_unavailable"), x, y - 58, MUTED);
    }

    private void renderStatus(PoseStack pose, int x, int y, int maxWidth) {
        AuroraSession session = Aurora.services().sessions().current();
        drawString(pose, font, fit(new TranslatableComponent("aurora_core.welcome").getString(), maxWidth), x, y, TEXT);
        drawString(pose, font, fit(session.username().isEmpty()
            ? new TranslatableComponent("aurora_core.session.offline").getString() : session.username(), maxWidth), x, y + 18, MUTED);
        drawString(pose, font, fit(new TranslatableComponent("aurora_core.launcher", new TranslatableComponent(
            Aurora.services().ipc().isConnected() ? "aurora_core.connected" : "aurora_core.disconnected")).getString(), maxWidth), x, y + 36, MUTED);
        drawString(pose, font, fit(new TranslatableComponent("aurora_core.installed_modules",
            Aurora.services().modules().modules().size()).getString(), maxWidth), x, y + 54, MUTED);
        int moduleY = y + 78;
        for (AuroraModuleMetadata module : Aurora.services().modules().modules()) {
            if (moduleY > height - 118) break;
            drawString(pose, font, fit("• " + module.name() + "  " + module.version(), maxWidth), x, moduleY, TEXT);
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
        if ("skin".equals(value)) return "◇";
        if ("assistant".equals(value)) return "✦";
        if ("audio".equals(value)) return "♪";
        return "•";
    }

    @Override public void onClose() { minecraft.setScreen(parent); }
    @Override public boolean isPauseScreen() { return false; }
}
