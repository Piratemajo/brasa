//! Explorar: búsqueda en Modrinth y CurseForge.

use crate::content::modrinth::ProjectType;
use crate::gui::action::Action;
use crate::gui::model::{cf_class_of, Model, Session};
use crate::gui::state::ViewState;
use crate::gui::view::fmt::*;
use crate::gui::view::theme::*;
use crate::gui::view::widgets::*;
use eframe::egui::{self, RichText};

pub fn draw(ui: &mut egui::Ui, mdl: &Model, vw: &mut ViewState, s: &Session, out: &mut Vec<Action>) {
    page_header(ui, t!("Explorar"), t!("Mods, resource packs, shaders y modpacks con dependencias automáticas."));

    card(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            if mdl.settings.curseforge_available() {
                segmented(ui, &mut vw.browse_cf, &[(false, t!("Modrinth")), (true, t!("CurseForge"))]);
            } else {
                vw.browse_cf = false;
            }
            ui.add_space(12.0);
            let opts: Vec<(ProjectType, &str)> = ProjectType::ALL.iter().map(|k| (*k, k.label())).collect();
            segmented(ui, &mut vw.ptype, &opts);
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let hint = if vw.browse_cf { t!("🔎  Buscar en CurseForge…") } else { t!("🔎  Buscar en Modrinth…") };
            let w = (ui.available_width() - if vw.ptype != ProjectType::Modpack { 420.0 } else { 120.0 }).max(200.0);
            let r = ui.add(egui::TextEdit::singleline(&mut vw.query).hint_text(hint).desired_width(w).margin(egui::Margin::symmetric(10.0, 8.0)));
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.add(primary_button(t!("Buscar"), 100.0, 34.0)).clicked() || enter {
                out.push(Action::Search);
            }
            if vw.ptype != ProjectType::Modpack {
                ui.label(RichText::new("en").color(TEXT_DIM));
                let cur = vw.target.clone().or(mdl.selected.clone());
                let shown = cur
                    .as_ref()
                    .and_then(|id| mdl.instances.iter().find(|i| &i.id == id))
                    .map(|i| format!("{} ({} {})", i.name, i.loader.label(), i.mc_version))
                    .unwrap_or_else(|| t!("— elige instancia —").into());
                egui::ComboBox::new("target", "").selected_text(elide(&shown, 34)).width(260.0).show_ui(ui, |ui| {
                    for i in &mdl.instances {
                        ui.selectable_value(&mut vw.target, Some(i.id.clone()), format!("{} ({} {})", i.name, i.loader.label(), i.mc_version));
                    }
                });
            }
        });
    });
    ui.add_space(10.0);

    let install_label = if vw.ptype == ProjectType::Modpack { t!("Instalar como instancia") } else { t!("Instalar") };
    ui.push_id("hits", |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let empty = if vw.browse_cf { s.cf_hits.is_empty() } else { s.hits.is_empty() };
            if empty {
                crate::gui::view::art::empty_state(
                    ui,
                    t!("Busca algo o pulsa Buscar para ver lo más popular"),
                    t!("Los resultados se filtran por la versión y el loader de la instancia elegida"),
                );
            }
            if vw.browse_cf {
                for h in &s.cf_hits {
                    let icon = h.logo.as_ref().map(|l| l.thumbnail_url.clone()).filter(|u| !u.is_empty());
                    let author = h.authors.first().map(|a| a.name.clone()).unwrap_or_default();
                    let web = h.links.as_ref().and_then(|l| l.website_url.clone());
                    if result_card(ui, icon, &h.name, &author, &h.summary, h.download_count as u64, web, install_label) {
                        out.push(Action::CfInstall(h.id, cf_class_of(vw.ptype)));
                    }
                }
            } else {
                for h in &s.hits {
                    let web = Some(format!("https://modrinth.com/{}/{}", h.project_type, h.slug));
                    if result_card(ui, h.icon_url.clone(), &h.title, &h.author, &h.description, h.downloads, web, install_label) {
                        out.push(Action::InstallProject(h.project_id.clone(), vw.ptype));
                    }
                }
            }
        });
    });
}
