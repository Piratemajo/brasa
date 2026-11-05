//! Asistente de instalación y de desinstalación, con el aspecto de Brasa.

use super::{Layout, Options, Step};
use crate::gui::view::art::{self, BigKind};
use crate::gui::view::fonts::Weight;
use crate::gui::view::theme::*;
use crate::gui::view::widgets::*;
use eframe::egui::{self, RichText};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Install,
    Uninstall,
}

#[derive(Clone, PartialEq)]
enum Page {
    Start,
    Working,
    Done,
    Failed(String),
}

#[derive(Default)]
struct Progress {
    steps: Vec<Step>,
    result: Option<Result<(), String>>,
}

struct Setup {
    mode: Mode,
    layout: Layout,
    opts: Options,
    remove_data: bool,
    data_dir: Option<PathBuf>,
    page: Page,
    show_license: bool,
    progress: Arc<Mutex<Progress>>,
    /// Carpeta escrita o elegida por el usuario.
    dir_text: String,
    /// Por qué no se puede instalar ahí (si no se puede).
    dir_error: Option<String>,
}

pub fn run(mode: Mode) -> anyhow::Result<()> {
    crate::i18n::apply(&crate::config::Settings::default().language);
    let title = match mode {
        Mode::Install => t!("Instalar Brasa Launcher"),
        Mode::Uninstall => t!("Desinstalar Brasa Launcher"),
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(title)
            .with_inner_size([700.0, 490.0])
            .with_resizable(false)
            .with_maximize_button(false)
            .with_icon(Arc::new(crate::gui::window_icon()))
            .with_app_id("brasa-setup"),
        centered: true,
        ..Default::default()
    };
    eframe::run_native(
        title,
        options,
        Box::new(move |cc| {
            crate::gui::view::fonts::install(&cc.egui_ctx);
            apply_theme(&cc.egui_ctx);
            let data_dir = data_dir();
            Ok(Box::new(Setup {
                mode,
                layout: {
                    let base = Layout::default_for_this_system();
                    match mode {
                        // desinstalar: la carpeta donde está esta copia
                        Mode::Uninstall => base.installed_here(),
                        // instalar: donde ya estaba (para actualizar), o la predeterminada
                        Mode::Install => base.previous_install().map(|d| base.with_dir(d)).unwrap_or(base),
                    }
                },
                dir_text: String::new(),
                dir_error: None,
                opts: Options::default(),
                remove_data: false,
                data_dir,
                page: Page::Start,
                show_license: false,
                progress: Default::default(),
            }))
        }),
    )
    .map_err(|e| anyhow::anyhow!("error de la interfaz: {e}"))
}

impl Setup {
    fn start(&mut self, ctx: &egui::Context) {
        self.page = Page::Working;
        *self.progress.lock().unwrap() = Progress::default();
        let (pr, ctx2, layout, opts, mode) = (self.progress.clone(), ctx.clone(), self.layout.clone(), self.opts, self.mode);
        let data = if self.remove_data { self.data_dir.clone() } else { None };
        std::thread::spawn(move || {
            let res = match mode {
                Mode::Install => std::env::current_exe().map_err(anyhow::Error::from).and_then(|src| {
                    super::install(&src, &layout, &opts, |s| {
                        pr.lock().unwrap().steps.push(s);
                        ctx2.request_repaint();
                        // que se vea cada paso aunque vaya muy rápido
                        std::thread::sleep(std::time::Duration::from_millis(350));
                    })
                }),
                Mode::Uninstall => {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    super::uninstall(&layout, data.as_deref())
                }
            };
            pr.lock().unwrap().result = Some(res.map_err(|e| format!("{e:#}")));
            ctx2.request_repaint();
        });
    }
}

/// Columna izquierda: la llama sobre un degradado de brasas.
fn side(ui: &mut egui::Ui, lit: bool) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(230.0, ui.available_height()), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, BG_SIDE);
    art::gradient_v(&p, rect, EMBER.gamma_multiply(0.0), EMBER.gamma_multiply(if lit { 0.42 } else { 0.18 }));
    art::sparks(&p, rect, "setup");
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(20.0)).layout(egui::Layout::top_down(egui::Align::Center)));
    child.add_space(rect.height() * 0.2);
    art::flame(&mut child, 6.0, if lit { 0.0 } else { 0.5 });
    child.add_space(12.0);
    child.label(RichText::new("BRASA").size(36.0).display().color(TEXT));
    child.label(RichText::new(format!("launcher v{}", env!("CARGO_PKG_VERSION"))).size(12.0).color(TEXT_DIM));
}

