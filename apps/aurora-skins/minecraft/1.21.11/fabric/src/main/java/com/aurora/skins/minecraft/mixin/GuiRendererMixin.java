package com.aurora.skins.minecraft.mixin;

import com.aurora.skins.minecraft.ui.MinecraftSkinPreviewRenderer;
import net.minecraft.client.gui.render.GuiRenderer;
import net.minecraft.client.gui.render.pip.GuiSkinRenderer;
import net.minecraft.client.gui.render.state.GuiRenderState;
import net.minecraft.client.gui.render.state.pip.GuiSkinRenderState;
import net.minecraft.client.gui.render.state.pip.PictureInPictureRenderState;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(GuiRenderer.class)
public abstract class GuiRendererMixin {
    @Shadow @Final private GuiRenderState renderState;

    @Inject(method = "preparePictureInPictureState", at = @At("HEAD"), cancellable = true, require = 1)
    private void auroraSkins$prepareCard(PictureInPictureRenderState state, int scale, CallbackInfo callback) {
        if (state instanceof GuiSkinRenderState skin) {
            GuiSkinRenderer renderer = MinecraftSkinPreviewRenderer.cardTarget(skin.texture());
            if (renderer != null) {
                renderer.prepare(skin, renderState, scale);
                callback.cancel();
            }
        }
    }

    @Inject(method = "close", at = @At("HEAD"), require = 1)
    private void auroraSkins$releaseCards(CallbackInfo callback) {
        MinecraftSkinPreviewRenderer.closeCardTargets();
    }
}
