//! Vista: dibuja el [`Model`] con egui y devuelve las [`Action`]s que pide el usuario.
//!
//! Recibe el modelo **solo en lectura**: no puede cambiar datos del dominio. Sí puede escribir en
//! [`ViewState`] (formularios, pestañas, filtros).

pub mod accounts;
pub mod art;
pub mod browse;
pub mod content;
pub mod create;
pub mod fmt;
pub mod fonts;
pub mod home;
pub mod hosting;
pub mod humanize;
pub mod import;
pub mod instance;
pub mod instances;
pub mod logline;
pub mod logs;
pub mod notice;
pub mod onboarding;
pub mod palette;
pub mod play;
pub mod render3d;
pub mod report;
pub mod servers;
pub mod settings;
pub mod sidebar;
pub mod status;
pub mod theme;
pub mod wardrobe;
pub mod widgets;
pub mod worlds;

use crate::gui::action::Action;
use crate::gui::model::Model;
use crate::gui::state::{Tab, ViewState};
use crate::gui::view::theme::*;
use eframe::egui::{self, Color32};
use std::path::PathBuf;

/// Dibuja un frame completo y devuelve las acciones pedidas.
pub fn draw(ctx: &egui::Context, mdl: &Model, vw: &mut ViewState) -> Vec<Action> {
    let mut out: Vec<Action> = vec![];
    palette::shortcut(ctx, vw);

    // Fondo: ceniza con un resplandor de brasas que sube desde el borde inferior.
    {
        let screen = ctx.screen_rect();
        let bg = ctx.layer_painter(egui::LayerId::background());
        bg.rect_filled(screen, 0.0, BG);
        let glow = egui::Rect::from_min_max(egui::pos2(screen.min.x, screen.max.y - 260.0), screen.max);
        art::gradient_v(&bg, glow, EMBER.gamma_multiply(0.0), EMBER.gamma_multiply(0.11));
    }

    // Arrastrar y soltar: .mrpack/.zip → importar, .jar → añadir mod
    let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
    for path in dropped {
        match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()) {
            Some(e) if e == "mrpack" || e == "zip" => out.push(Action::ImportPack(path)),
            Some(e) if e == "jar" => out.push(Action::AddJar(path)),
            _ => {}
        }
    }
    let hovering_files = ctx.input(|i| !i.raw.hovered_files.is_empty());

    // Primer arranque: la bienvenida ocupa toda la ventana
    if vw.onboard.is_some() {
        let s = mdl.session.lock().unwrap();
        onboarding::draw(ctx, mdl, vw, &s, &mut out);
        status::toasts(ctx, &s, vw, &mut out);
        if s.background_tasks() > 0 {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
        return out;
    }
    onboarding::pending(&mdl.session.lock().unwrap(), vw, &mut out);

    {
        let s = mdl.session.lock().unwrap();
        // ventana estrecha: la barra lateral se queda en un riel de iconos
        let compact = ctx.screen_rect().width() < 1000.0;
        egui::SidePanel::left("nav")
            .exact_width(if compact { 68.0 } else { 214.0 })
            .resizable(false)
            .frame(egui::Frame::none().fill(BG_SIDE).inner_margin(egui::Margin::symmetric(if compact { 10.0 } else { 12.0 }, 0.0)))
            .show(ctx, |ui| {
                sidebar::draw(ui, mdl, vw, &s, &mut out, compact);
            });
        if vw.tab == Tab::Instances {
            egui::SidePanel::left("instances")
                .resizable(true)
                .default_width(250.0)
                .width_range(200.0..=380.0)
                .frame(
                    egui::Frame::none()
                        .fill(art::mix(BG, BG_SIDE, 0.55))
                        .inner_margin(egui::Margin::symmetric(10.0, 0.0))
                        .stroke(egui::Stroke::new(1.0_f32, BORDER)),
                )
                .show(ctx, |ui| {
                    instances::draw(ui, mdl, vw, &s, &mut out);
                });
        }
        if vw.tab == Tab::Hosting {
            egui::SidePanel::left("hosting_list")
                .resizable(true)
                .default_width(250.0)
                .width_range(200.0..=360.0)
                .frame(
                    egui::Frame::none()
                        .fill(art::mix(BG, BG_SIDE, 0.55))
                        .inner_margin(egui::Margin::symmetric(10.0, 0.0))
                        .stroke(egui::Stroke::new(1.0_f32, BORDER)),
                )
                .show(ctx, |ui| {
                    hosting::list(ui, mdl, vw, &mut out);
                });
        }
        egui::CentralPanel::default().frame(egui::Frame::none().fill(Color32::TRANSPARENT).inner_margin(egui::Margin::same(22.0))).show(ctx, |ui| {
            match vw.tab {
                Tab::Instances => {
                    if vw.show_create {
                        create::draw(ui, mdl, vw, &s, &mut out);
                    } else {
                        instance::draw(ui, mdl, vw, &s, &mut out);
                    }
                }
                Tab::Home => home::draw(ui, mdl, vw, &s, &mut out),
                Tab::Wardrobe => wardrobe::draw(ui, mdl, vw, &s, &mut out),
                Tab::Browse => browse::draw(ui, mdl, vw, &s, &mut out),
                Tab::Accounts => accounts::draw(ui, mdl, vw, &s, &mut out),
                Tab::Settings => settings::draw(ui, mdl, vw, &s, &mut out),
                Tab::Logs => logs::draw(ui, vw, &s, &mut out),
                Tab::Import => import::draw(ui, mdl, &mut out),
                Tab::Hosting => hosting::draw(ui, mdl, vw, &s, &mut out),
            }
        });
        // Mientras se juega sólo hace falta refrescar el contador de minutos: casi cero CPU.
        if s.background_tasks() > 0 || s.any_preparing() {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        } else if s.any_running() {
            ctx.request_repaint_after(std::time::Duration::from_secs(15));
        }
    }

    {
        let s = mdl.session.lock().unwrap();
        status::toasts(ctx, &s, vw, &mut out);
        report::draw(ctx, mdl, vw, &s, &mut out);
    }
    hosting::approvals(ctx, &mut out);
    palette::draw(ctx, mdl, vw, &mut out);
    notice::window(ctx, vw, &mut out);

    // Capa al arrastrar ficheros sobre la ventana
    if hovering_files {
        let screen = ctx.screen_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("drop")));
        painter.rect_filled(screen, 0.0, Color32::from_black_alpha(190));
        let inner = screen.shrink(40.0);
        painter.rect_stroke(inner, 18.0, egui::Stroke::new(2.0_f32, ACCENT));
        painter.text(
            screen.center(),
            egui::Align2::CENTER_CENTER,
            t!("Suelta aquí: .mrpack / .zip → nueva instancia · .jar → mod"),
            egui::FontId::proportional(22.0),
            TEXT,
        );
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Instance, Loader, Settings};
    use crate::gui::state::InstTab;
    use crate::paths::Paths;

    fn model() -> (Model, PathBuf) {
        let root = std::env::temp_dir().join(format!("brasa-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let mut m = Model::load(Paths { root: root.clone() });
        Instance::create(&m.p, "Vista", "1.21.1", Loader::Vanilla, None).unwrap();
        m.instances = Instance::list(&m.p);
        m.selected = m.instances.first().map(|i| i.id.clone());
        (m, root)
    }

    /// Dibuja un frame completo sin ventana y devuelve las acciones.
    fn frame(m: &Model, v: &mut ViewState) -> Vec<Action> {
        // Igual que al arrancar la app: sin las tipografías propias, egui no conoce sus familias.
        let ctx = egui::Context::default();
        fonts::install(&ctx);
        apply_theme(&ctx);
        let mut out = vec![];
        let _ = ctx.run(Default::default(), |ctx| out = draw(ctx, m, v));
        out
    }

    #[test]
    fn every_tab_draws_without_panicking_or_acting() {
        let (m, root) = model();
        let mut v = ViewState::new(&m.settings);
        for tab in [Tab::Home, Tab::Instances, Tab::Browse, Tab::Accounts, Tab::Settings, Tab::Logs, Tab::Import, Tab::Hosting] {
            v.tab = tab;
            let acts = frame(&m, &mut v);
            assert!(acts.is_empty(), "sin interacción, la vista no pide nada");
        }
        for t in [InstTab::Mods, InstTab::Worlds, InstTab::Settings] {
            v.tab = Tab::Instances;
            v.inst_tab = t;
            let _ = frame(&m, &mut v);
        }
        v.show_create = true;
        let _ = frame(&m, &mut v);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn edited_settings_draft_is_sent_as_an_action() {
        let (m, root) = model();
        let mut v = ViewState::new(&m.settings);
        v.tab = Tab::Settings;
        v.settings.backups_to_keep += 3;
        let acts = frame(&m, &mut v);
        let applied: Vec<&Settings> = acts.iter().filter_map(|a| if let Action::ApplySettings(s) = a { Some(s) } else { None }).collect();
        assert_eq!(applied.len(), 1);
        assert_eq!(applied[0].backups_to_keep, m.settings.backups_to_keep + 3);
        assert_ne!(m.settings, v.settings, "la vista no ha tocado el modelo: eso lo hace el controlador");
        let _ = std::fs::remove_dir_all(root);
    }
}
