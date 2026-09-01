package com.aurora.skins.legacy;

import com.aurora.core.api.ui.AuroraPlayerPreview;
import com.aurora.core.api.ui.AuroraPreviewRenderer;

import java.lang.reflect.Constructor;
import java.lang.reflect.Field;
import java.lang.reflect.Method;

final class LegacySkinPreviewRenderer implements AuroraPreviewRenderer {
    private Object playerModel;
    private String builtForModel = "";

    @Override public boolean render(Object nativeContext, int x, int y, int scale, AuroraPlayerPreview pose) {
        try {
            Object texture = LegacySkinManager.texture();
            if (texture == null) texture = LegacySkinManager.defaultTexture();
            if (texture == null) return false;
            String modelName = LegacySkinManager.model();
            if (playerModel == null || !builtForModel.equals(modelName)) {
                Class<?> type = Class.forName("net.minecraft.client.model.ModelPlayer");
                Constructor<?> constructor = type.getConstructor(float.class, boolean.class);
                playerModel = constructor.newInstance(0.0F, "slim".equals(modelName));
                builtForModel = modelName;
            }
            Object minecraft = LegacyMinecraft.minecraft();
            Object manager = LegacyMinecraft.invoke(minecraft, new String[] {"getTextureManager", "func_110434_K"});
            LegacyMinecraft.invoke(manager, new String[] {"bindTexture", "func_110577_a"}, texture);

            staticInvoke("net.minecraft.client.renderer.GlStateManager", new String[] {"pushMatrix", "func_179094_E"});
            staticInvoke("net.minecraft.client.renderer.GlStateManager", new String[] {"enableDepth", "func_179126_j"});
            staticInvoke("net.minecraft.client.renderer.GlStateManager", new String[] {"enableBlend", "func_179147_l"});
            staticInvoke("net.minecraft.client.renderer.GlStateManager", new String[] {"color", "func_179131_c"}, 1F, 1F, 1F, 1F);
            staticInvoke("net.minecraft.client.renderer.GlStateManager", new String[] {"translate", "func_179109_b"},
                (float)x, (float)(y - scale * 1.5F), 100F);
            staticInvoke("net.minecraft.client.renderer.GlStateManager", new String[] {"scale", "func_179152_a"}, (float)scale, (float)scale, (float)-scale);
            staticInvoke("net.minecraft.client.renderer.GlStateManager", new String[] {"rotate", "func_179114_b"}, pose.bodyYaw(), 0F, 1F, 0F);
            staticInvoke("net.minecraft.client.renderer.RenderHelper", new String[] {"enableGUIStandardItemLighting", "func_74519_b"});

            Object head = field(playerModel, "bipedHead", "field_78116_c");
            setFloat(head, radians(pose.headPitch()), "rotateAngleX", "field_78795_f");
            setFloat(head, radians(pose.headYaw() - pose.bodyYaw()), "rotateAngleY", "field_78796_g");
            Object headwear = field(playerModel, "bipedHeadwear", "field_78114_d");
            copyAngles(head, headwear);
            for (String[] names : parts()) renderPart(field(playerModel, names));

            staticInvoke("net.minecraft.client.renderer.RenderHelper", new String[] {"disableStandardItemLighting", "func_74518_a"});
            staticInvoke("net.minecraft.client.renderer.GlStateManager", new String[] {"popMatrix", "func_179121_F"});
            return true;
        } catch (Throwable error) {
            System.err.println("[Aurora Skins] Preview legado indisponível: " + error.getClass().getSimpleName());
            try { staticInvoke("net.minecraft.client.renderer.GlStateManager", new String[] {"popMatrix", "func_179121_F"}); }
            catch (Throwable ignored) { }
            return false;
        }
    }

    private static String[][] parts() {
        return new String[][] {
            {"bipedHead", "field_78116_c"}, {"bipedHeadwear", "field_78114_d"},
            {"bipedBody", "field_78115_e"}, {"bipedRightArm", "field_78112_f"},
            {"bipedLeftArm", "field_78113_g"}, {"bipedRightLeg", "field_78123_h"},
            {"bipedLeftLeg", "field_78124_i"}, {"bipedBodyWear", "field_178730_v"},
            {"bipedRightArmwear", "field_178732_b"}, {"bipedLeftArmwear", "field_178734_a"},
            {"bipedRightLegwear", "field_178731_d"}, {"bipedLeftLegwear", "field_178733_c"}
        };
    }

    private static void renderPart(Object part) throws Exception {
        if (part == null) return;
        for (String name : new String[] {"render", "func_78785_a"}) {
            try { Method method = part.getClass().getMethod(name, float.class); method.invoke(part, 0.0625F); return; }
            catch (NoSuchMethodException ignored) { }
        }
    }

    private static void copyAngles(Object source, Object target) {
        if (source == null || target == null) return;
        setFloat(target, number(field(source, "rotateAngleX", "field_78795_f")), "rotateAngleX", "field_78795_f");
        setFloat(target, number(field(source, "rotateAngleY", "field_78796_g")), "rotateAngleY", "field_78796_g");
        setFloat(target, number(field(source, "rotateAngleZ", "field_78808_h")), "rotateAngleZ", "field_78808_h");
    }

    private static Object field(Object target, String... names) { return LegacyMinecraft.field(target, names); }
    private static float number(Object value) { return value instanceof Number ? ((Number)value).floatValue() : 0F; }
    private static float radians(float degrees) { return degrees * ((float)Math.PI / 180F); }
    private static void setFloat(Object target, float value, String... names) {
        if (target == null) return;
        Class<?> type = target.getClass();
        while (type != null) {
            for (String name : names) try { Field field = type.getDeclaredField(name); field.setAccessible(true); field.setFloat(target, value); return; }
            catch (ReflectiveOperationException ignored) { }
            type = type.getSuperclass();
        }
    }

    private static Object staticInvoke(String className, String[] names, Object... arguments) throws Exception {
        Class<?> type = Class.forName(className);
        for (String name : names) for (Method method : type.getMethods()) {
            if (!method.getName().equals(name) || method.getParameterTypes().length != arguments.length) continue;
            try { method.setAccessible(true); return method.invoke(null, arguments); }
            catch (IllegalArgumentException ignored) { }
        }
        throw new NoSuchMethodException(className + "." + names[0]);
    }
}
