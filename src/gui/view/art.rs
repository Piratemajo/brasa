//! Identidad visual de Brasa: pixel-art (la llama, los avatares), degradados y el botón principal.
//!
//! El motivo es el «escalón» de píxel de la interfaz de Minecraft: esquinas recortadas en un paso
//! en lugar de redondeadas, y bisel claro arriba / sombra abajo en lo que se puede pulsar.

use crate::config::Loader;
use crate::gui::view::fonts::Weight;
use crate::gui::view::theme::*;
use eframe::egui::{self, epaint::Mesh, Color32, Painter, Pos2, Rect, RichText, Vec2};

// ----------------------------------------------------------------------------- color

/// Mezcla lineal de dos colores opacos (`t = 0` → `a`, `t = 1` → `b`).
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

// ----------------------------------------------------------------------------- degradados

fn quad(mesh: &mut Mesh, r: Rect, tl: Color32, tr: Color32, br: Color32, bl: Color32) {
    let i = mesh.vertices.len() as u32;
    mesh.colored_vertex(r.left_top(), tl);
    mesh.colored_vertex(r.right_top(), tr);
    mesh.colored_vertex(r.right_bottom(), br);
    mesh.colored_vertex(r.left_bottom(), bl);
    mesh.add_triangle(i, i + 1, i + 2);
    mesh.add_triangle(i, i + 2, i + 3);
}

/// Rectángulo con degradado horizontal (izquierda → derecha).
pub fn gradient_h(p: &Painter, rect: Rect, left: Color32, right: Color32) {
    let mut m = Mesh::default();
    quad(&mut m, rect, left, right, right, left);
    p.add(egui::Shape::mesh(m));
}

/// Rectángulo con degradado vertical (arriba → abajo).
pub fn gradient_v(p: &Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut m = Mesh::default();
    quad(&mut m, rect, top, top, bottom, bottom);
    p.add(egui::Shape::mesh(m));
}

/// Las tres franjas que forman un rectángulo con las cuatro esquinas recortadas en un escalón.
fn notch_parts(rect: Rect, step: f32) -> [Rect; 3] {
    [
        Rect::from_min_max(rect.min + Vec2::new(0.0, step), rect.max - Vec2::new(0.0, step)),
        Rect::from_min_max(rect.min + Vec2::new(step, 0.0), Pos2::new(rect.max.x - step, rect.min.y + step)),
        Rect::from_min_max(Pos2::new(rect.min.x + step, rect.max.y - step), rect.max - Vec2::new(step, 0.0)),
    ]
}

/// Rectángulo escalonado de un solo color.
pub fn notched(p: &Painter, rect: Rect, step: f32, color: Color32) {
    for r in notch_parts(rect, step) {
        p.rect_filled(r, 0.0, color);
    }
}

/// Rectángulo escalonado con degradado vertical continuo entre las tres franjas.
pub fn notched_gradient_v(p: &Painter, rect: Rect, step: f32, top: Color32, bottom: Color32) {
    let at = |y: f32| mix(top, bottom, (y - rect.min.y) / rect.height().max(1.0));
    let mut m = Mesh::default();
    for r in notch_parts(rect, step) {
        quad(&mut m, r, at(r.min.y), at(r.min.y), at(r.max.y), at(r.max.y));
    }
    p.add(egui::Shape::mesh(m));
}

/// Rectángulo escalonado con degradado horizontal.
pub fn notched_gradient_h(p: &Painter, rect: Rect, step: f32, left: Color32, right: Color32) {
    let at = |x: f32| mix(left, right, (x - rect.min.x) / rect.width().max(1.0));
    let mut m = Mesh::default();
    for r in notch_parts(rect, step) {
        quad(&mut m, r, at(r.min.x), at(r.max.x), at(r.max.x), at(r.min.x));
    }
    p.add(egui::Shape::mesh(m));
}

// ----------------------------------------------------------------------------- la llama

/// La llama de Brasa, en pixel-art. `r` brasa, `o` llama, `y` ámbar, `w` núcleo.
pub const FLAME: [&str; 16] = [
    "......r.....",
    "......rr....",
    ".....rrr....",
    ".....rorr...",
    "....rroorr..",
    "...rrooorr..",
    "..rrooooorr.",
    "..roooyoorr.",
    ".rrooyyyoor.",
    ".roooyyyyor.",
    ".rooyyywyoor",
    ".rooyywwyoor",
    ".rooyyywyoor",
    "..roooyyoor.",
    "...rrooorr..",
    "....rrrrr...",
];

