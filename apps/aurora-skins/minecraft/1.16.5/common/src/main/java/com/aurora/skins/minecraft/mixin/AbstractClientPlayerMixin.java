package com.aurora.skins.minecraft.mixin;

import com.aurora.skins.minecraft.appearance.AuroraSkinManager;
import net.minecraft.client.player.AbstractClientPlayer;
import net.minecraft.resources.ResourceLocation;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

@Mixin(AbstractClientPlayer.class)
public abstract class AbstractClientPlayerMixin {
    @Inject(method = "getSkinTextureLocation", at = @At("HEAD"), cancellable = true, require = 1)
    private void auroraSkins$skin(CallbackInfoReturnable<ResourceLocation> callback) {
        ResourceLocation location = AuroraSkinManager.skin(((AbstractClientPlayer) (Object) this).getUUID());
        if (location != null) callback.setReturnValue(location);
    }

    @Inject(method = "getCloakTextureLocation", at = @At("HEAD"), cancellable = true, require = 1)
    private void auroraSkins$cape(CallbackInfoReturnable<ResourceLocation> callback) {
        ResourceLocation cape = AuroraSkinManager.cape(((AbstractClientPlayer) (Object) this).getUUID());
        if (cape != null) callback.setReturnValue(cape);
    }

    @Inject(method = "getModelName", at = @At("HEAD"), cancellable = true, require = 1)
    private void auroraSkins$model(CallbackInfoReturnable<String> callback) {
        String model = AuroraSkinManager.model(((AbstractClientPlayer) (Object) this).getUUID());
        if (model != null) callback.setReturnValue("slim".equals(model) ? "slim" : "default");
    }
}