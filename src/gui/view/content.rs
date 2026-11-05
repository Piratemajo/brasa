//! Pestañas Texturas, Shaders y Capturas de una instancia.

use crate::content::modrinth::ProjectType;
use crate::content::resources::{Item, Kind};
use crate::gui::action::Action;
use crate::gui::model::Model;
use crate::gui::state::{Tab, ViewState};
use crate::gui::view::art::{caption, empty_state, notched};
use crate::gui::view::fmt::*;
use crate::gui::view::fonts::Weight;
use crate::gui::view::theme::*;
use crate::gui::view::widgets::*;
use eframe::egui::{self, Color32, RichText};
use std::path::Path;

/// Máximo de capturas que se muestran a la vez (las más recientes).
const MAX_SHOTS: usize = 120;

fn items(mdl: &Model, kind: Kind) -> &[Item] {
    match kind {
        Kind::ResourcePack => &mdl.packs,
        Kind::Shader => &mdl.shaders,
        Kind::Screenshot => &mdl.shots,
    }
}

fn delete_controls(ui: &mut egui::Ui, vw: &mut ViewState, id: &str, kind: Kind, it: &Item, out: &mut Vec<Action>) {
    if vw.confirm_content_delete.as_deref() == Some(it.path.as_path()) {
        let b = egui::Button::new(RichText::new(t!("Borrar")).semibold().color(Color32::WHITE)).fill(RED).rounding(7.0);
        if ui.add(b).clicked() {
            out.push(Action::DeleteContent(id.to_string(), kind, it.path.clone()));
        }
        if ui.add(ghost_button(t!("Cancelar"))).clicked() {
            vw.confirm_content_delete = None;
        }
    } else if ui.add(danger_ghost_button("🗑")).on_hover_text(t!("Borrar")).clicked() {
        vw.confirm_content_delete = Some(it.path.clone());
    }
}

pub fn tab(ui: &mut egui::Ui, mdl: &Model, vw: &mut ViewState, id: &str, game_dir: &Path, kind: Kind, out: &mut Vec<Action>) {
    let list = items(mdl, kind);
    let total: u64 = list.iter().map(|i| i.size).sum();
    let (one, many) = match kind {
        Kind::ResourcePack => ("textura", "texturas"),
        Kind::Shader => ("shader", "shaders"),
        Kind::Screenshot => ("captura", "capturas"),
    };

    ui.horizontal(|ui| {
        let n = list.len();
        ui.label(RichText::new(format!("{n} {} · {}", if n == 1 { one } else { many }, fmt_size(total))).color(TEXT_DIM));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add(secondary_button(t!("📁  Abrir carpeta"))).clicked() {
                out.push(Action::OpenDir(game_dir.join(kind.dir())));
            }
            if ui.add(secondary_button(t!("➕  Añadir"))).clicked() {
                out.push(Action::AddContent(id.to_string(), kind));
            }
            let ptype = match kind {
                Kind::ResourcePack => Some(ProjectType::ResourcePack),
                Kind::Shader => Some(ProjectType::ShaderPack),
                Kind::Screenshot => None,
            };
            if let Some(pt) = ptype {
                if ui.add(secondary_button(t!("🔍  Buscar más"))).on_hover_text(t!("Abrir Explorar con esta instancia como destino")).clicked() {
                    vw.ptype = pt;
                    vw.target = Some(id.to_string());
                    vw.tab = Tab::Browse;
                }
            }
        });
    });
    ui.add_space(8.0);

    match kind {
        Kind::ResourcePack => packs(ui, mdl, vw, id, list, out),
        Kind::Shader => shaders(ui, mdl, vw, id, list, out),
        Kind::Screenshot => shots(ui, mdl, vw, id, list, out),
    }
}

fn packs(ui: &mut egui::Ui, mdl: &Model, vw: &mut ViewState, id: &str, list: &[Item], out: &mut Vec<Action>) {
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        if list.is_empty() {
            empty_state(ui, t!("Sin texturas"), t!("Añade un .zip, arrástralo a la carpeta o búscalas en Explorar"));
            return;
        }
        ui.label(caption(t!("Las activas se aplican al abrir el juego; la última activada tiene prioridad.")));
        ui.add_space(4.0);
        for it in list {
            let on = mdl.enabled_packs.iter().any(|p| p == &it.name);
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    if toggle_switch(ui, on).clicked() {
                        out.push(Action::TogglePack(id.to_string(), it.name.clone(), !on));
                    }
                    ui.add_space(6.0);
                    ui.vertical(|ui| {
                        ui.set_max_width((ui.available_width() - 120.0).max(120.0));
                        ui.add(egui::Label::new(RichText::new(&it.name).semibold().color(if on { TEXT } else { TEXT_DIM })).truncate());
                        ui.label(caption(&fmt_size(it.size)));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        delete_controls(ui, vw, id, Kind::ResourcePack, it, out);
                    });
                });
            });
            ui.add_space(4.0);
        }
    });
}