fn flame_color(c: char) -> Option<Color32> {
    Some(match c {
        'r' => EMBER,
        'o' => ACCENT,
        'y' => YELLOW,
        'w' => Color32::from_rgb(255, 240, 190),
        _ => return None,
    })
}

/// Dibuja la llama. `ash` (0..1) la apaga hacia gris ceniza: sirve para estados vacíos.
pub fn flame(ui: &mut egui::Ui, px: f32, ash: f32) -> egui::Response {
    let w = FLAME[0].len() as f32 * px;
    let h = FLAME.len() as f32 * px;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), egui::Sense::hover());
    let p = ui.painter();
    for (y, row) in FLAME.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            if let Some(c) = flame_color(ch) {
                let c = if ash > 0.0 { mix(c, mix(TEXT_DIM, BG, 0.45), ash) } else { c };
                let min = rect.min + Vec2::new(x as f32 * px, y as f32 * px);
                p.rect_filled(Rect::from_min_size(min, Vec2::splat(px)), 0.0, c);
            }
        }
    }
    resp
}

// ----------------------------------------------------------------------------- avatar de instancia

pub const GRID: usize = 7;

fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Patrón 7×7 simétrico (como un «invasor») derivado del nombre. Cada celda: `None` = fondo,
/// `Some(false)` = tono medio, `Some(true)` = tono claro.
pub fn identicon_cells(name: &str) -> [[Option<bool>; GRID]; GRID] {
    let h = fnv1a(name);
    let half = GRID.div_ceil(2); // 4 columnas se reflejan en 7
    let mut cells = [[None; GRID]; GRID];
    for (y, row) in cells.iter_mut().enumerate() {
        for x in 0..half {
            let i = y * half + x;
            let on = (h >> (i % 28)) & 1 == 1;
            let light = (h >> (28 + (i % 18) * 2)) & 3 == 0;
            let cell = on.then_some(light);
            row[x] = cell;
            row[GRID - 1 - x] = cell;
        }
    }
    cells
}

/// Pinta el avatar de una instancia dentro de `rect`: patrón único por nombre, teñido con el color de su loader.
pub fn paint_identicon(p: &Painter, rect: Rect, name: &str, loader: Loader) {
    let size = rect.width().min(rect.height());
    let base = loader_color(loader);
    let cell = (size / (GRID as f32 + 1.0)).floor().max(2.0);
    let side = cell * GRID as f32;
    let origin = rect.center() - Vec2::splat(side / 2.0);

    notched(p, rect, (size / 14.0).round().max(2.0), mix(base, BG, 0.80));
    let (mid, light) = (base, mix(base, Color32::WHITE, 0.38));
    for (y, row) in identicon_cells(name).iter().enumerate() {
        for (x, c) in row.iter().enumerate() {
            if let Some(is_light) = c {
                let min = origin + Vec2::new(x as f32 * cell, y as f32 * cell);
                p.rect_filled(Rect::from_min_size(min, Vec2::splat(cell)), 0.0, if *is_light { light } else { mid });
            }
        }
    }
}

/// Reserva un cuadrado y pinta el avatar.
pub fn identicon(ui: &mut egui::Ui, name: &str, loader: Loader, size: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    paint_identicon(ui.painter(), rect, name, loader);
    resp
}

/// Icono de una instancia: su imagen propia si la tiene (`uri`) o el avatar generado.
pub fn instance_avatar(ui: &mut egui::Ui, uri: Option<String>, name: &str, loader: Loader, size: f32) -> egui::Response {
    match uri {
        Some(uri) => ui.add(egui::Image::new(uri).fit_to_exact_size(Vec2::splat(size)).rounding((size / 11.0).round())),
        None => identicon(ui, name, loader, size),
    }
}