fn step_row(ui: &mut egui::Ui, label: &str, state: u8) {
    // 0 pendiente, 1 en curso, 2 hecho
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
        let p = ui.painter();
        let c = r.center();
        match state {
            2 => {
                p.circle_filled(c, 8.0, GREEN);
                p.line_segment([c + egui::vec2(-4.0, 0.0), c + egui::vec2(-1.0, 3.0)], egui::Stroke::new(2.0_f32, BG));
                p.line_segment([c + egui::vec2(-1.0, 3.0), c + egui::vec2(4.0, -3.0)], egui::Stroke::new(2.0_f32, BG));
            }
            1 => {
                ui.ctx().request_repaint();
                let t = ui.input(|i| i.time) as f32;
                p.circle_stroke(c, 7.0, egui::Stroke::new(1.5_f32, BORDER));
                let a = t * 6.0;
                p.line_segment(
                    [c + egui::vec2(a.cos(), a.sin()) * 7.0, c + egui::vec2((a + 1.4).cos(), (a + 1.4).sin()) * 7.0],
                    egui::Stroke::new(2.5_f32, ACCENT),
                );
            }
            _ => {
                p.circle_stroke(c, 7.0, egui::Stroke::new(1.5_f32, BORDER));
            }
        }
        let col = if state == 0 { TEXT_DIM } else { TEXT };
        ui.label(RichText::new(label).size(14.0).color(col));
    });
}

impl eframe::App for Setup {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.page == Page::Working {
            if let Some(res) = self.progress.lock().unwrap().result.clone() {
                self.page = match res {
                    Ok(()) => Page::Done,
                    Err(e) => Page::Failed(e),
                };
            }
        }
        let lit = matches!(self.page, Page::Done) && self.mode == Mode::Install;
        egui::CentralPanel::default().frame(egui::Frame::none().fill(BG)).show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                side(ui, lit || (self.mode == Mode::Install && self.page != Page::Start));
                ui.add_space(30.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.set_width(ui.available_width() - 30.0);
                    ui.add_space(30.0);
                    match (self.mode, self.page.clone()) {
                        (Mode::Install, Page::Start) => self.install_start(ui, ctx),
                        (Mode::Uninstall, Page::Start) => self.uninstall_start(ui, ctx),
                        (_, Page::Working) => self.working(ui),
                        (_, Page::Done) => self.done(ui, ctx),
                        (_, Page::Failed(e)) => self.failed(ui, ctx, &e),
                    }
                });
            });
        });
    }
}

