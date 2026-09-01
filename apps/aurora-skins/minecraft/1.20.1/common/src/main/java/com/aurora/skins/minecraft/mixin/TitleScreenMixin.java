package com.aurora.skins.minecraft.mixin;

import com.aurora.core.api.Aurora;
import com.aurora.core.api.ui.AuroraPlayerPreview;
import com.aurora.skins.minecraft.AuroraSkinsClient;
import com.aurora.skins.ui.AuroraTitleLayout;
import net.minecraft.client.gui.GuiGraphics;
import net.minecraft.client.gui.components.AbstractWidget;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.components.events.GuiEventListener;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.network.chat.Component;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

import java.util.ArrayList;
import java.util.List;

@Mixin(value = TitleScreen.class, priority = 1)
public abstract class TitleScreenMixin extends Screen {
    @Unique private final AuroraPlayerPreview auroraSkins$preview = new AuroraPlayerPreview();
    @Unique private long auroraSkins$lastFrame = System.nanoTime();
    @Unique private AuroraTitleLayout auroraSkins$titleLayout;

    protected TitleScreenMixin(Component title) { super(title); }

    @Inject(method = "init", at = @At("TAIL"), require = 0)
    private void auroraSkins$addButton(CallbackInfo callback) {
        if (!Aurora.isAvailable() || !Aurora.services().modules().isInstalled("aurora_skins")) return;
        List<AuroraTitleLayout.Rect> occupied = new ArrayList<AuroraTitleLayout.Rect>();
        for (GuiEventListener listener : children()) {
            if (listener instanceof AbstractWidget) {
                AbstractWidget widget = (AbstractWidget) listener;
                occupied.add(new AuroraTitleLayout.Rect(
                    widget.getX(), widget.getY(), widget.getWidth(), widget.getHeight()));
            }
        }
        auroraSkins$titleLayout = AuroraTitleLayout.calculate(width, height, occupied);
        addRenderableWidget(Button.builder(Component.translatable("aurora_skins.title_button"),
            button -> AuroraSkinsClient.open((Screen) (Object) this))
            .bounds(auroraSkins$titleLayout.buttonX(), auroraSkins$titleLayout.buttonY(),
                auroraSkins$titleLayout.buttonWidth(), auroraSkins$titleLayout.buttonHeight()).build());
    }

    @Inject(method = "render", at = @At("TAIL"), require = 0)
    private void auroraSkins$renderPreview(GuiGraphics graphics, int mouseX, int mouseY,
                                           float partialTick, CallbackInfo callback) {
        if (!Aurora.isAvailable() || !Aurora.services().modules().isInstalled("aurora_skins")) return;
        AuroraTitleLayout layout = auroraSkins$titleLayout == null
            ? AuroraTitleLayout.calculate(width, height) : auroraSkins$titleLayout;
        int x = layout.previewX();
        int y = layout.previewY();
        int scale = layout.previewScale();
        graphics.fill(layout.panelLeft(), layout.panelTop(), layout.panelRight(), layout.panelBottom(), 0x661A112B);
        graphics.fill(layout.panelLeft(), layout.panelTop(), layout.panelLeft() + 3, layout.panelBottom(), 0xFFB78CFF);
        auroraSkins$preview.aim(mouseX, mouseY, x, y - scale, 180.0F, 150.0F);
        long now = System.nanoTime();
        auroraSkins$preview.update((now - auroraSkins$lastFrame) / 1_000_000_000.0F);
        auroraSkins$lastFrame = now;
        Aurora.services().previews().render(graphics, x, y, scale, auroraSkins$preview);
    }
}
