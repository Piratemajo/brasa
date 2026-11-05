//! Inicio: «Continuar jugando» con la última instancia, las recientes y accesos rápidos.

use crate::actions;
use crate::config::Instance;
use crate::gui::action::Action;
use crate::gui::model::{Model, Session};
use crate::gui::state::{Tab, ViewState};
use crate::gui::view::art::{caption, empty_state, hero_background, instance_avatar};
use crate::gui::view::fmt::*;
use crate::gui::view::fonts::Weight;
use crate::gui::view::theme::*;
use crate::gui::view::widgets::*;
use eframe::egui::{self, RichText};

/// Cuántas instancias recientes se muestran además de la principal.
const RECENT: usize = 4;

/// Orden de «continuar jugando»: la última jugada primero; las nunca jugadas, al final por nombre.
pub fn by_recency(instances: &[Instance]) -> Vec<&Instance> {
    let mut v: Vec<&Instance> = instances.iter().collect();
    v.sort_by(|a, b| b.last_played.cmp(&a.last_played).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    v
}

fn last_line(i: &Instance) -> String {
    match i.last_played {
        Some(t) => t!("{} jugadas · última {}", actions::fmt_duration(i.playtime_secs), ago(t)),
        None => t!("Aún no has jugado a esta instancia").to_string(),
    }
}

pub fn draw(ui: &mut egui::Ui, mdl: &Model, vw: &mut ViewState, s: &Session, out: &mut Vec<Action>) {
    let total: u64 = mdl.instances.iter().map(|i| i.playtime_secs).sum();
    let subtitle = match mdl.instances.len() {
        0 => t!("Crea tu primera instancia o tráete las de otro launcher.").to_string(),
        1 => t!("{} jugadas en 1 instancia.", actions::fmt_duration(total)),
        n => t!("{} jugadas en {n} instancias.", actions::fmt_duration(total), n = n),
    };
    page_header(ui, t!("Inicio"), &subtitle);

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let ordered = by_recency(&mdl.instances);
        let Some(main) = ordered.first().copied() else {
            welcome(ui, vw, out);
            return;
        };

        section_title(ui, if main.last_played.is_some() { t!("Continuar jugando") } else { t!("Empezar a jugar") });
        hero(ui, mdl, vw, s, main, out);

        let rest: Vec<&Instance> = ordered.iter().skip(1).take(RECENT).copied().collect();
        if !rest.is_empty() {
            ui.add_space(18.0);
            ui.horizontal(|ui| {
                section_title(ui, t!("Recientes"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(ghost_button(t!("Ver todas"))).clicked() {
                        vw.tab = Tab::Instances;
                    }
                });
            });
            // tantas columnas como quepan (cada ficha necesita unos 220 px)
            let n = ((ui.available_width() / 220.0) as usize).clamp(1, RECENT);
            ui.columns(n, |cols| {
                for (col, i) in cols.iter_mut().zip(rest) {
                    recent_tile(col, mdl, vw, s, i, out);
                }
            });
        }

        ui.add_space(18.0);
        section_title(ui, t!("Accesos rápidos"));
        quick_actions(ui, vw, out);
    });
}

