//! Componentes visuales reutilizables (tarjetas, botones, chips...).
use crate::config::{Instance, Loader};
use crate::gui::view::fonts::Weight;
use eframe::egui::{self, Color32, RichText};

use crate::gui::view::fmt::*;
use crate::gui::view::theme::*;

pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::none()
        .fill(CARD)
        .rounding(10.0)
        .stroke(egui::Stroke::new(1.0_f32, BORDER.gamma_multiply(0.55)))
        .inner_margin(egui::Margin::same(16.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

pub fn page_header(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.label(RichText::new(title).size(32.0).display().color(TEXT));
    ui.label(RichText::new(subtitle).color(TEXT_DIM));
    ui.add_space(12.0);
}

pub fn section_title(ui: &mut egui::Ui, t: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(6.0, 15.0), egui::Sense::hover());
        let mark = egui::Rect::from_min_size(rect.left_top() + egui::vec2(0.0, 5.0), egui::vec2(6.0, 6.0));
        ui.painter().rect_filled(mark, 0.0, ACCENT);
        ui.label(RichText::new(t).size(15.5).semibold().color(TEXT));
    });
    ui.add_space(4.0);
}

pub fn field_label(ui: &mut egui::Ui, t: &str) {
    ui.label(RichText::new(t).size(12.5).semibold().color(TEXT_DIM));
}

pub fn grid_label(ui: &mut egui::Ui, t: &str) {
    ui.label(RichText::new(t).color(TEXT_DIM));
}

pub fn primary_button(text: &str, w: f32, h: f32) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_string()).semibold().color(ON_ACCENT)).fill(ACCENT).rounding(7.0).min_size(egui::vec2(w, h))
}

pub fn secondary_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_string()).color(TEXT))
        .fill(CARD_HI)
        .stroke(egui::Stroke::new(1.0_f32, BORDER))
        .rounding(7.0)
        .min_size(egui::vec2(0.0, 32.0))
}

pub fn ghost_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_string()).color(TEXT_DIM))
        .fill(Color32::TRANSPARENT)
        .stroke(egui::Stroke::NONE)
        .rounding(7.0)
        .min_size(egui::vec2(0.0, 32.0))
}

pub fn danger_ghost_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_string()).color(RED))
        .fill(Color32::TRANSPARENT)
        .stroke(egui::Stroke::new(1.0_f32, RED.gamma_multiply(0.4)))
        .rounding(7.0)
        .min_size(egui::vec2(0.0, 32.0))
}

pub fn icon_button(icon: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(icon.to_string()).color(TEXT_DIM))
        .fill(Color32::TRANSPARENT)
        .stroke(egui::Stroke::NONE)
        .rounding(6.0)
        .min_size(egui::vec2(28.0, 26.0))
}

pub fn chip(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::none().fill(color.gamma_multiply(0.16)).rounding(20.0).inner_margin(egui::Margin::symmetric(9.0, 3.0)).show(ui, |ui| {
        ui.add(egui::Label::new(RichText::new(text).size(12.0).semibold().color(color)).wrap_mode(egui::TextWrapMode::Extend));
    });
}

pub fn avatar(ui: &mut egui::Ui, name: &str, size: f32, color: Color32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
    let p = ui.painter();
    p.circle_filled(rect.center(), size / 2.0, color.gamma_multiply(0.25));
    p.circle_stroke(rect.center(), size / 2.0 - 0.5, egui::Stroke::new(1.0_f32, color.gamma_multiply(0.8)));
    p.text(rect.center(), egui::Align2::CENTER_CENTER, initial(name), egui::FontId::proportional(size * 0.45), color);
    resp
}

