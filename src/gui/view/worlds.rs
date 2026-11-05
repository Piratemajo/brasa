//! Pestaña Mundos: mundos de la instancia y sus copias de seguridad.

use crate::gui::action::Action;
use crate::gui::model::Model;
use crate::gui::state::ViewState;
use crate::gui::view::fmt::*;
use crate::gui::view::fonts::Weight;
use crate::gui::view::theme::*;
use crate::gui::view::widgets::*;
use crate::instance::worlds;
use eframe::egui::{self, Color32, RichText};

pub fn tab(ui: &mut egui::Ui, mdl: &Model, vw: &mut ViewState, id: &str, game_dir: &std::path::Path, busy: bool, out: &mut Vec<Action>) {
    egui::Frame::none().inner_margin(egui::Margin::symmetric(10.0, 0.0)).show(ui, |ui| {
        body(ui, mdl, vw, id, game_dir, busy, out);
    });
}

fn body(ui: &mut egui::Ui, mdl: &Model, vw: &mut ViewState, id: &str, game_dir: &std::path::Path, busy: bool, out: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        let (nw, nb) = (mdl.worlds.len(), mdl.backups.len());
        let txt = format!("{nw} {} · {nb} {}", if nw == 1 { "mundo" } else { "mundos" }, if nb == 1 { "copia" } else { "copias" });
        ui.label(RichText::new(txt).color(TEXT_DIM));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add(secondary_button(t!("📁  Abrir saves"))).clicked() {
                out.push(Action::OpenDir(game_dir.join("saves")));
            }
            if ui.add(secondary_button(t!("📁  Abrir copias"))).clicked() {
                let dir = mdl.p.instances().join(id).join("backups");
                let _ = std::fs::create_dir_all(&dir);
                out.push(Action::OpenDir(dir));
            }
        });
    });
    ui.add_space(8.0);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        section_title(ui, t!("Mundos"));
        if mdl.worlds.is_empty() {
            ui.label(RichText::new(t!("Todavía no hay mundos: se crean al jugar.")).color(TEXT_DIM));
        }
        for w in mdl.worlds.clone() {
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&w.name).semibold().size(15.0).color(TEXT));
                        ui.label(RichText::new(format!("{} · {}", worlds::fmt_size(w.size), ago(w.modified))).color(TEXT_DIM));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_enabled(!busy, secondary_button(t!("💾  Copiar ahora"))).clicked() {
                            out.push(Action::BackupWorld(id.to_string(), w.name.clone()));
                        }
                    });
                });
            });
            ui.add_space(6.0);
        }
        ui.add_space(10.0);
        section_title(ui, t!("Copias de seguridad"));
        if mdl.backups.is_empty() {
            ui.label(RichText::new(t!("Aún no hay copias. Se hacen solas al jugar (Ajustes) o con «Copiar ahora».")).color(TEXT_DIM));
        }
        for b in mdl.backups.clone() {
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&b.world).semibold().color(TEXT));
                        ui.label(RichText::new(format!("{} · {}", worlds::fmt_size(b.size), ago(b.created))).color(TEXT_DIM));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if vw.confirm_backup_delete.as_ref() == Some(&b.file) {
                            if ui.add(ghost_button(t!("Cancelar"))).clicked() {
                                vw.confirm_backup_delete = None;
                            }
                            let btn = egui::Button::new(RichText::new(t!("Sí, borrar")).semibold().color(Color32::WHITE)).fill(RED).rounding(8.0);
                            if ui.add(btn).clicked() {
                                out.push(Action::DeleteBackup(id.to_string(), b.file.clone()));
                            }
                        } else {
                            if ui.add(danger_ghost_button("🗑")).on_hover_text(t!("Borrar copia")).clicked() {
                                vw.confirm_backup_delete = Some(b.file.clone());
                            }
                            if ui.add_enabled(!busy, secondary_button(t!("↩  Restaurar"))).on_hover_text(t!("El mundo actual se conserva aparte")).clicked() {
                                out.push(Action::RestoreBackup(id.to_string(), b.file.clone()));
                            }
                        }
                    });
                });
            });
            ui.add_space(6.0);
        }
    });
}