impl Setup {
    fn footer(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
        let h = ui.available_height();
        ui.add_space((h - 80.0).max(8.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add);
    }

    fn install_start(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let upgrading = self.layout.is_installed();
        ui.label(RichText::new(if upgrading { t!("Actualizar Brasa Launcher") } else { t!("Instalar Brasa Launcher") }).size(26.0).display().color(TEXT));
        ui.add_space(4.0);
        ui.label(RichText::new(t!("Un launcher de Minecraft rápido, con mods, instancias, servidores y cosméticos.")).color(TEXT_DIM));
        ui.add_space(18.0);
        if self.show_license {
            egui::Frame::none().fill(INPUT).rounding(10.0).stroke(egui::Stroke::new(1.0_f32, BORDER)).inner_margin(egui::Margin::same(12.0)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::ScrollArea::vertical().max_height(190.0).show(ui, |ui| {
                    ui.label(RichText::new(include_str!("../../LICENSE")).monospace().size(11.5).color(TEXT_DIM));
                });
            });
        } else {
            if self.dir_text.is_empty() {
                self.dir_text = self.layout.dir.display().to_string();
            }
            let default_dir = Layout::default_for_this_system().dir;
            egui::Frame::none().fill(CARD).rounding(10.0).inner_margin(egui::Margin::symmetric(14.0, 10.0)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(t!("Dónde")).size(12.0).color(TEXT_DIM));
                ui.horizontal(|ui| {
                    let mut changed = false;
                    let w = ui.available_width() - 120.0;
                    let r = ui.add_enabled(!upgrading, egui::TextEdit::singleline(&mut self.dir_text).font(egui::FontId::monospace(12.0)).desired_width(w));
                    changed |= r.changed();
                    if ui.add_enabled(!upgrading, secondary_button(t!("Cambiar…"))).on_hover_text(t!("Elegir otra carpeta")).clicked() {
                        if let Some(picked) = rfd::FileDialog::new()
                            .set_title(t!("Dónde instalar Brasa"))
                            .set_directory(self.layout.dir.parent().unwrap_or(&self.layout.dir))
                            .pick_folder()
                        {
                            self.dir_text = super::install_dir_for(&picked).display().to_string();
                            changed = true;
                        }
                    }
                    if changed {
                        let dir = PathBuf::from(self.dir_text.trim());
                        self.layout = self.layout.with_dir(dir.clone());
                        self.dir_error = if self.dir_text.trim().is_empty() {
                            Some(t!("Escribe o elige una carpeta.").to_string())
                        } else {
                            super::check_writable(&dir).err().map(|e| format!("{e:#}"))
                        };
                    }
                });
                if let Some(e) = &self.dir_error {
                    ui.label(
                        RichText::new(t!("No se puede instalar ahí: {e}. Elige una carpeta tuya, por ejemplo dentro de tu usuario.", e = e))
                            .size(12.0)
                            .color(RED),
                    );
                } else if upgrading {
                    ui.label(RichText::new(t!("Se actualiza donde ya estaba instalado.")).size(12.0).color(TEXT_DIM));
                } else {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(t!("Sólo para tu usuario: no pide permisos de administrador.")).size(12.0).color(TEXT_DIM));
                        if self.layout.dir != default_dir
                            && ui
                                .add(egui::Label::new(RichText::new(t!("Volver a la predeterminada")).size(12.0).color(ACCENT_HI)).sense(egui::Sense::click()))
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                        {
                            self.layout = self.layout.with_dir(default_dir.clone());
                            self.dir_text = default_dir.display().to_string();
                            self.dir_error = None;
                        }
                    });
                }
            });
            ui.add_space(12.0);
            ui.checkbox(&mut self.opts.desktop_shortcut, t!("Acceso directo en el escritorio"));
            ui.checkbox(&mut self.opts.launch_after, t!("Abrir Brasa al terminar"));
            if upgrading {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(t!("Ya está instalado: se sustituye por esta versión. Tus instancias y cuentas no se tocan.")).size(12.5).color(ACCENT_HI),
                );
            }
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(t!("Al instalar aceptas la licencia MIT.")).size(12.0).color(TEXT_DIM));
            let label = if self.show_license { t!("Ocultar") } else { t!("Ver licencia") };
            if ui
                .add(egui::Label::new(RichText::new(label).size(12.0).color(ACCENT_HI)).sense(egui::Sense::click()))
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                self.show_license = !self.show_license;
            }
        });
        Self::footer(ui, |ui| {
            let label = if upgrading { t!("ACTUALIZAR") } else { t!("INSTALAR") };
            if art::big_button(ui, BigKind::Play, label, egui::vec2(190.0, 52.0), self.dir_error.is_none()).clicked() {
                self.start(ctx);
            }
            ui.add_space(10.0);
            if ui.add(ghost_button(t!("Cancelar"))).clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }

    fn uninstall_start(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.label(RichText::new(t!("Desinstalar Brasa Launcher")).size(26.0).display().color(TEXT));
        ui.add_space(6.0);
        ui.label(
            RichText::new(t!("Se quitan el programa y sus accesos directos. Tus instancias, mundos y cuentas se conservan, por si vuelves.")).color(TEXT_DIM),
        );
        ui.add_space(18.0);
        let has_data = self.data_dir.as_ref().is_some_and(|d| d.exists());
        egui::Frame::none()
            .fill(if self.remove_data { RED.gamma_multiply(0.12) } else { CARD })
            .rounding(10.0)
            .stroke(egui::Stroke::new(1.0_f32, if self.remove_data { RED.gamma_multiply(0.5) } else { BORDER.gamma_multiply(0.6) }))
            .inner_margin(egui::Margin::symmetric(14.0, 10.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.add_enabled(has_data, egui::Checkbox::new(&mut self.remove_data, t!("Borrar también mis datos")));
                ui.label(RichText::new(t!("Instancias, mundos, capturas, ajustes y cuentas. No se puede deshacer.")).size(12.0).color(if self.remove_data {
                    RED
                } else {
                    TEXT_DIM
                }));
                if let Some(d) = &self.data_dir {
                    ui.add(egui::Label::new(RichText::new(d.display().to_string()).monospace().size(11.5).color(TEXT_DIM)).truncate());
                }
            });
        Self::footer(ui, |ui| {
            if art::big_button(ui, BigKind::Stop, t!("DESINSTALAR"), egui::vec2(210.0, 52.0), true).clicked() {
                self.start(ctx);
            }
            ui.add_space(10.0);
            if ui.add(ghost_button(t!("Cancelar"))).clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }

    fn working(&mut self, ui: &mut egui::Ui) {
        let (title, steps): (&str, Vec<(Step, &str)>) = match self.mode {
            Mode::Install => (
                t!("Instalando…"),
                vec![
                    (Step::Copy, t!("Copiando Brasa")),
                    (Step::Shortcuts, t!("Creando los accesos directos")),
                    (Step::Register, t!("Apuntándolo en el sistema")),
                ],
            ),
            Mode::Uninstall => (t!("Desinstalando…"), vec![]),
        };
        ui.label(RichText::new(title).size(26.0).display().color(TEXT));
        ui.add_space(20.0);
        let done = self.progress.lock().unwrap().steps.clone();
        if steps.is_empty() {
            ui.add(egui::Spinner::new().size(28.0).color(ACCENT));
        }
        for (k, (s, label)) in steps.iter().enumerate() {
            let state = match done.iter().position(|d| d == s) {
                Some(i) if i + 1 < done.len() => 2,
                Some(_) => 1,
                None if k == 0 && done.is_empty() => 1,
                None => 0,
            };
            step_row(ui, label, state);
            ui.add_space(6.0);
        }
        let frac = if steps.is_empty() { 0.5 } else { done.len() as f32 / steps.len() as f32 };
        ui.add_space(16.0);
        ui.add(egui::ProgressBar::new(frac).desired_height(6.0).fill(ACCENT));
    }

    fn done(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        match self.mode {
            Mode::Install => {
                ui.label(RichText::new(t!("Brasa está listo")).size(28.0).display().color(TEXT));
                ui.add_space(6.0);
                let where_ = if cfg!(windows) {
                    t!("Lo encontrarás en el menú Inicio como «Brasa Launcher».")
                } else {
                    t!("Lo encontrarás en el menú de aplicaciones como «Brasa Launcher».")
                };
                ui.label(RichText::new(where_).color(TEXT_DIM));
                ui.add_space(16.0);
                for label in [t!("Copiando Brasa"), t!("Creando los accesos directos"), t!("Apuntándolo en el sistema")] {
                    step_row(ui, label, 2);
                    ui.add_space(6.0);
                }
                Self::footer(ui, |ui| {
                    if art::big_button(ui, BigKind::Play, t!("ABRIR BRASA"), egui::vec2(200.0, 52.0), true).clicked() {
                        if let Err(e) = super::launch(&self.layout) {
                            self.page = Page::Failed(format!("{e:#}"));
                            return;
                        }
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    ui.add_space(10.0);
                    if ui.add(ghost_button(t!("Cerrar"))).clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                // «Abrir Brasa al terminar»: se abre solo
                if self.opts.launch_after {
                    self.opts.launch_after = false;
                    if super::launch(&self.layout).is_ok() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
            }
            Mode::Uninstall => {
                ui.label(RichText::new(t!("Brasa se ha desinstalado")).size(28.0).display().color(TEXT));
                ui.add_space(6.0);
                let msg = if self.remove_data {
                    t!("También se han borrado tus datos.")
                } else {
                    t!("Tus instancias y cuentas siguen ahí si algún día vuelves.")
                };
                ui.label(RichText::new(msg).color(TEXT_DIM));
                Self::footer(ui, |ui| {
                    if ui.add(primary_button(t!("Cerrar"), 140.0, 40.0)).clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            }
        }
    }

    fn failed(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, e: &str) {
        ui.label(RichText::new(t!("Algo ha fallado")).size(26.0).display().color(TEXT));
        ui.add_space(8.0);
        egui::Frame::none()
            .fill(RED.gamma_multiply(0.10))
            .rounding(10.0)
            .stroke(egui::Stroke::new(1.0_f32, RED.gamma_multiply(0.45)))
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(e).size(12.5).color(TEXT));
            });
        ui.add_space(8.0);
        ui.label(RichText::new(t!("Si Brasa está abierto, ciérralo y vuelve a intentarlo.")).size(12.5).color(TEXT_DIM));
        Self::footer(ui, |ui| {
            if ui.add(primary_button(t!("Reintentar"), 140.0, 40.0)).clicked() {
                self.start(ctx);
            }
            ui.add_space(10.0);
            if ui.add(ghost_button(t!("Cerrar"))).clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }
}

/// Desinstalar sin ventana (`brasa uninstall --quiet`).
pub fn quiet_uninstall(remove_data: bool) -> anyhow::Result<()> {
    let l = Layout::default_for_this_system();
    let data = if remove_data { data_dir() } else { None };
    super::uninstall(&l, data.as_deref())
}

/// La carpeta de datos de Brasa (la misma que usa el launcher: `BRASA_HOME` o la del sistema).
fn data_dir() -> Option<PathBuf> {
    std::env::var_os("BRASA_HOME").map(PathBuf::from).or_else(|| directories::ProjectDirs::from("dev", "brasa", "brasa").map(|d| d.data_dir().to_path_buf()))
}