fn shaders(ui: &mut egui::Ui, mdl: &Model, vw: &mut ViewState, id: &str, list: &[Item], out: &mut Vec<Action>) {
    let names: Vec<String> = mdl.mods.iter().filter(|m| m.enabled).map(|m| m.file_name.clone()).collect();
    let loader = crate::content::resources::shader_loader(&names);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        match loader {
            Some(l) => {
                ui.label(caption(&t!("Se usan con {l}. Sólo puede haber uno activo.", l = l)));
            }
            None => {
                ui.label(RichText::new(t!("Para usar shaders, la instancia necesita Iris (Fabric, Quilt o NeoForge) u OptiFine.")).size(12.5).color(YELLOW));
            }
        }
        ui.add_space(4.0);
        if list.is_empty() {
            empty_state(ui, t!("Sin shaders"), t!("Añade un .zip o búscalos en Explorar"));
            return;
        }
        for it in list {
            let active = mdl.active_shader.as_deref() == Some(it.name.as_str());
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_max_width((ui.available_width() - 210.0).max(120.0));
                        ui.add(egui::Label::new(RichText::new(&it.name).semibold().color(TEXT)).truncate());
                        ui.label(caption(&fmt_size(it.size)));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        delete_controls(ui, vw, id, Kind::Shader, it, out);
                        if active {
                            if ui.add(secondary_button(t!("Desactivar"))).clicked() {
                                out.push(Action::SetShader(id.to_string(), None));
                            }
                            chip(ui, t!("Activo"), GREEN);
                        } else if ui.add_enabled(loader.is_some(), secondary_button(t!("Activar"))).clicked() {
                            out.push(Action::SetShader(id.to_string(), Some(it.name.clone())));
                        }
                    });
                });
            });
            ui.add_space(4.0);
        }
    });
}

fn shots(ui: &mut egui::Ui, mdl: &Model, vw: &mut ViewState, id: &str, list: &[Item], out: &mut Vec<Action>) {
    if vw.thumbs_for.as_deref() != Some(id) {
        vw.thumbs_for = Some(id.to_string());
        out.push(Action::MakeThumbs(id.to_string()));
    }
    let cache = mdl.p.cache();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        if list.is_empty() {
            empty_state(ui, t!("Sin capturas"), t!("Pulsa F2 en el juego para hacer una"));
            return;
        }
        let cell = egui::vec2(176.0, 99.0);
        let gap = 10.0;
        let cols = (((ui.available_width() + gap) / (cell.x + gap)).floor() as usize).max(1);
        for row in list.iter().take(MAX_SHOTS).collect::<Vec<_>>().chunks(cols) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for it in row {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        let thumb = crate::content::resources::thumb_path(&cache, it);
                        let resp = if thumb.exists() {
                            ui.add(egui::Image::new(format!("file://{}", thumb.display())).fit_to_exact_size(cell).rounding(4.0).sense(egui::Sense::click()))
                        } else {
                            let (rect, r) = ui.allocate_exact_size(cell, egui::Sense::click());
                            notched(ui.painter(), rect, 4.0, CARD_HI);
                            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "…", egui::FontId::proportional(18.0), TEXT_DIM);
                            r
                        };
                        let resp =
                            resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(t!("{}\nClic para abrir · clic derecho para más", it.name));
                        if resp.clicked() {
                            out.push(Action::OpenFile(it.path.clone()));
                        }
                        resp.context_menu(|ui| {
                            if ui.button(t!("Abrir")).clicked() {
                                out.push(Action::OpenFile(it.path.clone()));
                                ui.close_menu();
                            }
                            if ui.button(RichText::new(t!("Borrar…")).color(RED)).clicked() {
                                vw.confirm_content_delete = Some(it.path.clone());
                                ui.close_menu();
                            }
                        });
                        ui.set_max_width(cell.x);
                        if vw.confirm_content_delete.as_deref() == Some(it.path.as_path()) {
                            ui.horizontal(|ui| delete_controls(ui, vw, id, Kind::Screenshot, it, out));
                        } else {
                            ui.label(caption(&ago(it.modified)));
                        }
                    });
                }
            });
            ui.add_space(gap);
        }
        if list.len() > MAX_SHOTS {
            ui.label(caption(&t!("Se muestran las {MAX_SHOTS} más recientes de {}. «Abrir carpeta» para ver todas.", list.len(), MAX_SHOTS = MAX_SHOTS)));
        }
    });
}
