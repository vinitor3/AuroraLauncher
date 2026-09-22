package com.aurora.skins.minecraft.mixin;

import com.aurora.skins.minecraft.appearance.AuroraSkinManager;
import net.minecraft.client.player.AbstractClientPlayer;
import net.minecraft.client.resources.PlayerSkin;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

@Mixin(AbstractClientPlayer.class)
public abstract class AbstractClientPlayerMixin {
    @Inject(method = "getSkin", at = @At("RETURN"), cancellable = true, require = 1)
    private void auroraSkins$skin(CallbackInfoReturnable<PlayerSkin> callback) {
        PlayerSkin original = callback.getReturnValue();
        PlayerSkin selected = AuroraSkinManager.overrideSkin(((AbstractClientPlayer) (Object) this).getUUID(), original);
        if (selected != original) callback.setReturnValue(selected);
    }
}