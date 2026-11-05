//! Catálogo de Brasa: todos los diseños son originales y se generan en código (pixel-art),
//! así que no hay que descargar nada y el color de los teñibles se aplica al vuelo.

use super::paint::{lerp3, noise, shade, Canvas, Face, Mat, ModelBuilder};
use super::{Attach, Axis, Cosmetic, Emote, Key, Model, Motion, Slot};

const fn c(id: &'static str, name: &'static str, slot: Slot, description: &'static str) -> Cosmetic {
    Cosmetic { id, name, slot, description, tint: None, frames: 1, fps: 0.0 }
}
const fn tinted(mut x: Cosmetic, t: [u8; 3]) -> Cosmetic {
    x.tint = Some(t);
    x
}
const fn animated(mut x: Cosmetic, frames: u32, fps: f32) -> Cosmetic {
    x.frames = frames;
    x.fps = fps;
    x
}

pub static ALL: &[Cosmetic] = &[
    // capas
    animated(c("cape.brasa", "Brasa", Slot::Cape, "Llamas que suben desde el borde, como las del logo"), 8, 7.0),
    animated(c("cape.noche", "Noche estrellada", Slot::Cape, "Azul profundo con luna y estrellas que titilan"), 4, 3.0),
    animated(c("cape.aurora", "Aurora", Slot::Cape, "Ondas verdes y violetas que se mueven"), 12, 8.0),
    animated(c("cape.lava", "Lava", Slot::Cape, "Roca fundida que fluye despacio"), 8, 5.0),
    c("cape.real", "Real", Slot::Cape, "Púrpura con ribete y emblema dorados"),
    c("cape.ajedrez", "Ajedrez", Slot::Cape, "Cuadros blancos y negros"),
    c("cape.arcoiris", "Arcoíris", Slot::Cape, "Franjas de todos los colores"),
    tinted(c("cape.clasica", "Clásica", Slot::Cape, "Lisa, del color que quieras, con la llama de Brasa"), [196, 60, 40]),
    animated(c("cape.calabazas", "Calabazas", Slot::Cape, "Halloween: calabazas con la cara encendida que parpadea"), 4, 4.0),
    c("cape.telarana", "Telaraña", Slot::Cape, "Halloween: una tela en la esquina y su araña"),
    animated(c("cape.oceano", "Océano", Slot::Cape, "Olas con espuma que van y vienen"), 8, 6.0),
    animated(c("cape.galaxia", "Galaxia", Slot::Cape, "Nebulosa violeta con estrellas que titilan"), 8, 4.0),
    c("cape.bosque", "Bosque", Slot::Cape, "Pinos al atardecer"),
    // sombreros
    c("hat.corona", "Corona", Slot::Hat, "Oro con gemas"),
    tinted(c("hat.gorro", "Gorro de lana", Slot::Hat, "Con pompón; elige el color"), [200, 56, 56]),
    c("hat.cuernos", "Cuernos", Slot::Hat, "De marfil, curvados hacia fuera"),
    c("hat.aureola", "Aureola", Slot::Hat, "Flota y gira sobre la cabeza"),
    tinted(c("hat.chistera", "Sombrero de copa", Slot::Hat, "Negro con cinta de color"), [176, 32, 44]),
    tinted(c("hat.orejas", "Orejas de gato", Slot::Hat, "Puntiagudas, del color que quieras"), [70, 58, 58]),
    c("hat.gafas", "Gafas de sol", Slot::Hat, "Cristales oscuros con brillo"),
    c("hat.calabaza", "Cabeza de calabaza", Slot::Hat, "Halloween: tallada y con luz dentro"),
    tinted(c("hat.bruja", "Sombrero de bruja", Slot::Hat, "Halloween: punta doblada y cinta de color"), [236, 120, 30]),
    c("hat.vikingo", "Casco vikingo", Slot::Hat, "Hierro, bronce y cuernos"),
    c("hat.flores", "Corona de flores", Slot::Hat, "Ramas verdes con flores de colores"),
    tinted(c("hat.auriculares", "Auriculares", Slot::Hat, "Para no oír a los creepers; elige el color"), [60, 130, 220]),
    // espalda
    c("back.dragon", "Alas de dragón", Slot::Back, "Membrana violeta; se baten despacio"),
    c("back.angel", "Alas de ángel", Slot::Back, "Plumas blancas"),
    tinted(c("back.mochila", "Mochila", Slot::Back, "Para llevarlo todo; elige el color"), [60, 120, 90]),
    c("back.propulsor", "Propulsor", Slot::Back, "Dos depósitos y llama encendida"),
    c("back.carcaj", "Carcaj", Slot::Back, "Flechas a la espalda, en diagonal"),
    tinted(c("back.mariposa", "Alas de mariposa", Slot::Back, "Aletean despacio; elige el color"), [90, 170, 240]),
    c("back.espada", "Espada", Slot::Back, "Hoja de acero y empuñadura dorada, a la espalda"),
    c("back.escoba", "Escoba de bruja", Slot::Back, "Halloween: para volar a casa"),
    tinted(c("back.guitarra", "Guitarra", Slot::Back, "Colgada a la espalda; elige el color"), [178, 44, 44]),
    // emotes
    c("emote.saludar", "Saludar", Slot::Emote, "Hola con la mano en alto"),
    c("emote.bailar", "Bailar", Slot::Emote, "Brazos arriba, de lado a lado"),
    c("emote.aplaudir", "Aplaudir", Slot::Emote, "Bravo"),
    c("emote.victoria", "Victoria", Slot::Emote, "Saltos con los brazos en alto"),
    c("emote.dab", "Dab", Slot::Emote, "El clásico"),
    c("emote.facepalm", "Facepalm", Slot::Emote, "La mano a la cara"),
    c("emote.pensar", "Pensar", Slot::Emote, "Mano en la barbilla"),
    c("emote.sentarse", "Sentarse", Slot::Emote, "Al suelo, hasta que te muevas"),
    c("emote.zombi", "Zombi", Slot::Emote, "Halloween: brazos al frente y paso arrastrado"),
    c("emote.reverencia", "Reverencia", Slot::Emote, "Una inclinación, mano al pecho"),
    c("emote.presumir", "Presumir", Slot::Emote, "Brazos arriba, enseñando músculo"),
    c("emote.encogerse", "Ni idea", Slot::Emote, "Encogerse de hombros"),
    // mascotas
    c("pet.brasita", "Brasita", Slot::Pet, "Un espíritu de fuego que te acompaña"),
    tinted(c("pet.gato", "Minigato", Slot::Pet, "Flota a tu lado moviendo la cola"), [235, 150, 70]),
    c("pet.murcielago", "Murciélago", Slot::Pet, "Bate las alas sin parar"),
    tinted(c("pet.gelatina", "Gelatina", Slot::Pet, "Cubo translúcido que rebota"), [110, 200, 90]),
    c("pet.fantasma", "Fantasmita", Slot::Pet, "Halloween: translúcido, flota y se mece"),
    c("pet.calabacita", "Calabacita", Slot::Pet, "Halloween: una calabaza pequeña que va dando saltos"),
    tinted(c("pet.dragoncito", "Dragoncito", Slot::Pet, "Bate las alas y mueve la cola"), [70, 160, 110]),
    c("pet.abeja", "Abeja", Slot::Pet, "Zumba a tu lado"),
];

/// Los últimos en llegar (llevan la etiqueta «nuevo» en la Taquilla).
pub static NEW: &[&str] = &[
    "cape.calabazas",
    "cape.telarana",
    "cape.oceano",
    "cape.galaxia",
    "cape.bosque",
    "hat.calabaza",
    "hat.bruja",
    "hat.vikingo",
    "hat.flores",
    "hat.auriculares",
    "back.mariposa",
    "back.espada",
    "back.escoba",
    "back.guitarra",
    "emote.zombi",
    "emote.reverencia",
    "emote.presumir",
    "emote.encogerse",
    "pet.fantasma",
    "pet.calabacita",
    "pet.dragoncito",
    "pet.abeja",
];

pub fn get(id: &str) -> Option<&'static Cosmetic> {
    ALL.iter().find(|x| x.id == id)
}

