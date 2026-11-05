package dev.brasa.cosmetics.mixin;

import dev.brasa.cosmetics.Hooks;
import net.minecraft.client.network.AbstractClientPlayerEntity;
import net.minecraft.client.util.SkinTextures;
import net.minecraft.util.Identifier;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** La capa de la Taquilla sustituye a la del jugador (y sirve también para los élitros). */
@Mixin(AbstractClientPlayerEntity.class)
public abstract class SkinTexturesMixin {
    @Inject(method = "getSkinTextures", at = @At("RETURN"), cancellable = true)
    private void brasa$cape(CallbackInfoReturnable<SkinTextures> cir) {
        Identifier cape = Hooks.cape((AbstractClientPlayerEntity) (Object) this);
        if (cape == null) {
            return;
        }
        SkinTextures s = cir.getReturnValue();
        cir.setReturnValue(new SkinTextures(s.texture(), s.textureUrl(), cape, cape, s.model(), s.secure()));
    }
}
