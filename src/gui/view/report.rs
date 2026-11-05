//! Ventana de «Informar de un problema»: el usuario cuenta qué pasó, ve exactamente qué se va a
//! enviar (ya sin datos privados) y lo sube a mclo.gs. Con el enlace puede abrir un issue en GitHub.

use crate::gui::action::Action;
use crate::gui::model::{Model, ReportState, Session};
use crate::gui::state::ViewState;
use crate::gui::view::fonts::Weight;
use crate::gui::view::theme::*;
use crate::gui::view::widgets::*;
use eframe::egui::{self, RichText};

/// El informe tal cual se subiría: resumen, instancia elegida, diagnóstico y registro, sin lo privado.
pub fn report_text(mdl: &Model, s: &Session, description: &str, with_log: bool) -> String {
    let inst = mdl.selected_instance();
    let mut c = crate::report::Context_ { description: description.to_string(), ..Default::default() };
    if let Some(i) = inst {
        c.instance = Some(i.name.clone());
        let lv = i.loader_version.clone().map(|v| format!(" {v}")).unwrap_or_default();
        c.loader = Some(format!("{}{lv} · Minecraft {}", i.loader.label(), i.mc_version));
        c.memory_mb = i.memory_mb.or(Some(mdl.settings.default_memory_mb)).filter(|_| !mdl.settings.auto_memory || i.memory_mb.is_some());
        c.java = i.java_path.clone().filter(|j| !j.is_empty()).or_else(|| Some(mdl.settings.java_path.clone()).filter(|j| !j.is_empty()));
        c.mods = mdl.mods.iter().map(|m| if m.enabled { m.file_name.clone() } else { format!("{} (desactivado)", m.file_name) }).collect();
    }
    if let Some(cr) = crate::gui::view::logline::last_crash(&s.logs) {
        c.diagnosis = cr.causes.iter().map(|(t, _)| t.clone()).collect();
    }
    let logs: &[String] = if with_log { &s.logs } else { &[] };
    let raw = crate::report::build(&c, logs);
    let (home, user) = crate::report::system_identity();
    let players: Vec<String> = mdl.accounts.accounts.iter().map(|a| a.username.clone()).collect();
    crate::report::redact(&raw, home.as_deref(), user.as_deref(), &players)
}

pub fn open(vw: &mut ViewState) {
    vw.report_open = true;
    vw.report_preview = false;
    vw.report_focus = true;
}

pub fn draw(ctx: &egui::Context, mdl: &Model, vw: &mut ViewState, s: &Session, out: &mut Vec<Action>) {
    if !vw.report_open {
        return;
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && s.report != Some(ReportState::Uploading) {
        vw.report_open = false;
        out.push(Action::DismissReport);
        return;
    }
    let screen = ctx.screen_rect();
    egui::Area::new(egui::Id::new("report_veil")).order(egui::Order::Tooltip).fixed_pos(screen.min).show(ctx, |ui| {
        let (r, _) = ui.allocate_exact_size(screen.size(), egui::Sense::click());
        ui.painter().rect_filled(r, 0.0, egui::Color32::from_black_alpha(160));
    });
    let width = (screen.width() - 48.0).min(600.0);
    egui::Area::new(egui::Id::new("report"))
        .order(egui::Order::Tooltip)
        .fixed_pos(egui::pos2(screen.center().x - width / 2.0, screen.min.y + (screen.height() * 0.1).max(20.0)))
        .show(ctx, |ui| {
            egui::Frame::none()
                .fill(CARD_HI)
                .rounding(14.0)
                .stroke(egui::Stroke::new(1.0_f32, BORDER))
                .shadow(egui::epaint::Shadow { offset: egui::vec2(0.0, 12.0), blur: 40.0, spread: 0.0, color: egui::Color32::from_black_alpha(140) })
                .inner_margin(egui::Margin::same(20.0))
                .show(ui, |ui| {
                    ui.set_width(width - 40.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(t!("Informar de un problema")).size(20.0).display().color(TEXT));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if s.report != Some(ReportState::Uploading) && close_button(ui).clicked() {
                                vw.report_open = false;
                                out.push(Action::DismissReport);
                            }
                        });
                    });
                    ui.add_space(6.0);
                    match &s.report {
                        None | Some(ReportState::Uploading) => form(ui, mdl, vw, s, out),
                        Some(ReportState::Done(url)) => done(ui, mdl, vw, url, out),
                        Some(ReportState::Failed(e)) => failed(ui, mdl, vw, s, e, out),
                    }
                });
        });
}

