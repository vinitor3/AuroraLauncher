package com.aurora.skins.minecraft.mixin;

import com.aurora.skins.minecraft.appearance.AuroraSkinManager;
import net.minecraft.client.multiplayer.PlayerInfo;
import net.minecraft.resources.ResourceLocation;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Chat heads and the player list use PlayerInfo without visiting a player entity. */
@Mixin(PlayerInfo.class)
public abstract class PlayerInfoMixin {
    @Inject(method = "getSkinLocation", at = @At("HEAD"), cancellable = true, require = 1)
    private void auroraSkins$skin(CallbackInfoReturnable<ResourceLocation> callback) {
        ResourceLocation location = AuroraSkinManager.skin(((PlayerInfo) (Object) this).getProfile().getId());
        if (location != null) callback.setReturnValue(location);
    }

    @Inject(method = "getModelName", at = @At("HEAD"), cancellable = true, require = 1)
    private void auroraSkins$model(CallbackInfoReturnable<String> callback) {
        String model = AuroraSkinManager.model(((PlayerInfo) (Object) this).getProfile().getId());
        if (model != null) callback.setReturnValue("slim".equals(model) ? "slim" : "default");
    }

    @Inject(method = "isSkinLoaded", at = @At("HEAD"), cancellable = true, require = 1)
    private void auroraSkins$loaded(CallbackInfoReturnable<Boolean> callback) {
        if (AuroraSkinManager.skin(((PlayerInfo) (Object) this).getProfile().getId()) != null) {
            callback.setReturnValue(true);
        }
    }
}