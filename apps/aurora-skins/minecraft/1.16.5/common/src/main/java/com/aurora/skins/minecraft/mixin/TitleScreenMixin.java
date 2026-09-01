package com.aurora.skins.minecraft.mixin;

import com.aurora.core.api.Aurora;
import com.aurora.core.api.ui.AuroraButtonLayout;
import com.aurora.core.api.ui.AuroraPlayerPreview;
import com.aurora.skins.minecraft.AuroraSkinsClient;
import com.aurora.skins.ui.AuroraTitleLayout;
import com.mojang.blaze3d.vertex.PoseStack;
import net.minecraft.client.gui.components.AbstractWidget;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.TranslatableComponent;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

import java.util.ArrayList;
import java.util.List;

@Mixin(value = TitleScreen.class, priority = 1)
public abstract class TitleScreenMixin extends Screen {
    private final AuroraPlayerPreview auroraSkins$preview = new AuroraPlayerPreview();
    private long auroraSkins$lastFrame = System.nanoTime();
    protected TitleScreenMixin(Component title) { super(title); }

    @Inject(method = "init", at = @At("TAIL"), require = 0)
    private void auroraSkins$addButton(CallbackInfo callback) {
        if (!Aurora.isAvailable() || !Aurora.services().modules().isInstalled("aurora_skins")) return;
        List<AuroraButtonLayout.Rect> occupied = new ArrayList<AuroraButtonLayout.Rect>();
        for (AbstractWidget widget : buttons) occupied.add(new AuroraButtonLayout.Rect(widget.x, widget.y, widget.getWidth(), widget.getHeight()));
        AuroraButtonLayout.Slot slot = AuroraButtonLayout.findSlot(width, height, occupied);
        addButton(new Button(slot.x(), slot.y(), slot.width(), slot.height(), new TranslatableComponent("aurora_skins.title_button"),
            button -> AuroraSkinsClient.open((Screen) (Object) this)));
    }

    @Inject(method = "render", at = @At("TAIL"), require = 0)
    private void auroraSkins$renderPreview(PoseStack pose, int mouseX, int mouseY, float partialTick, CallbackInfo callback) {
        if (!Aurora.isAvailable() || !Aurora.services().modules().isInstalled("aurora_skins")) return;
        AuroraTitleLayout layout = AuroraTitleLayout.calculate(width, height);
        int x = layout.previewX(), y = layout.previewY(), scale = layout.previewScale();
        fill(pose, x - 42, y - scale * 2 - 12, x + 42, y + 5, 0x661A112B);
        fill(pose, x - 42, y - scale * 2 - 12, x - 39, y + 5, 0xFFB78CFF);
        auroraSkins$preview.aim(mouseX, mouseY, x, y - scale, 180.0F, 150.0F);
        long now = System.nanoTime(); auroraSkins$preview.update((now - auroraSkins$lastFrame) / 1_000_000_000.0F); auroraSkins$lastFrame = now;
        Aurora.services().previews().render(pose, x, y, scale, auroraSkins$preview);
    }
}
