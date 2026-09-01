package com.aurora.skins.minecraft.mixin;

import com.aurora.skins.minecraft.appearance.AuroraSkinManager;
import net.minecraft.client.Minecraft;
import net.minecraft.client.player.AbstractClientPlayer;
import net.minecraft.client.resources.PlayerSkin;
import net.minecraft.resources.ResourceLocation;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

@Mixin(AbstractClientPlayer.class)
public abstract class AbstractClientPlayerMixin {
    @Inject(method = "getSkin", at = @At("RETURN"), cancellable = true, require = 0)
    private void auroraSkins$skin(CallbackInfoReturnable<PlayerSkin> callback) {
        Minecraft minecraft = Minecraft.getInstance();
        AbstractClientPlayer player = (AbstractClientPlayer) (Object) this;
        ResourceLocation texture = AuroraSkinManager.skin();
        if (texture == null || minecraft.player == null || !minecraft.player.getUUID().equals(player.getUUID())) return;
        PlayerSkin original = callback.getReturnValue();
        PlayerSkin.Model model = "slim".equals(AuroraSkinManager.model()) ? PlayerSkin.Model.SLIM : PlayerSkin.Model.WIDE;
        callback.setReturnValue(new PlayerSkin(texture, "aurora-local",
            original == null ? null : original.capeTexture(), original == null ? null : original.elytraTexture(), model, true));
    }
}
