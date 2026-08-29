package com.aurora.mod.mixin;

import com.aurora.mod.appearance.AuroraSkinManager;

import net.minecraft.client.Minecraft;
import net.minecraft.client.player.AbstractClientPlayer;
import net.minecraft.resources.ResourceLocation;

import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

@Mixin(AbstractClientPlayer.class)
public abstract class AbstractClientPlayerMixin {
    private boolean aurora$isLocalPlayer() {
        Minecraft minecraft = Minecraft.getInstance();
        return minecraft.player != null
            && minecraft.player.getUUID().equals(((AbstractClientPlayer) (Object) this).getUUID());
    }

    @Inject(method = "getSkinTextureLocation", at = @At("HEAD"), cancellable = true, require = 0)
    private void aurora$skin(CallbackInfoReturnable<ResourceLocation> callback) {
        ResourceLocation location = AuroraSkinManager.skin();
        if (location != null && aurora$isLocalPlayer()) callback.setReturnValue(location);
    }

    @Inject(method = "getCloakTextureLocation", at = @At("HEAD"), cancellable = true, require = 0)
    private void aurora$cape(CallbackInfoReturnable<ResourceLocation> callback) {
        ResourceLocation location = AuroraSkinManager.cape();
        if (location != null && aurora$isLocalPlayer()) callback.setReturnValue(location);
    }

    @Inject(method = "getModelName", at = @At("HEAD"), cancellable = true, require = 0)
    private void aurora$model(CallbackInfoReturnable<String> callback) {
        if (aurora$isLocalPlayer()
            && "slim".equals(System.getProperty("aurora.profile.skinModel"))) {
            callback.setReturnValue("slim");
        }
    }
}
