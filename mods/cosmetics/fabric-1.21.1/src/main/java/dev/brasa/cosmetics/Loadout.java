package dev.brasa.cosmetics;

import com.google.gson.Gson;
import java.io.Reader;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;

/**
 * Lo que escribe Brasa Launcher en {@code config/brasa/cosmetics/loadout.json} antes de lanzar.
 * Coordenadas en el sistema de Brasa (ver {@code src/cosmetics/mod.rs} en el launcher):
 * x hacia la izquierda del jugador, y hacia arriba desde los pies, z hacia delante; grados.
 */
public final class Loadout {
    /** Versión del formato que entiende este mod. */
    public static final int FORMAT = 1;

    public int format;
    public String generator;
    public String player;
    public Cape cape;
    public List<Item> items = new ArrayList<>();
    public List<EmoteOut> emotes = new ArrayList<>();

    public static final class Cape {
        public String id;
        public String name;
        public List<String> frames = new ArrayList<>();
        public float fps;
    }

    public static final class Item {
        public String id;
        public String name;
        /** hat, back o pet. */
        public String slot;
        public String texture;
        public ModelDef model;
    }

    public static final class ModelDef {
        public int[] texture_size = {64, 64};
        /** head, body o beside. */
        public String attach;
        public List<BoneDef> bones = new ArrayList<>();
    }

    public static final class BoneDef {
        public String name;
        public float[] pivot = {0, 0, 0};
        public float[] rot = {0, 0, 0};
        public List<CubeDef> cubes = new ArrayList<>();
        public Motion motion;
        public List<BoneDef> children = new ArrayList<>();
    }

    public static final class CubeDef {
        public float[] from;
        public float[] size;
        public int[] uv;
        public float inflate;
    }

    public static final class Motion {
        /** swing, bob o spin. */
        public String kind;
        public String axis;
        public float amp;
        public float speed;
        public float phase;
    }

    public static final class EmoteOut {
        public String id;
        public String name;
        public Emote emote;
    }

    public static final class Emote {
        public float duration;
        public boolean looping;
        public List<Key> keys = new ArrayList<>();
    }

    public static final class Key {
        public float t;
        public Map<String, float[]> bones = Map.of();
        public float lift;
        public float yaw;
    }

    /** Lee el fichero; {@code null} si no existe o es de un formato que no se conoce. */
    public static Loadout read(Path file) {
        if (!Files.isRegularFile(file)) {
            return null;
        }
        try (Reader r = Files.newBufferedReader(file)) {
            Loadout l = new Gson().fromJson(r, Loadout.class);
            if (l == null || l.format != FORMAT) {
                BrasaCosmetics.LOG.warn("[Brasa] loadout.json con formato {} (este mod entiende {})", l == null ? "?" : l.format, FORMAT);
                return null;
            }
            return l;
        } catch (Exception e) {
            BrasaCosmetics.LOG.error("[Brasa] No se pudo leer {}", file, e);
            return null;
        }
    }
}
