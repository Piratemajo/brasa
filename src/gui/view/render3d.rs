//! Dibujo 3D mínimo para la vista previa de cosméticos: cajas texturizadas con «box UV»,
//! proyectadas con una cámara que orbita, caras traseras descartadas y ordenadas de lejos a cerca
//! (algoritmo del pintor). Con modelos de Minecraft, todo cajas, basta y sobra.

use crate::cosmetics::paint::{face_rect, Face};
use crate::cosmetics::pose::{Pose, Xf, V3};
use crate::cosmetics::{Attach, Bone, Cube, Model};
use eframe::egui::{self, Color32, Pos2, Rect};

#[derive(Clone, Copy)]
pub struct Tex {
    pub id: egui::TextureId,
    pub size: [f32; 2],
}

struct Quad {
    pts: [V3; 4],
    uv: [Pos2; 4],
    normal: V3,
    tex: egui::TextureId,
    alpha_hint: bool,
}

#[derive(Default)]
pub struct Scene {
    quads: Vec<Quad>,
}

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    /// Giro alrededor del jugador (grados; 0 = de frente).
    pub yaw: f32,
    /// Inclinación (grados; positivo = desde arriba).
    pub pitch: f32,
    /// Punto del mundo que queda en el centro.
    pub target: V3,
    /// Píxeles de pantalla por píxel de skin.
    pub scale: f32,
}

impl Scene {
    pub fn add_cube(&mut self, xf: &Xf, c: &Cube, tex: Tex) {
        let i = c.inflate;
        let (x0, y0, z0) = (c.from[0] - i, c.from[1] - i, c.from[2] - i);
        let (x1, y1, z1) = (c.from[0] + c.size[0] + i, c.from[1] + c.size[1] + i, c.from[2] + c.size[2] + i);
        let isize = [c.size[0].ceil().max(1.0) as u32, c.size[1].ceil().max(1.0) as u32, c.size[2].ceil().max(1.0) as u32];
        // esquinas en orden: arriba-izq, arriba-dcha, abajo-dcha, abajo-izq (vistas desde fuera)
        let faces: [(Face, [V3; 4], V3); 6] = [
            (Face::Front, [[x0, y1, z1], [x1, y1, z1], [x1, y0, z1], [x0, y0, z1]], [0.0, 0.0, 1.0]),
            (Face::Back, [[x1, y1, z0], [x0, y1, z0], [x0, y0, z0], [x1, y0, z0]], [0.0, 0.0, -1.0]),
            (Face::Right, [[x0, y1, z0], [x0, y1, z1], [x0, y0, z1], [x0, y0, z0]], [-1.0, 0.0, 0.0]),
            (Face::Left, [[x1, y1, z1], [x1, y1, z0], [x1, y0, z0], [x1, y0, z1]], [1.0, 0.0, 0.0]),
            (Face::Top, [[x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1]], [0.0, 1.0, 0.0]),
            (Face::Bottom, [[x0, y0, z1], [x1, y0, z1], [x1, y0, z0], [x0, y0, z0]], [0.0, -1.0, 0.0]),
        ];
        for (f, pts, n) in faces {
            let (u, v, w, h) = face_rect(c.uv, isize, f);
            let (tw, th) = (tex.size[0], tex.size[1]);
            let (u0, v0, u1, v1) = (u as f32 / tw, v as f32 / th, (u + w) as f32 / tw, (v + h) as f32 / th);
            self.quads.push(Quad {
                pts: pts.map(|p| xf.apply(p)),
                uv: [egui::pos2(u0, v0), egui::pos2(u1, v0), egui::pos2(u1, v1), egui::pos2(u0, v1)],
                normal: xf.apply_dir(n),
                tex: tex.id,
                alpha_hint: c.inflate > 0.0,
            });
        }
    }

    /// Un hueso de cosmético con sus hijos; `t` para los movimientos.
    pub fn add_bone(&mut self, parent: &Xf, b: &Bone, tex: Tex, t: f32) {
        let (mrot, moff) = b.motion.as_ref().map(|m| crate::cosmetics::pose::motion(m, t)).unwrap_or(([0.0; 3], [0.0; 3]));
        let rot = [b.rot[0] + mrot[0], b.rot[1] + mrot[1], b.rot[2] + mrot[2]];
        let xf = parent.then_local(Xf::translate(moff)).then_local(Xf::around(b.pivot, rot));
        for c in &b.cubes {
            self.add_cube(&xf, c, tex);
        }
        for ch in &b.children {
            self.add_bone(&xf, ch, tex, t);
        }
    }

    pub fn add_model(&mut self, m: &Model, pose: &Pose, tex: Tex, t: f32) {
        let parent = match m.attach {
            Attach::Head => pose.bone_xf("head"),
            Attach::Body => pose.bone_xf("body"),
            Attach::Beside => Xf::ID,
        };
        for b in &m.bones {
            self.add_bone(&parent, b, tex, t);
        }
    }

