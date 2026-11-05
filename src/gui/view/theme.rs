//! Paleta y estilo global de la interfaz.
use crate::config::Loader;
use eframe::egui::{self, Color32};

// «La forja de noche»: ceniza cálida y calor sólo donde hay acción.
/// Llama: la acción principal y los indicadores activos.
pub const ACCENT: Color32 = Color32::from_rgb(255, 138, 43);
/// Ámbar: el extremo claro del calor (enlaces, selección).
pub const ACCENT_HI: Color32 = Color32::from_rgb(255, 184, 92);
/// Brasa: el extremo oscuro del degradado del botón principal.
pub const EMBER: Color32 = Color32::from_rgb(214, 64, 28);
/// Texto sobre fondo de acento: marrón muy oscuro, más cálido que el negro puro.
pub const ON_ACCENT: Color32 = Color32::from_rgb(34, 16, 6);
pub const BG: Color32 = Color32::from_rgb(33, 29, 26);
pub const BG_SIDE: Color32 = Color32::from_rgb(26, 23, 21);
pub const CARD: Color32 = Color32::from_rgb(42, 37, 33);
pub const CARD_HI: Color32 = Color32::from_rgb(52, 46, 41);
pub const INPUT: Color32 = Color32::from_rgb(24, 21, 19);
pub const BORDER: Color32 = Color32::from_rgb(67, 59, 53);
/// Hueso: el texto principal, cálido en lugar de blanco azulado.
pub const TEXT: Color32 = Color32::from_rgb(239, 231, 219);
pub const TEXT_DIM: Color32 = Color32::from_rgb(168, 153, 138);
/// Verdigris: estados correctos (instalado, conectado).
pub const GREEN: Color32 = Color32::from_rgb(111, 194, 165);
pub const RED: Color32 = Color32::from_rgb(229, 96, 77);
pub const BLUE: Color32 = Color32::from_rgb(126, 166, 208);
pub const YELLOW: Color32 = Color32::from_rgb(255, 194, 75);

pub fn apply_theme(ctx: &egui::Context) {
    use egui::{FontFamily, FontId, Rounding, Stroke, TextStyle};
    let mut style = (*ctx.style()).clone();
    style.text_styles = [
        (TextStyle::Heading, FontId::new(26.0, super::fonts::display_family())),
        (TextStyle::Body, FontId::new(14.5, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Small, FontId::new(12.0, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.spacing.interact_size.y = 30.0;
    style.spacing.slider_width = 260.0;
    style.spacing.combo_height = 400.0;

    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = CARD;
    v.window_stroke = Stroke::new(1.0_f32, BORDER);
    v.window_rounding = Rounding::same(14.0);
    v.menu_rounding = Rounding::same(10.0);
    v.extreme_bg_color = INPUT;
    v.faint_bg_color = CARD_HI;
    v.code_bg_color = INPUT;
    v.hyperlink_color = ACCENT_HI;
    // también es el relleno de los deslizadores (slider_trailing_fill)
    v.selection.bg_fill = Color32::from_rgb(196, 98, 34);
    v.slider_trailing_fill = true;
    v.selection.stroke = Stroke::new(1.0_f32, TEXT);
    v.warn_fg_color = YELLOW;
    v.error_fg_color = RED;
    v.override_text_color = None;

    let r = Rounding::same(7.0);
    v.widgets.noninteractive.bg_fill = CARD;
    v.widgets.noninteractive.weak_bg_fill = CARD;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    v.widgets.noninteractive.rounding = r;

    // bg_fill es también el carril de los deslizadores: un tono visible sobre las tarjetas
    v.widgets.inactive.bg_fill = Color32::from_rgb(74, 65, 58);
    v.widgets.inactive.weak_bg_fill = CARD_HI;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    v.widgets.inactive.rounding = r;

    v.widgets.hovered.bg_fill = Color32::from_rgb(64, 56, 49);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(48, 52, 64);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT.gamma_multiply(0.7));
    v.widgets.hovered.fg_stroke = Stroke::new(1.5_f32, Color32::WHITE);
    v.widgets.hovered.rounding = r;

    v.widgets.active.bg_fill = Color32::from_rgb(78, 67, 58);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(58, 62, 76);
    v.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    v.widgets.active.fg_stroke = Stroke::new(1.5_f32, Color32::WHITE);
    v.widgets.active.rounding = r;

    v.widgets.open.bg_fill = CARD_HI;
    v.widgets.open.weak_bg_fill = CARD_HI;
    v.widgets.open.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    v.widgets.open.rounding = r;

    style.visuals = v;
    ctx.set_style(style);
}

pub fn loader_color(l: Loader) -> Color32 {
    match l {
        Loader::Vanilla => Color32::from_rgb(110, 190, 90),
        Loader::Fabric => Color32::from_rgb(222, 196, 150),
        Loader::Quilt => Color32::from_rgb(180, 120, 240),
        Loader::Forge => Color32::from_rgb(110, 150, 220),
        Loader::NeoForge => Color32::from_rgb(240, 140, 60),
    }
}
