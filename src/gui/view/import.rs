//! Importar instancias de otros launchers.

use crate::gui::action::Action;
use crate::gui::model::Model;
use crate::gui::view::fonts::Weight;
use crate::gui::view::theme::*;
use crate::gui::view::widgets::*;
use eframe::egui::{self, RichText};

pub fn draw(ui: &mut egui::Ui, mdl: &Model, out: &mut Vec<Action>) {
    page_header(ui, t!("Importar"), t!("Trae tus instancias de otros launchers. Se copian: el original no se toca."));
    ui.columns(3, |cols| {
        let items: [(&str, &str, &str, Action); 3] = [
            ("🔎", t!("Buscar en este PC"), t!("Prism, CurseForge, ATLauncher y el oficial"), Action::ScanImport),
            ("📁", t!("Elegir carpeta"), t!("Una instancia de cualquiera de ellos"), Action::ImportFolder),
            ("📦", t!("Modpack"), t!(".mrpack de Modrinth o .zip de CurseForge"), Action::PickPack),
        ];
        for (col, (icon, title, sub, action)) in cols.iter_mut().zip(items) {
            if big_tile(col, icon, title, sub) {
                out.push(action);
            }
        }
    });
    ui.add_space(14.0);
    if mdl.importable.is_empty() {
        ui.add_space(20.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(t!("Pulsa «Buscar en este PC» para encontrar instancias de Prism, MultiMC, CurseForge, ATLauncher o el launcher oficial"))
                    .color(TEXT_DIM),
            );
        });
        return;
    }
    section_title(ui, &t!("Encontradas ({})", mdl.importable.len()));
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for (i, f) in mdl.importable.iter().enumerate() {
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    crate::gui::view::art::identicon(ui, &f.name, f.loader, 40.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(RichText::new(&f.name).semibold().color(TEXT));
                        ui.horizontal(|ui| {
                            chip(ui, &format!("{} {}", f.loader.label(), f.mc), loader_color(f.loader));
                            chip(ui, f.source, TEXT_DIM);
                        });
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(primary_button(t!("Importar"), 110.0, 32.0)).clicked() {
                            out.push(Action::ImportFound(i));
                        }
                    });
                });
            });
            ui.add_space(6.0);
        }
    });
}
