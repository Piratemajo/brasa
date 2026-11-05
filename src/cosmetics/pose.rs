//! Esqueleto del jugador, poses, emotes y movimientos de los huesos de los cosméticos.
//! Lo usa la vista previa; el mod del juego implementa lo mismo (ver el formato en `mod.rs`).

use super::{Axis, Emote, Motion};

pub type V3 = [f32; 3];

/// Transformación afín 3×4 (rotación + traslación).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Xf {
    pub r: [[f32; 3]; 3],
    pub t: V3,
}

impl Xf {
    pub const ID: Xf = Xf { r: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], t: [0.0; 3] };

    pub fn translate(t: V3) -> Xf {
        Xf { t, ..Xf::ID }
    }

    /// Rotación en grados `R = Rz · Ry · Rx` (primero X).
    pub fn euler(deg: V3) -> Xf {
        let (x, y, z) = (deg[0].to_radians(), deg[1].to_radians(), deg[2].to_radians());
        let (sx, cx, sy, cy, sz, cz) = (x.sin(), x.cos(), y.sin(), y.cos(), z.sin(), z.cos());
        let rx = [[1.0, 0.0, 0.0], [0.0, cx, -sx], [0.0, sx, cx]];
        let ry = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
        let rz = [[cz, -sz, 0.0], [sz, cz, 0.0], [0.0, 0.0, 1.0]];
        Xf { r: mul3(rz, mul3(ry, rx)), t: [0.0; 3] }
    }

    /// Rotación alrededor de un punto.
    pub fn around(pivot: V3, deg: V3) -> Xf {
        Xf::translate(pivot).then_local(Xf::euler(deg)).then_local(Xf::translate([-pivot[0], -pivot[1], -pivot[2]]))
    }

    /// `self · other`: aplica `other` primero y luego `self`.
    pub fn then_local(&self, o: Xf) -> Xf {
        Xf { r: mul3(self.r, o.r), t: add(self.apply_dir(o.t), self.t) }
    }

    pub fn apply(&self, p: V3) -> V3 {
        add(self.apply_dir(p), self.t)
    }

    pub fn apply_dir(&self, p: V3) -> V3 {
        let r = &self.r;
        [r[0][0] * p[0] + r[0][1] * p[1] + r[0][2] * p[2], r[1][0] * p[0] + r[1][1] * p[1] + r[1][2] * p[2], r[2][0] * p[0] + r[2][1] * p[1] + r[2][2] * p[2]]
    }
}

fn mul3(a: [[f32; 3]; 3], b: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let mut o = [[0.0; 3]; 3];
    for (i, row) in o.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            *v = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
        }
    }
    o
}

pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// Huesos del jugador, en el orden de los emotes.
pub const BONES: [&str; 6] = ["head", "body", "right_arm", "left_arm", "right_leg", "left_leg"];

