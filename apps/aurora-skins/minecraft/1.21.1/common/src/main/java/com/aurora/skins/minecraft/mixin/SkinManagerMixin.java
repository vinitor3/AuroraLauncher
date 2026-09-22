package com.aurora.skins.minecraft.mixin;

import com.aurora.skins.minecraft.appearance.AuroraSkinManager;
import com.mojang.authlib.GameProfile;
import net.minecraft.client.resources.PlayerSkin;
import net.minecraft.client.resources.SkinManager;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

import java.util.function.Supplier;

@Mixin(SkinManager.class)
public abstract class SkinManagerMixin {
    @Inject(method = "getInsecureSkin", at = @At("RETURN"), cancellable = true, require = 1)
    private void auroraSkins$profileSkin(GameProfile profile, CallbackInfoReturnable<PlayerSkin> callback) {
        callback.setReturnValue(AuroraSkinManager.overrideSkin(profile.getId(), callback.getReturnValue()));
    }

    @Inject(method = "lookupInsecure", at = @At("RETURN"), cancellable = true, require = 1)
    private void auroraSkins$lookup(GameProfile profile, CallbackInfoReturnable<Supplier<PlayerSkin>> callback) {
        Supplier<PlayerSkin> original = callback.getReturnValue();
        // Consult Aurora on each invocation so cached suppliers follow skin changes and disconnects.
        callback.setReturnValue(() -> AuroraSkinManager.overrideSkin(profile.getId(), original.get()));
    }
}
