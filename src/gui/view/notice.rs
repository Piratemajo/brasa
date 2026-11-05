//! Visor de textos legales (licencia, avisos de terceros, privacidad) en una ventana propia del
//! launcher, con el mismo tema. Interpreta el Markdown sencillo que usan esos ficheros: títulos,
//! párrafos, listas, tablas, citas, separadores y **negrita**, `código`, *cursiva* y `[enlaces](url)`.

use crate::gui::action::{Action, Notice};
use crate::gui::state::ViewState;
use crate::gui::view::fonts::{semibold_family, Weight};
use crate::gui::view::theme::*;
use crate::gui::view::widgets::*;
use eframe::egui::{self, text::LayoutJob, Color32, FontFamily, FontId, RichText, TextFormat};

impl Notice {
    pub fn title(self) -> &'static str {
        match self {
            Notice::License => t!("Licencia"),
            Notice::ThirdParty => t!("Avisos de terceros"),
            Notice::Privacy => t!("Privacidad"),
        }
    }
}

/// Un bloque del documento.
#[derive(Debug, PartialEq)]
pub enum Block {
    Heading(u8, String),
    Para(String),
    Item(String),
    Quote(String),
    Rule,
    Table(Vec<Vec<String>>),
}

/// Trocea el Markdown en bloques. Las líneas seguidas de un párrafo se juntan con un espacio.
pub fn parse(md: &str) -> Vec<Block> {
    let mut out = vec![];
    let mut para: Vec<&str> = vec![];
    let mut table: Vec<Vec<String>> = vec![];
    let flush = |para: &mut Vec<&str>, table: &mut Vec<Vec<String>>, out: &mut Vec<Block>| {
        if !para.is_empty() {
            out.push(Block::Para(para.join(" ")));
            para.clear();
        }
        if !table.is_empty() {
            out.push(Block::Table(std::mem::take(table)));
        }
    };
    for raw in md.lines() {
        let line = raw.trim_end();
        let t = line.trim_start();
        if t.starts_with('|') {
            if !para.is_empty() {
                out.push(Block::Para(para.join(" ")));
                para.clear();
            }
            let cells: Vec<String> = t.trim_matches('|').split('|').map(|c| c.trim().to_string()).collect();
            // la fila |---|---| sólo separa la cabecera
            if !cells.iter().all(|c| !c.is_empty() && c.chars().all(|ch| ch == '-' || ch == ':')) {
                table.push(cells);
            }
            continue;
        }
        if !table.is_empty() {
            out.push(Block::Table(std::mem::take(&mut table)));
        }
        if t.is_empty() {
            flush(&mut para, &mut table, &mut out);
        } else if let Some(h) = t.strip_prefix("### ") {
            flush(&mut para, &mut table, &mut out);
            out.push(Block::Heading(3, h.to_string()));
        } else if let Some(h) = t.strip_prefix("## ") {
            flush(&mut para, &mut table, &mut out);
            out.push(Block::Heading(2, h.to_string()));
        } else if let Some(h) = t.strip_prefix("# ") {
            flush(&mut para, &mut table, &mut out);
            out.push(Block::Heading(1, h.to_string()));
        } else if t == "---" || t == "***" {
            flush(&mut para, &mut table, &mut out);
            out.push(Block::Rule);
        } else if let Some(q) = t.strip_prefix("> ") {
            flush(&mut para, &mut table, &mut out);
            out.push(Block::Quote(q.to_string()));
        } else if let Some(i) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            flush(&mut para, &mut table, &mut out);
            out.push(Block::Item(i.to_string()));
        } else if let Some((n, rest)) = t.split_once(". ").filter(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())) {
            flush(&mut para, &mut table, &mut out);
            out.push(Block::Item(format!("{n}. {rest}")));
        } else if raw.starts_with("   ") && matches!(out.last(), Some(Block::Item(_))) && para.is_empty() {
            // continuación de un punto de la lista
            if let Some(Block::Item(s)) = out.last_mut() {
                s.push(' ');
                s.push_str(t);
            }
        } else {
            para.push(t);
        }
    }
    flush(&mut para, &mut table, &mut out);
    out
}

