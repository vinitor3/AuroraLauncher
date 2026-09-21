package com.aurora.skins.minecraft.ui;

import com.aurora.core.api.ui.AuroraPlayerPreview;
import com.aurora.core.api.ui.AuroraPreviewRenderer;
import com.aurora.skins.minecraft.appearance.AuroraSkinManager;
import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.blaze3d.vertex.VertexConsumer;
import com.mojang.math.Vector3f;
import net.minecraft.client.Minecraft;
import net.minecraft.client.model.PlayerModel;
import net.minecraft.client.player.AbstractClientPlayer;
import net.minecraft.client.renderer.MultiBufferSource;
import net.minecraft.client.renderer.RenderType;
import net.minecraft.client.renderer.texture.OverlayTexture;
import net.minecraft.client.resources.DefaultPlayerSkin;
import net.minecraft.resources.ResourceLocation;

import java.util.UUID;

public final class MinecraftSkinPreviewRenderer implements AuroraPreviewRenderer {
    private static final AuroraPlayerPreview CARD_POSE = new AuroraPlayerPreview();

    /** Catalog cards must never fall back to the equipped/default skin. */
    public boolean isReady() { return previewTexture != null; }

    /** Fixed frontal pose, independently owned texture and clipping for one card. */
    public boolean renderCard(Object nativeContext, int x, int y, int scale,
                              int left, int top, int right, int bottom) {
        if (!isReady() || !(nativeContext instanceof PoseStack)) return false;
        PoseStack graphics = (PoseStack) nativeContext;
        boolean depth = org.lwjgl.opengl.GL11.glIsEnabled(org.lwjgl.opengl.GL11.GL_DEPTH_TEST);
        boolean blend = org.lwjgl.opengl.GL11.glIsEnabled(org.lwjgl.opengl.GL11.GL_BLEND);
        boolean cull = org.lwjgl.opengl.GL11.glIsEnabled(org.lwjgl.opengl.GL11.GL_CULL_FACE);
        boolean depthMask = org.lwjgl.opengl.GL11.glGetBoolean(org.lwjgl.opengl.GL11.GL_DEPTH_WRITEMASK);
        int sourceRgb = org.lwjgl.opengl.GL11.glGetInteger(org.lwjgl.opengl.GL14.GL_BLEND_SRC_RGB);
        int destRgb = org.lwjgl.opengl.GL11.glGetInteger(org.lwjgl.opengl.GL14.GL_BLEND_DST_RGB);
        int sourceAlpha = org.lwjgl.opengl.GL11.glGetInteger(org.lwjgl.opengl.GL14.GL_BLEND_SRC_ALPHA);
        int destAlpha = org.lwjgl.opengl.GL11.glGetInteger(org.lwjgl.opengl.GL14.GL_BLEND_DST_ALPHA);
        float[] color = new float[4];
        org.lwjgl.opengl.GL11.glGetFloatv(org.lwjgl.opengl.GL11.GL_CURRENT_COLOR, color);
        boolean clipped = org.lwjgl.opengl.GL11.glIsEnabled(org.lwjgl.opengl.GL11.GL_SCISSOR_TEST);
        int[] scissor = new int[4];
        org.lwjgl.opengl.GL11.glGetIntegerv(org.lwjgl.opengl.GL11.GL_SCISSOR_BOX, scissor);
        Minecraft minecraft = Minecraft.getInstance();
        double guiScale = minecraft.getWindow().getGuiScale();
        int clipLeft = (int) Math.floor(left * guiScale);
        int clipRight = (int) Math.ceil(right * guiScale);
        int clipBottom = minecraft.getWindow().getHeight() - (int) Math.ceil(bottom * guiScale);
        int clipTop = minecraft.getWindow().getHeight() - (int) Math.floor(top * guiScale);
        if (clipped) {
            clipLeft = Math.max(clipLeft, scissor[0]); clipRight = Math.min(clipRight, scissor[0] + scissor[2]);
            clipBottom = Math.max(clipBottom, scissor[1]); clipTop = Math.min(clipTop, scissor[1] + scissor[3]);
        }
        RenderSystem.enableScissor(clipLeft, clipBottom, Math.max(0, clipRight - clipLeft), Math.max(0, clipTop - clipBottom));
        try { return render(graphics, x, y, scale, CARD_POSE); }
        finally {
            if (clipped) RenderSystem.enableScissor(scissor[0], scissor[1], scissor[2], scissor[3]);
            else RenderSystem.disableScissor();
            if (depth) RenderSystem.enableDepthTest(); else RenderSystem.disableDepthTest();
            if (blend) RenderSystem.enableBlend(); else RenderSystem.disableBlend();
            if (cull) RenderSystem.enableCull(); else RenderSystem.disableCull();
            RenderSystem.depthMask(depthMask);
            RenderSystem.blendFuncSeparate(sourceRgb, destRgb, sourceAlpha, destAlpha);
            RenderSystem.color4f(color[0], color[1], color[2], color[3]);
        }
    }
    private final String textureOwner = UUID.randomUUID().toString();
    private ResourceLocation previewTexture;
    private String previewModel = "classic";
    public void setSkin(com.aurora.skins.api.SkinEntry entry) throws java.io.IOException {
        close();
        if (entry == null || entry.file() == null) return;
        try (java.io.InputStream input = java.nio.file.Files.newInputStream(entry.file())) {
            com.mojang.blaze3d.platform.NativeImage image = AuroraSkinManager.normalizeLegacySkin(
                com.mojang.blaze3d.platform.NativeImage.read(input));
            previewTexture = new ResourceLocation("aurora_skins", "preview/" + textureOwner + "/" + entry.contentHash());
            Minecraft.getInstance().getTextureManager().register(previewTexture,
                new net.minecraft.client.renderer.texture.DynamicTexture(image));
            previewModel = entry.model().id();
        }
    }
    public void close() {
        if (previewTexture != null) Minecraft.getInstance().getTextureManager().release(previewTexture);
        previewTexture = null;
    }
    private PlayerModel<AbstractClientPlayer> wide;
    private PlayerModel<AbstractClientPlayer> slim;