/// Fondo de la cabecera de una instancia: el color del loader fundiéndose con la superficie,
/// chispas propias de la instancia y la línea de brasa del borde inferior.
pub fn hero_background(p: &Painter, rect: Rect, loader_color: Color32, seed: &str) {
    notched_gradient_h(p, rect, 6.0, mix(CARD, loader_color, 0.15), CARD);
    sparks(p, rect, seed);
    gradient_h(p, Rect::from_min_size(rect.left_bottom() - Vec2::new(0.0, 3.0), Vec2::new(rect.width(), 3.0)), ACCENT, ACCENT.gamma_multiply(0.0));
}

// ----------------------------------------------------------------------------- botón principal

#[derive(Clone, Copy, PartialEq)]
pub enum BigKind {
    Play,
    Stop,
}

/// El botón JUGAR / CERRAR: degradado de brasa, esquinas escalonadas, bisel y un resplandor que
/// crece al pasar el ratón (animación que responde a la acción del usuario, no decorativa).
pub fn big_button(ui: &mut egui::Ui, kind: BigKind, label: &str, size: Vec2, enabled: bool) -> egui::Response {
    let sense = if enabled { egui::Sense::click() } else { egui::Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    let hot = ui.ctx().animate_bool_with_time(resp.id.with("hot"), enabled && resp.hovered(), 0.14);
    let pressed = enabled && resp.is_pointer_button_down_on();
    let p = ui.painter();
    let step = 4.0;

    let (top, bottom, fg) = match (kind, enabled) {
        (_, false) => (mix(CARD_HI, TEXT_DIM, 0.15), mix(CARD, BG, 0.3), TEXT_DIM),
        (BigKind::Play, true) => (mix(ACCENT, Color32::WHITE, 0.10 + 0.08 * hot), mix(EMBER, ACCENT, 0.15 * hot), ON_ACCENT),
        (BigKind::Stop, true) => (mix(RED, Color32::WHITE, 0.12 + 0.08 * hot), mix(RED, BG, 0.45), Color32::WHITE),
    };
    let glow = if kind == BigKind::Play { ACCENT } else { RED };
    if hot > 0.0 {
        for (grow, a) in [(11.0, 0.05), (7.0, 0.09), (3.5, 0.16)] {
            notched(p, rect.expand(grow * hot), step + grow * hot * 0.5, glow.gamma_multiply(a * hot * 2.0));
        }
    }
    let body = if pressed { rect.translate(Vec2::new(0.0, 1.5)) } else { rect };
    notched_gradient_v(p, body, step, top, bottom);
    if enabled {
        // bisel: luz arriba, sombra abajo
        let hi = Rect::from_min_size(body.min + Vec2::new(step, 0.0), Vec2::new(body.width() - 2.0 * step, 2.0));
        p.rect_filled(hi, 0.0, Color32::WHITE.gamma_multiply(0.30));
        let lo = Rect::from_min_size(Pos2::new(body.min.x + step, body.max.y - 3.0), Vec2::new(body.width() - 2.0 * step, 3.0));
        p.rect_filled(lo, 0.0, Color32::BLACK.gamma_multiply(0.30));
    }

    // icono (triángulo o cuadrado) + texto
    let font = egui::FontId::new(size.y * 0.42, crate::gui::view::fonts::display_family());
    let galley = ui.painter().layout_no_wrap(label.to_string(), font, fg);
    let icon = size.y * 0.30;
    let gap = size.y * 0.22;
    let total = icon + gap + galley.size().x;
    let x0 = body.center().x - total / 2.0;
    let cy = body.center().y;
    let p = ui.painter();
    match kind {
        BigKind::Play => {
            let pts = vec![Pos2::new(x0, cy - icon * 0.6), Pos2::new(x0, cy + icon * 0.6), Pos2::new(x0 + icon, cy)];
            p.add(egui::Shape::convex_polygon(pts, fg, egui::Stroke::NONE));
        }
        BigKind::Stop => {
            p.rect_filled(Rect::from_center_size(Pos2::new(x0 + icon / 2.0, cy), Vec2::splat(icon * 0.95)), 0.0, fg);
        }
    }
    p.galley(Pos2::new(x0 + icon + gap, cy - galley.size().y / 2.0), galley, fg);

    resp.on_hover_cursor(if enabled { egui::CursorIcon::PointingHand } else { egui::CursorIcon::NotAllowed })
}

/// El botón JUGAR mientras se prepara: cuerpo oscuro con la fase en palabras y una franja de brasa
/// en la base que se llena con el progreso (o recorre la franja si aún no se sabe cuánto falta).
pub fn progress_button(ui: &mut egui::Ui, phase: &str, fraction: Option<f32>, size: Vec2) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::hover());
    let p = ui.painter();
    let step = 4.0;
    notched_gradient_v(p, rect, step, mix(CARD_HI, ACCENT, 0.06), mix(CARD, BG, 0.35));

    // franja de progreso: dentro del escalón inferior para no tocar las esquinas recortadas
    let strip = Rect::from_min_max(Pos2::new(rect.min.x + step, rect.max.y - step - 6.0), Pos2::new(rect.max.x - step, rect.max.y - step));
    p.rect_filled(strip, 0.0, mix(BG, Color32::BLACK, 0.35));
    let fill = match fraction {
        Some(f) => Rect::from_min_size(strip.min, Vec2::new(strip.width() * f.clamp(0.0, 1.0), strip.height())),
        None => {
            let band = strip.width() * 0.32;
            let t = (ui.input(|i| i.time) * 0.55).fract() as f32;
            let x = strip.min.x - band + (strip.width() + band) * t;
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
            Rect::from_min_max(Pos2::new(x.max(strip.min.x), strip.min.y), Pos2::new((x + band).min(strip.max.x), strip.max.y))
        }
    };
    if fill.width() > 0.5 {
        gradient_h(p, fill, EMBER, ACCENT);
        // brillo en la punta: el metal más caliente
        let tip = Rect::from_min_max(Pos2::new((fill.max.x - 3.0).max(fill.min.x), fill.min.y), fill.max);
        p.rect_filled(tip, 0.0, YELLOW);
    }

    let area = Rect::from_min_max(rect.min, Pos2::new(rect.max.x, strip.min.y));
    let phase_font = egui::FontId::new(15.0, crate::gui::view::fonts::semibold_family());
    let max_w = rect.width() - 24.0;
    let mut phase_g = p.layout_no_wrap(phase.to_string(), phase_font.clone(), TEXT);
    if phase_g.size().x > max_w {
        let mut s: String = phase.to_string();
        while !s.is_empty() && p.layout_no_wrap(format!("{s}…"), phase_font.clone(), TEXT).size().x > max_w {
            s.pop();
        }
        phase_g = p.layout_no_wrap(format!("{}…", s.trim_end()), phase_font, TEXT);
    }
    let sub = match fraction {
        Some(f) => format!("{} %", (f.clamp(0.0, 1.0) * 100.0).floor() as u32),
        None => t!("un momento…").to_string(),
    };
    let sub_g = p.layout_no_wrap(sub, egui::FontId::new(12.5, crate::gui::view::fonts::semibold_family()), ACCENT_HI);
    let gap = 1.0;
    let h = phase_g.size().y + gap + sub_g.size().y;
    let y0 = area.center().y - h / 2.0;
    p.galley(Pos2::new(area.center().x - phase_g.size().x / 2.0, y0), phase_g.clone(), TEXT);
    p.galley(Pos2::new(area.center().x - sub_g.size().x / 2.0, y0 + phase_g.size().y + gap), sub_g, ACCENT_HI);
    resp
}

