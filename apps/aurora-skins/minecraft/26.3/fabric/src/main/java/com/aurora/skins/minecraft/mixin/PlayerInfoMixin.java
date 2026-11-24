package com.aurora.skins.minecraft.mixin;

import com.aurora.skins.minecraft.appearance.AuroraSkinManager;
import net.minecraft.client.multiplayer.PlayerInfo;
import net.minecraft.world.entity.player.PlayerSkin;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

@Mixin(PlayerInfo.class)
public abstract class PlayerInfoMixin {
    @Inject(method = "getSkin", at = @At("RETURN"), cancellable = true, require = 1)
    private void auroraSkins$skin(CallbackInfoReturnable<PlayerSkin> callback) {
        PlayerSkin original = callback.getReturnValue();
        PlayerSkin selected = AuroraSkinManager.overrideSkin(((PlayerInfo) (Object) this).getProfile().id(), original);
        if (selected != original) callback.setReturnValue(selected);
    }
}