/// Trozos de una línea con su estilo: (texto, negrita, código, cursiva, enlace).
#[derive(Debug, PartialEq)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub code: bool,
    pub italic: bool,
    pub link: Option<String>,
}

pub fn spans(s: &str) -> Vec<Span> {
    let mut out: Vec<Span> = vec![];
    let (mut bold, mut italic) = (false, false);
    let mut buf = String::new();
    let push = |buf: &mut String, out: &mut Vec<Span>, bold, italic, code, link: Option<String>| {
        if !buf.is_empty() {
            out.push(Span { text: std::mem::take(buf), bold, code, italic, link });
        }
    };
    let c: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < c.len() {
        if c[i] == '*' && c.get(i + 1) == Some(&'*') {
            push(&mut buf, &mut out, bold, italic, false, None);
            bold = !bold;
            i += 2;
        } else if c[i] == '*' && (italic || c.get(i + 1).is_some_and(|n| !n.is_whitespace())) {
            push(&mut buf, &mut out, bold, italic, false, None);
            italic = !italic;
            i += 1;
        } else if c[i] == '`' {
            if let Some(end) = c[i + 1..].iter().position(|&x| x == '`') {
                push(&mut buf, &mut out, bold, italic, false, None);
                let code: String = c[i + 1..i + 1 + end].iter().collect();
                out.push(Span { text: code, bold: false, code: true, italic: false, link: None });
                i += end + 2;
            } else {
                buf.push(c[i]);
                i += 1;
            }
        } else if c[i] == '[' {
            // [texto](url)
            let rest: String = c[i..].iter().collect();
            if let Some(close) = rest.find("](") {
                if let Some(end) = rest[close..].find(')') {
                    push(&mut buf, &mut out, bold, italic, false, None);
                    let text = rest[1..close].to_string();
                    let url = rest[close + 2..close + end].to_string();
                    out.push(Span { text, bold, code: false, italic, link: Some(url) });
                    i += rest[..close + end + 1].chars().count();
                    continue;
                }
            }
            buf.push(c[i]);
            i += 1;
        } else if c[i] == '<' && c[i..].iter().collect::<String>().starts_with("<http") {
            // <https://…>
            let rest: String = c[i..].iter().collect();
            if let Some(end) = rest.find('>') {
                push(&mut buf, &mut out, bold, italic, false, None);
                let url = rest[1..end].to_string();
                out.push(Span { text: url.clone(), bold, code: false, italic, link: Some(url) });
                i += rest[..end + 1].chars().count();
                continue;
            }
            buf.push(c[i]);
            i += 1;
        } else {
            buf.push(c[i]);
            i += 1;
        }
    }
    push(&mut buf, &mut out, bold, italic, false, None);
    out
}

fn job(text: &str, size: f32, color: Color32, wrap: f32) -> LayoutJob {
    let mut j = LayoutJob::default();
    for s in spans(text) {
        let family = if s.bold { semibold_family() } else { FontFamily::Proportional };
        let mut f = TextFormat { font_id: FontId::new(size, family), color: if s.bold { TEXT } else { color }, italics: s.italic, ..Default::default() };
        if s.code {
            f.font_id = FontId::monospace(size - 1.0);
            f.background = CARD_HI;
            f.color = ACCENT_HI;
        }
        if s.link.is_some() {
            f.color = ACCENT_HI;
            f.underline = egui::Stroke::new(1.0_f32, ACCENT.gamma_multiply(0.6));
        }
        j.append(&s.text, 0.0, f);
    }
    j.wrap.max_width = wrap;
    j
}

/// Un párrafo con estilo; si tiene enlaces, al pulsarlo se abre el primero.
fn rich(ui: &mut egui::Ui, text: &str, size: f32, color: Color32) {
    let link = spans(text).into_iter().find_map(|s| s.link);
    let g = ui.fonts(|f| f.layout_job(job(text, size, color, ui.available_width())));
    let sense = if link.is_some() { egui::Sense::click() } else { egui::Sense::hover() };
    let r = ui.add(egui::Label::new(g).sense(sense));
    if let Some(url) = link {
        if r.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(url.as_str()).clicked() {
            let _ = open::that(&url);
        }
    }
}

