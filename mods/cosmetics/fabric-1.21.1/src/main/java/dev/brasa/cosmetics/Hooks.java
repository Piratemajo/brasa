package dev.brasa.cosmetics;

import net.minecraft.client.network.AbstractClientPlayerEntity;
import net.minecraft.client.render.entity.model.PlayerEntityModel;
import net.minecraft.util.Identifier;

/** Puerta pública para los mixins (el resto del mod es privado al paquete). */
public final class Hooks {
    private Hooks() {}

    public static Identifier cape(AbstractClientPlayerEntity p) {
        return State.cape(p);
    }

    public static void emote(PlayerEntityModel<?> model, AbstractClientPlayerEntity p) {
        if (State.isOwner(p)) {
            Emotes.apply(model);
        }
    }
}
