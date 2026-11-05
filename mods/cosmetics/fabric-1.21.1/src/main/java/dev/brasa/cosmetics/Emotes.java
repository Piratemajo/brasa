package dev.brasa.cosmetics;

import java.util.List;
import net.minecraft.client.MinecraftClient;
import net.minecraft.client.model.ModelPart;
import net.minecraft.client.option.Perspective;
import net.minecraft.client.render.entity.model.PlayerEntityModel;

/**
 * Reproduce emotes: interpola los fotogramas clave igual que el launcher (suavizado «smoothstep»,
 * en bucle si toca), pone la cámara en tercera persona mientras dura y lo para al moverse.
 */
final class Emotes {
    private static final String[] BONES = {"head", "body", "right_arm", "left_arm", "right_leg", "left_leg"};
    /** Pivotes por defecto de esas piezas en el modelo de Minecraft. */
    private static final float[][] PIVOTS = {{0, 0, 0}, {0, 0, 0}, {-5, 2, 0}, {5, 2, 0}, {-1.9f, 12, 0}, {1.9f, 12, 0}};

    static Loadout.EmoteOut playing;
    private static float start;
    private static Perspective before;

    private Emotes() {}

    static void play(Loadout.EmoteOut e) {
        MinecraftClient mc = MinecraftClient.getInstance();
        if (playing == null) {
            before = mc.options.getPerspective();
        }
        playing = e;
        start = State.now();
        mc.options.setPerspective(Perspective.THIRD_PERSON_FRONT);
    }

    static void stop() {
        if (playing == null) {
            return;
        }
        playing = null;
        if (before != null) {
            MinecraftClient.getInstance().options.setPerspective(before);
        }
    }

    /** Cada tick: moverse, saltar o agacharse corta el emote; uno sin bucle acaba solo. */
    static void tick(MinecraftClient mc) {
        if (playing == null || mc.player == null) {
            return;
        }
        var in = mc.player.input;
        boolean moving = in != null && (in.movementForward != 0 || in.movementSideways != 0 || in.jumping || in.sneaking);
        float t = State.now() - start;
        Loadout.Emote e = playing.emote;
        boolean finished = !e.looping && t > e.duration + 0.3f && !holdsAtEnd(e);
        if (moving || finished) {
            stop();
        }
    }

    /** Un emote que no acaba en reposo (sentarse) se queda así hasta que te muevas. */
    private static boolean holdsAtEnd(Loadout.Emote e) {
        if (e.keys.isEmpty()) {
            return false;
        }
        Loadout.Key last = e.keys.get(e.keys.size() - 1);
        return last.lift != 0 || !last.bones.isEmpty();
    }

    private static float smooth(float x) {
        x = Math.max(0, Math.min(1, x));
        return x * x * (3 - 2 * x);
    }

    /** Pose en el segundo {@code t}: rotación de cada hueso (grados, sistema de Brasa), altura y giro. */
    static float[][] sample(Loadout.Emote e, float t, float[] liftYaw) {
        float[][] out = new float[BONES.length][3];
        List<Loadout.Key> keys = e.keys;
        if (keys.isEmpty()) {
            return out;
        }
        t = e.looping && e.duration > 0 ? ((t % e.duration) + e.duration) % e.duration : Math.max(0, Math.min(e.duration, t));
        int next = -1;
        for (int i = 0; i < keys.size(); i++) {
            if (keys.get(i).t > t) {
                next = i;
                break;
            }
        }
        Loadout.Key a;
        Loadout.Key b;
        float f;
        if (next == -1) {
            Loadout.Key last = keys.get(keys.size() - 1);
            if (e.looping) {
                Loadout.Key first = keys.get(0);
                float span = Math.max(1e-3f, e.duration - last.t + first.t);
                a = last;
                b = first;
                f = smooth((t - last.t) / span);
            } else {
                a = last;
                b = last;
                f = 0;
            }
        } else if (next == 0) {
            a = keys.get(0);
            b = a;
            f = 0;
        } else {
            a = keys.get(next - 1);
            b = keys.get(next);
            f = smooth((t - a.t) / Math.max(1e-3f, b.t - a.t));
        }
        for (int i = 0; i < BONES.length; i++) {
            float[] ra = a.bones.getOrDefault(BONES[i], new float[3]);
            float[] rb = b.bones.getOrDefault(BONES[i], new float[3]);
            for (int k = 0; k < 3; k++) {
                out[i][k] = ra[k] + (rb[k] - ra[k]) * f;
            }
        }
        liftYaw[0] = a.lift + (b.lift - a.lift) * f;
        liftYaw[1] = a.yaw + (b.yaw - a.yaw) * f;
        return out;
    }

    /** Llamado al final de {@code PlayerEntityModel.setAngles}: pone la pose del emote. */
    static void apply(PlayerEntityModel<?> m) {
        if (playing == null) {
            return;
        }
        float[] liftYaw = new float[2];
        float[][] pose = sample(playing.emote, State.now() - start, liftYaw);
        ModelPart[] parts = {m.head, m.body, m.rightArm, m.leftArm, m.rightLeg, m.leftLeg};
        double yaw = Math.toRadians(-liftYaw[1]);
        float cos = (float) Math.cos(yaw);
        float sin = (float) Math.sin(yaw);
        for (int i = 0; i < parts.length; i++) {
            ModelPart p = parts[i];
            p.pitch = (float) Math.toRadians(pose[i][0]);
            p.yaw = (float) Math.toRadians(-pose[i][1]) + (float) yaw;
            p.roll = (float) Math.toRadians(-pose[i][2]);
            // giro de todo el cuerpo: los pivotes también giran alrededor del eje vertical
            float px = PIVOTS[i][0];
            float pz = PIVOTS[i][2];
            p.pivotX = px * cos + pz * sin;
            p.pivotZ = -px * sin + pz * cos;
            p.pivotY = PIVOTS[i][1] - liftYaw[0];
        }
        // las capas exteriores de la skin siguen a sus piezas
        m.hat.copyTransform(m.head);
        m.jacket.copyTransform(m.body);
        m.rightSleeve.copyTransform(m.rightArm);
        m.leftSleeve.copyTransform(m.leftArm);
        m.rightPants.copyTransform(m.rightLeg);
        m.leftPants.copyTransform(m.leftLeg);
    }
}