/// Entrada de la barra lateral. Con `icon_only` (riel estrecho) el nombre sale al pasar el ratón.
pub fn nav_item(ui: &mut egui::Ui, icon: &str, label: &str, selected: bool, badge: Option<Color32>, h: f32, icon_only: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), h), egui::Sense::click());
    let resp = if icon_only { resp.on_hover_text(label) } else { resp };
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, 7.0, CARD_HI);
        p.rect_filled(egui::Rect::from_min_size(rect.min + egui::vec2(0.0, 8.0), egui::vec2(3.0, rect.height() - 16.0)), 0.0, ACCENT);
    } else if resp.hovered() {
        p.rect_filled(rect, 7.0, CARD.gamma_multiply(0.8));
    }
    let (icon_col, text_col) = if selected {
        (ACCENT, TEXT)
    } else if resp.hovered() {
        (TEXT, TEXT)
    } else {
        (TEXT_DIM, TEXT_DIM)
    };
    let font = egui::FontId::new(14.5, if selected { crate::gui::view::fonts::semibold_family() } else { egui::FontFamily::Proportional });
    if icon_only {
        p.text(rect.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(18.0), icon_col);
        if let Some(b) = badge {
            p.circle_filled(rect.center() + egui::vec2(12.0, -10.0), 4.0, b);
        }
    } else {
        p.text(rect.left_center() + egui::vec2(18.0, 0.0), egui::Align2::LEFT_CENTER, icon, egui::FontId::proportional(16.0), icon_col);
        p.text(rect.left_center() + egui::vec2(46.0, 0.0), egui::Align2::LEFT_CENTER, label, font, text_col);
        if let Some(b) = badge {
            p.circle_filled(rect.right_center() - egui::vec2(16.0, 0.0), 4.0, b);
        }
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

pub fn instance_card(ui: &mut egui::Ui, i: &Instance, selected: bool, playing: bool, icon_uri: Option<&str>) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 60.0), egui::Sense::click());
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, 7.0, CARD_HI);
        p.rect_filled(egui::Rect::from_min_size(rect.min + egui::vec2(0.0, 8.0), egui::vec2(3.0, rect.height() - 16.0)), 0.0, ACCENT);
    } else if resp.hovered() {
        p.rect_filled(rect, 7.0, CARD.gamma_multiply(0.8));
    }
    let icon = egui::Rect::from_min_size(rect.min + egui::vec2(12.0, 10.0), egui::vec2(40.0, 40.0));
    match icon_uri {
        Some(uri) => egui::Image::new(uri.to_string()).rounding(5.0).paint_at(ui, icon),
        None => crate::gui::view::art::paint_identicon(ui.painter(), icon, &i.name, i.loader),
    }
    let p = ui.painter();
    let max_chars = (((rect.width() - 84.0) / 7.5) as usize).max(8);
    p.text(
        rect.min + egui::vec2(64.0, 21.0),
        egui::Align2::LEFT_CENTER,
        elide(&i.name, max_chars),
        egui::FontId::new(14.5, crate::gui::view::fonts::semibold_family()),
        TEXT,
    );
    let small = egui::FontId::proportional(12.0);
    let loader = p.layout_no_wrap(i.loader.label().to_string(), small.clone(), loader_color(i.loader));
    let loader_w = loader.size().x;
    let sub = rect.min + egui::vec2(64.0, 41.0);
    p.galley(egui::pos2(sub.x, sub.y - loader.size().y / 2.0), loader, TEXT_DIM);
    p.text(egui::pos2(sub.x + loader_w, sub.y), egui::Align2::LEFT_CENTER, format!("  {}", i.mc_version), small, TEXT_DIM);
    if playing {
        p.circle_filled(rect.right_center() - egui::vec2(14.0, 0.0), 5.0, GREEN);
    }
    ui.add_space(2.0);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn loader_pill(ui: &mut egui::Ui, l: Loader, selected: bool) -> bool {
    let c = loader_color(l);
    let b = egui::Button::new(RichText::new(l.label()).semibold().color(if selected { ON_ACCENT } else { c }))
        .fill(if selected { c } else { c.gamma_multiply(0.12) })
        .stroke(egui::Stroke::new(1.0_f32, c.gamma_multiply(if selected { 1.0 } else { 0.5 })))
        .rounding(20.0)
        .min_size(egui::vec2(96.0, 34.0));
    ui.add(b).clicked()
}

