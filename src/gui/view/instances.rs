//! Lista de instancias (panel central izquierdo): búsqueda, orden y grupos plegables.

use crate::gui::action::Action;
use crate::gui::model::{Model, Session};
use crate::gui::state::ViewState;
use crate::gui::view::fonts::Weight;
use crate::gui::view::theme::*;
use crate::gui::view::widgets::*;
use crate::instance::organize::{self, Sort};
use eframe::egui::{self, RichText};

pub fn draw(ui: &mut egui::Ui, mdl: &Model, vw: &mut ViewState, s: &Session, out: &mut Vec<Action>) {
    ui.add_space(14.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(t!("Instancias")).size(17.0).semibold().color(TEXT));
        ui.label(RichText::new(mdl.instances.len().to_string()).color(TEXT_DIM));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.menu_button(RichText::new(mdl.ui.sort.label()).size(12.5).color(TEXT_DIM), |ui| {
                ui.label(RichText::new(t!("Ordenar por")).size(12.0).color(TEXT_DIM));
                for sort in Sort::ALL {
                    if ui.selectable_label(mdl.ui.sort == sort, sort.label()).clicked() {
                        out.push(Action::SetSort(sort));
                        ui.close_menu();
                    }
                }
            })
            .response
            .on_hover_text(t!("Ordenar la lista"));
        });
    });
    ui.add_space(4.0);
    if ui.add(primary_button(t!("➕  Nueva instancia"), ui.available_width(), 36.0)).clicked() {
        out.push(Action::ShowCreate);
    }
    ui.add_space(6.0);
    if !mdl.instances.is_empty() {
        ui.add(egui::TextEdit::singleline(&mut vw.inst_filter).hint_text(t!("🔎 Buscar…")).desired_width(f32::INFINITY))
            .on_hover_text(t!("Por nombre, grupo, loader o versión"));
    }
    ui.add_space(4.0);

    let searching = !vw.inst_filter.trim().is_empty();
    let groups = organize::grouped(&mdl.instances, mdl.ui.sort, &vw.inst_filter);
    let has_named = groups.iter().any(|g| g.name.is_some());
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        if mdl.instances.is_empty() {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(t!("Aún no hay instancias")).color(TEXT_DIM));
            });
        } else if groups.is_empty() {
            ui.add_space(16.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(t!("Ninguna instancia coincide")).color(TEXT_DIM));
            });
        }
        for g in &groups {
            // las sueltas sólo llevan cabecera si también hay grupos con nombre
            let label = g.name.clone().unwrap_or_else(|| t!("Sin grupo").to_string());
            let open = searching || !mdl.ui.is_collapsed(&label);
            if has_named {
                ui.add_space(2.0);
                if group_header(ui, &label, g.items.len(), open) && !searching {
                    out.push(Action::ToggleGroup(label.clone()));
                }
            }
            if !open {
                continue;
            }
            for i in &g.items {
                let sel = mdl.selected.as_deref() == Some(i.id.as_str()) && !vw.show_create;
                let uri = mdl.icon_uri(i);
                let resp = instance_card(ui, i, sel, s.is_running(&i.id), uri.as_deref());
                if resp.clicked() {
                    out.push(Action::Select(i.id.clone()));
                }
                if resp.double_clicked() {
                    out.push(Action::Play(i.id.clone()));
                }
                resp.on_hover_text(t!("Doble clic para jugar"));
            }
        }
    });
}