    @Override public boolean render(Object nativeContext, int x, int y, int scale, AuroraPlayerPreview pose) {
        if (!(nativeContext instanceof PoseStack)) return false;
        try {
            if (wide == null) {
                wide = new PlayerModel<AbstractClientPlayer>(0.0F, false);
                slim = new PlayerModel<AbstractClientPlayer>(0.0F, true);
            }
            PlayerModel<AbstractClientPlayer> model = "slim".equals(previewTexture == null ? AuroraSkinManager.model() : previewModel) ? slim : wide;
            ResourceLocation texture = previewTexture == null ? AuroraSkinManager.skin() : previewTexture;
            if (texture == null) texture = DefaultPlayerSkin.getDefaultSkin(new UUID(0L, 0L));
            PoseStack matrices = (PoseStack) nativeContext;
            matrices.pushPose();
            try {
            com.mojang.blaze3d.platform.Lighting.setupForFlatItems();
            RenderSystem.enableTexture();
            RenderSystem.color4f(1.0F, 1.0F, 1.0F, 1.0F);
            matrices.translate(x, y - scale * 1.5D, 120.0D);
            matrices.scale(scale, scale, -scale);
            matrices.mulPose(Vector3f.YP.rotationDegrees(com.aurora.skins.ui.PreviewRotation.bodyYaw(pose.bodyYaw())));
            reset(model);
            model.head.yRot = com.aurora.skins.ui.PreviewRotation.headYaw(pose.headYaw(), pose.bodyYaw());
            model.head.xRot = com.aurora.skins.ui.PreviewRotation.headPitch(pose.headPitch());
            model.hat.copyFrom(model.head);
            RenderSystem.enableDepthTest();
            Minecraft minecraft = Minecraft.getInstance();
            MultiBufferSource.BufferSource buffers = minecraft.renderBuffers().bufferSource();
            VertexConsumer vertices = buffers.getBuffer(RenderType.entityTranslucent(texture));
            model.renderToBuffer(matrices, vertices, 0xF000F0, OverlayTexture.NO_OVERLAY,
                1.0F, 1.0F, 1.0F, 1.0F);
            buffers.endBatch();
            return true;
            } finally {
                matrices.popPose();
                RenderSystem.disableDepthTest();
                com.mojang.blaze3d.platform.Lighting.setupFor3DItems();
            }
        } catch (Throwable error) {
            System.err.println("[Aurora Skins] Preview 3D indisponível: " + error.getClass().getSimpleName());
            return false;
        }
    }

    private static void reset(PlayerModel<?> model) {
        model.young = false; model.crouching = false; model.riding = false; model.attackTime = 0.0F;
        model.head.xRot = model.head.yRot = model.head.zRot = 0.0F; model.hat.copyFrom(model.head);
        model.body.xRot = model.body.yRot = model.body.zRot = 0.0F;
        model.rightArm.xRot = model.rightArm.yRot = model.rightArm.zRot = 0.0F;
        model.leftArm.xRot = model.leftArm.yRot = model.leftArm.zRot = 0.0F;
        model.rightLeg.xRot = model.rightLeg.yRot = model.rightLeg.zRot = 0.0F;
        model.leftLeg.xRot = model.leftLeg.yRot = model.leftLeg.zRot = 0.0F;
    }
    private static float radians(float degrees) { return degrees * ((float) Math.PI / 180.0F); }
}