pub fn render(ui: &mut egui::Ui, md: &str) {
    for b in parse(md) {
        match b {
            Block::Heading(1, t) => {
                ui.add_space(4.0);
                ui.label(RichText::new(t).size(26.0).display().color(TEXT));
                ui.add_space(6.0);
            }
            Block::Heading(lvl, t) => {
                ui.add_space(if lvl == 2 { 14.0 } else { 8.0 });
                if lvl == 2 {
                    section_title(ui, &t);
                } else {
                    ui.label(RichText::new(t).size(15.0).semibold().color(TEXT));
                }
                ui.add_space(4.0);
            }
            Block::Para(t) => {
                rich(ui, &t, 13.5, TEXT_DIM);
                ui.add_space(6.0);
            }
            Block::Item(t) => {
                ui.horizontal_top(|ui| {
                    let bullet = if t.chars().next().is_some_and(|c| c.is_ascii_digit()) { "" } else { "•" };
                    ui.add_sized([14.0, 18.0], egui::Label::new(RichText::new(bullet).color(ACCENT)));
                    ui.vertical(|ui| rich(ui, &t, 13.5, TEXT_DIM));
                });
                ui.add_space(2.0);
            }
            Block::Quote(t) => {
                egui::Frame::none().fill(CARD).rounding(8.0).inner_margin(egui::Margin::symmetric(12.0, 8.0)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    rich(ui, &t, 13.0, TEXT_DIM);
                });
                ui.add_space(6.0);
            }
            Block::Rule => {
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(6.0);
            }
            Block::Table(rows) => {
                egui::Frame::none()
                    .fill(CARD)
                    .rounding(10.0)
                    .stroke(egui::Stroke::new(1.0_f32, BORDER.gamma_multiply(0.7)))
                    .inner_margin(egui::Margin::same(10.0))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        // columnas que caben en el ancho: cada celda se parte en varias líneas si hace falta
                        let cols = rows.iter().map(|r| r.len()).max().unwrap_or(1).max(1) as f32;
                        let cell_w = ((ui.available_width() - 18.0 * (cols - 1.0) - 8.0) / cols).max(80.0);
                        egui::ScrollArea::horizontal().id_salt(ui.next_auto_id()).show(ui, |ui| {
                            egui::Grid::new(ui.next_auto_id()).striped(true).spacing([18.0, 6.0]).min_col_width(cell_w).max_col_width(cell_w).show(ui, |ui| {
                                for (n, row) in rows.iter().enumerate() {
                                    for cell in row {
                                        if n == 0 {
                                            ui.add(egui::Label::new(RichText::new(cell.replace("**", "")).size(12.5).semibold().color(TEXT)).wrap());
                                        } else {
                                            let g = ui.fonts(|f| f.layout_job(job(cell, 12.5, TEXT_DIM, cell_w)));
                                            ui.label(g);
                                        }
                                    }
                                    ui.end_row();
                                }
                            });
                        });
                    });
                ui.add_space(8.0);
            }
        }
    }
}

