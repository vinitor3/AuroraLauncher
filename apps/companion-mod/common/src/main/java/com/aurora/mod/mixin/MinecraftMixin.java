package com.aurora.mod.mixin;

import com.aurora.mod.AuroraCompanion;

import net.minecraft.client.Minecraft;

import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Usa o ciclo real do cliente; o remapeador converte os nomes para cada loader. */
@Mixin(Minecraft.class)
public abstract class MinecraftMixin {
    @Inject(method = "tick", at = @At("TAIL"), require = 1)
    private void aurora$clientTick(CallbackInfo callback) {
        Minecraft minecraft = (Minecraft) (Object) this;
        AuroraCompanion.tickOverlayShortcut(minecraft.getWindow().getWindow());
    }
}
