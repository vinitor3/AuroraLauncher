package com.aurora.skins.minecraft.mixin;

import com.aurora.skins.minecraft.appearance.AuroraSkinManager;
import net.minecraft.client.Minecraft;
import net.minecraft.client.player.AbstractClientPlayer;
import net.minecraft.resources.ResourceLocation;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

@Mixin(AbstractClientPlayer.class)
public abstract class AbstractClientPlayerMixin {
    private boolean auroraSkins$isLocalPlayer() {
        Minecraft minecraft = Minecraft.getInstance();
        return minecraft.player != null && minecraft.player.getUUID().equals(((AbstractClientPlayer) (Object) this).getUUID());
    }
    @Inject(method = "getSkinTextureLocation", at = @At("HEAD"), cancellable = true, require = 0)
    private void auroraSkins$skin(CallbackInfoReturnable<ResourceLocation> callback) {
        ResourceLocation location = AuroraSkinManager.skin();
        if (location != null && auroraSkins$isLocalPlayer()) callback.setReturnValue(location);
    }
    @Inject(method = "getModelName", at = @At("HEAD"), cancellable = true, require = 0)
    private void auroraSkins$model(CallbackInfoReturnable<String> callback) {
        if (auroraSkins$isLocalPlayer()) callback.setReturnValue(AuroraSkinManager.model());
    }
}
