//! Formulario de nueva instancia.

use crate::config::Loader;
use crate::content::modrinth::ProjectType;
use crate::gui::action::Action;
use crate::gui::model::{Model, Session};
use crate::gui::state::{Tab, ViewState};
use crate::gui::view::fonts::Weight;
use crate::gui::view::theme::*;
use crate::gui::view::widgets::*;
use eframe::egui::{self, RichText};

pub fn draw(ui: &mut egui::Ui, mdl: &Model, vw: &mut ViewState, s: &Session, out: &mut Vec<Action>) {
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        page_header(ui, t!("Nueva instancia"), t!("Cada instancia tiene sus propios mods, mundos y ajustes."));

        if s.manifest.is_empty() {
            card(ui, |ui| {
                if s.manifest_error {
                    ui.label(RichText::new(t!("No se pudo descargar la lista de versiones de Mojang")).semibold().color(RED));
                    ui.label(RichText::new(t!("Suele ser la red: proxy de empresa/instituto, firewall o antivirus, o IPv6 roto. Brasa ya reintenta por IPv4 y por el espejo BMCLAPI.")).color(TEXT_DIM));
                    ui.horizontal(|ui| {
                        if ui.add(secondary_button(t!("↻  Reintentar"))).clicked() {
                            out.push(Action::RetryManifest);
                        }
                        if ui.add(secondary_button(t!("🔎  Probar conexión"))).clicked() {
                            out.push(Action::NetTest);
                            vw.tab = Tab::Settings;
                        }
                    });
                } else {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new(t!("Cargando versiones…")).color(TEXT_DIM));
                    });
                }
            });
            ui.add_space(10.0);
        }
        if vw.new_version.is_empty() && !s.latest_release.is_empty() {
            vw.new_version = s.latest_release.clone();
        }

        card(ui, |ui| {
            field_label(ui, t!("Nombre"));
            ui.add(egui::TextEdit::singleline(&mut vw.new_name).hint_text(t!("Automático (p. ej. Fabric 1.21.1)")).desired_width(f32::INFINITY));
            ui.add_space(8.0);

            field_label(ui, t!("Versión de Minecraft"));
            ui.horizontal(|ui| {
                let prev = vw.new_version.clone();
                egui::ComboBox::new("mcver", "")
                    .selected_text(RichText::new(vw.new_version.clone()).semibold())
                    .width(220.0)
                    .height(400.0)
                    .show_ui(ui, |ui| {
                        for v in s.manifest.iter().filter(|v| vw.show_snapshots || v.kind == "release") {
                            ui.selectable_value(&mut vw.new_version, v.id.clone(), v.id.as_str());
                        }
                    });
                ui.checkbox(&mut vw.show_snapshots, t!("Mostrar snapshots"));
                if prev != vw.new_version {
                    vw.new_loader_version.clear();
                }
            });
            ui.add_space(8.0);

            field_label(ui, t!("Mod loader"));
            ui.horizontal_wrapped(|ui| {
                for l in Loader::ALL {
                    if loader_pill(ui, l, vw.new_loader == l) {
                        vw.new_loader = l;
                        vw.new_loader_version.clear();
                    }
                }
            });

            if vw.new_loader != Loader::Vanilla {
                ui.add_space(8.0);
                field_label(ui, t!("Versión del loader"));
                ui.horizontal(|ui| {
                    let shown = if vw.new_loader_version.is_empty() { t!("Recomendada").to_string() } else { vw.new_loader_version.clone() };
                    egui::ComboBox::new("loaderver", "").selected_text(shown).width(220.0).height(400.0).show_ui(ui, |ui| {
                        ui.selectable_value(&mut vw.new_loader_version, String::new(), t!("Recomendada"));
                        for v in &s.loader_versions {
                            ui.selectable_value(&mut vw.new_loader_version, v.clone(), v.as_str());
                        }
                    });
                    if ui.add(secondary_button(t!("↻  Cargar lista"))).clicked() {
                        out.push(Action::LoadLoaderVersions);
                    }
                });
            }
            ui.add_space(16.0);
            let ok = !vw.new_version.is_empty();
            ui.horizontal(|ui| {
                if ui.add_enabled(ok, primary_button(t!("Crear e instalar"), 200.0, 42.0)).clicked() {
                    out.push(Action::Create);
                }
                if !mdl.instances.is_empty() && ui.add(ghost_button(t!("Cancelar"))).clicked() {
                    if let Some(first) = mdl.instances.first().map(|i| i.id.clone()) {
                        out.push(Action::Select(first));
                    }
                }
            });
        });

        ui.add_space(12.0);
        card(ui, |ui| {
            ui.label(RichText::new(t!("¿Ya tienes un modpack o instancias en otro launcher?")).semibold().color(TEXT));
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                if ui.add(secondary_button(t!("📦  Importar .mrpack / .zip"))).clicked() {
                    out.push(Action::PickPack);
                }
                if ui.add(secondary_button(t!("📥  Desde Prism / launcher oficial"))).clicked() {
                    vw.tab = Tab::Import;
                    out.push(Action::ScanImport);
                }
                if ui.add(secondary_button(t!("🔍  Buscar modpacks"))).clicked() {
                    vw.ptype = ProjectType::Modpack;
                    vw.tab = Tab::Browse;
                }
            });
            ui.label(RichText::new(t!("También puedes arrastrar el fichero directamente a la ventana.")).size(12.5).color(TEXT_DIM));
        });
    });
}
