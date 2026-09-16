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
import net.minecraft.client.model.geom.ModelLayers;
import net.minecraft.client.player.AbstractClientPlayer;
import net.minecraft.client.renderer.MultiBufferSource;
import net.minecraft.client.renderer.RenderType;
import net.minecraft.client.renderer.texture.OverlayTexture;
import net.minecraft.client.resources.DefaultPlayerSkin;
import net.minecraft.resources.ResourceLocation;

import java.util.UUID;

public final class MinecraftSkinPreviewRenderer implements AuroraPreviewRenderer {
    private ResourceLocation previewTexture;
    private String previewModel = "classic";
    public void setSkin(com.aurora.skins.api.SkinEntry entry) throws java.io.IOException {
        close();
        if (entry == null || entry.file() == null) return;
        try (java.io.InputStream input = java.nio.file.Files.newInputStream(entry.file())) {
            com.mojang.blaze3d.platform.NativeImage image = AuroraSkinManager.normalizeLegacySkin(
                com.mojang.blaze3d.platform.NativeImage.read(input));
            previewTexture = new ResourceLocation("aurora_skins", "preview/" + entry.contentHash());
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
            Minecraft minecraft = Minecraft.getInstance();
            if (wide == null) {
                wide = new PlayerModel<AbstractClientPlayer>(minecraft.getEntityModels().bakeLayer(ModelLayers.PLAYER), false);
                slim = new PlayerModel<AbstractClientPlayer>(minecraft.getEntityModels().bakeLayer(ModelLayers.PLAYER_SLIM), true);
            }
            PlayerModel<AbstractClientPlayer> model = "slim".equals(previewTexture == null ? AuroraSkinManager.model() : previewModel) ? slim : wide;
            ResourceLocation texture = previewTexture == null ? AuroraSkinManager.skin() : previewTexture;
            if (texture == null) texture = DefaultPlayerSkin.getDefaultSkin(new UUID(0L, 0L));
            PoseStack matrices = (PoseStack) nativeContext;
            matrices.pushPose();
            matrices.translate(x, y - scale * 1.5D, 120.0D);
            matrices.scale(scale, scale, -scale);
            matrices.mulPose(Vector3f.YP.rotationDegrees(pose.bodyYaw()));
            reset(model);
            model.head.yRot = radians(pose.headYaw() - pose.bodyYaw());
            model.head.xRot = radians(pose.headPitch());
            model.hat.copyFrom(model.head);
            RenderSystem.enableDepthTest();
            MultiBufferSource.BufferSource buffers = minecraft.renderBuffers().bufferSource();
            VertexConsumer vertices = buffers.getBuffer(RenderType.entityTranslucent(texture));
            model.renderToBuffer(matrices, vertices, 0xF000F0, OverlayTexture.NO_OVERLAY,
                1.0F, 1.0F, 1.0F, 1.0F);
            buffers.endBatch();
            matrices.popPose();
            return true;
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
