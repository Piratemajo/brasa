//! Consola: el registro del launcher y del juego, legible.
//!
//! - cada línea con su hora, su origen y un color por nivel; las fases de Brasa como separadores
//! - las trazas de Java plegadas bajo la línea que las provoca
//! - filtros (todo, problemas, Brasa), búsqueda con resaltado y contador
//! - si el juego se ha caído, arriba el diagnóstico con «Abrir crash report» y «Copiar informe»
//! - sigue el final mientras llegan líneas; si subes, un botón te devuelve abajo

use super::logline::{self, Level, Row, Show};
use crate::gui::action::Action;
use crate::gui::model::Session;
use crate::gui::state::ViewState;
use crate::gui::view::fonts::Weight;
use crate::gui::view::theme::*;
use crate::gui::view::widgets::*;
use eframe::egui::{self, Color32, RichText};

const ROW_H: f32 = 22.0;
/// Columnas: hora, punto de nivel y origen (este sólo si hay sitio).
const TIME_W: f32 = 74.0;
const DOT_W: f32 = 16.0;
const SRC_W: f32 = 116.0;

fn msg_x(rect: egui::Rect, wide: bool) -> f32 {
    rect.min.x + 8.0 + TIME_W + DOT_W + if wide { SRC_W } else { 0.0 }
}
const LOG_TEXT: Color32 = Color32::from_rgb(200, 192, 182);

fn level_color(l: Level) -> Color32 {
    match l {
        Level::Error => RED,
        Level::Warn => YELLOW,
        Level::Info => LOG_TEXT,
        Level::Debug => TEXT_DIM,
        Level::Brasa => TEXT,
        Level::Stage => ACCENT,
        Level::Trace => Color32::from_rgb(214, 140, 128),
    }
}

