package com.aurora.core.minecraft.mixin;

import com.aurora.core.minecraft.ui.AuroraSettingsScreen;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.screens.OptionsScreen;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.network.chat.Component;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(OptionsScreen.class)
public abstract class OptionsScreenMixin extends Screen {
    protected OptionsScreenMixin(Component title) { super(title); }

    @Inject(method = "init", at = @At("TAIL"), require = 0)
    private void aurora$addOptionsButton(CallbackInfo callback) {
        final Screen parent = (Screen) (Object) this;
        addRenderableWidget(Button.builder(Component.translatable("aurora_core.options"), button ->
            Minecraft.getInstance().setScreen(new AuroraSettingsScreen(parent)))
            .bounds(width / 2 - 100, height - 52, 200, 20)
            .build());
    }
}