/// Abre (o mantiene abierta) la ventana del texto elegido.
pub fn window(ctx: &egui::Context, vw: &mut ViewState, out: &mut Vec<Action>) {
    let Some(n) = vw.notice else { return };
    let (_, text) = n.file_and_text();
    let title = format!("{} · Brasa Launcher", n.title());
    let builder = egui::ViewportBuilder::default()
        .with_title(title)
        .with_inner_size([760.0, 620.0])
        .with_min_inner_size([480.0, 360.0])
        .with_icon(std::sync::Arc::new(crate::gui::window_icon()));
    let mut close = false;
    let mut body = |ui: &mut egui::Ui| {
        // pestañas para pasar de un texto a otro sin cerrar
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for k in [Notice::License, Notice::ThirdParty, Notice::Privacy] {
                if sub_tab(ui, k.title(), None, vw.notice == Some(k)) {
                    vw.notice = Some(k);
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(ghost_button(t!("Abrir como archivo"))).on_hover_text(t!("Con el programa del sistema")).clicked() {
                    out.push(Action::OpenNotice(n));
                }
                if ui.add(secondary_button(t!("📋  Copiar"))).clicked() {
                    out.push(Action::Copy(text.to_string(), t!("Texto copiado").to_string()));
                }
            });
        });
        let r = ui.min_rect();
        ui.painter().hline(r.x_range(), ui.cursor().top() - 1.0, egui::Stroke::new(1.0_f32, BORDER.gamma_multiply(0.6)));
        ui.add_space(8.0);
        egui::ScrollArea::vertical().id_salt(("notice", n as u8)).auto_shrink([false, false]).show(ui, |ui| {
            ui.set_max_width(ui.available_width().min(860.0));
            if n == Notice::License {
                // texto plano: en monoespaciada, tal cual
                ui.label(RichText::new(text).monospace().size(12.5).color(TEXT_DIM));
            } else {
                render(ui, text);
            }
            ui.add_space(16.0);
        });
    };
    ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("brasa-notice"), builder, |ctx, class| {
        if class == egui::ViewportClass::Embedded {
            // sin ventanas múltiples (p. ej. algunas plataformas): ventana dentro del launcher
            let mut open = true;
            egui::Window::new(n.title()).open(&mut open).default_size([680.0, 520.0]).show(ctx, |ui| body(ui));
            close |= !open;
            return;
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(BG).inner_margin(egui::Margin { left: 22.0, right: 22.0, top: 18.0, bottom: 10.0 }))
            .show(ctx, |ui| body(ui));
        if ctx.input(|i| i.viewport().close_requested() || i.key_pressed(egui::Key::Escape)) {
            close = true;
        }
    });
    if close {
        vw.notice = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_shapes_used_in_our_documents() {
        let md = "# Título\n\nUn párrafo\nen dos líneas.\n\n## Sección\n\n| A | B |\n|---|---|\n| 1 | 2 |\n\n- uno\n- dos\n   sigue\n\n1. paso\n\n> cita\n\n---\nfin";
        let b = parse(md);
        assert_eq!(b[0], Block::Heading(1, "Título".into()));
        assert_eq!(b[1], Block::Para("Un párrafo en dos líneas.".into()));
        assert_eq!(b[2], Block::Heading(2, "Sección".into()));
        assert_eq!(b[3], Block::Table(vec![vec!["A".into(), "B".into()], vec!["1".into(), "2".into()]]));
        assert_eq!(b[4], Block::Item("uno".into()));
        assert_eq!(b[5], Block::Item("dos sigue".into()));
        assert_eq!(b[6], Block::Item("1. paso".into()));
        assert_eq!(b[7], Block::Quote("cita".into()));
        assert_eq!(b[8], Block::Rule);
        assert_eq!(b[9], Block::Para("fin".into()));
    }

    #[test]
    fn inline_styles_and_links() {
        let s = spans("Hola **fuerte** y `código` y *suave* en [Brasa](https://x.y) o <https://z.w>.");
        let texts: Vec<(&str, bool, bool, bool, bool)> = s.iter().map(|x| (x.text.as_str(), x.bold, x.code, x.italic, x.link.is_some())).collect();
        assert_eq!(
            texts,
            vec![
                ("Hola ", false, false, false, false),
                ("fuerte", true, false, false, false),
                (" y ", false, false, false, false),
                ("código", false, true, false, false),
                (" y ", false, false, false, false),
                ("suave", false, false, true, false),
                (" en ", false, false, false, false),
                ("Brasa", false, false, false, true),
                (" o ", false, false, false, false),
                ("https://z.w", false, false, false, true),
                (".", false, false, false, false),
            ]
        );
        // un asterisco suelto no abre cursiva
        assert_eq!(spans("2 * 3").len(), 1);
    }

    #[test]
    fn our_legal_files_parse() {
        for n in [Notice::ThirdParty, Notice::Privacy] {
            let b = parse(n.file_and_text().1);
            assert!(b.iter().any(|x| matches!(x, Block::Table(r) if r.len() > 2)), "{n:?} sin tablas");
            assert!(matches!(b[0], Block::Heading(1, _)));
        }
    }
}
