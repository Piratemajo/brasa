//! Interfaz gráfica, organizada en Modelo–Vista–Controlador.
//!
//! ```text
//!   usuario ──► View::draw(&Model, &mut ViewState) ──► Vec<Action>
//!                                                          │
//!   Model ◄── Controller::handle(&mut Model, &mut ViewState, Action)
//! ```
//!
//! - [`model`]: datos (instancias, ajustes, cuentas) y estado compartido con las tareas.
//! - [`state`]: estado puramente visual (pestañas, formularios, borradores).
//! - [`view`]: dibujo con egui. Lee el modelo, no lo modifica.
//! - [`controller`]: ejecuta las [`action::Action`]s, modifica el modelo y lanza tareas.

pub mod action;
pub mod controller;
#[cfg(debug_assertions)]
mod demo;
pub mod model;
pub mod state;
pub mod view;

use crate::paths::Paths;
use anyhow::Result;
use controller::Controller;
use eframe::egui;
use model::Model;
use state::ViewState;

pub fn run(p: Paths) -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Brasa Launcher")
            // la llama de Brasa en la barra de tareas, el dock y Alt+Tab (en vez de la «e» de eframe)
            .with_icon(std::sync::Arc::new(window_icon()))
            .with_app_id("brasa")
            .with_inner_size([1100.0, 700.0])
            .with_min_inner_size([820.0, 520.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native("Brasa Launcher", options, Box::new(move |cc| Ok(Box::new(App::new(cc, p))))).map_err(|e| anyhow::anyhow!("error de la interfaz: {e}"))
}

/// Icono de la ventana (la misma llama que el logo; `assets/icon/`).
pub(crate) fn window_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../../assets/icon/brasa-256.png")).unwrap_or_default()
}

/// Une las tres capas y las conecta con eframe.
struct App {
    model: Model,
    view: ViewState,
    ctl: Controller,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, p: Paths) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        view::fonts::install(&cc.egui_ctx);
        view::theme::apply_theme(&cc.egui_ctx);

        let mut model = Model::load(p.clone());
        let mut view = ViewState::new(&model.settings);
        let ctl = Controller::new(p, model.session.clone(), cc.egui_ctx.clone());
        if let Some(first) = model.instances.first().map(|i| i.id.clone()) {
            ctl.select(&mut model, &mut view, &first);
        } else {
            view.show_create = true;
        }
        if view::onboarding::should_show(&model) {
            view::onboarding::start(&mut view);
        }
        ctl.start(&model);
        #[cfg(debug_assertions)]
        demo::apply(&model);
        App { model, view, ctl }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if std::mem::take(&mut self.model.session.lock().unwrap().refresh) | crate::hosting::p2p::take_dirty() {
            self.ctl.reload(&mut self.model, &mut self.view);
        }
        // avisos de Brasa P2P a la consola de cada servidor
        for (sid, line) in crate::hosting::p2p::take_log() {
            crate::hosting::run::live(&sid).lock().unwrap().push(line);
        }
        // túnel listo: a jugar
        let launch = self.model.session.lock().unwrap().p2p_launch.take();
        if let Some((id, addr)) = launch {
            self.ctl.handle(action::Action::JoinServer(id, addr), &mut self.model, &mut self.view);
        }
        // Cerrar con servidores en marcha: primero se apagan bien (guardando el mundo).
        if ctx.input(|i| i.viewport().close_requested()) && !crate::hosting::run::running_ids().is_empty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.view.hosting.closing = true;
            for id in crate::hosting::run::running_ids() {
                crate::hosting::run::stop(&id);
            }
        }
        if self.view.hosting.closing {
            if crate::hosting::run::running_ids().is_empty() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                view::hosting::closing_overlay(ctx);
                ctx.request_repaint_after(std::time::Duration::from_millis(300));
            }
        }
        let actions = view::draw(ctx, &self.model, &mut self.view);
        for a in actions {
            self.ctl.handle(a, &mut self.model, &mut self.view);
        }
    }
}

#[cfg(test)]
mod icon_tests {
    #[test]
    fn window_icon_is_the_flame() {
        let i = super::window_icon();
        assert_eq!((i.width, i.height), (256, 256));
        // esquina transparente (bordes redondeados) y centro de color
        assert_eq!(i.rgba[3], 0);
        let mid = ((128 * 256 + 128) * 4) as usize;
        assert!(i.rgba[mid + 3] == 255 && i.rgba[mid] > 200, "centro de la llama");
    }
}
