//! Pestaña Servidores: la lista del juego con estado en vivo y botón para entrar.

use crate::gui::action::Action;
use crate::gui::model::{Model, PingState, Session};
use crate::gui::state::ViewState;
use crate::gui::view::art::{caption, empty_state, notched};
use crate::gui::view::fonts::Weight;
use crate::gui::view::theme::*;
use crate::gui::view::widgets::*;
use eframe::egui::{self, Color32, RichText};

fn latency_color(ms: u32) -> Color32 {
    match ms {
        0..=79 => GREEN,
        80..=199 => YELLOW,
        _ => RED,
    }
}

/// Icono del servidor (el que devuelve el ping o el que guardó el juego) o un hueco con su inicial.
fn server_icon(ui: &mut egui::Ui, name: &str, ip: &str, b64: Option<&str>) {
    let size = egui::vec2(48.0, 48.0);
    if let Some(bytes) = b64.and_then(crate::instance::servers::base64_decode) {
        let uri = format!("bytes://servidor/{ip}/{}.png", bytes.len());
        ui.add(egui::Image::from_bytes(uri, bytes).fit_to_exact_size(size).rounding(4.0));
        return;
    }
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    notched(ui.painter(), rect, 4.0, CARD_HI);
    let initial = name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, initial, egui::FontId::new(22.0, crate::gui::view::fonts::display_family()), TEXT_DIM);
}

pub fn tab(ui: &mut egui::Ui, mdl: &Model, vw: &mut ViewState, s: &Session, id: &str, busy: bool, out: &mut Vec<Action>) {
    // Al abrir la pestaña de esta instancia, se consulta el estado una vez.
    if vw.pinged_for.as_deref() != Some(id) {
        vw.pinged_for = Some(id.to_string());
        out.push(Action::PingServers(id.to_string()));
    }

    ui.horizontal(|ui| {
        let n = mdl.servers.len();
        ui.label(RichText::new(format!("{n} {}", if n == 1 { "servidor" } else { "servidores" })).color(TEXT_DIM));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add(secondary_button(t!("⟳  Actualizar"))).on_hover_text(t!("Volver a consultar jugadores y latencia")).clicked() {
                out.push(Action::PingServers(id.to_string()));
            }
        });
    });
    ui.add_space(6.0);

    // Añadir
    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut vw.new_server_name).hint_text(t!("Nombre (opcional)")).desired_width(150.0));
            let ip_resp = ui.add(egui::TextEdit::singleline(&mut vw.new_server_ip).hint_text(t!("dirección (p. ej. play.ejemplo.com)")).desired_width(260.0));
            let ok = !vw.new_server_ip.trim().is_empty();
            let enter = ip_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.add_enabled(ok, primary_button(t!("Añadir"), 90.0, 30.0)).clicked() || (enter && ok)) && ok {
                out.push(Action::AddServer(id.to_string(), vw.new_server_name.clone(), vw.new_server_ip.clone()));
            }
        });
        ui.label(caption(t!("Se guarda en la lista del juego: también aparece en su menú Multijugador.")));
    });
    ui.add_space(8.0);

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        if mdl.servers.is_empty() {
            empty_state(ui, t!("Sin servidores todavía"), t!("Añade uno arriba para ver si está en línea y entrar con un clic"));
            return;
        }
        for (i, sv) in mdl.servers.iter().enumerate() {
            let state = s.pings.get(sv.ip());
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    let icon = match state {
                        Some(PingState::Ok(st)) => st.favicon_b64.as_deref().or(sv.icon_b64()),
                        _ => sv.icon_b64(),
                    };
                    server_icon(ui, sv.name(), sv.ip(), icon);
                    ui.add_space(4.0);
                    // la columna de texto deja sitio a los botones
                    let text_w = (ui.available_width() - 250.0).max(120.0);
                    ui.vertical(|ui| {
                        ui.set_max_width(text_w);
                        ui.spacing_mut().item_spacing.y = 3.0;
                        ui.add(egui::Label::new(RichText::new(sv.name()).semibold().color(TEXT)).truncate());
                        match state {
                            Some(PingState::Ok(st)) => {
                                let motd = if st.motd.is_empty() { sv.ip().to_string() } else { st.motd.replace('\n', " · ") };
                                ui.add(egui::Label::new(RichText::new(motd).size(12.5).color(TEXT_DIM)).truncate());
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(t!("• {} ms", st.latency_ms)).size(12.5).color(latency_color(st.latency_ms)));
                                    let players = ui.label(RichText::new(t!("{} / {} jugadores", st.online, st.max)).size(12.5).color(TEXT));
                                    if !st.sample.is_empty() {
                                        players.on_hover_text(st.sample.join("\n"));
                                    }
                                    if !st.version.is_empty() {
                                        ui.add(egui::Label::new(RichText::new(&st.version).size(12.0).color(TEXT_DIM)).truncate());
                                    }
                                });
                            }
                            Some(PingState::Pending) => {
                                ui.label(RichText::new(t!("{}  ·  consultando…", sv.ip())).size(12.5).color(TEXT_DIM));
                            }
                            Some(PingState::Failed(e)) => {
                                ui.add(egui::Label::new(RichText::new(t!("{}  ·  sin respuesta: {e}", sv.ip(), e = e)).size(12.5).color(RED)).truncate())
                                    .on_hover_text(e);
                            }
                            None => {
                                ui.label(RichText::new(sv.ip()).size(12.5).color(TEXT_DIM));
                            }
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if vw.confirm_server_delete == Some(i) {
                            let b = egui::Button::new(RichText::new(t!("Quitar")).semibold().color(Color32::WHITE)).fill(RED).rounding(7.0);
                            if ui.add(b).clicked() {
                                out.push(Action::RemoveServer(id.to_string(), i));
                            }
                            if ui.add(ghost_button(t!("Cancelar"))).clicked() {
                                vw.confirm_server_delete = None;
                            }
                        } else {
                            if ui.add(danger_ghost_button("🗑")).on_hover_text(t!("Quitar de la lista")).clicked() {
                                vw.confirm_server_delete = Some(i);
                            }
                            let join = ui
                                .add_enabled(!busy, primary_button(t!("▶  Entrar"), 104.0, 34.0))
                                .on_hover_text(t!("Lanza el juego y entra directamente a este servidor"));
                            if join.clicked() {
                                out.push(Action::JoinServer(id.to_string(), sv.ip().to_string()));
                            }
                        }
                    });
                });
            });
            ui.add_space(6.0);
        }
    });
}