/// Pastilla de filtro con contador.
fn filter_pill(ui: &mut egui::Ui, label: &str, count: Option<usize>, color: Color32, selected: bool) -> bool {
    let text = match count {
        Some(n) if n > 0 => format!("{label}  {n}"),
        _ => label.to_string(),
    };
    let rt = RichText::new(text).size(13.0);
    let rt = if selected { rt.semibold().color(TEXT) } else { rt.color(TEXT_DIM) };
    let b = egui::Button::new(rt)
        .fill(if selected { color.gamma_multiply(0.18) } else { Color32::TRANSPARENT })
        .stroke(egui::Stroke::new(1.0_f32, if selected { color.gamma_multiply(0.6) } else { BORDER.gamma_multiply(0.7) }))
        .rounding(20.0)
        .min_size(egui::vec2(0.0, 28.0));
    ui.add(b).on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

pub fn draw(ui: &mut egui::Ui, vw: &mut ViewState, s: &Session, out: &mut Vec<Action>) {
    vw.log_cache.sync(&s.logs, s.logs_dropped);
    let parsed = &vw.log_cache.parsed;
    let errors = parsed.iter().filter(|p| p.level == Level::Error).count();
    let warns = parsed.iter().filter(|p| p.level == Level::Warn).count();

    // ---------------- Cabecera
    ui.horizontal(|ui| {
        ui.label(RichText::new(t!("Consola")).size(24.0).semibold().color(TEXT));
        ui.add_space(6.0);
        let n = s.logs.len();
        ui.label(RichText::new(if n == 1 { t!("1 línea").to_string() } else { t!("{n} líneas", n = n) }).color(TEXT_DIM));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(secondary_button(t!("Informar de un problema")))
                .on_hover_text(t!("Sube el registro sin tus datos privados y te da un enlace para pedir ayuda"))
                .clicked()
            {
                crate::gui::view::report::open(vw);
            }
            if ui
                .add_enabled(n > 0, ghost_button(t!("Limpiar")))
                .on_hover_text(t!("Vaciar la consola (el registro en disco de cada instancia se conserva)"))
                .clicked()
            {
                out.push(Action::ClearLogs);
                vw.log_open.clear();
            }
            if ui.add_enabled(n > 0, secondary_button(t!("📋  Copiar"))).on_hover_text(t!("Copiar lo que se ve con el filtro actual")).clicked() {
                let open = vw.log_open_rel(s.logs_dropped);
                let shown: Vec<&str> = logline::rows(&s.logs, &vw.log_cache.parsed, vw.log_show, vw.log_filter.trim(), &open)
                    .iter()
                    .filter_map(|r| if let Row::Line(i) = r { Some(s.logs[*i].as_str()) } else { None })
                    .collect();
                out.push(Action::Copy(shown.join("\n"), t!("{} líneas copiadas", shown.len())));
            }
        });
    });
    ui.add_space(10.0);

    // ---------------- Diagnóstico del último cierre inesperado
    if let Some(c) = logline::last_crash(&s.logs).filter(|c| vw.log_crash_dismissed != Some(s.logs_dropped + c.at)) {
        crash_card(ui, vw, s, &c, out);
        ui.add_space(10.0);
    }

    // ---------------- Filtros y búsqueda
    let open = vw.log_open_rel(s.logs_dropped);
    let rows = logline::rows(&s.logs, &vw.log_cache.parsed, vw.log_show, vw.log_filter.trim(), &open);
    let needle = vw.log_filter.trim().to_string();
    let hits =
        if needle.is_empty() { 0 } else { rows.iter().filter(|r| matches!(r, Row::Line(i) if !logline::matches(&s.logs[*i], &needle).is_empty())).count() };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        if filter_pill(ui, t!("Todo"), None, ACCENT, vw.log_show == Show::All) {
            vw.log_show = Show::All;
        }
        let problems = errors + warns;
        let pc = if errors > 0 { RED } else { YELLOW };
        if filter_pill(ui, t!("Problemas"), Some(problems), pc, vw.log_show == Show::Problems) {
            vw.log_show = Show::Problems;
        }
        if filter_pill(ui, t!("Brasa"), None, ACCENT, vw.log_show == Show::Brasa) {
            vw.log_show = Show::Brasa;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if !vw.log_filter.is_empty() && ui.add(icon_button("✖")).on_hover_text(t!("Quitar la búsqueda")).clicked() {
                vw.log_filter.clear();
            }
            let w = (ui.available_width() - 110.0).clamp(140.0, 280.0);
            // id fijo: si no, al aparecer la ✖ cambia de sitio y pierde el foco
            let search_id = egui::Id::new("log_search");
            let r = ui.add(egui::TextEdit::singleline(&mut vw.log_filter).id(search_id).hint_text(t!("🔎 Buscar en la consola…  Ctrl F")).desired_width(w));
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::F)) {
                r.request_focus();
            }
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                vw.log_filter.clear();
            }
            if !needle.is_empty() {
                let t = match hits {
                    0 => t!("sin coincidencias").to_string(),
                    1 => t!("1 línea").to_string(),
                    n => t!("{n} líneas", n = n),
                };
                ui.label(RichText::new(t).size(12.5).color(if hits == 0 { YELLOW } else { TEXT_DIM }));
            }
        });
    });
    ui.add_space(8.0);

    // (si la búsqueda ha cambiado en este fotograma, se rehace)
    let rows = if vw.log_filter.trim() == needle { rows } else { logline::rows(&s.logs, &vw.log_cache.parsed, vw.log_show, vw.log_filter.trim(), &open) };
    let needle = vw.log_filter.trim().to_string();

    // ---------------- Registro
    let frame = egui::Frame::none().fill(INPUT).rounding(12.0).stroke(egui::Stroke::new(1.0_f32, BORDER)).inner_margin(egui::Margin::symmetric(6.0, 8.0));
    let mut toggle: Option<usize> = None;
    let mut copy_line: Option<usize> = None;
    let resp = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.set_min_height((ui.available_height() - 16.0).max(120.0));
        if s.logs.is_empty() {
            crate::gui::view::art::empty_state(
                ui,
                t!("La consola está vacía"),
                t!("Aquí verás lo que hace Brasa al instalar y el registro del juego mientras juegas"),
            );
            return None;
        }
        if rows.is_empty() {
            let what = if needle.is_empty() { t!("Nada con este filtro").to_string() } else { t!("Nada coincide con «{needle}»", needle = needle) };
            crate::gui::view::art::empty_state(ui, &what, t!("Prueba con «Todo» o con otra búsqueda"));
            return None;
        }
        let wide = ui.available_width() > 640.0;
        let mut area = egui::ScrollArea::vertical().id_salt("logs").stick_to_bottom(true).auto_shrink([false, false]);
        if std::mem::take(&mut vw.log_jump) {
            area = area.vertical_scroll_offset(f32::MAX);
        }
        let o = area.show_rows(ui, ROW_H, rows.len(), |ui, range| {
            for row in &rows[range] {
                match row {
                    Row::Line(i) => {
                        if line_row(ui, &s.logs[*i], &vw.log_cache.parsed[*i], &needle, wide).clicked() {
                            copy_line = Some(*i);
                        }
                    }
                    Row::Fold { first, count, open } => {
                        if fold_row(ui, *count, *open, wide).clicked() {
                            toggle = Some(*first);
                        }
                    }
                }
            }
        });
        Some(o)
    });

    if let Some(i) = copy_line {
        out.push(Action::Copy(s.logs[i].clone(), t!("Línea copiada").into()));
    }
    if let Some(first) = toggle {
        let abs = s.logs_dropped + first;
        if !vw.log_open.remove(&abs) {
            vw.log_open.insert(abs);
        }
    }

    // Si se ha subido, un botón para volver abajo (y seguir lo que llega)
    if let Some(o) = resp.inner {
        let bottom = o.state.offset.y + o.inner_rect.height() >= o.content_size.y - ROW_H;
        if !bottom {
            let r = resp.response.rect;
            let size = egui::vec2(150.0, 32.0);
            let at = egui::Rect::from_min_size(egui::pos2(r.center().x - size.x / 2.0, r.max.y - size.y - 14.0), size);
            let b = egui::Button::new(RichText::new(t!("⬇  Ir al final")).semibold().color(ON_ACCENT)).fill(ACCENT).rounding(16.0);
            if ui.put(at, b).on_hover_text(t!("Volver a lo último y seguirlo")).clicked() {
                vw.log_jump = true;
            }
        }
    }
}

