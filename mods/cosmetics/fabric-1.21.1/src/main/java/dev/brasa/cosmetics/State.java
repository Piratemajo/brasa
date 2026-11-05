package dev.brasa.cosmetics;

import com.mojang.blaze3d.systems.RenderSystem;
import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.client.MinecraftClient;
import net.minecraft.client.network.AbstractClientPlayerEntity;
import net.minecraft.client.texture.NativeImage;
import net.minecraft.client.texture.NativeImageBackedTexture;
import net.minecraft.util.Identifier;
import net.minecraft.util.Util;

/** Lo que lleva puesto el jugador y sus texturas, ya cargadas en la GPU. */
final class State {
    static Loadout loadout;
    static Path dir;
    static boolean visible = true;

    record Worn(String slot, String attach, Rig rig, Identifier texture) {}

    static final List<Worn> worn = new ArrayList<>();
    static final List<Identifier> capeFrames = new ArrayList<>();
    private static boolean loaded;

    private State() {}

    static void init(Path cosmeticsDir) {
        dir = cosmeticsDir;
        loadout = Loadout.read(dir.resolve("loadout.json"));
        if (loadout != null) {
            BrasaCosmetics.LOG.info("[Brasa] Cosméticos de {}: capa {}, {} objetos, {} emotes", loadout.player, loadout.cape == null ? "no" : loadout.cape.name, loadout.items.size(), loadout.emotes.size());
        }
    }

    static boolean isOwner(AbstractClientPlayerEntity p) {
        return visible && loadout != null && loadout.player != null && p.getGameProfile().getName().equalsIgnoreCase(loadout.player);
    }

    static float now() {
        return Util.getMeasuringTimeMs() / 1000f;
    }

    /** Sube las texturas la primera vez que hacen falta (tiene que ser en el hilo de render). */
    static void ensureLoaded() {
        if (loaded || loadout == null || !RenderSystem.isOnRenderThread()) {
            return;
        }
        loaded = true;
        try {
            if (loadout.cape != null) {
                for (String f : loadout.cape.frames) {
                    capeFrames.add(texture(f));
                }
            }
            for (Loadout.Item it : loadout.items) {
                if (it.model == null) {
                    continue;
                }
                worn.add(new Worn(it.slot, it.model.attach, Rig.build(it.model), texture(it.texture)));
            }
        } catch (Exception e) {
            BrasaCosmetics.LOG.error("[Brasa] No se pudieron cargar los cosméticos", e);
            worn.clear();
            capeFrames.clear();
        }
    }

    private static Identifier texture(String file) throws Exception {
        String name = file.replace(".png", "").toLowerCase().replaceAll("[^a-z0-9_]", "_");
        Identifier id = Identifier.of("brasa-cosmetics", "worn/" + name);
        try (InputStream in = Files.newInputStream(dir.resolve(file))) {
            NativeImage img = NativeImage.read(in);
            MinecraftClient.getInstance().getTextureManager().registerTexture(id, new NativeImageBackedTexture(img));
        }
        return id;
    }

    /** Fotograma actual de la capa, o {@code null} si no lleva. */
    static Identifier cape(AbstractClientPlayerEntity p) {
        if (!isOwner(p)) {
            return null;
        }
        ensureLoaded();
        if (capeFrames.isEmpty()) {
            return null;
        }
        float fps = Math.max(loadout.cape.fps, 0.0001f);
        int frame = capeFrames.size() == 1 ? 0 : (int) (now() * fps) % capeFrames.size();
        return capeFrames.get(frame);
    }
}
