//! Pintar texturas en pixel-art y construir modelos de cajas con su «box UV».
//!
//! Cada cubo recibe su propia zona de la textura (se reparten por filas) pintada con un material:
//! color base, un poco de ruido, caras de arriba más claras y de abajo más oscuras, borde opcional
//! y un dibujo opcional en la cara de delante (ojos, gemas…). Así los modelos se describen con
//! pocas líneas y la textura sale sola.

use super::{Attach, Bone, Cube, Model, Motion};

pub type Rgba = [u8; 4];

#[derive(Clone)]
pub struct Canvas {
    pub w: u32,
    pub h: u32,
    pub px: Vec<Rgba>,
}

impl Canvas {
    pub fn new(w: u32, h: u32) -> Canvas {
        Canvas { w, h, px: vec![[0, 0, 0, 0]; (w * h) as usize] }
    }
    pub fn get(&self, x: u32, y: u32) -> Rgba {
        if x < self.w && y < self.h {
            self.px[(y * self.w + x) as usize]
        } else {
            [0, 0, 0, 0]
        }
    }
    pub fn set(&mut self, x: u32, y: u32, c: Rgba) {
        if x < self.w && y < self.h {
            self.px[(y * self.w + x) as usize] = c;
        }
    }
    pub fn to_png(&self) -> Vec<u8> {
        let img = image::RgbaImage::from_fn(self.w, self.h, |x, y| image::Rgba(self.get(x, y)));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).expect("png en memoria");
        out.into_inner()
    }
}

/// Ruido determinista en [0, 1) para (x, y, semilla).
pub fn noise(x: u32, y: u32, seed: u32) -> f32 {
    let mut h = x.wrapping_mul(374_761_393) ^ y.wrapping_mul(668_265_263) ^ seed.wrapping_mul(2_246_822_519);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^= h >> 16;
    (h & 0xffff) as f32 / 65536.0
}

pub fn shade(c: [u8; 3], f: f32) -> Rgba {
    let s = |v: u8| ((v as f32 * f).round().clamp(0.0, 255.0)) as u8;
    [s(c[0]), s(c[1]), s(c[2]), 255]
}

pub fn lerp3(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t.clamp(0.0, 1.0)).round() as u8;
    [l(a[0], b[0]), l(a[1], b[1]), l(a[2], b[2])]
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Face {
    Top,
    Bottom,
    Right,
    Front,
    Left,
    Back,
}

/// Rectángulo (x, y, w, h) de cada cara dentro del «box UV» de un cubo `w × h × d`.
pub fn face_rect(uv: [u32; 2], size: [u32; 3], f: Face) -> (u32, u32, u32, u32) {
    let [u, v] = uv;
    let [w, h, d] = size;
    match f {
        Face::Top => (u + d, v, w, d),
        Face::Bottom => (u + d + w, v, w, d),
        Face::Right => (u, v + d, d, h),
        Face::Front => (u + d, v + d, w, h),
        Face::Left => (u + d + w, v + d, d, h),
        Face::Back => (u + 2 * d + w, v + d, w, h),
    }
}

pub fn box_uv_size(size: [u32; 3]) -> (u32, u32) {
    let [w, h, d] = size;
    (2 * (w + d), h + d)
}

/// Dibujo en una cara: filas de caracteres; cada carácter es un color de `palette` («.» = nada).
#[derive(Clone)]
pub struct Decal {
    pub face: Face,
    pub rows: &'static [&'static str],
    pub palette: &'static [(char, [u8; 3])],
    /// `true`: centrado; `false`: arriba a la izquierda de la cara.
    pub center: bool,
}

#[derive(Clone)]
pub struct Mat {
    pub color: [u8; 3],
    /// Variación aleatoria de brillo (0 = liso).
    pub noise: f32,
    /// Color del borde de cada cara (aristas marcadas).
    pub edge: Option<[u8; 3]>,
    /// Degradado vertical hasta este color (de arriba abajo de las caras laterales).
    pub fade: Option<[u8; 3]>,
    pub decals: Vec<Decal>,
    /// Semitransparente (cristal, fantasmas).
    pub alpha: u8,
}

