package dev.brasa.cosmetics;

import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.fabricmc.fabric.api.client.keybinding.v1.KeyBindingHelper;
import net.fabricmc.fabric.api.client.rendering.v1.LivingEntityFeatureRendererRegistrationCallback;
import net.fabricmc.loader.api.FabricLoader;
import net.minecraft.client.option.KeyBinding;
import net.minecraft.client.render.entity.PlayerEntityRenderer;
import net.minecraft.client.util.InputUtil;
import org.lwjgl.glfw.GLFW;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * Cosméticos de Brasa Launcher para Fabric 1.21–1.21.1. Lee lo que el launcher deja en
 * {@code config/brasa/cosmetics/} y lo dibuja sólo en tu jugador (nadie más los ve).
 */
public final class BrasaCosmetics implements ClientModInitializer {
    public static final Logger LOG = LoggerFactory.getLogger("brasa-cosmetics");
    static KeyBinding WHEEL;
    static KeyBinding TOGGLE;

    @Override
    public void onInitializeClient() {
        State.init(FabricLoader.getInstance().getConfigDir().resolve("brasa").resolve("cosmetics"));

        WHEEL = KeyBindingHelper.registerKeyBinding(new KeyBinding("key.brasa-cosmetics.emotes", InputUtil.Type.KEYSYM, GLFW.GLFW_KEY_B, "category.brasa-cosmetics"));
        TOGGLE = KeyBindingHelper.registerKeyBinding(new KeyBinding("key.brasa-cosmetics.toggle", InputUtil.Type.KEYSYM, GLFW.GLFW_KEY_UNKNOWN, "category.brasa-cosmetics"));

        LivingEntityFeatureRendererRegistrationCallback.EVENT.register((entityType, entityRenderer, helper, context) -> {
            if (entityRenderer instanceof PlayerEntityRenderer player) {
                helper.register(new CosmeticFeature(player));
            }
        });

        ClientTickEvents.END_CLIENT_TICK.register(client -> {
            while (WHEEL.wasPressed()) {
                if (client.currentScreen == null && client.player != null) {
                    client.setScreen(new EmoteWheelScreen());
                }
            }
            while (TOGGLE.wasPressed()) {
                State.visible = !State.visible;
                Emotes.stop();
            }
            Emotes.tick(client);
        });
    }
}
