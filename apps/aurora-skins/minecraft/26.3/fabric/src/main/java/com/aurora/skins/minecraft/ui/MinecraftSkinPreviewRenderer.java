package com.aurora.skins.minecraft.ui;

import com.aurora.core.api.ui.AuroraPlayerPreview;
import com.aurora.core.api.ui.AuroraPreviewRenderer;
import com.aurora.skins.minecraft.appearance.AuroraSkinManager;
import com.aurora.skins.ui.PreviewRotation;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.render.pip.GuiSkinRenderer;
import net.minecraft.client.model.geom.ModelLayers;
import net.minecraft.client.model.geom.ModelPart;
import net.minecraft.client.model.Model;
import net.minecraft.client.renderer.rendertype.RenderTypes;
import net.minecraft.client.resources.DefaultPlayerSkin;
import net.minecraft.resources.Identifier;

import java.util.UUID;
import java.util.HashMap;
import java.util.Map;

public final class MinecraftSkinPreviewRenderer implements AuroraPreviewRenderer {
    private static final AuroraPlayerPreview CARD_POSE = new AuroraPlayerPreview();
    // Vanilla reuses one render target for all skin widgets. Catalog cards are submitted
    // in one frame and each needs a distinct target until its draw has been consumed.
    private static final Map<Identifier, GuiSkinRenderer> CARD_TARGETS = new HashMap<>();

    public static GuiSkinRenderer cardTarget(Identifier texture) { return CARD_TARGETS.get(texture); }

    public static void closeCardTargets() {
        CARD_TARGETS.values().forEach(GuiSkinRenderer::close);
        CARD_TARGETS.clear();
    }

    /** Catalog cards must never fall back to the equipped/default skin. */
    public boolean isReady() { return previewTexture != null; }

    /** Fixed frontal pose, independently owned texture and clipping for one card. */
    public boolean renderCard(Object nativeContext, int x, int y, int scale,
                              int left, int top, int right, int bottom) {
        if (!isReady() || !(nativeContext instanceof GuiGraphicsExtractor)) return false;
        CARD_TARGETS.computeIfAbsent(previewTexture, unused -> new GuiSkinRenderer());
        GuiGraphicsExtractor graphics = (GuiGraphicsExtractor) nativeContext;
        graphics.enableScissor(left, top, right, bottom);
        try { return render(graphics, x, y, scale, CARD_POSE); }
        finally { graphics.disableScissor(); }
    }
    private final String textureOwner = UUID.randomUUID().toString();
    private Identifier previewTexture;
    private String previewModel = "classic";
    public void setSkin(com.aurora.skins.api.SkinEntry entry) throws java.io.IOException {
        close();
        if (entry == null || entry.file() == null) return;
        try (java.io.InputStream input = java.nio.file.Files.newInputStream(entry.file())) {
            com.mojang.blaze3d.platform.NativeImage image = AuroraSkinManager.normalizeLegacySkin(
                com.mojang.blaze3d.platform.NativeImage.read(input));
            previewTexture = Identifier.fromNamespaceAndPath("aurora_skins", "preview/" + textureOwner + "/" + entry.contentHash());
            Minecraft.getInstance().getTextureManager().register(previewTexture,
                new net.minecraft.client.renderer.texture.DynamicTexture(previewTexture::toString, image));
            previewModel = entry.model().id();
        }
    }
    public void close() {
        GuiSkinRenderer target = CARD_TARGETS.remove(previewTexture);
        if (target != null) target.close();
        if (previewTexture != null) Minecraft.getInstance().getTextureManager().release(previewTexture);
        previewTexture = null;
    }
    private Model.Simple wide, slim;
    @Override public boolean render(Object context, int x, int y, int scale, AuroraPlayerPreview pose) {
        if (!(context instanceof GuiGraphicsExtractor graphics)) return false;
        Minecraft minecraft = Minecraft.getInstance();
        if (wide == null) {
            wide = new Model.Simple(minecraft.getEntityModels().bakeLayer(ModelLayers.PLAYER), RenderTypes::entityTranslucent);
            slim = new Model.Simple(minecraft.getEntityModels().bakeLayer(ModelLayers.PLAYER_SLIM), RenderTypes::entityTranslucent);
        }
        Model.Simple model = "slim".equals(previewTexture == null ? AuroraSkinManager.model() : previewModel) ? slim : wide;
        Identifier texture = previewTexture == null ? AuroraSkinManager.skin() : previewTexture;
        if (texture == null) texture = DefaultPlayerSkin.get(new UUID(0L, 0L)).body().texturePath();
        int halfWidth = Math.max(4, scale);
        int height = Math.max(8, Math.round(scale * 2.125f));
        // The 26.3 renderer queues a model for a separate picture-in-picture pass.
        // Each preview owns its model so another card cannot change its head pose.
        ModelPart head = model.root().getChild("head");
        head.yRot = PreviewRotation.headYaw(pose.headYaw(), pose.bodyYaw());
        head.xRot = PreviewRotation.headPitch(pose.headPitch());
        graphics.skin(model, texture, scale, 0, pose.bodyYaw(), -1.0625f,
            x - halfWidth, y - height, x + halfWidth, y);
        return true;
    }
}