impl Mat {
    pub fn new(color: [u8; 3]) -> Mat {
        Mat { color, noise: 0.08, edge: None, fade: None, decals: vec![], alpha: 255 }
    }
    pub fn noise(mut self, n: f32) -> Mat {
        self.noise = n;
        self
    }
    pub fn edge(mut self, c: [u8; 3]) -> Mat {
        self.edge = Some(c);
        self
    }
    pub fn fade(mut self, c: [u8; 3]) -> Mat {
        self.fade = Some(c);
        self
    }
    pub fn alpha(mut self, a: u8) -> Mat {
        self.alpha = a;
        self
    }
    pub fn decal(mut self, face: Face, rows: &'static [&'static str], palette: &'static [(char, [u8; 3])]) -> Mat {
        self.decals.push(Decal { face, rows, palette, center: true });
        self
    }
}

fn face_light(f: Face) -> f32 {
    match f {
        Face::Top => 1.12,
        Face::Bottom => 0.72,
        Face::Front | Face::Back => 1.0,
        Face::Left | Face::Right => 0.9,
    }
}

/// Pinta el «box UV» entero de un cubo con un material.
pub fn paint_box(c: &mut Canvas, uv: [u32; 2], size: [u32; 3], m: &Mat, seed: u32) {
    for f in [Face::Top, Face::Bottom, Face::Right, Face::Front, Face::Left, Face::Back] {
        let (x0, y0, w, h) = face_rect(uv, size, f);
        for y in 0..h {
            for x in 0..w {
                let mut base = m.color;
                if let (Some(to), true) = (m.fade, matches!(f, Face::Front | Face::Back | Face::Left | Face::Right)) {
                    base = lerp3(m.color, to, if h > 1 { y as f32 / (h - 1) as f32 } else { 0.0 });
                }
                let edge = w > 2 && h > 2 && (x == 0 || y == 0 || x == w - 1 || y == h - 1);
                if let (Some(e), true) = (m.edge, edge) {
                    base = e;
                }
                let n = 1.0 + (noise(x0 + x, y0 + y, seed) - 0.5) * 2.0 * m.noise;
                let mut px = shade(base, face_light(f) * n);
                px[3] = m.alpha;
                c.set(x0 + x, y0 + y, px);
            }
        }
        for d in m.decals.iter().filter(|d| d.face == f) {
            let dh = d.rows.len() as u32;
            let dw = d.rows.iter().map(|r| r.chars().count()).max().unwrap_or(0) as u32;
            let (ox, oy) = if d.center { (x0 + w.saturating_sub(dw) / 2, y0 + h.saturating_sub(dh) / 2) } else { (x0, y0) };
            for (yy, row) in d.rows.iter().enumerate() {
                for (xx, ch) in row.chars().enumerate() {
                    if let Some((_, col)) = d.palette.iter().find(|(k, _)| *k == ch) {
                        c.set(ox + xx as u32, oy + yy as u32, shade(*col, face_light(f)));
                    }
                }
            }
        }
    }
}

/// Construye un modelo hueso a hueso; cada cubo recibe su zona de textura y su material.
pub struct ModelBuilder {
    attach: Attach,
    bones: Vec<Bone>,
    jobs: Vec<([u32; 2], [u32; 3], Mat)>,
    // reparto por filas
    cursor: (u32, u32),
    row_h: u32,
    width: u32,
}

impl ModelBuilder {
    pub fn new(attach: Attach, width: u32) -> ModelBuilder {
        ModelBuilder { attach, bones: vec![], jobs: vec![], cursor: (0, 0), row_h: 0, width }
    }

    pub fn bone(&mut self, name: &str, pivot: [f32; 3]) -> &mut ModelBuilder {
        self.bones.push(Bone { name: name.into(), pivot, rot: [0.0; 3], cubes: vec![], motion: None, children: vec![] });
        self
    }
    pub fn rot(&mut self, r: [f32; 3]) -> &mut ModelBuilder {
        self.bones.last_mut().expect("primero un hueso").rot = r;
        self
    }
    pub fn motion(&mut self, m: Motion) -> &mut ModelBuilder {
        self.bones.last_mut().expect("primero un hueso").motion = Some(m);
        self
    }

