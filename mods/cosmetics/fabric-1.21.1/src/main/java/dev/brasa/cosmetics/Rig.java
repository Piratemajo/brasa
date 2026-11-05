package dev.brasa.cosmetics;

import java.util.ArrayList;
import java.util.EnumSet;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import net.minecraft.client.model.ModelPart;
import net.minecraft.util.math.Direction;

/**
 * Modelo de un cosmético convertido a piezas de Minecraft ({@link ModelPart}), que ya saben pintar
 * cajas con «box UV» igual que el launcher.
 *
 * <p>Paso del sistema de Brasa al de los modelos de Minecraft (y hacia abajo desde el cuello,
 * mirando hacia -z): punto {@code (x, 24 - y, -z)}, rotaciones {@code (rx, -ry, -rz)}.
 */
final class Rig {
    final ModelPart root;
    private final List<Animated> animated = new ArrayList<>();

    private record Animated(ModelPart part, Loadout.Motion motion, float pitch, float yaw, float roll, float pivotY) {}

    private Rig(ModelPart root) {
        this.root = root;
    }

    static Rig build(Loadout.ModelDef def) {
        float tw = def.texture_size[0];
        float th = def.texture_size[1];
        Map<String, ModelPart> top = new HashMap<>();
        List<Animated> anim = new ArrayList<>();
        int n = 0;
        for (Loadout.BoneDef b : def.bones) {
            top.put("b" + (n++) + "_" + b.name, bone(b, new float[] {0, 0, 0}, tw, th, anim));
        }
        Rig rig = new Rig(new ModelPart(List.of(), top));
        rig.animated.addAll(anim);
        return rig;
    }

    private static float[] toMc(float[] p) {
        return new float[] {p[0], 24f - p[1], -p[2]};
    }

    private static ModelPart bone(Loadout.BoneDef b, float[] parentPivot, float tw, float th, List<Animated> anim) {
        float[] pivot = toMc(b.pivot);
        List<ModelPart.Cuboid> cuboids = new ArrayList<>();
        for (Loadout.CubeDef c : b.cubes) {
            // esquina mínima en el espacio de Minecraft, relativa al pivote del hueso
            float x = c.from[0] - pivot[0];
            float y = (24f - (c.from[1] + c.size[1])) - pivot[1];
            float z = -(c.from[2] + c.size[2]) - pivot[2];
            cuboids.add(new ModelPart.Cuboid(c.uv[0], c.uv[1], x, y, z, c.size[0], c.size[1], c.size[2], c.inflate, c.inflate, c.inflate, false, tw, th, EnumSet.allOf(Direction.class)));
        }
        Map<String, ModelPart> children = new HashMap<>();
        int n = 0;
        for (Loadout.BoneDef ch : b.children) {
            children.put("c" + (n++) + "_" + ch.name, bone(ch, pivot, tw, th, anim));
        }
        ModelPart part = new ModelPart(cuboids, children);
        part.pivotX = pivot[0] - parentPivot[0];
        part.pivotY = pivot[1] - parentPivot[1];
        part.pivotZ = pivot[2] - parentPivot[2];
        part.pitch = (float) Math.toRadians(b.rot[0]);
        part.yaw = (float) Math.toRadians(-b.rot[1]);
        part.roll = (float) Math.toRadians(-b.rot[2]);
        if (b.motion != null) {
            anim.add(new Animated(part, b.motion, part.pitch, part.yaw, part.roll, part.pivotY));
        }
        return part;
    }

    /** Aplica los movimientos (alas, aureolas, colas…) para el segundo {@code t}. */
    void animate(float t) {
        double tau = Math.PI * 2;
        for (Animated a : animated) {
            Loadout.Motion m = a.motion;
            a.part.pitch = a.pitch;
            a.part.yaw = a.yaw;
            a.part.roll = a.roll;
            a.part.pivotY = a.pivotY;
            switch (m.kind) {
                case "swing" -> {
                    float deg = (float) (m.amp * Math.sin(tau * m.speed * t + Math.toRadians(m.phase)));
                    float rad = (float) Math.toRadians(deg);
                    switch (m.axis == null ? "y" : m.axis) {
                        case "x" -> a.part.pitch += rad;
                        case "z" -> a.part.roll -= rad;
                        default -> a.part.yaw -= rad;
                    }
                }
                case "bob" -> a.part.pivotY -= (float) (m.amp * Math.sin(tau * m.speed * t));
                case "spin" -> a.part.yaw -= (float) Math.toRadians((m.speed * t) % 360f);
                default -> {}
            }
        }
    }
}