/// Una línea: hora, punto de nivel, origen y mensaje (con lo buscado resaltado).
fn line_row(ui: &mut egui::Ui, line: &str, p: &logline::Parsed, needle: &str, wide: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW_H), egui::Sense::click());
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let painter = ui.painter_at(rect);
    let mono = egui::FontId::monospace(12.5);
    let tint = match p.level {
        Level::Error | Level::Trace => Some(RED.gamma_multiply(0.07)),
        Level::Warn => Some(YELLOW.gamma_multiply(0.05)),
        _ => None,
    };
    if resp.hovered() {
        painter.rect_filled(rect, 4.0, CARD.gamma_multiply(0.9));
    } else if let Some(t) = tint {
        painter.rect_filled(rect, 4.0, t);
    }
    let cy = rect.center().y;
    let mut x = rect.min.x + 8.0;

    if p.level == Level::Stage {
        // separador de sección: la fase en ámbar y una línea hasta el final
        let g = painter.layout_no_wrap(line[p.msg..].to_string(), egui::FontId::new(13.0, crate::gui::view::fonts::semibold_family()), ACCENT);
        let w = g.size().x;
        painter.galley(egui::pos2(x, cy - g.size().y / 2.0), g, ACCENT);
        let x2 = x + w + 10.0;
        if x2 < rect.max.x - 8.0 {
            painter.hline(x2..=rect.max.x - 8.0, cy, egui::Stroke::new(1.0_f32, ACCENT.gamma_multiply(0.25)));
        }
        return resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(t!("Clic para copiar"));
    }

    // hora
    if let Some(t) = &p.time {
        painter.text(egui::pos2(x, cy), egui::Align2::LEFT_CENTER, &line[t.clone()], mono.clone(), TEXT_DIM.gamma_multiply(0.8));
    }
    x += TIME_W;
    // nivel
    let col = level_color(p.level);
    if p.level != Level::Trace {
        let dot = match p.level {
            Level::Info => TEXT_DIM.gamma_multiply(0.5),
            Level::Brasa => ACCENT,
            _ => col,
        };
        painter.circle_filled(egui::pos2(x + 3.0, cy), 3.0, dot);
    }
    x += DOT_W;
    // origen
    if wide {
        let src = match p.level {
            Level::Brasa => Some(t!("Brasa").to_string()),
            _ => p.source.clone(),
        };
        if let Some(src) = src {
            let short: String = if src.chars().count() > 14 { src.chars().take(13).collect::<String>() + "…" } else { src };
            painter.text(
                egui::pos2(x, cy),
                egui::Align2::LEFT_CENTER,
                short,
                egui::FontId::proportional(12.0),
                if p.level == Level::Brasa { ACCENT } else { TEXT_DIM },
            );
        }
        x += SRC_W;
    }
    // mensaje
    let msg = &line[p.msg..];
    let indent = if p.level == Level::Trace { 18.0 } else { 0.0 };
    let mut job = egui::text::LayoutJob::default();
    let normal = egui::TextFormat { font_id: mono.clone(), color: col, ..Default::default() };
    let hl = egui::TextFormat { font_id: mono, color: ON_ACCENT, background: ACCENT_HI, ..Default::default() };
    let mut at = 0;
    for m in logline::matches(msg, needle) {
        job.append(&msg[at..m.start], 0.0, normal.clone());
        job.append(&msg[m.clone()], 0.0, hl.clone());
        at = m.end;
    }
    job.append(&msg[at..], 0.0, normal);
    job.wrap =
        egui::text::TextWrapping { max_width: (rect.max.x - x - indent - 8.0).max(40.0), max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    let g = ui.fonts(|f| f.layout_job(job));
    let elided = g.rows.first().is_some_and(|r| r.glyphs.last().is_some_and(|gl| gl.chr == '…')) && !msg.ends_with('…');
    painter.galley(egui::pos2(x + indent, cy - g.size().y / 2.0), g, col);
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    if elided {
        resp.on_hover_ui(|ui| {
            ui.set_max_width(640.0);
            ui.label(RichText::new(line).monospace().size(12.0).color(TEXT));
            ui.label(RichText::new(t!("Clic para copiar")).size(11.5).color(TEXT_DIM));
        })
    } else {
        resp.on_hover_text(t!("Clic para copiar"))
    }
}

