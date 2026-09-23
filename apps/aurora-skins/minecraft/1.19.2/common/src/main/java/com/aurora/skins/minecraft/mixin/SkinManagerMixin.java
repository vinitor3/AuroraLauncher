package com.aurora.skins.minecraft.mixin;

import com.aurora.skins.minecraft.appearance.AuroraSkinManager;
import com.mojang.authlib.GameProfile;
import net.minecraft.client.resources.SkinManager;
import net.minecraft.resources.ResourceLocation;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

@Mixin(SkinManager.class)
public abstract class SkinManagerMixin {
    @Inject(method = "getInsecureSkinLocation", at = @At("HEAD"), cancellable = true, require = 1)
    private void auroraSkins$profileSkin(GameProfile profile, CallbackInfoReturnable<ResourceLocation> callback) {
        ResourceLocation location = AuroraSkinManager.skin(profile.getId());
        if (location != null) callback.setReturnValue(location);
    }
}