pub fn of(slot: Slot) -> impl Iterator<Item = &'static Cosmetic> {
    ALL.iter().filter(move |x| x.slot == slot)
}

// ------------------------------------------------------------------ paleta y dibujos
const GOLD: [u8; 3] = [238, 186, 58];
const BLACK: [u8; 3] = [26, 24, 28];
const WHITE: [u8; 3] = [242, 240, 236];

static EYES_DARK: &[(char, [u8; 3])] = &[('k', [24, 20, 22]), ('w', [250, 250, 250])];
static EYES_RED: &[(char, [u8; 3])] = &[('r', [230, 40, 40])];
static EYES_GREEN: &[(char, [u8; 3])] = &[('g', [90, 220, 110]), ('k', [20, 20, 20]), ('p', [240, 130, 150])];
static GEM_RED: &[(char, [u8; 3])] = &[('r', [220, 30, 60]), ('w', [255, 200, 210])];
static SHINE: &[(char, [u8; 3])] = &[('w', [180, 200, 230])];
static GLOW: &[(char, [u8; 3])] = &[('y', [255, 210, 70]), ('o', [255, 150, 40])];
static EYES_YELLOW: &[(char, [u8; 3])] = &[('y', [250, 220, 70]), ('k', [20, 20, 20])];
static BUCKLE: &[(char, [u8; 3])] = &[('y', [238, 186, 58])];
static HOLE: &[(char, [u8; 3])] = &[('k', [20, 14, 12]), ('w', [230, 220, 200])];

// la llama de Brasa (6 × 8): r rojo, o naranja, y amarillo
static FLAME: &[&str] = &["..o...", "..oo..", ".ooo..", ".oyoo.", "ooyyo.", "oyyyoo", "oyyyyo", ".oyyo."];

fn mirror(from: [f32; 3], size: [f32; 3]) -> [f32; 3] {
    [-(from[0] + size[0]), from[1], from[2]]
}

