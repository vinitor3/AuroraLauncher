package com.aurora.skins.minecraft.ui;

import com.aurora.core.api.ui.AuroraPlayerPreview;
import com.aurora.core.api.ui.AuroraPreviewRenderer;
import com.aurora.skins.minecraft.appearance.AuroraSkinManager;
import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.blaze3d.vertex.VertexConsumer;
import com.mojang.math.Axis;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphics;
import net.minecraft.client.model.PlayerModel;
import net.minecraft.client.model.geom.ModelLayers;
import net.minecraft.client.player.AbstractClientPlayer;
import net.minecraft.client.renderer.MultiBufferSource;
import net.minecraft.client.renderer.RenderType;
import net.minecraft.client.renderer.texture.OverlayTexture;
import net.minecraft.client.resources.DefaultPlayerSkin;
import net.minecraft.resources.ResourceLocation;

import java.util.UUID;

public final class MinecraftSkinPreviewRenderer implements AuroraPreviewRenderer {
    private final String textureOwner = UUID.randomUUID().toString();
    private ResourceLocation previewTexture;
    private String previewModel = "classic";
    public void setSkin(com.aurora.skins.api.SkinEntry entry) throws java.io.IOException {
        close();
        if (entry == null || entry.file() == null) return;
        try (java.io.InputStream input = java.nio.file.Files.newInputStream(entry.file())) {
            com.mojang.blaze3d.platform.NativeImage image = AuroraSkinManager.normalizeLegacySkin(
                com.mojang.blaze3d.platform.NativeImage.read(input));
            previewTexture = ResourceLocation.fromNamespaceAndPath("aurora_skins", "preview/" + textureOwner + "/" + entry.contentHash());
            Minecraft.getInstance().getTextureManager().register(previewTexture,
                new net.minecraft.client.renderer.texture.DynamicTexture(image));
            previewModel = entry.model().id();
        }
    }
    public void close() {
        if (previewTexture != null) Minecraft.getInstance().getTextureManager().release(previewTexture);
        previewTexture = null;
    }
    private PlayerModel<AbstractClientPlayer> wide, slim;
    @Override public boolean render(Object nativeContext, int x, int y, int scale, AuroraPlayerPreview pose) {
        if (!(nativeContext instanceof GuiGraphics graphics)) return false;
        try {
            Minecraft minecraft = Minecraft.getInstance();
            if (wide == null) {
                wide = new PlayerModel<>(minecraft.getEntityModels().bakeLayer(ModelLayers.PLAYER), false);
                slim = new PlayerModel<>(minecraft.getEntityModels().bakeLayer(ModelLayers.PLAYER_SLIM), true);
            }
            PlayerModel<AbstractClientPlayer> model = "slim".equals(previewTexture == null ? AuroraSkinManager.model() : previewModel) ? slim : wide;
            ResourceLocation texture = previewTexture == null ? AuroraSkinManager.skin() : previewTexture;
            if (texture == null) texture = DefaultPlayerSkin.get(new UUID(0L, 0L)).texture();
            graphics.flush(); PoseStack matrices = graphics.pose(); matrices.pushPose();
            try {
            com.mojang.blaze3d.platform.Lighting.setupForFlatItems();
            RenderSystem.setShaderColor(1.0F, 1.0F, 1.0F, 1.0F);
            matrices.translate(x, y - scale * 1.5D, 120.0D);
            matrices.scale(scale, scale, -scale); matrices.mulPose(Axis.YP.rotationDegrees(com.aurora.skins.ui.PreviewRotation.bodyYaw(pose.bodyYaw())));
            reset(model); model.head.yRot = com.aurora.skins.ui.PreviewRotation.headYaw(pose.headYaw(), pose.bodyYaw()); model.head.xRot = com.aurora.skins.ui.PreviewRotation.headPitch(pose.headPitch()); model.hat.copyFrom(model.head);
            RenderSystem.enableDepthTest(); MultiBufferSource.BufferSource buffers = minecraft.renderBuffers().bufferSource();
            VertexConsumer vertices = buffers.getBuffer(RenderType.entityTranslucent(texture));
            model.renderToBuffer(matrices, vertices, 0xF000F0, OverlayTexture.NO_OVERLAY); buffers.endBatch(); return true;
            } finally {
                matrices.popPose();
                RenderSystem.disableDepthTest();
                com.mojang.blaze3d.platform.Lighting.setupFor3DItems();
            }
        } catch (Throwable error) { System.err.println("[Aurora Skins] Preview 3D indisponível: " + error.getClass().getSimpleName()); return false; }
    }
    private static void reset(PlayerModel<?> model) {
        model.young=false; model.crouching=false; model.riding=false; model.attackTime=0;
        model.head.xRot=model.head.yRot=model.head.zRot=0; model.hat.copyFrom(model.head); model.body.xRot=model.body.yRot=model.body.zRot=0;
        model.rightArm.xRot=model.rightArm.yRot=model.rightArm.zRot=0; model.leftArm.xRot=model.leftArm.yRot=model.leftArm.zRot=0;
        model.rightLeg.xRot=model.rightLeg.yRot=model.rightLeg.zRot=0; model.leftLeg.xRot=model.leftLeg.yRot=model.leftLeg.zRot=0;
    }
    private static float radians(float degrees) { return degrees * ((float)Math.PI / 180.0F); }
}