// ----------------------------------------------------------------------------- chispas y vacíos

/// Chispas de brasa: cuadrados diminutos y semitransparentes, siempre los mismos para un mismo nombre.
/// Se esparcen por el tercio derecho de `rect` (alrededor del botón principal), más densas abajo (el calor sube).
pub fn sparks(p: &Painter, rect: Rect, seed: &str) {
    for (x, y, size, alpha, hot) in spark_layout(rect, seed) {
        let c = if hot { YELLOW } else { ACCENT };
        p.rect_filled(Rect::from_min_size(Pos2::new(x, y), Vec2::splat(size)), 0.0, c.gamma_multiply(alpha));
    }
}

/// Posición, tamaño y opacidad de cada chispa (separado de `sparks` para poder probarlo).
pub fn spark_layout(rect: Rect, seed: &str) -> Vec<(f32, f32, f32, f32, bool)> {
    let mut h = fnv1a(seed) | 1;
    let mut next = move || {
        // xorshift: suficiente para esparcir puntos
        h ^= h << 13;
        h ^= h >> 7;
        h ^= h << 17;
        (h >> 11) as f32 / (1u64 << 53) as f32
    };
    (0..14)
        .map(|_| {
            let fx = 0.60 + 0.39 * next();
            let fy = 1.0 - next().powf(1.7); // sesgado hacia abajo
            let size = if next() < 0.35 { 4.0 } else { 3.0 };
            let alpha = 0.10 + 0.32 * next();
            let hot = next() < 0.3;
            let x = (rect.min.x + rect.width() * fx - size).floor();
            let y = (rect.min.y + (rect.height() - 6.0) * fy).floor();
            (x, y, size, alpha, hot)
        })
        .collect()
}

