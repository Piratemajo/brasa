//! Barra lateral: logo, navegación, cuenta activa y aviso de actualización.

use crate::account::auth::AccountKind;
use crate::gui::action::Action;
use crate::gui::model::{Model, Session};
use crate::gui::state::{Tab, ViewState};
use crate::gui::view::fmt::*;
use crate::gui::view::fonts::Weight;
use crate::gui::view::theme::*;
use crate::gui::view::widgets::*;
use eframe::egui::{self, RichText};

/// Barra lateral: logo, navegación, cuenta activa y aviso de actualización.
/// En ventanas estrechas (`compact`) se queda en un riel de iconos con nombre al pasar el ratón.
pub fn draw(ui: &mut egui::Ui, mdl: &Model, vw: &mut ViewState, s: &Session, out: &mut Vec<Action>, compact: bool) {
    // poca altura: filas algo más bajas para que la cuenta no tape la navegación
    let row_h = if ui.available_height() < 600.0 { 36.0 } else { 40.0 };
    if compact {
        return rail(ui, mdl, vw, s, out, row_h);
    }
    ui.add_space(18.0);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        super::art::flame(ui, 2.5, 0.0);
        ui.add_space(4.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = -2.0;
            ui.label(RichText::new(t!("BRASA")).size(30.0).display().color(TEXT));
            ui.label(RichText::new(t!("launcher v{}", env!("CARGO_PKG_VERSION"))).size(11.0).color(TEXT_DIM));
        });
    });
    ui.add_space(16.0);
    super::palette::sidebar_hint(ui, vw);
    ui.add_space(12.0);

    let n_playing = n_playing(s);
    for (t, icon, name) in NAV {
        let badge = if t == Tab::Logs && n_playing > 0 { Some(GREEN) } else { None };
        if nav_item(ui, icon, crate::i18n::tr(name), vw.tab == t, badge, row_h, false) {
            vw.tab = t;
        }
        ui.add_space(2.0);
    }

    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
        ui.add_space(12.0);
        // (de abajo arriba: la cuenta queda al fondo y la actividad justo encima)
        // Cuenta activa
        let (name, label, color) = account_view(mdl);
        let resp = egui::Frame::none()
            .fill(CARD)
            .rounding(10.0)
            .stroke(egui::Stroke::new(1.0_f32, BORDER))
            .inner_margin(egui::Margin::same(10.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let _ = avatar(ui, &name, 34.0, color);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        ui.label(RichText::new(elide(&name, 16)).semibold().color(TEXT));
                        ui.label(RichText::new(elide(&label, 22)).size(11.5).color(color));
                    });
                });
            })
            .response
            .interact(egui::Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if resp.clicked() {
            vw.tab = Tab::Accounts;
        }
        ui.add_space(8.0);
        super::status::activity(ui, s);
        if let Some(rel) = &s.update {
            ui.add_space(8.0);
            let b = egui::Button::new(RichText::new(t!("⬆  Actualizar a {}", rel.tag_name)).semibold().color(ON_ACCENT))
                .fill(GREEN)
                .rounding(8.0)
                .min_size(egui::vec2(ui.available_width(), 34.0));
            if ui.add(b).clicked() {
                out.push(Action::ApplyUpdate);
            }
        }
    });
}

const NAV: [(Tab, &str, &str); 9] = [
    (Tab::Home, "🏠", "Inicio"),
    (Tab::Instances, "🎮", "Instancias"),
    (Tab::Wardrobe, "👕", "Taquilla"),
    (Tab::Browse, "🔍", "Explorar"),
    (Tab::Hosting, "📡", "Servidores"),
    (Tab::Import, "📥", "Importar"),
    (Tab::Accounts, "👤", "Cuentas"),
    (Tab::Settings, "⚙", "Ajustes"),
    (Tab::Logs, "🖥", "Consola"),
];

fn n_playing(s: &Session) -> usize {
    s.launches.values().filter(|l| matches!(l, crate::gui::model::Launch::Running { .. })).count()
}

fn account_view(mdl: &Model) -> (String, String, egui::Color32) {
    match mdl.accounts.active() {
        Some(a) => {
            let c = if a.is_premium() {
                GREEN
            } else if a.kind == AccountKind::Yggdrasil {
                BLUE
            } else {
                TEXT_DIM
            };
            (a.username.clone(), a.label(), c)
        }
        None => (t!("Sin cuenta").to_string(), t!("Pulsa para añadir una").to_string(), YELLOW),
    }
}

/// Riel estrecho: llama, buscar, iconos y avatar de la cuenta.
fn rail(ui: &mut egui::Ui, mdl: &Model, vw: &mut ViewState, s: &Session, out: &mut Vec<Action>, row_h: f32) {
    ui.add_space(16.0);
    ui.vertical_centered(|ui| {
        super::art::flame(ui, 2.5, 0.0).on_hover_text(t!("Brasa v{}", env!("CARGO_PKG_VERSION")));
    });
    ui.add_space(12.0);
    if nav_item(ui, "🔍", t!("Buscar (Ctrl K)"), vw.palette_open, None, row_h, true) {
        vw.palette_open = true;
        vw.palette_query.clear();
        vw.palette_sel = 0;
    }
    ui.add_space(4.0);
    ui.painter().hline(ui.min_rect().x_range().shrink(8.0), ui.cursor().top(), egui::Stroke::new(1.0_f32, BORDER));
    ui.add_space(6.0);
    let n = n_playing(s);
    for (t, icon, name) in NAV {
        let badge = if t == Tab::Logs && n > 0 { Some(GREEN) } else { None };
        if nav_item(ui, icon, crate::i18n::tr(name), vw.tab == t, badge, row_h, true) {
            vw.tab = t;
        }
        ui.add_space(2.0);
    }
    ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
        ui.add_space(12.0);
        let (name, label, color) = account_view(mdl);
        let r = avatar(ui, &name, 36.0, color)
            .interact(egui::Sense::click())
            .on_hover_text(format!("{name} · {label}"))
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if r.clicked() {
            vw.tab = Tab::Accounts;
        }
        if s.background_tasks() > 0 {
            ui.add_space(10.0);
            ui.add(egui::Spinner::new().size(18.0).color(ACCENT)).on_hover_text(s.stage.as_str());
        }
        if let Some(rel) = &s.update {
            ui.add_space(10.0);
            let b = egui::Button::new(RichText::new("⬆").size(16.0).color(ON_ACCENT)).fill(GREEN).rounding(8.0).min_size(egui::vec2(40.0, 34.0));
            if ui.add(b).on_hover_text(t!("Actualizar a {}", rel.tag_name)).clicked() {
                out.push(Action::ApplyUpdate);
            }
        }
    });
}