/// Pivote de cada hueso (hombros, caderas, cuello) en el sistema de Brasa.
pub fn pivot(bone: &str) -> V3 {
    match bone {
        "head" => [0.0, 24.0, 0.0],
        "body" => [0.0, 24.0, 0.0],
        "right_arm" => [-5.0, 22.0, 0.0],
        "left_arm" => [5.0, 22.0, 0.0],
        "right_leg" => [-2.0, 12.0, 0.0],
        "left_leg" => [2.0, 12.0, 0.0],
        _ => [0.0; 3],
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pose {
    /// Rotación de cada hueso de `BONES`, en grados.
    pub rot: [V3; 6],
    pub lift: f32,
    pub yaw: f32,
}

impl Pose {
    pub fn bone(&self, name: &str) -> V3 {
        BONES.iter().position(|b| *b == name).map(|i| self.rot[i]).unwrap_or([0.0; 3])
    }

    /// Transformación de todo el cuerpo (giro y altura).
    pub fn root(&self) -> Xf {
        Xf::translate([0.0, self.lift, 0.0]).then_local(Xf::euler([0.0, self.yaw, 0.0]))
    }

    /// Transformación de un hueso, incluida la del cuerpo.
    pub fn bone_xf(&self, name: &str) -> Xf {
        self.root().then_local(Xf::around(pivot(name), self.bone(name)))
    }
}

/// Andar: brazos y piernas que se balancean, como en el juego. `amount` 0..1.
pub fn walk(t: f32, amount: f32) -> Pose {
    let s = (t * std::f32::consts::TAU * 0.9).sin() * 38.0 * amount;
    let mut p = Pose::default();
    p.rot[2] = [s, 0.0, 0.0];
    p.rot[3] = [-s, 0.0, 0.0];
    p.rot[4] = [-s, 0.0, 0.0];
    p.rot[5] = [s, 0.0, 0.0];
    // reposo: un leve balanceo de respiración en los brazos
    let breathe = (t * 1.3).sin() * 3.0;
    p.rot[2][2] -= 3.0 + breathe;
    p.rot[3][2] += 3.0 + breathe;
    p
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

impl Emote {
    /// Pose del emote en el segundo `t` (en bucle si es `looping`; si no, se queda en el final).
    pub fn sample(&self, t: f32) -> Pose {
        let mut out = Pose::default();
        if self.keys.is_empty() {
            return out;
        }
        let t = if self.looping && self.duration > 0.0 { t.rem_euclid(self.duration) } else { t.clamp(0.0, self.duration) };
        let next = self.keys.iter().position(|k| k.t > t);
        let (a, b, f) = match next {
            None => {
                let last = self.keys.last().unwrap();
                if self.looping {
                    // del último al primero, cerrando el bucle
                    let first = &self.keys[0];
                    let span = (self.duration - last.t + first.t).max(1e-3);
                    (last, first, smooth((t - last.t) / span))
                } else {
                    (last, last, 0.0)
                }
            }
            Some(0) => (&self.keys[0], &self.keys[0], 0.0),
            Some(i) => {
                let (a, b) = (&self.keys[i - 1], &self.keys[i]);
                (a, b, smooth((t - a.t) / (b.t - a.t).max(1e-3)))
            }
        };
        for (i, name) in BONES.iter().enumerate() {
            let ra = a.bones.get(*name).copied().unwrap_or([0.0; 3]);
            let rb = b.bones.get(*name).copied().unwrap_or([0.0; 3]);
            out.rot[i] = [ra[0] + (rb[0] - ra[0]) * f, ra[1] + (rb[1] - ra[1]) * f, ra[2] + (rb[2] - ra[2]) * f];
        }
        out.lift = a.lift + (b.lift - a.lift) * f;
        out.yaw = a.yaw + (b.yaw - a.yaw) * f;
        out
    }
}

/// Rotación (grados) y desplazamiento (píxeles) de un hueso con movimiento, en el segundo `t`.
pub fn motion(m: &Motion, t: f32) -> (V3, V3) {
    let tau = std::f32::consts::TAU;
    match *m {
        Motion::Swing { axis, amp, speed, phase } => {
            let a = amp * (tau * speed * t + phase.to_radians()).sin();
            let r = match axis {
                Axis::X => [a, 0.0, 0.0],
                Axis::Y => [0.0, a, 0.0],
                Axis::Z => [0.0, 0.0, a],
            };
            (r, [0.0; 3])
        }
        Motion::Bob { amp, speed } => ([0.0; 3], [0.0, amp * (tau * speed * t).sin(), 0.0]),
        Motion::Spin { speed } => ([0.0, (speed * t).rem_euclid(360.0), 0.0], [0.0; 3]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cosmetics::Key;

    fn close(a: V3, b: V3) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-3)
    }

    #[test]
    fn rotations_follow_the_documented_frame() {
        // brazo colgando (hacia -y) rotado -90 en X: apunta hacia delante (+z)
        assert!(close(Xf::euler([-90.0, 0.0, 0.0]).apply([0.0, -1.0, 0.0]), [0.0, 0.0, 1.0]));
        // brazo derecho (-x) hacia fuera: -90 en Z
        assert!(close(Xf::euler([0.0, 0.0, -90.0]).apply([0.0, -1.0, 0.0]), [-1.0, 0.0, 0.0]));
        // alrededor de un pivote, el pivote no se mueve
        let p = [3.0, 22.0, 1.0];
        assert!(close(Xf::around(p, [40.0, 20.0, 10.0]).apply(p), p));
        // orden: primero X, luego Z
        let a = Xf::euler([90.0, 0.0, 90.0]).apply([0.0, 1.0, 0.0]);
        let b = Xf::euler([0.0, 0.0, 90.0]).apply(Xf::euler([90.0, 0.0, 0.0]).apply([0.0, 1.0, 0.0]));
        assert!(close(a, b));
    }

    #[test]
    fn emotes_interpolate_and_loop() {
        let k = |t: f32, z: f32| Key { t, bones: [("right_arm".to_string(), [0.0, 0.0, z])].into_iter().collect(), lift: 0.0, yaw: 0.0 };
        let e = Emote { duration: 2.0, looping: true, keys: vec![k(0.0, 0.0), k(1.0, -100.0)] };
        assert_eq!(e.sample(0.0).bone("right_arm")[2], 0.0);
        assert!((e.sample(1.0).bone("right_arm")[2] + 100.0).abs() < 1e-3);
        assert!((e.sample(0.5).bone("right_arm")[2] + 50.0).abs() < 1e-3, "a mitad, a mitad");
        // en bucle vuelve al principio
        assert!((e.sample(2.0).bone("right_arm")[2]).abs() < 1e-3);
        assert!((e.sample(1.5).bone("right_arm")[2] + 50.0).abs() < 1e-3);
        let once = Emote { looping: false, ..e };
        assert!((once.sample(9.0).bone("right_arm")[2] + 100.0).abs() < 1e-3, "sin bucle se queda al final");
    }

    #[test]
    fn motions() {
        let (r, _) = motion(&Motion::Swing { axis: Axis::Y, amp: 30.0, speed: 1.0, phase: 0.0 }, 0.25);
        assert!((r[1] - 30.0).abs() < 1e-3);
        let (_, o) = motion(&Motion::Bob { amp: 2.0, speed: 1.0 }, 0.25);
        assert!((o[1] - 2.0).abs() < 1e-3);
        let (r, _) = motion(&Motion::Spin { speed: 90.0 }, 5.0);
        assert!((r[1] - 90.0).abs() < 1e-3);
    }
}