/// Modelo y textura de un sombrero, objeto de espalda o mascota.
pub fn build(id: &str, tint: Option<[u8; 3]>) -> Option<(Model, Canvas)> {
    let x = get(id)?;
    let t = tint.or(x.tint).unwrap_or([200, 200, 200]);
    let mut b;
    match id {
        "hat.corona" => {
            b = ModelBuilder::new(Attach::Head, 64);
            let gold = Mat::new(GOLD).noise(0.12);
            b.bone("root", [0.0, 32.0, 0.0]);
            b.cube([-4.5, 31.5, 3.5], [9.0, 2.0, 1.0], &gold.clone().decal(Face::Front, &["..r.r.r.."], GEM_RED));
            b.cube([-4.5, 31.5, -4.5], [9.0, 2.0, 1.0], &gold);
            b.cube([-4.5, 31.5, -3.5], [1.0, 2.0, 7.0], &gold);
            b.cube([3.5, 31.5, -3.5], [1.0, 2.0, 7.0], &gold);
            for xx in [-4.5, -0.5, 3.5] {
                b.cube([xx, 33.5, 3.5], [1.0, 1.5, 1.0], &gold);
                b.cube([xx, 33.5, -4.5], [1.0, 1.5, 1.0], &gold);
            }
            b.cube([-4.5, 33.5, -0.5], [1.0, 1.5, 1.0], &gold);
            b.cube([3.5, 33.5, -0.5], [1.0, 1.5, 1.0], &gold);
        }
        "hat.gorro" => {
            b = ModelBuilder::new(Attach::Head, 64);
            b.bone("root", [0.0, 32.0, 0.0]);
            b.cube([-4.5, 29.0, -4.5], [9.0, 4.0, 9.0], &Mat::new(t).noise(0.14));
            b.cube([-4.75, 28.25, -4.75], [9.5, 1.5, 9.5], &Mat::new(lerp3(t, WHITE, 0.55)).noise(0.1));
            b.cube([-1.5, 33.0, -1.5], [3.0, 3.0, 3.0], &Mat::new(WHITE).noise(0.12));
        }
        "hat.cuernos" => {
            b = ModelBuilder::new(Attach::Head, 64);
            b.bone("root", [0.0, 32.0, 0.0]);
            let parts = [
                ([-4.25, 31.25, -1.0], [2.0, 2.0, 2.0], [232, 222, 200]),
                ([-5.25, 32.75, -0.75], [1.5, 2.0, 1.5], [196, 178, 150]),
                ([-5.75, 34.25, -0.5], [1.0, 1.75, 1.0], [120, 100, 80]),
            ];
            for (f, s, col) in parts {
                let m = Mat::new(col).noise(0.08);
                b.cube(f, s, &m);
                b.cube(mirror(f, s), s, &m);
            }
        }
        "hat.aureola" => {
            b = ModelBuilder::new(Attach::Head, 64);
            let glow = Mat::new([255, 232, 140]).noise(0.04);
            b.bone("float", [0.0, 35.5, 0.0]).motion(Motion::Bob { amp: 0.6, speed: 0.5 });
            b.bone("ring", [0.0, 35.5, 0.0]).motion(Motion::Spin { speed: 45.0 });
            b.cube([-4.0, 35.0, -4.0], [8.0, 0.75, 1.0], &glow);
            b.cube([-4.0, 35.0, 3.0], [8.0, 0.75, 1.0], &glow);
            b.cube([-4.0, 35.0, -3.0], [1.0, 0.75, 6.0], &glow);
            b.cube([3.0, 35.0, -3.0], [1.0, 0.75, 6.0], &glow);
            b.nest();
        }
        "hat.chistera" => {
            b = ModelBuilder::new(Attach::Head, 64);
            b.bone("root", [0.0, 32.0, 0.0]);
            let felt = Mat::new(BLACK).noise(0.1);
            b.cube([-6.0, 31.75, -6.0], [12.0, 0.75, 12.0], &felt);
            b.cube([-4.0, 32.5, -4.0], [8.0, 7.0, 8.0], &felt);
            b.cube([-4.25, 32.5, -4.25], [8.5, 1.5, 8.5], &Mat::new(t).noise(0.06));
        }
        "hat.orejas" => {
            b = ModelBuilder::new(Attach::Head, 64);
            b.bone("root", [0.0, 32.0, 0.0]);
            let fur = Mat::new(t).noise(0.12);
            let pink = Mat::new([236, 150, 170]).noise(0.05);
            for (f, s, m) in [
                ([-4.0, 31.75, -1.0], [3.0, 1.0, 1.0], &fur),
                ([-3.5, 32.75, -1.0], [2.0, 1.0, 1.0], &fur),
                ([-3.0, 33.75, -1.0], [1.0, 1.0, 1.0], &fur),
                ([-3.5, 32.0, -0.1], [2.0, 1.0, 0.25], &pink),
            ] {
                b.cube(f, s, m);
                b.cube(mirror(f, s), s, m);
            }
        }
        "hat.gafas" => {
            b = ModelBuilder::new(Attach::Head, 64);
            b.bone("root", [0.0, 28.0, 4.0]);
            let lens = Mat::new([22, 22, 30]).noise(0.02);
            b.cube([-3.75, 27.0, 4.05], [3.0, 2.0, 0.5], &lens.clone().decal(Face::Front, &["w..", "..."], SHINE));
            b.cube([0.75, 27.0, 4.05], [3.0, 2.0, 0.5], &lens.decal(Face::Front, &["w..", "..."], SHINE));
            let frame = Mat::new([40, 40, 46]).noise(0.02);
            b.cube([-0.75, 28.25, 4.05], [1.5, 0.5, 0.5], &frame);
            b.cube([-4.4, 28.25, -2.0], [0.4, 0.5, 6.1], &frame);
            b.cube([4.0, 28.25, -2.0], [0.4, 0.5, 6.1], &frame);
        }
        "back.dragon" | "back.angel" => {
            b = ModelBuilder::new(Attach::Body, 128);
            let angel = id == "back.angel";
            let (top, bottom, bar) = if angel { (WHITE, [196, 208, 232], [255, 255, 255]) } else { ([96, 48, 128], [40, 18, 60], [56, 30, 70]) };
            let mem = Mat::new(top).fade(bottom).noise(if angel { 0.06 } else { 0.1 });
            let barm = Mat::new(bar).noise(0.08);
            let segs: [([f32; 3], [f32; 3]); 3] = if angel {
                [([-6.0, 10.0, -3.0], [5.0, 11.0, 0.5]), ([-11.0, 11.0, -3.0], [5.0, 10.0, 0.5]), ([-15.0, 13.0, -3.0], [4.0, 8.0, 0.5])]
            } else {
                [([-6.0, 13.0, -3.0], [5.0, 8.0, 0.5]), ([-11.0, 14.5, -3.0], [5.0, 6.5, 0.5]), ([-15.0, 16.5, -3.0], [4.0, 4.5, 0.5])]
            };
            let (amp, speed) = if angel { (12.0, 0.35) } else { (18.0, 0.55) };
            b.bone("body", [0.0, 24.0, 0.0]);
            for (side, phase) in [(1.0, 0.0), (-1.0, 180.0)] {
                let name = if side > 0.0 { "right_wing" } else { "left_wing" };
                b.bone(name, [-side, 21.0, -2.5]).rot([0.0, -18.0 * side, 0.0]).motion(Motion::Swing { axis: Axis::Y, amp, speed, phase });
                for (f, s) in segs {
                    let f = if side > 0.0 { f } else { mirror(f, s) };
                    b.cube(f, s, &mem);
                }
                let (bf, bs) = ([-15.0, 21.0, -3.25], [14.0, 1.0, 1.0]);
                b.cube(if side > 0.0 { bf } else { mirror(bf, bs) }, bs, &barm);
                b.nest();
            }
        }
        "back.mochila" => {
            b = ModelBuilder::new(Attach::Body, 64);
            b.bone("root", [0.0, 18.0, -2.0]);
            let canvas = Mat::new(t).noise(0.12);
            let dark = Mat::new(lerp3(t, BLACK, 0.35)).noise(0.1);
            b.cube([-3.5, 13.0, -5.0], [7.0, 9.0, 3.0], &canvas);
            b.cube([-3.75, 20.0, -5.25], [7.5, 2.25, 3.5], &dark.clone().decal(Face::Back, &[".....y.....", ".....y....."], &[('y', [230, 200, 90])]));
            b.cube([-2.5, 14.0, -5.5], [5.0, 4.0, 0.5], &dark);
        }
        "back.propulsor" => {
            b = ModelBuilder::new(Attach::Body, 64);
            b.bone("root", [0.0, 18.0, -2.0]);
            let metal = Mat::new([150, 156, 168]).noise(0.08).edge([90, 96, 110]);
            let dark = Mat::new([60, 62, 70]).noise(0.05);
            b.cube([-3.75, 13.0, -5.0], [3.0, 9.0, 3.0], &metal);
            b.cube([0.75, 13.0, -5.0], [3.0, 9.0, 3.0], &metal);
            b.cube([-3.25, 11.5, -4.5], [2.0, 1.5, 2.0], &dark);
            b.cube([1.25, 11.5, -4.5], [2.0, 1.5, 2.0], &dark);
            b.bone("flame", [0.0, 11.0, -3.5]).motion(Motion::Bob { amp: 0.35, speed: 5.0 });
            let fire = Mat::new([255, 170, 50]).fade([230, 60, 20]).noise(0.15).alpha(230);
            b.cube([-2.75, 9.0, -4.0], [1.0, 2.5, 1.0], &fire);
            b.cube([1.75, 9.0, -4.0], [1.0, 2.5, 1.0], &fire);
            b.nest();
        }
        "back.carcaj" => {
            b = ModelBuilder::new(Attach::Body, 64);
            b.bone("root", [0.0, 18.0, -3.5]).rot([0.0, 0.0, 35.0]);
            b.cube([-1.5, 12.0, -4.75], [3.0, 11.0, 2.5], &Mat::new([120, 78, 44]).noise(0.12).edge([84, 52, 30]));
            let shaft = Mat::new([200, 170, 120]).noise(0.05);
            let feather = Mat::new([235, 235, 235]).noise(0.05);
            for (i, xx) in [-1.0, 0.0, 0.75].into_iter().enumerate() {
                let h = 2.5 + i as f32 * 0.5;
                b.cube([xx, 23.0, -4.0 + i as f32 * 0.4], [0.5, h, 0.5], &shaft);
                b.cube([xx - 0.25, 23.0 + h, -4.25 + i as f32 * 0.4], [1.0, 1.0, 1.0], &feather);
            }
        }
        "pet.brasita" => {
            b = ModelBuilder::new(Attach::Beside, 64);
            b.bone("float", [-13.0, 26.0, -3.0]).motion(Motion::Bob { amp: 1.2, speed: 0.5 });
            b.cube([-14.5, 22.75, -4.5], [3.0, 1.0, 3.0], &Mat::new([210, 60, 30]).noise(0.1));
            b.cube([-15.0, 23.75, -5.0], [4.0, 4.0, 4.0], &Mat::new([250, 130, 40]).noise(0.12).decal(Face::Front, &["....", "k..k", "...."], EYES_DARK));
            b.cube([-14.5, 27.75, -4.5], [3.0, 2.0, 3.0], &Mat::new([255, 200, 70]).noise(0.1));
            b.bone("tip", [-13.0, 30.0, -3.0]).motion(Motion::Swing { axis: Axis::Z, amp: 10.0, speed: 1.2, phase: 0.0 });
            b.cube([-14.0, 29.75, -4.0], [2.0, 1.5, 2.0], &Mat::new([255, 240, 150]).noise(0.08).alpha(220));
            b.nest();
        }
        "pet.gato" => {
            b = ModelBuilder::new(Attach::Beside, 64);
            let fur = Mat::new(t).noise(0.12);
            b.bone("float", [-14.5, 24.0, -3.0]).motion(Motion::Bob { amp: 0.8, speed: 0.45 });
            b.cube([-16.0, 22.0, -6.0], [3.0, 3.0, 5.0], &fur);
            b.cube([-16.25, 23.5, -1.25], [3.5, 3.0, 3.0], &fur.clone().decal(Face::Front, &["g.g", "...", ".p."], EYES_GREEN));
            b.cube([-16.25, 26.5, 0.25], [1.0, 1.0, 1.0], &fur);
            b.cube([-13.75, 26.5, 0.25], [1.0, 1.0, 1.0], &fur);
            for (lx, lz) in [(-16.0, -1.75), (-14.0, -1.75), (-16.0, -6.0), (-14.0, -6.0)] {
                b.cube([lx, 21.0, lz], [1.0, 1.25, 1.0], &fur);
            }
            b.bone("tail", [-14.5, 24.5, -6.0]).rot([30.0, 0.0, 0.0]).motion(Motion::Swing { axis: Axis::Y, amp: 28.0, speed: 0.7, phase: 0.0 });
            b.cube([-15.0, 24.0, -9.0], [1.0, 1.0, 3.0], &fur);
            b.nest();
        }
        "pet.murcielago" => {
            b = ModelBuilder::new(Attach::Beside, 64);
            let skin = Mat::new([60, 48, 58]).noise(0.1);
            let wing = Mat::new([44, 34, 44]).noise(0.08);
            b.bone("float", [-13.0, 27.0, -3.0]).motion(Motion::Bob { amp: 1.0, speed: 0.9 });
            b.cube([-14.5, 25.5, -4.0], [3.0, 3.0, 2.0], &skin.clone().decal(Face::Front, &["...", "r.r", "..."], EYES_RED));
            b.cube([-14.5, 28.5, -3.5], [0.75, 1.0, 0.75], &skin);
            b.cube([-12.25, 28.5, -3.5], [0.75, 1.0, 0.75], &skin);
            b.bone("right_wing", [-14.5, 27.5, -3.0]).motion(Motion::Swing { axis: Axis::Z, amp: 35.0, speed: 3.0, phase: 0.0 });
            b.cube([-19.5, 26.0, -3.25], [5.0, 2.0, 0.5], &wing);
            b.nest();
            b.bone("left_wing", [-11.5, 27.5, -3.0]).motion(Motion::Swing { axis: Axis::Z, amp: 35.0, speed: 3.0, phase: 180.0 });
            b.cube([-11.5, 26.0, -3.25], [5.0, 2.0, 0.5], &wing);
            b.nest();
        }
        "pet.gelatina" => {
            b = ModelBuilder::new(Attach::Beside, 64);
            b.bone("float", [-13.5, 24.0, -3.0]).motion(Motion::Bob { amp: 1.5, speed: 0.9 });
            b.cube([-14.5, 23.0, -4.0], [2.0, 2.0, 2.0], &Mat::new(lerp3(t, BLACK, 0.4)).noise(0.05));
            b.cube([-16.0, 21.5, -5.5], [5.0, 5.0, 5.0], &Mat::new(t).noise(0.1).alpha(165).decal(Face::Front, &["k.k", "...", ".k."], EYES_DARK));
        }
        "hat.calabaza" => {
            b = ModelBuilder::new(Attach::Head, 64);
            b.bone("root", [0.0, 32.0, 0.0]);
            static FACE: &[&str] = &[".........", ".........", ".yy...yy.", ".yo...oy.", "....y....", ".........", ".y.yyy.y.", "..yyoyy..", "........."];
            b.cube([-4.5, 23.5, -4.5], [9.0, 9.0, 9.0], &Mat::new([226, 118, 28]).noise(0.14).edge([170, 76, 16]).decal(Face::Front, FACE, GLOW));
            b.cube([-0.5, 32.5, -0.5], [1.0, 1.5, 1.0], &Mat::new([70, 110, 40]).noise(0.1));
            b.cube([0.5, 33.25, -0.5], [1.5, 0.5, 1.0], &Mat::new([90, 150, 50]).noise(0.1));
        }
        "hat.bruja" => {
            b = ModelBuilder::new(Attach::Head, 64);
            b.bone("root", [0.0, 32.0, 0.0]);
            let felt = Mat::new([46, 30, 56]).noise(0.12);
            b.cube([-6.0, 31.75, -6.0], [12.0, 0.75, 12.0], &felt);
            b.cube([-4.0, 32.5, -4.0], [8.0, 3.0, 8.0], &felt);
            b.cube([-4.25, 32.5, -4.25], [8.5, 1.25, 8.5], &Mat::new(t).noise(0.06).decal(Face::Front, &["....y....", "....y...."], BUCKLE));
            b.cube([-3.0, 35.5, -3.0], [6.0, 2.5, 6.0], &felt);
            b.bone("tip", [0.0, 38.0, 0.0]).rot([-25.0, 0.0, 0.0]);
            b.cube([-2.0, 38.0, -2.0], [4.0, 2.0, 4.0], &felt);
            b.cube([-1.0, 40.0, -1.0], [2.0, 2.0, 2.0], &felt);
            b.nest();
        }
        "hat.vikingo" => {
            b = ModelBuilder::new(Attach::Head, 64);
            b.bone("root", [0.0, 32.0, 0.0]);
            let iron = Mat::new([150, 154, 162]).noise(0.1).edge([96, 100, 110]);
            b.cube([-4.5, 31.0, -4.5], [9.0, 3.0, 9.0], &iron);
            b.cube([-4.75, 30.5, -4.75], [9.5, 1.0, 9.5], &Mat::new([176, 120, 60]).noise(0.1));
            b.cube([-0.5, 28.0, 4.25], [1.0, 3.0, 0.5], &iron);
            for (f, s, col) in [
                ([-6.5, 31.75, -0.75], [2.0, 1.5, 1.5], [236, 226, 204]),
                ([-7.5, 32.75, -0.5], [1.25, 1.75, 1.0], [210, 194, 166]),
                ([-7.75, 34.5, -0.4], [0.75, 1.25, 0.8], [150, 130, 104]),
            ] {
                let m = Mat::new(col).noise(0.08);
                b.cube(f, s, &m);
                b.cube(mirror(f, s), s, &m);
            }
        }
        "hat.flores" => {
            b = ModelBuilder::new(Attach::Head, 64);
            b.bone("root", [0.0, 32.0, 0.0]);
            let vine = Mat::new([70, 130, 60]).noise(0.12);
            b.cube([-4.25, 31.25, 3.75], [8.5, 0.75, 0.75], &vine);
            b.cube([-4.25, 31.25, -4.5], [8.5, 0.75, 0.75], &vine);
            b.cube([-4.5, 31.25, -4.5], [0.75, 0.75, 9.0], &vine);
            b.cube([3.75, 31.25, -4.5], [0.75, 0.75, 9.0], &vine);
            let petals = [[240, 120, 160], [250, 220, 80], [245, 245, 245], [120, 160, 240], [230, 90, 90]];
            for (i, (x, z)) in [(-3.5, 3.75), (-0.5, 4.0), (2.5, 3.75), (3.75, 1.0), (3.75, -2.5), (1.0, -4.5), (-2.5, -4.5), (-4.5, -1.5), (-4.5, 1.75)]
                .into_iter()
                .enumerate()
            {
                b.cube([x, 31.75, z], [1.0, 1.0, 1.0], &Mat::new(petals[i % petals.len()]).noise(0.06));
            }
        }
        "hat.auriculares" => {
            b = ModelBuilder::new(Attach::Head, 64);
            b.bone("root", [0.0, 32.0, 0.0]);
            let dark = Mat::new([40, 40, 46]).noise(0.06);
            let cup = Mat::new(t).noise(0.08).edge(lerp3(t, BLACK, 0.4));
            b.cube([-4.5, 32.0, -0.75], [9.0, 0.75, 1.5], &dark);
            b.cube([-4.75, 29.0, -0.75], [0.75, 3.5, 1.5], &dark);
            b.cube([4.0, 29.0, -0.75], [0.75, 3.5, 1.5], &dark);
            b.cube([-5.25, 26.5, -1.5], [1.0, 3.0, 3.0], &cup);
            b.cube([4.25, 26.5, -1.5], [1.0, 3.0, 3.0], &cup);
        }
        "back.mariposa" => {
            // cada ala, en pixel-art: una fila de cubitos por tramo del mismo color
            b = ModelBuilder::new(Attach::Body, 128);
            static WING: &[&str] = &[
                "..kkkk..", ".k####k.", "k##ww##k", "k##ww##k", ".k#####k", "..k####k", "...kkkk.", "........", "...kkk..", "..k#w#k.", "..k###k.", "...kkk..",
            ];
            let mats = [('#', Mat::new(t).noise(0.08)), ('k', Mat::new(lerp3(t, BLACK, 0.7)).noise(0.05)), ('w', Mat::new(WHITE).noise(0.04))];
            b.bone("body", [0.0, 24.0, 0.0]);
            for (side, phase) in [(1.0, 0.0), (-1.0, 180.0)] {
                let name = if side > 0.0 { "right_wing" } else { "left_wing" };
                b.bone(name, [-side, 18.0, -2.5]).rot([0.0, -12.0 * side, 0.0]).motion(Motion::Swing { axis: Axis::Y, amp: 28.0, speed: 0.8, phase });
                for (r, row) in WING.iter().enumerate() {
                    let cells: Vec<char> = row.chars().collect();
                    let mut c = 0;
                    while c < cells.len() {
                        let ch = cells[c];
                        let start = c;
                        while c < cells.len() && cells[c] == ch {
                            c += 1;
                        }
                        let Some((_, m)) = mats.iter().find(|(k, _)| *k == ch) else { continue };
                        // la columna 0 está junto a la espalda; hacia fuera, x más negativa
                        let (f, s) = ([-(1.0 + c as f32), 22.5 - r as f32, -3.0], [(c - start) as f32, 1.0, 0.5]);
                        b.cube(if side > 0.0 { f } else { mirror(f, s) }, s, m);
                    }
                }
                b.nest();
            }
        }
        "back.espada" => {
            b = ModelBuilder::new(Attach::Body, 64);
            b.bone("root", [0.0, 18.0, -3.0]).rot([0.0, 0.0, 40.0]);
            b.cube([-0.5, 9.0, -3.75], [1.0, 12.5, 0.5], &Mat::new([210, 216, 226]).noise(0.05).edge([150, 158, 172]));
            b.cube([-2.0, 21.5, -4.0], [4.0, 1.0, 1.0], &Mat::new(GOLD).noise(0.1));
            b.cube([-0.5, 22.5, -3.85], [1.0, 3.0, 0.7], &Mat::new([100, 60, 36]).noise(0.12));
            b.cube([-0.75, 25.5, -4.0], [1.5, 1.0, 1.0], &Mat::new(GOLD).noise(0.1));
        }
        "back.escoba" => {
            b = ModelBuilder::new(Attach::Body, 64);
            b.bone("root", [0.0, 18.0, -3.0]).rot([0.0, 0.0, -35.0]);
            b.cube([-0.5, 8.0, -3.75], [1.0, 17.0, 1.0], &Mat::new([120, 80, 46]).noise(0.12));
            b.cube([-1.75, 7.5, -4.5], [3.5, 0.75, 2.5], &Mat::new([60, 40, 30]).noise(0.08));
            b.cube([-1.5, 3.5, -4.25], [3.0, 4.0, 2.0], &Mat::new([226, 196, 110]).fade([170, 120, 60]).noise(0.18));
        }
        "back.guitarra" => {
            b = ModelBuilder::new(Attach::Body, 64);
            b.bone("root", [0.0, 18.0, -3.0]).rot([0.0, 0.0, 25.0]);
            b.cube(
                [-3.0, 10.5, -4.5],
                [6.0, 7.0, 1.5],
                &Mat::new(t).noise(0.08).edge(lerp3(t, BLACK, 0.5)).decal(
                    Face::Back,
                    &["......", "..kk..", ".kkkk.", "..kk..", "......", "wwwwww", "......"],
                    HOLE,
                ),
            );
            b.cube([-0.5, 17.5, -4.25], [1.0, 7.0, 1.0], &Mat::new([110, 72, 40]).noise(0.1));
            b.cube([-0.75, 24.5, -4.3], [1.5, 2.0, 1.1], &Mat::new([40, 30, 26]).noise(0.08));
        }
        "pet.fantasma" => {
            b = ModelBuilder::new(Attach::Beside, 64);
            let sheet = Mat::new([236, 238, 246]).noise(0.05).alpha(200);
            b.bone("float", [-13.5, 25.0, -3.0]).motion(Motion::Bob { amp: 1.6, speed: 0.4 });
            b.bone("sway", [-13.5, 28.0, -3.0]).motion(Motion::Swing { axis: Axis::Z, amp: 8.0, speed: 0.5, phase: 0.0 });
            b.cube([-15.5, 23.0, -5.0], [4.0, 5.0, 4.0], &sheet.clone().decal(Face::Front, &["....", "k.k.", "....", ".k..", "...."], EYES_DARK));
            b.cube([-15.5, 22.0, -5.0], [1.0, 1.0, 4.0], &sheet);
            b.cube([-13.5, 22.0, -5.0], [1.0, 1.0, 4.0], &sheet);
            b.cube([-16.25, 25.0, -3.5], [0.75, 1.0, 1.0], &sheet);
            b.cube([-11.5, 25.0, -3.5], [0.75, 1.0, 1.0], &sheet);
            b.nest();
        }
        "pet.calabacita" => {
            b = ModelBuilder::new(Attach::Beside, 64);
            b.bone("hop", [-13.5, 22.0, -3.0]).motion(Motion::Bob { amp: 2.0, speed: 1.2 });
            b.cube(
                [-15.5, 21.0, -5.0],
                [4.0, 3.5, 4.0],
                &Mat::new([230, 122, 30]).noise(0.12).edge([170, 80, 18]).decal(Face::Front, &["y..y", "....", "yyyy", "...."], GLOW),
            );
            b.cube([-14.0, 24.5, -3.5], [1.0, 1.0, 1.0], &Mat::new([70, 110, 40]).noise(0.1));
            b.cube([-13.0, 24.75, -3.5], [1.5, 0.5, 1.0], &Mat::new([90, 150, 50]).noise(0.1));
        }
        "pet.dragoncito" => {
            b = ModelBuilder::new(Attach::Beside, 64);
            let scale = Mat::new(t).noise(0.14);
            let belly = Mat::new(lerp3(t, [240, 220, 150], 0.6)).noise(0.08);
            b.bone("float", [-14.5, 25.0, -3.0]).motion(Motion::Bob { amp: 1.0, speed: 0.6 });
            b.cube([-16.0, 23.0, -5.5], [3.0, 3.0, 5.0], &scale);
            b.cube([-15.5, 22.75, -5.0], [2.0, 0.5, 4.0], &belly);
            b.cube([-16.25, 24.5, -0.75], [3.5, 3.0, 3.0], &scale.clone().decal(Face::Front, &["y.y", "...", "..."], EYES_YELLOW));
            b.cube([-15.5, 24.5, 2.25], [2.0, 1.5, 1.5], &scale);
            b.cube([-16.0, 27.5, 0.0], [0.75, 1.0, 0.75], &Mat::new([236, 226, 204]).noise(0.05));
            b.cube([-13.75, 27.5, 0.0], [0.75, 1.0, 0.75], &Mat::new([236, 226, 204]).noise(0.05));
            let wing = Mat::new(lerp3(t, BLACK, 0.3)).noise(0.08);
            b.bone("right_wing", [-16.0, 26.0, -3.0]).motion(Motion::Swing { axis: Axis::Z, amp: 30.0, speed: 1.8, phase: 0.0 });
            b.cube([-20.0, 25.5, -4.5], [4.0, 0.5, 3.0], &wing);
            b.nest();
            b.bone("left_wing", [-13.0, 26.0, -3.0]).motion(Motion::Swing { axis: Axis::Z, amp: 30.0, speed: 1.8, phase: 180.0 });
            b.cube([-13.0, 25.5, -4.5], [4.0, 0.5, 3.0], &wing);
            b.nest();
            b.bone("tail", [-14.5, 24.0, -5.5]).rot([20.0, 0.0, 0.0]).motion(Motion::Swing { axis: Axis::Y, amp: 22.0, speed: 0.8, phase: 0.0 });
            b.cube([-15.0, 23.5, -8.5], [1.0, 1.0, 3.0], &scale);
            b.nest();
        }
        "pet.abeja" => {
            b = ModelBuilder::new(Attach::Beside, 64);
            let yellow = Mat::new([246, 200, 50]).noise(0.1);
            let black = Mat::new([34, 30, 30]).noise(0.08);
            b.bone("float", [-13.5, 26.0, -3.0]).motion(Motion::Bob { amp: 1.0, speed: 1.4 });
            b.cube([-14.75, 25.25, -1.0], [2.5, 2.5, 1.5], &black.clone().decal(Face::Front, &["w.w", "..."], &[('w', [250, 250, 250])]));
            b.cube([-15.0, 25.0, -2.5], [3.0, 3.0, 1.5], &yellow);
            b.cube([-15.0, 25.0, -3.25], [3.0, 3.0, 0.75], &black);
            b.cube([-15.0, 25.0, -4.5], [3.0, 3.0, 1.25], &yellow);
            b.cube([-14.0, 26.0, -5.0], [1.0, 1.0, 0.5], &black);
            let wing = Mat::new([235, 245, 255]).noise(0.04).alpha(170);
            b.bone("right_wing", [-15.0, 28.0, -2.75]).motion(Motion::Swing { axis: Axis::Z, amp: 30.0, speed: 6.0, phase: 0.0 });
            b.cube([-17.0, 28.0, -3.75], [2.0, 0.25, 2.0], &wing);
            b.nest();
            b.bone("left_wing", [-12.0, 28.0, -2.75]).motion(Motion::Swing { axis: Axis::Z, amp: 30.0, speed: 6.0, phase: 180.0 });
            b.cube([-12.0, 28.0, -3.75], [2.0, 0.25, 2.0], &wing);
            b.nest();
        }
        _ => return None,
    }
    Some(b.finish())
}