/// Estado vacío: la llama apagada, qué pasa y qué hacer.
pub fn empty_state(ui: &mut egui::Ui, title: &str, hint: &str) {
    ui.add_space(26.0);
    ui.vertical_centered(|ui| {
        flame(ui, 4.0, 0.82);
        ui.add_space(8.0);
        ui.label(RichText::new(title).size(16.0).semibold().color(TEXT));
        ui.label(RichText::new(hint).color(TEXT_DIM));
    });
}

/// Una línea de texto de ayuda bajo el nombre (p. ej. «Última partida hace 2 h»).
pub fn caption(text: &str) -> RichText {
    RichText::new(text).size(12.5).color(TEXT_DIM)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flame_bitmap_is_rectangular_and_uses_known_colors() {
        let w = FLAME[0].len();
        for row in FLAME {
            assert_eq!(row.len(), w, "todas las filas miden lo mismo");
            assert!(row.chars().all(|c| c == '.' || flame_color(c).is_some()), "{row}");
        }
    }

    #[test]
    fn identicon_is_deterministic_and_symmetric() {
        let a = identicon_cells("Mi survival");
        assert_eq!(a, identicon_cells("Mi survival"));
        for row in a {
            for x in 0..GRID {
                assert_eq!(row[x], row[GRID - 1 - x], "simétrico respecto al eje vertical");
            }
        }
    }

    #[test]
    fn identicon_differs_between_names_and_is_not_empty() {
        let names = ["Prueba", "Mi survival", "Create 1.20", "Skyblock", "Vanilla+"];
        let patterns: Vec<_> = names.iter().map(|n| identicon_cells(n)).collect();
        for (i, p) in patterns.iter().enumerate() {
            let filled = p.iter().flatten().filter(|c| c.is_some()).count();
            assert!(filled >= 8, "{} casi vacío ({filled})", names[i]);
            for q in &patterns[i + 1..] {
                assert_ne!(p, q, "dos nombres distintos no deben dar el mismo avatar");
            }
        }
    }

    #[test]
    fn sparks_are_stable_and_stay_inside() {
        let r = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(600.0, 140.0));
        let a = spark_layout(r, "Prueba");
        assert_eq!(a, spark_layout(r, "Prueba"));
        assert_ne!(a, spark_layout(r, "Otra"));
        assert_eq!(a.len(), 14);
        for (x, y, s, alpha, _) in a {
            assert!(x >= r.min.x && x + s <= r.max.x, "x dentro: {x}");
            assert!(y >= r.min.y && y + s <= r.max.y, "y dentro: {y}");
            assert!((0.0..=1.0).contains(&alpha));
        }
    }

    #[test]
    fn mix_endpoints_and_clamp() {
        let (a, b) = (Color32::from_rgb(0, 100, 200), Color32::from_rgb(100, 200, 0));
        assert_eq!(mix(a, b, 0.0), a);
        assert_eq!(mix(a, b, 1.0), b);
        assert_eq!(mix(a, b, 9.0), b, "t fuera de rango se recorta");
        assert_eq!(mix(a, b, 0.5), Color32::from_rgb(50, 150, 100));
    }

    #[test]
    fn notch_parts_cover_a_rect_without_the_corners() {
        let r = Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 40.0));
        let area: f32 = notch_parts(r, 4.0).iter().map(|p| p.width() * p.height()).sum();
        assert_eq!(area, 100.0 * 40.0 - 4.0 * 4.0 * 4.0, "se quitan exactamente cuatro cuadrados de 4×4");
    }
}
