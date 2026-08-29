package com.aurora.mod.modern;

import com.aurora.mod.AuroraCompanion;

import java.lang.reflect.Method;

import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Pseudo;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Adaptador dual: nome intermediary no Fabric e nome oficial no Forge 1.21.1. */
@Pseudo
@Mixin(targets = { "net.minecraft.class_310", "net.minecraft.client.Minecraft" })
public abstract class AuroraMinecraftMixin {
    @Inject(method = { "method_1574", "tick" }, at = @At("TAIL"), require = 0)
    private void aurora$clientTick(CallbackInfo callback) {
        long handle = findWindowHandle(this);
        if (handle != 0L) AuroraCompanion.tickOverlayShortcut(handle);
    }

    private static long findWindowHandle(Object minecraft) {
        Object window = invokeAny(minecraft, "getWindow", "method_22683");
        Object handle = invokeAny(window, "getWindow", "getHandle", "method_4490");
        return handle instanceof Number ? ((Number) handle).longValue() : 0L;
    }

    private static Object invokeAny(Object target, String... names) {
        if (target == null) return null;
        for (String name : names) {
            try {
                Method method = target.getClass().getMethod(name);
                method.setAccessible(true);
                return method.invoke(target);
            } catch (ReflectiveOperationException ignored) { }
        }
        return null;
    }
}
