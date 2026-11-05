package dev.brasa.cosmetics;

import net.minecraft.client.network.AbstractClientPlayerEntity;
import net.minecraft.client.render.OverlayTexture;
import net.minecraft.client.render.RenderLayer;
import net.minecraft.client.render.VertexConsumer;
import net.minecraft.client.render.VertexConsumerProvider;
import net.minecraft.client.render.entity.feature.FeatureRenderer;
import net.minecraft.client.render.entity.feature.FeatureRendererContext;
import net.minecraft.client.render.entity.model.PlayerEntityModel;
import net.minecraft.client.util.math.MatrixStack;
import net.minecraft.entity.EquipmentSlot;
import net.minecraft.item.Items;

/** Dibuja sombreros, cosas a la espalda y mascotas pegados al modelo del jugador. */
final class CosmeticFeature extends FeatureRenderer<AbstractClientPlayerEntity, PlayerEntityModel<AbstractClientPlayerEntity>> {
    CosmeticFeature(FeatureRendererContext<AbstractClientPlayerEntity, PlayerEntityModel<AbstractClientPlayerEntity>> ctx) {
        super(ctx);
    }

    @Override
    public void render(MatrixStack matrices, VertexConsumerProvider vertexConsumers, int light, AbstractClientPlayerEntity player, float limbAngle, float limbDistance, float tickDelta, float animationProgress, float headYaw, float headPitch) {
        if (!State.isOwner(player) || player.isInvisible()) {
            return;
        }
        State.ensureLoaded();
        float t = State.now();
        PlayerEntityModel<AbstractClientPlayerEntity> model = getContextModel();
        for (State.Worn w : State.worn) {
            // con casco no se dibuja el sombrero; con élitros, nada a la espalda
            if ("hat".equals(w.slot()) && !player.getEquippedStack(EquipmentSlot.HEAD).isEmpty()) {
                continue;
            }
            if ("back".equals(w.slot()) && player.getEquippedStack(EquipmentSlot.CHEST).isOf(Items.ELYTRA)) {
                continue;
            }
            matrices.push();
            switch (w.attach()) {
                case "head" -> model.head.rotate(matrices);
                case "body" -> model.body.rotate(matrices);
                default -> {}
            }
            w.rig().animate(t);
            VertexConsumer vc = vertexConsumers.getBuffer(RenderLayer.getEntityTranslucent(w.texture()));
            w.rig().root.render(matrices, vc, light, OverlayTexture.DEFAULT_UV);
            matrices.pop();
        }
    }
}