// ------------------------------------------------------------------ capas
/// Textura de capa 64×32 (formato de Minecraft: cara de fuera en (1,1), de dentro en (12,1)).
pub fn cape(id: &str, tint: Option<[u8; 3]>, frame: u32) -> Option<Canvas> {
    let x = get(id).filter(|x| x.slot == Slot::Cape)?;
    let t = tint.or(x.tint).unwrap_or([200, 60, 40]);
    let phase = if x.frames > 1 { (frame % x.frames) as f32 / x.frames as f32 } else { 0.0 };
    let design = |px: u32, py: u32| -> [u8; 3] {
        let (fx, fy) = (px as f32 / 9.0, py as f32 / 15.0);
        match id {
            "cape.brasa" => {
                // llamas que suben: intensidad mayor abajo, con ruido que se desplaza
                let shift = (phase * 16.0) as u32;
                let n = noise(px, py + shift, 7) * 0.55 + noise(px / 2, (py + shift) / 2, 11) * 0.45;
                let heat = (fy * 1.25 + (n - 0.5) * 0.7).clamp(0.0, 1.0);
                if heat < 0.35 {
                    lerp3([22, 12, 10], [90, 20, 12], heat / 0.35)
                } else if heat < 0.7 {
                    lerp3([150, 30, 14], [240, 110, 30], (heat - 0.35) / 0.35)
                } else {
                    lerp3([250, 140, 40], [255, 225, 120], (heat - 0.7) / 0.3)
                }
            }
            "cape.noche" => {
                let base = lerp3([10, 14, 44], [30, 40, 96], fy);
                let star = noise(px, py, 3);
                let moon = ((px as i32 - 6).pow(2) + (py as i32 - 3).pow(2)) <= 3;
                let bite = ((px as i32 - 7).pow(2) + (py as i32 - 2).pow(2)) <= 2;
                if moon && !bite {
                    [245, 238, 200]
                } else if star > 0.9 {
                    let tw = (phase * std::f32::consts::TAU + star * 40.0).sin() * 0.5 + 0.5;
                    lerp3(base, [255, 255, 230], 0.45 + tw * 0.55)
                } else {
                    base
                }
            }
            "cape.aurora" => {
                let base = lerp3([6, 16, 30], [10, 30, 50], fy);
                let tau = std::f32::consts::TAU;
                let w1 = ((fx * 2.0 + phase) * tau).sin() * 0.18 + 0.35;
                let w2 = ((fx * 1.5 - phase + 0.3) * tau).sin() * 0.15 + 0.62;
                let d1 = (fy - w1).abs();
                let d2 = (fy - w2).abs();
                if d1 < 0.09 {
                    lerp3([80, 240, 160], base, d1 / 0.09)
                } else if d2 < 0.08 {
                    lerp3([170, 90, 230], base, d2 / 0.08)
                } else {
                    base
                }
            }
            "cape.lava" => {
                let shift = (phase * 16.0) as u32;
                let n = noise(px / 2, (py + 32 - shift) / 2, 5) * 0.6 + noise(px, py + 32 - shift, 9) * 0.4;
                if n > 0.62 {
                    lerp3([255, 150, 30], [255, 230, 120], (n - 0.62) / 0.38)
                } else if n > 0.48 {
                    lerp3([180, 40, 10], [255, 120, 20], (n - 0.48) / 0.14)
                } else {
                    lerp3([40, 24, 22], [70, 34, 28], n / 0.48)
                }
            }
            "cape.real" => {
                let border = px == 0 || px == 9 || py == 0 || py == 15;
                let emblem = FLAME.get(py.wrapping_sub(4) as usize).and_then(|r| r.chars().nth(px.wrapping_sub(2) as usize)).filter(|ch| *ch != '.');
                if border {
                    GOLD
                } else if emblem.is_some() {
                    [250, 210, 90]
                } else {
                    lerp3([84, 28, 120], [50, 14, 76], fy)
                }
            }
            "cape.ajedrez" => {
                if ((px / 2) + (py / 2)).is_multiple_of(2) {
                    [236, 236, 236]
                } else {
                    [28, 28, 30]
                }
            }
            "cape.arcoiris" => {
                const BANDS: [[u8; 3]; 6] = [[228, 52, 52], [244, 140, 40], [246, 214, 60], [70, 180, 80], [60, 120, 220], [130, 70, 190]];
                BANDS[((fy * 5.99) as usize).min(5)]
            }
            "cape.calabazas" => {
                let base = lerp3([34, 16, 46], [16, 8, 24], fy);
                let flicker = 0.55 + 0.45 * ((phase * std::f32::consts::TAU).sin() * 0.5 + 0.5);
                let mut col = if noise(px, py, 21) > 0.93 { [200, 190, 220] } else { base };
                for (cx, cy, r) in [(3i32, 4i32, 2i32), (6, 11, 3)] {
                    let (dx, dy) = (px as i32 - cx, py as i32 - cy);
                    if dx == 0 && dy == -r - 1 {
                        col = [70, 110, 40];
                    }
                    if dx * dx + dy * dy <= r * r + 1 {
                        let face = (dy == -1 && dx.abs() == 1) || (dy == 1 && dx.abs() <= 1) || (r >= 3 && dy == 2 && dx == 0);
                        col = if face {
                            lerp3([140, 60, 10], [255, 220, 90], flicker)
                        } else {
                            lerp3([236, 130, 30], [190, 90, 20], (dx.abs() as f32) / r as f32)
                        };
                    }
                }
                col
            }
            "cape.telarana" => {
                // un cuarto de tela en la esquina de arriba y la araña colgando de su hilo
                let (x, y) = (px as i32, py as i32);
                let d = ((x * x + y * y) as f32).sqrt();
                let arc = (2.5..3.4).contains(&d) || (5.5..6.4).contains(&d);
                let web = d < 7.0 && (x == 0 || y == 0 || x == y || arc);
                let thread = px == 8 && py <= 10;
                let body = (7..=8).contains(&px) && (11..=12).contains(&py);
                let legs = (py == 11 || py == 12) && (px == 6 || px == 9);
                if body {
                    if px == 8 && py == 12 {
                        [210, 30, 30]
                    } else {
                        [14, 12, 14]
                    }
                } else if legs {
                    [34, 30, 34]
                } else if web || thread {
                    [206, 206, 218]
                } else {
                    lerp3([46, 32, 62], [16, 10, 24], fy)
                }
            }
            "cape.oceano" => {
                let tau = std::f32::consts::TAU;
                let base = lerp3([70, 170, 220], [10, 40, 110], fy);
                let mut col = base;
                for band in 0..4 {
                    let y0 = 0.15 + band as f32 * 0.22;
                    let w = ((fx * 1.5 + phase + band as f32 * 0.3) * tau).sin() * 0.05 + y0;
                    if (fy - w).abs() < 0.035 {
                        col = lerp3([240, 250, 255], base, 0.15 + band as f32 * 0.15);
                    }
                }
                col
            }
            "cape.galaxia" => {
                let n = noise(px / 2, py / 2, 31) * 0.6 + noise(px, py, 37) * 0.4;
                let base = if n > 0.6 {
                    lerp3([120, 50, 160], [230, 110, 190], (n - 0.6) / 0.4)
                } else if n > 0.42 {
                    lerp3([30, 30, 90], [120, 50, 160], (n - 0.42) / 0.18)
                } else {
                    [10, 8, 24]
                };
                let star = noise(px, py, 41);
                if star > 0.88 {
                    let tw = (phase * std::f32::consts::TAU + star * 50.0).sin() * 0.5 + 0.5;
                    lerp3(base, [255, 255, 255], 0.4 + tw * 0.6)
                } else {
                    base
                }
            }
            "cape.bosque" => {
                let sky = if fy < 0.5 { lerp3([250, 170, 90], [200, 90, 110], fy * 2.0) } else { lerp3([200, 90, 110], [70, 40, 90], (fy - 0.5) * 2.0) };
                let sun = ((px as i32 - 5).pow(2) + (py as i32 - 6).pow(2)) <= 4;
                let tree = [(1i32, 5i32), (4, 7), (8, 6)].iter().any(|(cx, h)| {
                    let top = 15 - h;
                    py as i32 >= top && (px as i32 - cx).abs() * 2 <= (py as i32 - top) + 1
                });
                if tree {
                    lerp3([20, 40, 30], [10, 22, 18], fy)
                } else if sun {
                    [255, 220, 140]
                } else {
                    sky
                }
            }
            _ => {
                // clásica: lisa con borde claro y la llama en el centro
                let border = px == 0 || px == 9 || py == 0 || py == 15;
                let flame = FLAME.get(py.wrapping_sub(4) as usize).and_then(|r| r.chars().nth(px.wrapping_sub(2) as usize));
                match flame {
                    Some('o') => lerp3(t, WHITE, 0.55),
                    Some('y') => lerp3(t, WHITE, 0.8),
                    _ if border => lerp3(t, WHITE, 0.3),
                    _ => t,
                }
            }
        }
    };
    let mut cv = Canvas::new(64, 32);
    for py in 0..16 {
        for px in 0..10 {
            let col = design(px, py);
            let n = 1.0 + (noise(px, py, 99) - 0.5) * 0.06;
            cv.set(1 + px, 1 + py, shade(col, n));
            // por dentro, el mismo dibujo en espejo y más oscuro
            cv.set(12 + (9 - px), 1 + py, shade(col, 0.55));
        }
        cv.set(0, 1 + py, shade(design(0, py), 0.8));
        cv.set(11, 1 + py, shade(design(9, py), 0.8));
    }
    for px in 0..10 {
        cv.set(1 + px, 0, shade(design(px, 0), 1.1));
        cv.set(11 + px, 0, shade(design(px, 15), 0.7));
    }
    Some(cv)
}