/// Control segmentado (botones unidos, uno seleccionado).
pub fn segmented<T: PartialEq + Clone>(ui: &mut egui::Ui, value: &mut T, opts: &[(T, &str)]) {
    egui::Frame::none().fill(INPUT).rounding(9.0).inner_margin(egui::Margin::same(3.0)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for (v, label) in opts {
                let sel = *value == *v;
                let b = egui::Button::new(RichText::new(*label).color(if sel { ACCENT_HI } else { TEXT_DIM }).semibold())
                    .fill(if sel { CARD_HI } else { Color32::TRANSPARENT })
                    .stroke(if sel { egui::Stroke::new(1.0_f32, ACCENT.gamma_multiply(0.45)) } else { egui::Stroke::NONE })
                    .rounding(7.0)
                    .min_size(egui::vec2(0.0, 28.0));
                if ui.add(b).clicked() {
                    *value = v.clone();
                }
            }
        });
    });
}

/// Pestaña con su número al lado sólo si hay algo (un «0» en cada pestaña es ruido).
pub fn sub_tab(ui: &mut egui::Ui, label: &str, count: Option<usize>, selected: bool) -> bool {
    let fam = if selected { crate::gui::view::fonts::semibold_family() } else { egui::FontFamily::Proportional };
    let font = egui::FontId::new(14.0, fam);
    let lg = ui.painter().layout_no_wrap(label.to_string(), font, TEXT);
    let cg = count.filter(|n| *n > 0).map(|n| ui.painter().layout_no_wrap(n.to_string(), egui::FontId::proportional(12.0), TEXT_DIM));
    let cw = cg.as_ref().map(|g| g.size().x + 6.0).unwrap_or(0.0);
    let size = egui::vec2(lg.size().x + cw + 24.0, 36.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let p = ui.painter();
    let col = if selected || resp.hovered() { TEXT } else { TEXT_DIM };
    let x0 = rect.min.x + 12.0;
    let cy = rect.center().y - 2.0;
    let lw = lg.size().x;
    p.galley(egui::pos2(x0, cy - lg.size().y / 2.0), lg.clone(), col);
    if let Some(g) = cg {
        let c = if selected { ACCENT_HI } else { TEXT_DIM };
        p.galley(egui::pos2(x0 + lw + 6.0, cy - g.size().y / 2.0 + 1.0), g, c);
    }
    if selected {
        p.rect_filled(egui::Rect::from_min_size(egui::pos2(rect.min.x + 8.0, rect.max.y - 3.0), egui::vec2(rect.width() - 16.0, 3.0)), 0.0, ACCENT);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// Interruptor on/off. Devuelve la respuesta (clicked = cambiar).
pub fn toggle_switch(ui: &mut egui::Ui, on: bool) -> egui::Response {
    let size = egui::vec2(36.0, 20.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let t = ui.ctx().animate_bool(resp.id, on);
    let p = ui.painter();
    let bg = if on { ACCENT } else { Color32::from_rgb(88, 77, 68) };
    p.rect_filled(rect, rect.height() / 2.0, bg);
    let x = egui::lerp((rect.left() + 10.0)..=(rect.right() - 10.0), t);
    p.circle_filled(egui::pos2(x, rect.center().y), 7.5, Color32::WHITE);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Tarjeta grande clicable (pantalla Importar).
pub fn big_tile(ui: &mut egui::Ui, icon: &str, title: &str, sub: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 110.0), egui::Sense::click());
    let p = ui.painter();
    let hovered = resp.hovered();
    p.rect_filled(rect, 12.0, if hovered { CARD_HI } else { CARD });
    p.rect_stroke(rect, 12.0, egui::Stroke::new(1.0_f32, if hovered { ACCENT.gamma_multiply(0.8) } else { BORDER }));
    p.text(rect.min + egui::vec2(18.0, 30.0), egui::Align2::LEFT_CENTER, icon, egui::FontId::proportional(24.0), ACCENT);
    p.text(rect.min + egui::vec2(18.0, 64.0), egui::Align2::LEFT_CENTER, title, egui::FontId::proportional(16.0), TEXT);
    p.text(rect.min + egui::vec2(18.0, 86.0), egui::Align2::LEFT_CENTER, sub, egui::FontId::proportional(12.0), TEXT_DIM);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// Tarjeta de resultado de búsqueda. Devuelve true si se pulsa "Instalar".
#[allow(clippy::too_many_arguments)]
pub fn result_card(
    ui: &mut egui::Ui,
    icon: Option<String>,
    title: &str,
    author: &str,
    desc: &str,
    downloads: u64,
    web: Option<String>,
    install_label: &str,
) -> bool {
    let mut clicked = false;
    egui::Frame::none().fill(CARD).rounding(12.0).stroke(egui::Stroke::new(1.0_f32, BORDER)).inner_margin(egui::Margin::same(12.0)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            match icon {
                Some(url) => {
                    ui.add(egui::Image::from_uri(url).fit_to_exact_size(egui::vec2(56.0, 56.0)).rounding(10.0));
                }
                None => {
                    // sin icono: el avatar de píxeles generado a partir del nombre
                    crate::gui::view::art::identicon(ui, title, Loader::Vanilla, 56.0);
                }
            }
            ui.add_space(6.0);
            let text_w = (ui.available_width() - 190.0).max(160.0);
            ui.vertical(|ui| {
                ui.set_max_width(text_w);
                ui.spacing_mut().item_spacing.y = 3.0;
                ui.horizontal(|ui| {
                    ui.label(RichText::new(title).semibold().size(16.0).color(TEXT));
                    if !author.is_empty() {
                        ui.label(RichText::new(t!("por {author}", author = author)).size(12.5).color(TEXT_DIM));
                    }
                });
                ui.add(egui::Label::new(RichText::new(elide(desc, 180)).color(TEXT_DIM)).wrap());
                ui.horizontal(|ui| {
                    chip(ui, &format!("⬇ {}", fmt_count(downloads)), TEXT_DIM);
                    if let Some(w) = &web {
                        ui.hyperlink_to(RichText::new(t!("Ver web ↗")).size(12.5), w);
                    }
                });
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(primary_button(install_label, 110.0, 34.0)).clicked() {
                    clicked = true;
                }
            });
        });
    });
    ui.add_space(6.0);
    clicked
}

/// Cabecera plegable de un grupo de la lista de instancias. Devuelve true al pulsarla.
pub fn group_header(ui: &mut egui::Ui, name: &str, count: usize, open: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0), egui::Sense::click());
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, 6.0, CARD.gamma_multiply(0.6));
    }
    let col = if resp.hovered() { TEXT } else { TEXT_DIM };
    // triángulo dibujado (no depende de que la fuente tenga ▸ / ▾)
    let c = rect.left_center() + egui::vec2(12.0, 0.0);
    let pts = if open {
        vec![c + egui::vec2(-4.0, -2.5), c + egui::vec2(4.0, -2.5), c + egui::vec2(0.0, 3.5)]
    } else {
        vec![c + egui::vec2(-2.5, -4.0), c + egui::vec2(3.5, 0.0), c + egui::vec2(-2.5, 4.0)]
    };
    p.add(egui::Shape::convex_polygon(pts, col, egui::Stroke::NONE));
    let font = egui::FontId::new(13.0, crate::gui::view::fonts::semibold_family());
    let g = p.layout_no_wrap(name.to_string(), font, col);
    let w = g.size().x;
    p.galley(egui::pos2(rect.min.x + 24.0, rect.center().y - g.size().y / 2.0), g, col);
    p.text(egui::pos2(rect.min.x + 30.0 + w, rect.center().y), egui::Align2::LEFT_CENTER, count.to_string(), egui::FontId::proportional(12.0), TEXT_DIM);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// Botón de cerrar dibujado (la «✕» no existe en las fuentes y saldría como un cuadrado).
pub fn close_button(ui: &mut egui::Ui) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::click());
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, 6.0, CARD.gamma_multiply(0.9));
    }
    let col = if resp.hovered() { TEXT } else { TEXT_DIM };
    let c = rect.center();
    let d = 4.5;
    let st = egui::Stroke::new(1.6_f32, col);
    p.line_segment([c + egui::vec2(-d, -d), c + egui::vec2(d, d)], st);
    p.line_segment([c + egui::vec2(-d, d), c + egui::vec2(d, -d)], st);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(t!("Cerrar"))
}
