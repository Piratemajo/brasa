//! Avisos y actividad de fondo.
//!
//! - Avisos: tarjetas abajo a la derecha. Los errores dicen qué ha pasado y qué hacer, con un botón
//!   que lleva a la solución (ver [`super::humanize`]); se quedan hasta cerrarlos. Los avisos
//!   correctos se van solos a los pocos segundos.
//! - Actividad: lo que Brasa hace en segundo plano (instalar, importar…) aparece en la barra
//!   lateral. Lo que pasa al pulsar JUGAR lo enseña el propio botón.

use crate::gui::action::Action;
use crate::gui::model::Session;
use crate::gui::state::{Tab, ViewState};
use crate::gui::view::fmt::*;
use crate::gui::view::fonts::Weight;
use crate::gui::view::humanize::{humanize, Fix};
use crate::gui::view::theme::*;
use crate::gui::view::widgets::*;
use eframe::egui::{self, RichText};

/// Segundos que se ve un aviso de que algo salió bien.
const INFO_SECS: f64 = 4.0;
/// Ancho de los avisos (menos en ventanas estrechas, para no tapar media pantalla).
fn toast_width(ctx: &egui::Context) -> f32 {
    (ctx.screen_rect().width() * 0.42).clamp(300.0, 380.0)
}

fn toast_frame(accent: egui::Color32) -> egui::Frame {
    egui::Frame::none()
        .fill(CARD_HI)
        .rounding(10.0)
        .stroke(egui::Stroke::new(1.0_f32, accent.gamma_multiply(0.55)))
        .inner_margin(egui::Margin::symmetric(16.0, 14.0))
        .shadow(egui::epaint::Shadow { offset: egui::vec2(0.0, 6.0), blur: 24.0, spread: 0.0, color: egui::Color32::from_black_alpha(110) })
}

/// Marca cuadrada (el «píxel» de Brasa) del color del aviso.
fn mark(ui: &mut egui::Ui, color: egui::Color32) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, 18.0), egui::Sense::hover());
    ui.painter().rect_filled(egui::Rect::from_center_size(r.center(), egui::vec2(8.0, 8.0)), 0.0, color);
}

pub fn toasts(ctx: &egui::Context, s: &Session, vw: &mut ViewState, out: &mut Vec<Action>) {
    let now = ctx.input(|i| i.time);
    // el aviso correcto se cierra solo: se recuerda cuándo apareció cada texto
    match &s.info {
        Some(info) if vw.info_seen.as_ref().map(|(t, _)| t != info).unwrap_or(true) => vw.info_seen = Some((info.clone(), now)),
        None => vw.info_seen = None,
        _ => {}
    }
    if let Some((_, since)) = vw.info_seen {
        let left = INFO_SECS - (now - since);
        if left <= 0.0 {
            out.push(Action::DismissInfo);
        } else {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(left));
        }
    }
    if s.error.is_none() && s.info.is_none() {
        return;
    }

    let width = toast_width(ctx);
    // en la bienvenida, arriba: abajo a la derecha está su botón principal
    let (corner, offset) =
        if vw.onboard.is_some() { (egui::Align2::RIGHT_TOP, egui::vec2(-20.0, 20.0)) } else { (egui::Align2::RIGHT_BOTTOM, egui::vec2(-20.0, -20.0)) };
    egui::Area::new(egui::Id::new("toasts")).order(egui::Order::Foreground).anchor(corner, offset).show(ctx, |ui| {
        ui.set_max_width(width);
        ui.spacing_mut().item_spacing.y = 10.0;
        if let Some(raw) = &s.error {
            let x = humanize(raw);
            toast_frame(RED).show(ui, |ui| {
                ui.set_width(width - 32.0);
                ui.horizontal(|ui| {
                    mark(ui, RED);
                    ui.add(egui::Label::new(RichText::new(&x.title).semibold().size(15.0).color(TEXT)).wrap());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        if close_button(ui).clicked() {
                            out.push(Action::DismissError);
                        }
                    });
                });
                if let Some(hint) = &x.hint {
                    ui.add(egui::Label::new(RichText::new(hint).color(TEXT_DIM)).wrap());
                }
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    if let Some(fix) = x.fix {
                        if ui.add(secondary_button(fix.label())).clicked() {
                            match fix {
                                Fix::Accounts => vw.tab = Tab::Accounts,
                                Fix::JavaSettings => vw.tab = Tab::Settings,
                                Fix::Console => vw.tab = Tab::Logs,
                                Fix::TestNetwork => {
                                    vw.tab = Tab::Settings;
                                    out.push(Action::NetTest);
                                }
                            }
                            out.push(Action::DismissError);
                        }
                    }
                    if ui.add(ghost_button(t!("Copiar detalles"))).on_hover_text(elide(raw, 300)).clicked() {
                        out.push(Action::CopyError(raw.clone()));
                    }
                });
            });
        }
        if let Some(info) = &s.info {
            toast_frame(GREEN).show(ui, |ui| {
                ui.set_width(width - 32.0);
                ui.horizontal(|ui| {
                    mark(ui, GREEN);
                    ui.add(egui::Label::new(RichText::new(info).color(TEXT)).wrap());
                });
            });
        }
    });
}

/// Lo que Brasa hace en segundo plano, encima de la cuenta en la barra lateral.
pub fn activity(ui: &mut egui::Ui, s: &Session) {
    if s.background_tasks() == 0 {
        return;
    }
    egui::Frame::none().fill(CARD).rounding(10.0).inner_margin(egui::Margin::same(10.0)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new().size(14.0).color(ACCENT));
            ui.add(egui::Label::new(RichText::new(&s.stage).size(12.5).color(TEXT)).truncate()).on_hover_text(s.stage.as_str());
        });
        if let Some((d, t)) = s.progress.filter(|(_, t)| *t > 0) {
            ui.add_space(4.0);
            ui.add(egui::ProgressBar::new(d as f32 / t as f32).desired_height(5.0).fill(ACCENT));
            ui.label(RichText::new(t!("{d} de {t}", d = d, t = t)).size(11.0).color(TEXT_DIM));
        }
    });
    ui.add_space(8.0);
}