// ------------------------------------------------------------------ skin por defecto
/// Skin original de Brasa (64×64) para la vista previa cuando la cuenta no tiene una propia:
/// pelo castaño, sudadera naranja, vaqueros y botas.
pub fn default_skin() -> Canvas {
    use super::paint::paint_box;
    let mut cv = Canvas::new(64, 64);
    let skin = Mat::new([214, 160, 120]).noise(0.04);
    let hoodie = Mat::new([226, 112, 44]).noise(0.08);
    let jeans = Mat::new([52, 72, 118]).noise(0.1);
    // cabeza: piel, con el pelo pintado encima
    paint_box(&mut cv, [0, 0], [8, 8, 8], &skin, 1);
    let hair = [70, 44, 30];
    for (x0, y0, w, h) in [(8, 0, 8, 8), (0, 8, 8, 3), (16, 8, 8, 3), (24, 8, 8, 8), (8, 8, 8, 2)] {
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                cv.set(x, y, shade(hair, 0.9 + noise(x, y, 4) * 0.2));
            }
        }
    }
    // cara: ojos, cejas y boca
    for (x, y, col) in [
        (9, 12, WHITE),
        (10, 12, [60, 110, 170]),
        (14, 12, WHITE),
        (13, 12, [60, 110, 170]),
        (9, 11, hair),
        (10, 11, hair),
        (13, 11, hair),
        (14, 11, hair),
        (11, 14, [160, 90, 80]),
        (12, 14, [160, 90, 80]),
    ] {
        cv.set(x, y, shade(col, 1.0));
    }
    // cuerpo y brazos: sudadera; manos de piel; cordones claros
    paint_box(&mut cv, [16, 16], [8, 12, 4], &hoodie, 2);
    cv.set(22, 21, shade(WHITE, 0.95));
    cv.set(25, 21, shade(WHITE, 0.95));
    for uv in [[40, 16], [32, 48]] {
        paint_box(&mut cv, uv, [4, 12, 4], &hoodie, 3);
        for x in 0..16 {
            cv.set(uv[0] + x, uv[1] + 4 + 11, shade([214, 160, 120], 0.95));
        }
    }
    // piernas: vaqueros con botas
    for uv in [[0, 16], [16, 48]] {
        paint_box(&mut cv, uv, [4, 12, 4], &jeans, 5);
        for y in 0..2 {
            for x in 0..16 {
                cv.set(uv[0] + x, uv[1] + 4 + 10 + y, shade([60, 40, 30], 0.9 + noise(x, y, 8) * 0.15));
            }
        }
    }
    cv
}