/// «▸ 23 líneas de traza»: clic para abrir o cerrar.
fn fold_row(ui: &mut egui::Ui, count: usize, open: bool, wide: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW_H), egui::Sense::click());
    let p = ui.painter_at(rect);
    if resp.hovered() {
        p.rect_filled(rect, 4.0, CARD.gamma_multiply(0.9));
    } else {
        p.rect_filled(rect, 4.0, RED.gamma_multiply(0.07));
    }
    let c = egui::pos2(msg_x(rect, wide) + 22.0, rect.center().y);
    let col = if resp.hovered() { TEXT } else { TEXT_DIM };
    // triángulo pintado (no depende de que la fuente tenga el glifo)
    let pts = if open {
        vec![c + egui::vec2(-4.0, -2.5), c + egui::vec2(4.0, -2.5), c + egui::vec2(0.0, 3.0)]
    } else {
        vec![c + egui::vec2(-2.5, -4.0), c + egui::vec2(3.0, 0.0), c + egui::vec2(-2.5, 4.0)]
    };
    p.add(egui::Shape::convex_polygon(pts, col, egui::Stroke::NONE));
    let txt = match (count, open) {
        (1, false) => t!("1 línea de traza").to_string(),
        (n, false) => t!("{n} líneas de traza", n = n),
        (_, true) => t!("Ocultar la traza").to_string(),
    };
    p.text(c + egui::vec2(12.0, 0.0), egui::Align2::LEFT_CENTER, txt, egui::FontId::proportional(12.5), col);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn crash_card(ui: &mut egui::Ui, vw: &mut ViewState, s: &Session, c: &logline::Crash, out: &mut Vec<Action>) {
    egui::Frame::none()
        .fill(RED.gamma_multiply(0.10))
        .rounding(12.0)
        .stroke(egui::Stroke::new(1.0_f32, RED.gamma_multiply(0.45)))
        .inner_margin(egui::Margin::symmetric(16.0, 12.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                ui.painter().rect_filled(r, 0.0, RED);
                ui.label(RichText::new(t!("El juego se cerró de forma inesperada (código {})", c.code)).size(15.0).semibold().color(TEXT));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if close_button(ui).on_hover_text(t!("Ocultar")).clicked() {
                        vw.log_crash_dismissed = Some(s.logs_dropped + c.at);
                    }
                });
            });
            ui.add_space(4.0);
            if c.causes.is_empty() {
                ui.label(
                    RichText::new(t!("No se reconoce la causa. El informe completo ayuda a quien te eche una mano: cópialo y compártelo.")).color(TEXT_DIM),
                );
            }
            for (title, fix) in &c.causes {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.label(RichText::new(format!("{title}.")).semibold().color(TEXT));
                    ui.label(RichText::new(fix).color(TEXT_DIM));
                });
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                if let Some(rep) = &c.report {
                    if ui.add(secondary_button(t!("Abrir crash report"))).on_hover_text(rep.display().to_string()).clicked() {
                        out.push(Action::OpenFile(rep.clone()));
                    }
                }
                if ui
                    .add(secondary_button(t!("Copiar informe")))
                    .on_hover_text(t!("Diagnóstico y registro de esa partida, listo para pegar en Discord o un foro"))
                    .clicked()
                {
                    out.push(Action::Copy(logline::crash_report_text(&s.logs, c, 400), t!("Informe copiado: pégalo donde pidas ayuda").into()));
                }
                if ui
                    .add(primary_button(t!("Pedir ayuda"), 120.0, 32.0))
                    .on_hover_text(t!("Sube el registro sin tus datos privados y te da un enlace para pedir ayuda"))
                    .clicked()
                {
                    crate::gui::view::report::open(vw);
                }
                if vw.log_show != Show::Problems && ui.add(ghost_button(t!("Ver sólo los problemas"))).clicked() {
                    vw.log_show = Show::Problems;
                }
            });
        });
}