/// La instancia principal en grande, con el mismo estilo que su ficha.
fn hero(ui: &mut egui::Ui, mdl: &Model, vw: &mut ViewState, s: &Session, i: &Instance, out: &mut Vec<Action>) {
    let loader_c = loader_color(i.loader);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 150.0), egui::Sense::hover());
    hero_background(ui.painter(), rect, loader_c, &i.name);
    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect.shrink2(egui::vec2(26.0, 20.0))).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
        instance_avatar(ui, mdl.icon_uri(i), &i.name, i.loader, 104.0);
        ui.add_space(14.0);
        let text_w = (ui.available_width() - 214.0).max(160.0);
        ui.vertical(|ui| {
            ui.set_max_width(text_w);
            ui.spacing_mut().item_spacing.y = 7.0;
            ui.add(egui::Label::new(RichText::new(&i.name).size(40.0).display().color(TEXT)).truncate());
            ui.horizontal(|ui| {
                chip(ui, i.loader.label(), loader_c);
                chip(ui, &t!("Minecraft {}", i.mc_version), TEXT_DIM);
            });
            let line = if s.is_running(&i.id) { t!("Jugando ahora").to_string() } else { last_line(i) };
            ui.add(egui::Label::new(caption(&line)).truncate());
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // columna de ancho fijo: en un diseño de derecha a izquierda queda pegada al borde derecho
            ui.allocate_ui_with_layout(egui::vec2(188.0, ui.available_height()), egui::Layout::top_down(egui::Align::Center), |ui| {
                super::play::play_control(ui, s, &i.id, egui::vec2(188.0, 60.0), out);
                ui.add_space(2.0);
                // preparando: ya lo dice el botón; en juego: el tiempo de partida
                if let Some((txt, col)) = super::play::status_text(s, &i.id).filter(|_| s.is_running(&i.id)) {
                    ui.label(RichText::new(txt).size(12.5).color(col));
                } else if ui.add(ghost_button(t!("Ver instancia"))).clicked() {
                    out.push(Action::Select(i.id.clone()));
                    vw.tab = Tab::Instances;
                }
            });
        });
    });
}

fn recent_tile(ui: &mut egui::Ui, mdl: &Model, vw: &mut ViewState, s: &Session, i: &Instance, out: &mut Vec<Action>) {
    card(ui, |ui| {
        ui.horizontal(|ui| {
            instance_avatar(ui, mdl.icon_uri(i), &i.name, i.loader, 44.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.add(egui::Label::new(RichText::new(&i.name).semibold().color(TEXT)).truncate());
                ui.add(egui::Label::new(RichText::new(format!("{} {}", i.loader.label(), i.mc_version)).size(12.0).color(loader_color(i.loader))).truncate());
            });
        });
        ui.add_space(6.0);
        match super::play::status_text(s, &i.id) {
            Some((txt, col)) => {
                ui.add(egui::Label::new(RichText::new(txt).size(12.5).color(col)).truncate());
            }
            None => {
                ui.add(
                    egui::Label::new(caption(&match i.last_played {
                        Some(t) => t!("Última {}", ago(t)),
                        None => t!("Sin jugar").to_string(),
                    }))
                    .truncate(),
                );
            }
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            super::play::small_play(ui, s, &i.id, out);
            if ui.add(ghost_button(t!("Abrir"))).clicked() {
                out.push(Action::Select(i.id.clone()));
                vw.tab = Tab::Instances;
            }
        });
    });
}

fn quick_actions(ui: &mut egui::Ui, vw: &mut ViewState, out: &mut Vec<Action>) {
    ui.columns(3, |cols| {
        if big_tile(&mut cols[0], "➕", t!("Nueva instancia"), t!("Vanilla, Fabric, Quilt, Forge o NeoForge")) {
            out.push(Action::ShowCreate);
            vw.tab = Tab::Instances;
        }
        if big_tile(&mut cols[1], "🔍", t!("Explorar mods"), t!("Modrinth y CurseForge")) {
            vw.tab = Tab::Browse;
        }
        if big_tile(&mut cols[2], "📥", t!("Importar"), t!("Prism, CurseForge, ATLauncher o modpacks")) {
            vw.tab = Tab::Import;
        }
    });
}

fn welcome(ui: &mut egui::Ui, vw: &mut ViewState, out: &mut Vec<Action>) {
    empty_state(ui, t!("Bienvenido a Brasa"), t!("Crea una instancia para jugar o importa las que ya tienes en otro launcher."));
    ui.add_space(18.0);
    quick_actions(ui, vw, out);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_played_first_then_never_played_by_name() {
        let mk = |name: &str, last: Option<u64>| Instance { id: name.into(), name: name.into(), last_played: last, ..Default::default() };
        let all = vec![mk("Zeta", None), mk("Vieja", Some(10)), mk("Alfa", None), mk("Nueva", Some(99))];
        let order: Vec<_> = by_recency(&all).iter().map(|i| i.name.as_str()).collect();
        assert_eq!(order, vec!["Nueva", "Vieja", "Alfa", "Zeta"]);
        assert!(by_recency(&[]).is_empty());
    }
}