// ------------------------------------------------------------------ emotes
fn key(t: f32, bones: &[(&str, [f32; 3])]) -> Key {
    Key { t, bones: bones.iter().map(|(n, r)| (n.to_string(), *r)).collect(), lift: 0.0, yaw: 0.0 }
}
fn lifted(mut k: Key, lift: f32, yaw: f32) -> Key {
    k.lift = lift;
    k.yaw = yaw;
    k
}

pub fn emote(id: &str) -> Option<Emote> {
    let e = |duration: f32, looping: bool, keys: Vec<Key>| Emote { duration, looping, keys };
    Some(match id {
        "emote.saludar" => e(
            1.2,
            true,
            vec![
                key(0.0, &[("right_arm", [0.0, 0.0, -145.0]), ("head", [0.0, 0.0, -8.0])]),
                key(0.3, &[("right_arm", [0.0, 0.0, -170.0]), ("head", [0.0, 0.0, -10.0])]),
                key(0.6, &[("right_arm", [0.0, 0.0, -145.0]), ("head", [0.0, 0.0, -8.0])]),
                key(0.9, &[("right_arm", [0.0, 0.0, -170.0]), ("head", [0.0, 0.0, -10.0])]),
            ],
        ),
        "emote.bailar" => e(
            1.0,
            true,
            vec![
                lifted(
                    key(0.0, &[("right_arm", [0.0, 0.0, -140.0]), ("left_arm", [0.0, 0.0, 25.0]), ("body", [0.0, 0.0, -6.0]), ("head", [0.0, 0.0, -10.0])]),
                    0.0,
                    -15.0,
                ),
                lifted(key(0.25, &[("right_arm", [0.0, 0.0, -80.0]), ("left_arm", [0.0, 0.0, 80.0]), ("right_leg", [-25.0, 0.0, 0.0])]), 1.2, 0.0),
                lifted(
                    key(0.5, &[("right_arm", [0.0, 0.0, -25.0]), ("left_arm", [0.0, 0.0, 140.0]), ("body", [0.0, 0.0, 6.0]), ("head", [0.0, 0.0, 10.0])]),
                    0.0,
                    15.0,
                ),
                lifted(key(0.75, &[("right_arm", [0.0, 0.0, -80.0]), ("left_arm", [0.0, 0.0, 80.0]), ("left_leg", [-25.0, 0.0, 0.0])]), 1.2, 0.0),
            ],
        ),
        "emote.aplaudir" => e(
            0.5,
            true,
            vec![
                key(0.0, &[("right_arm", [-75.0, 28.0, 0.0]), ("left_arm", [-75.0, -28.0, 0.0]), ("head", [8.0, 0.0, 0.0])]),
                key(0.25, &[("right_arm", [-75.0, 2.0, 0.0]), ("left_arm", [-75.0, -2.0, 0.0]), ("head", [4.0, 0.0, 0.0])]),
            ],
        ),
        "emote.victoria" => e(
            0.9,
            true,
            vec![
                lifted(key(0.0, &[("right_arm", [0.0, 0.0, -160.0]), ("left_arm", [0.0, 0.0, 160.0]), ("head", [-15.0, 0.0, 0.0])]), 0.0, 0.0),
                lifted(
                    key(
                        0.3,
                        &[
                            ("right_arm", [0.0, 0.0, -170.0]),
                            ("left_arm", [0.0, 0.0, 170.0]),
                            ("head", [-20.0, 0.0, 0.0]),
                            ("right_leg", [-15.0, 0.0, 0.0]),
                            ("left_leg", [15.0, 0.0, 0.0]),
                        ],
                    ),
                    3.5,
                    0.0,
                ),
                lifted(key(0.6, &[("right_arm", [0.0, 0.0, -150.0]), ("left_arm", [0.0, 0.0, 150.0]), ("head", [-12.0, 0.0, 0.0])]), 0.0, 0.0),
            ],
        ),
        "emote.dab" => e(
            2.0,
            false,
            vec![
                key(0.0, &[]),
                key(0.25, &[("right_arm", [0.0, 0.0, -130.0]), ("left_arm", [-115.0, -55.0, 0.0]), ("head", [30.0, 0.0, 25.0]), ("body", [0.0, 0.0, 6.0])]),
                key(1.7, &[("right_arm", [0.0, 0.0, -132.0]), ("left_arm", [-115.0, -57.0, 0.0]), ("head", [32.0, 0.0, 25.0]), ("body", [0.0, 0.0, 6.0])]),
                key(2.0, &[]),
            ],
        ),
        "emote.facepalm" => e(
            2.2,
            false,
            vec![
                key(0.0, &[]),
                key(0.35, &[("right_arm", [-135.0, 32.0, 0.0]), ("head", [28.0, 0.0, 0.0])]),
                key(0.9, &[("right_arm", [-135.0, 32.0, 0.0]), ("head", [28.0, -8.0, 0.0])]),
                key(1.4, &[("right_arm", [-135.0, 32.0, 0.0]), ("head", [28.0, 8.0, 0.0])]),
                key(1.9, &[("right_arm", [-135.0, 32.0, 0.0]), ("head", [26.0, 0.0, 0.0])]),
                key(2.2, &[]),
            ],
        ),
        "emote.pensar" => e(
            3.0,
            true,
            vec![
                key(0.0, &[("right_arm", [-115.0, 42.0, 0.0]), ("left_arm", [-45.0, -40.0, 0.0]), ("head", [-12.0, 0.0, 8.0])]),
                key(1.5, &[("right_arm", [-118.0, 44.0, 0.0]), ("left_arm", [-45.0, -40.0, 0.0]), ("head", [-16.0, 10.0, 10.0])]),
            ],
        ),
        "emote.sentarse" => e(
            0.6,
            false,
            vec![
                key(0.0, &[]),
                lifted(
                    key(
                        0.6,
                        &[
                            ("right_leg", [-90.0, 12.0, 0.0]),
                            ("left_leg", [-90.0, -12.0, 0.0]),
                            ("right_arm", [-35.0, 0.0, -8.0]),
                            ("left_arm", [-35.0, 0.0, 8.0]),
                        ],
                    ),
                    -12.0,
                    0.0,
                ),
            ],
        ),
        "emote.zombi" => e(
            1.6,
            true,
            vec![
                lifted(
                    key(
                        0.0,
                        &[
                            ("right_arm", [-90.0, 0.0, 0.0]),
                            ("left_arm", [-90.0, 0.0, 0.0]),
                            ("head", [10.0, 0.0, -12.0]),
                            ("right_leg", [-12.0, 0.0, 0.0]),
                            ("left_leg", [12.0, 0.0, 0.0]),
                        ],
                    ),
                    0.0,
                    -6.0,
                ),
                lifted(
                    key(
                        0.8,
                        &[
                            ("right_arm", [-84.0, 0.0, 0.0]),
                            ("left_arm", [-96.0, 0.0, 0.0]),
                            ("head", [10.0, 0.0, 12.0]),
                            ("right_leg", [12.0, 0.0, 0.0]),
                            ("left_leg", [-12.0, 0.0, 0.0]),
                        ],
                    ),
                    0.0,
                    6.0,
                ),
            ],
        ),
        "emote.reverencia" => e(
            2.0,
            false,
            vec![
                key(0.0, &[]),
                key(0.5, &[("body", [32.0, 0.0, 0.0]), ("head", [20.0, 0.0, 0.0]), ("right_arm", [-70.0, 40.0, 0.0]), ("left_arm", [25.0, 0.0, 0.0])]),
                key(1.5, &[("body", [32.0, 0.0, 0.0]), ("head", [22.0, 0.0, 0.0]), ("right_arm", [-70.0, 40.0, 0.0]), ("left_arm", [25.0, 0.0, 0.0])]),
                key(2.0, &[]),
            ],
        ),
        "emote.presumir" => e(
            1.0,
            true,
            vec![
                key(0.0, &[("right_arm", [0.0, 0.0, -90.0]), ("left_arm", [0.0, 0.0, 90.0]), ("head", [-8.0, 0.0, 0.0])]),
                lifted(key(0.5, &[("right_arm", [-25.0, 0.0, -115.0]), ("left_arm", [-25.0, 0.0, 115.0]), ("head", [-14.0, 0.0, 0.0])]), 0.6, 0.0),
            ],
        ),
        "emote.encogerse" => e(
            1.6,
            false,
            vec![
                key(0.0, &[]),
                lifted(key(0.35, &[("right_arm", [-20.0, 0.0, -35.0]), ("left_arm", [-20.0, 0.0, 35.0]), ("head", [0.0, 0.0, 12.0])]), 0.6, 0.0),
                lifted(key(1.1, &[("right_arm", [-20.0, 0.0, -35.0]), ("left_arm", [-20.0, 0.0, 35.0]), ("head", [0.0, 0.0, 12.0])]), 0.6, 0.0),
                key(1.6, &[]),
            ],
        ),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_item_builds_and_ids_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for x in ALL {
            assert!(seen.insert(x.id), "id repetido: {}", x.id);
            assert!(x.id.starts_with(match x.slot {
                Slot::Cape => "cape.",
                Slot::Hat => "hat.",
                Slot::Back => "back.",
                Slot::Emote => "emote.",
                Slot::Pet => "pet.",
            }));
            match x.slot {
                Slot::Cape => {
                    for f in 0..x.frames {
                        let cv = cape(x.id, None, f).unwrap();
                        assert_eq!((cv.w, cv.h), (64, 32));
                        assert_eq!(cv.get(1, 1)[3], 255, "{} sin pintar", x.id);
                    }
                }
                Slot::Emote => {
                    let e = emote(x.id).unwrap_or_else(|| panic!("{} sin emote", x.id));
                    assert!(e.keys.windows(2).all(|w| w[0].t < w[1].t), "{}: fotogramas desordenados", x.id);
                    assert!(e.keys.iter().all(|k| k.t <= e.duration));
                }
                _ => {
                    let (m, cv) = build(x.id, None).unwrap_or_else(|| panic!("{} sin modelo", x.id));
                    let mut cubes = 0;
                    fn walk(b: &crate::cosmetics::Bone, f: &mut dyn FnMut(&crate::cosmetics::Cube)) {
                        b.cubes.iter().for_each(&mut *f);
                        b.children.iter().for_each(|c| walk(c, f));
                    }
                    for bone in &m.bones {
                        walk(bone, &mut |cu| {
                            cubes += 1;
                            let (w, h) = crate::cosmetics::paint::box_uv_size([cu.size[0].ceil() as u32, cu.size[1].ceil() as u32, cu.size[2].ceil() as u32]);
                            assert!(cu.uv[0] + w <= cv.w && cu.uv[1] + h <= cv.h, "{}: UV fuera de la textura", x.id);
                        });
                    }
                    assert!(cubes > 0, "{} vacío", x.id);
                    let expected = match x.slot {
                        Slot::Hat => Attach::Head,
                        Slot::Back => Attach::Body,
                        _ => Attach::Beside,
                    };
                    assert_eq!(m.attach, expected, "{}", x.id);
                }
            }
        }
        for s in Slot::ALL {
            assert!(of(s).count() >= 4, "pocos {:?}", s);
        }
    }

    #[test]
    fn tint_changes_the_texture() {
        let a = build("hat.gorro", Some([200, 0, 0])).unwrap().1;
        let b = build("hat.gorro", Some([0, 0, 200])).unwrap().1;
        assert_ne!(a.px, b.px);
        let ca = cape("cape.clasica", Some([0, 200, 0]), 0).unwrap();
        assert!(ca.get(2, 8)[1] > ca.get(2, 8)[0]);
    }

    #[test]
    fn animated_capes_change_between_frames() {
        for id in ["cape.brasa", "cape.aurora", "cape.lava", "cape.noche", "cape.calabazas", "cape.oceano", "cape.galaxia"] {
            assert_ne!(cape(id, None, 0).unwrap().px, cape(id, None, 1).unwrap().px, "{id}");
        }
        assert_eq!(cape("cape.ajedrez", None, 0).unwrap().px, cape("cape.ajedrez", None, 3).unwrap().px);
    }
}