    fn alloc(&mut self, size: [u32; 3]) -> [u32; 2] {
        let (w, h) = box_uv_size(size);
        if self.cursor.0 + w > self.width {
            self.cursor = (0, self.cursor.1 + self.row_h);
            self.row_h = 0;
        }
        let uv = [self.cursor.0, self.cursor.1];
        self.cursor.0 += w;
        self.row_h = self.row_h.max(h);
        uv
    }

    /// Cubo en el último hueso. `size` puede tener decimales (se pinta redondeando hacia arriba).
    pub fn cube(&mut self, from: [f32; 3], size: [f32; 3], m: &Mat) -> &mut ModelBuilder {
        self.cube_inflated(from, size, 0.0, m)
    }

    pub fn cube_inflated(&mut self, from: [f32; 3], size: [f32; 3], inflate: f32, m: &Mat) -> &mut ModelBuilder {
        let isize = [size[0].ceil().max(1.0) as u32, size[1].ceil().max(1.0) as u32, size[2].ceil().max(1.0) as u32];
        let uv = self.alloc(isize);
        self.jobs.push((uv, isize, m.clone()));
        self.bones.last_mut().expect("primero un hueso").cubes.push(Cube { from, size, uv, inflate });
        self
    }

    /// El último hueso pasa a ser hijo del anterior.
    pub fn nest(&mut self) -> &mut ModelBuilder {
        let child = self.bones.pop().expect("hueso hijo");
        self.bones.last_mut().expect("hueso padre").children.push(child);
        self
    }

    pub fn finish(self) -> (Model, Canvas) {
        let h = (self.cursor.1 + self.row_h).max(1).next_power_of_two().max(16);
        let w = self.width.next_power_of_two();
        let mut canvas = Canvas::new(w, h);
        for (n, (uv, size, m)) in self.jobs.iter().enumerate() {
            paint_box(&mut canvas, *uv, *size, m, n as u32 * 7919 + 13);
        }
        (Model { texture_size: [w, h], attach: self.attach, bones: self.bones }, canvas)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_uv_matches_minecraft_skin_layout() {
        // la cabeza de una skin: 8×8×8 en (0, 0)
        assert_eq!(face_rect([0, 0], [8, 8, 8], Face::Front), (8, 8, 8, 8));
        assert_eq!(face_rect([0, 0], [8, 8, 8], Face::Top), (8, 0, 8, 8));
        assert_eq!(face_rect([0, 0], [8, 8, 8], Face::Back), (24, 8, 8, 8));
        // la capa: 10×16×1 en (0, 0): la cara de fuera empieza en (1, 1)
        assert_eq!(face_rect([0, 0], [10, 16, 1], Face::Front), (1, 1, 10, 16));
        assert_eq!(box_uv_size([10, 16, 1]), (22, 17));
    }

    #[test]
    fn builder_packs_cubes_without_overlap_and_paints_them() {
        let mut b = ModelBuilder::new(Attach::Head, 64);
        let m = Mat::new([200, 50, 50]);
        b.bone("root", [0.0, 32.0, 0.0]);
        for _ in 0..6 {
            b.cube([0.0; 3], [9.0, 3.0, 9.0], &m);
        }
        let (model, canvas) = b.finish();
        let cubes = &model.bones[0].cubes;
        for (i, a) in cubes.iter().enumerate() {
            for b2 in &cubes[i + 1..] {
                let (aw, ah) = box_uv_size([9, 3, 9]);
                let sep = a.uv[0] + aw <= b2.uv[0] || b2.uv[0] + aw <= a.uv[0] || a.uv[1] + ah <= b2.uv[1] || b2.uv[1] + ah <= a.uv[1];
                assert!(sep, "zonas solapadas: {:?} {:?}", a.uv, b2.uv);
            }
            assert!(a.uv[0] + 36 <= model.texture_size[0] && a.uv[1] + 12 <= model.texture_size[1]);
        }
        // la cara de delante del primer cubo tiene color
        let (x, y, _, _) = face_rect(cubes[0].uv, [9, 3, 9], Face::Front);
        assert!(canvas.get(x, y)[3] == 255 && canvas.get(x, y)[0] > 100);
        assert_eq!(canvas.w, model.texture_size[0]);
        assert!(!canvas.to_png().is_empty());
    }
}
