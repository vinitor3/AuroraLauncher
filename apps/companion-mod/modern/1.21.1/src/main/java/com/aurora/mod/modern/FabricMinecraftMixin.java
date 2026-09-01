package com.aurora.mod.modern;

import com.aurora.mod.AuroraCompanion;

import java.lang.reflect.Method;

import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Pseudo;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Adaptador do ciclo do cliente no namespace intermediary usado pelo Fabric. */
@Pseudo
@Mixin(targets = "net.minecraft.class_310")
public abstract class FabricMinecraftMixin {
    @Inject(method = "method_1574", at = @At("TAIL"), require = 0)
    private void aurora$clientTick(CallbackInfo callback) {
        long handle = findWindowHandle(this);
        if (handle != 0L) AuroraCompanion.tickOverlayShortcut(handle);
    }

    private static long findWindowHandle(Object minecraft) {
        Object window = invokeAny(minecraft, "method_22683", "getWindow");
        Object handle = invokeAny(window, "method_4490", "getWindow", "getHandle");
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