fn form(ui: &mut egui::Ui, mdl: &Model, vw: &mut ViewState, s: &Session, out: &mut Vec<Action>) {
    let uploading = s.report == Some(ReportState::Uploading);
    ui.label(
        RichText::new(t!("Se sube un informe a mclo.gs (el servicio de registros de la comunidad de Minecraft) y te da un enlace para pasárselo a quien te ayude. Antes se quitan tus datos privados: tokens de sesión, tu usuario y tus carpetas."))
            .size(12.5)
            .color(TEXT_DIM),
    );
    ui.add_space(10.0);
    field_label(ui, t!("¿Qué estabas haciendo y qué pasó?"));
    let desc = ui.add_enabled(
        !uploading,
        egui::TextEdit::multiline(&mut vw.report_text)
            .hint_text(t!("Por ejemplo: al darle a Jugar con Create se queda en negro y se cierra"))
            .desired_rows(3)
            .desired_width(f32::INFINITY),
    );
    if std::mem::take(&mut vw.report_focus) {
        desc.request_focus();
    }
    ui.add_space(6.0);
    ui.add_enabled(!uploading, egui::Checkbox::new(&mut vw.report_log, t!("Incluir el registro de la consola (recomendado)")));
    if let Some(i) = mdl.selected_instance() {
        ui.label(RichText::new(t!("También va la instancia elegida: {}, con su lista de mods.", i.name)).size(12.0).color(TEXT_DIM));
    }
    ui.add_space(6.0);
    let label = if vw.report_preview { t!("Ocultar lo que se envía") } else { t!("Ver exactamente lo que se envía") };
    if ui
        .add(egui::Label::new(RichText::new(label).size(12.5).color(ACCENT_HI)).sense(egui::Sense::click()))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    {
        vw.report_preview = !vw.report_preview;
    }
    if vw.report_preview {
        let text = report_text(mdl, s, &vw.report_text, vw.report_log);
        egui::Frame::none().fill(INPUT).rounding(8.0).inner_margin(egui::Margin::same(10.0)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                ui.label(RichText::new(text).monospace().size(11.0).color(TEXT_DIM));
            });
        });
    }
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if uploading {
                ui.add(egui::Spinner::new().size(18.0).color(ACCENT));
                ui.label(RichText::new(t!("Subiendo…")).color(TEXT_DIM));
            } else {
                if ui.add(primary_button(t!("Subir informe"), 150.0, 38.0)).clicked() {
                    out.push(Action::ReportProblem(vw.report_text.clone(), vw.report_log));
                }
                if ui.add(ghost_button(t!("Cancelar"))).clicked() {
                    vw.report_open = false;
                }
            }
        });
    });
}

fn done(ui: &mut egui::Ui, mdl: &Model, vw: &mut ViewState, url: &str, out: &mut Vec<Action>) {
    ui.label(RichText::new(t!("Informe subido. Pasa este enlace a quien te ayude:")).color(TEXT));
    ui.add_space(8.0);
    egui::Frame::none()
        .fill(INPUT)
        .rounding(8.0)
        .stroke(egui::Stroke::new(1.0_f32, ACCENT.gamma_multiply(0.5)))
        .inner_margin(egui::Margin::symmetric(12.0, 10.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(url).monospace().size(14.0).color(ACCENT_HI));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(secondary_button(t!("📋  Copiar enlace"))).clicked() {
                        out.push(Action::Copy(url.to_string(), t!("Enlace copiado").to_string()));
                    }
                    if ui.add(ghost_button(t!("Abrir"))).clicked() {
                        out.push(Action::OpenUrl(url.to_string()));
                    }
                });
            });
        });
    ui.add_space(12.0);
    let repo = mdl.settings.update_repo.trim();
    if crate::distro::valid_repo(repo) {
        ui.label(RichText::new(t!("¿Es un fallo de Brasa? Ábrelo en GitHub: el enlace del registro ya va dentro.")).size(12.5).color(TEXT_DIM));
        ui.add_space(6.0);
    }
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add(primary_button(t!("Hecho"), 120.0, 38.0)).clicked() {
                vw.report_open = false;
                vw.report_text.clear();
                out.push(Action::DismissReport);
            }
            if crate::distro::valid_repo(repo) && ui.add(secondary_button(t!("Abrir un issue en GitHub"))).clicked() {
                let title = vw.report_text.lines().next().unwrap_or("").chars().take(80).collect::<String>();
                let title = if title.trim().is_empty() { t!("Problema en Brasa").to_string() } else { title };
                out.push(Action::OpenUrl(crate::report::issue_url(repo, &title, &vw.report_text, Some(url))));
            }
        });
    });
}

fn failed(ui: &mut egui::Ui, mdl: &Model, vw: &mut ViewState, s: &Session, e: &str, out: &mut Vec<Action>) {
    egui::Frame::none()
        .fill(RED.gamma_multiply(0.10))
        .rounding(8.0)
        .stroke(egui::Stroke::new(1.0_f32, RED.gamma_multiply(0.45)))
        .inner_margin(egui::Margin::same(10.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(t!("No se pudo subir el informe")).semibold().color(TEXT));
            ui.label(RichText::new(e).size(12.0).color(TEXT_DIM));
        });
    ui.add_space(8.0);
    ui.label(RichText::new(t!("Puedes copiarlo y pegarlo tú donde pidas ayuda (ya va sin tus datos privados).")).size(12.5).color(TEXT_DIM));
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add(primary_button(t!("Reintentar"), 130.0, 38.0)).clicked() {
                out.push(Action::ReportProblem(vw.report_text.clone(), vw.report_log));
            }
            if ui.add(secondary_button(t!("📋  Copiar informe"))).clicked() {
                out.push(Action::Copy(report_text(mdl, s, &vw.report_text, vw.report_log), t!("Informe copiado: pégalo donde pidas ayuda").to_string()));
            }
        });
    });
}