    /// Caja que contiene todo lo añadido (para encuadrar miniaturas).
    pub fn bounds(&self) -> Option<(V3, V3)> {
        let mut it = self.quads.iter().flat_map(|q| q.pts.iter());
        let first = *it.next()?;
        Some(it.fold((first, first), |(lo, hi), p| ([lo[0].min(p[0]), lo[1].min(p[1]), lo[2].min(p[2])], [hi[0].max(p[0]), hi[1].max(p[1]), hi[2].max(p[2])])))
    }

    pub fn paint(&self, painter: &egui::Painter, rect: Rect, cam: &Camera) {
        let view = Xf::euler([cam.pitch, 0.0, 0.0]).then_local(Xf::euler([0.0, -cam.yaw, 0.0])).then_local(Xf::translate([
            -cam.target[0],
            -cam.target[1],
            -cam.target[2],
        ]));
        let light = norm([0.35, 1.0, 0.75]);
        let center = rect.center();
        struct P {
            depth: f32,
            pos: [Pos2; 4],
            uv: [Pos2; 4],
            col: Color32,
            tex: egui::TextureId,
        }
        let mut list: Vec<P> = Vec::with_capacity(self.quads.len());
        for q in &self.quads {
            let n = view.apply_dir(q.normal);
            if n[2] <= 1e-4 {
                continue; // de espaldas a la cámara
            }
            let vp = q.pts.map(|p| view.apply(p));
            let depth = vp.iter().map(|p| p[2]).sum::<f32>() / 4.0 + if q.alpha_hint { 0.05 } else { 0.0 };
            let pos = vp.map(|p| egui::pos2(center.x + p[0] * cam.scale, center.y - p[1] * cam.scale));
            let wn = norm(q.normal);
            let b = 0.58 + 0.42 * (wn[0] * light[0] + wn[1] * light[1] + wn[2] * light[2]).max(0.0);
            let g = (b * 255.0).round().clamp(0.0, 255.0) as u8;
            list.push(P { depth, pos, uv: q.uv, col: Color32::from_rgb(g, g, g), tex: q.tex });
        }
        list.sort_by(|a, b| a.depth.total_cmp(&b.depth));
        let painter = painter.with_clip_rect(rect);
        let mut mesh = egui::Mesh::default();
        for p in list {
            if mesh.texture_id != p.tex && !mesh.is_empty() {
                painter.add(egui::Shape::mesh(std::mem::take(&mut mesh)));
            }
            if mesh.is_empty() {
                mesh = egui::Mesh::with_texture(p.tex);
            }
            let base = mesh.vertices.len() as u32;
            for k in 0..4 {
                mesh.vertices.push(egui::epaint::Vertex { pos: p.pos[k], uv: p.uv[k], color: p.col });
            }
            mesh.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        if !mesh.is_empty() {
            painter.add(egui::Shape::mesh(mesh));
        }
    }
}

fn norm(v: V3) -> V3 {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
    [v[0] / l, v[1] / l, v[2] / l]
}

/// Las piezas del jugador con el «box UV» de una skin de 64×64.
pub fn player_cubes(slim: bool) -> Vec<(&'static str, Cube)> {
    let cube = |from: [f32; 3], size: [f32; 3], uv: [u32; 2], inflate: f32| Cube { from, size, uv, inflate };
    let aw = if slim { 3.0 } else { 4.0 };
    vec![
        ("head", cube([-4.0, 24.0, -4.0], [8.0, 8.0, 8.0], [0, 0], 0.0)),
        ("head", cube([-4.0, 24.0, -4.0], [8.0, 8.0, 8.0], [32, 0], 0.5)),
        ("body", cube([-4.0, 12.0, -2.0], [8.0, 12.0, 4.0], [16, 16], 0.0)),
        ("body", cube([-4.0, 12.0, -2.0], [8.0, 12.0, 4.0], [16, 32], 0.25)),
        ("right_arm", cube([-4.0 - aw, 12.0, -2.0], [aw, 12.0, 4.0], [40, 16], 0.0)),
        ("right_arm", cube([-4.0 - aw, 12.0, -2.0], [aw, 12.0, 4.0], [40, 32], 0.25)),
        ("left_arm", cube([4.0, 12.0, -2.0], [aw, 12.0, 4.0], [32, 48], 0.0)),
        ("left_arm", cube([4.0, 12.0, -2.0], [aw, 12.0, 4.0], [48, 48], 0.25)),
        ("right_leg", cube([-4.0, 0.0, -2.0], [4.0, 12.0, 4.0], [0, 16], 0.0)),
        ("right_leg", cube([-4.0, 0.0, -2.0], [4.0, 12.0, 4.0], [0, 32], 0.25)),
        ("left_leg", cube([0.0, 0.0, -2.0], [4.0, 12.0, 4.0], [16, 48], 0.0)),
        ("left_leg", cube([0.0, 0.0, -2.0], [4.0, 12.0, 4.0], [0, 48], 0.25)),
    ]
}

pub fn add_player(scene: &mut Scene, pose: &Pose, skin: Tex, slim: bool) {
    for (bone, c) in player_cubes(slim) {
        scene.add_cube(&pose.bone_xf(bone), &c, skin);
    }
}

/// La capa cuelga del borde de arriba de la espalda; `angle` la separa del cuerpo (grados).
pub fn add_cape(scene: &mut Scene, pose: &Pose, cape: Tex, angle: f32) {
    let xf =
        pose.bone_xf("body").then_local(Xf::translate([0.0, 24.0, -2.0])).then_local(Xf::euler([angle, 0.0, 0.0])).then_local(Xf::euler([0.0, 180.0, 0.0]));
    let c = Cube { from: [-5.0, -16.0, 0.0], size: [10.0, 16.0, 1.0], uv: [0, 0], inflate: 0.0 };
    scene.add_cube(&xf, &c, cape);
}

/// Pasa una skin antigua de 64×32 a 64×64 (copia brazo y pierna al lado izquierdo).
pub fn upgrade_legacy_skin(img: &image::RgbaImage) -> image::RgbaImage {
    if img.height() >= 64 {
        return img.clone();
    }
    let mut out = image::RgbaImage::new(64, 64);
    image::imageops::overlay(&mut out, img, 0, 0);
    let leg = image::imageops::crop_imm(img, 0, 16, 16, 16).to_image();
    let arm = image::imageops::crop_imm(img, 40, 16, 16, 16).to_image();
    image::imageops::overlay(&mut out, &leg, 16, 48);
    image::imageops::overlay(&mut out, &arm, 32, 48);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tex() -> Tex {
        Tex { id: egui::TextureId::Managed(0), size: [64.0, 64.0] }
    }

    #[test]
    fn front_faces_the_default_camera_and_back_faces_are_culled() {
        let mut s = Scene::default();
        s.add_cube(&Xf::ID, &Cube { from: [-4.0, 24.0, -4.0], size: [8.0; 3], uv: [0, 0], inflate: 0.0 }, tex());
        let cam = Camera { yaw: 0.0, pitch: 0.0, target: [0.0, 28.0, 0.0], scale: 10.0 };
        let view = |c: &Camera| {
            let v = Xf::euler([c.pitch, 0.0, 0.0]).then_local(Xf::euler([0.0, -c.yaw, 0.0]));
            s.quads.iter().filter(|q| v.apply_dir(q.normal)[2] > 1e-4).count()
        };
        assert_eq!(view(&cam), 1, "de frente sólo se ve la cara delantera");
        assert_eq!(view(&Camera { yaw: 30.0, pitch: 20.0, ..cam }), 3, "girado y desde arriba: delante, un lado y arriba");
        // la cara de delante usa la zona de la cara de la skin (8,8)-(16,16)
        let front = &s.quads[0];
        assert_eq!(front.uv[0], egui::pos2(8.0 / 64.0, 8.0 / 64.0));
        assert_eq!(front.uv[2], egui::pos2(16.0 / 64.0, 16.0 / 64.0));
    }

    #[test]
    fn player_is_complete_and_fits_its_box() {
        let mut s = Scene::default();
        add_player(&mut s, &Pose::default(), tex(), false);
        assert_eq!(s.quads.len(), 12 * 6);
        let (lo, hi) = s.bounds().unwrap();
        assert!((lo[1] - -0.25).abs() < 1e-3 && (hi[1] - 32.5).abs() < 1e-3, "{lo:?} {hi:?}");
        assert!((hi[0] - 8.25).abs() < 1e-3);
        // la capa queda detrás del cuerpo
        let mut c = Scene::default();
        add_cape(&mut c, &Pose::default(), tex(), 0.0);
        let (clo, chi) = c.bounds().unwrap();
        assert!(chi[2] <= -2.0 + 1e-3 && clo[2] >= -3.0 - 1e-3, "{clo:?} {chi:?}");
        assert!((chi[1] - 24.0).abs() < 1e-3 && (clo[1] - 8.0).abs() < 1e-3);
    }

    #[test]
    fn legacy_skins_get_left_limbs() {
        let mut old = image::RgbaImage::new(64, 32);
        old.put_pixel(44, 20, image::Rgba([255, 0, 0, 255]));
        let new = upgrade_legacy_skin(&old);
        assert_eq!(new.height(), 64);
        assert_eq!(new.get_pixel(36, 52)[0], 255);
    }
}
