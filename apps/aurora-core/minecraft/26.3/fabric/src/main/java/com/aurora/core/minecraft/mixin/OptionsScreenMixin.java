package com.aurora.core.minecraft.mixin;
import com.aurora.core.api.ui.AuroraButtonLayout;
import com.aurora.core.minecraft.ui.AuroraSettingsScreen;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.components.AbstractWidget;
import net.minecraft.client.gui.components.events.GuiEventListener;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.options.OptionsScreen;
import net.minecraft.network.chat.Component;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import java.util.ArrayList;
import java.util.List;
@Mixin(value = OptionsScreen.class, priority = 1)
public abstract class OptionsScreenMixin extends Screen {
    protected OptionsScreenMixin(Component title) { super(title); }
    @Inject(method = "init", at = @At("TAIL"), require = 0)
    private void aurora$addOptionsButton(CallbackInfo callback) {
        final Screen parent = (Screen) (Object) this;
        List<AuroraButtonLayout.Rect> occupied = new ArrayList<AuroraButtonLayout.Rect>();
        for (GuiEventListener listener : children()) {
            if (listener instanceof AbstractWidget) {
                AbstractWidget widget = (AbstractWidget) listener;
                occupied.add(new AuroraButtonLayout.Rect(widget.getX(), widget.getY(), widget.getWidth(), widget.getHeight()));
            }
        }
        AuroraButtonLayout.Slot slot = AuroraButtonLayout.findSlot(width, height, occupied);
        addRenderableWidget(Button.builder(Component.translatable("aurora_core.options"),
            button -> Minecraft.getInstance().gui.setScreen(new AuroraSettingsScreen(parent)))
            .bounds(slot.x(), slot.y(), slot.width(), slot.height()).build());
    }
}
