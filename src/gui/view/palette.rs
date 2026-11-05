//! Buscador rápido (Ctrl+K o ⌘K): jugar o abrir cualquier instancia, ir a cualquier sección o
//! buscar mods, sin soltar el teclado. Flechas para elegir, Intro para ejecutar, Esc para cerrar.

use crate::gui::action::Action;
use crate::gui::model::Model;
use crate::gui::state::{Tab, ViewState};
use crate::gui::view::theme::*;
use eframe::egui::{self, RichText};

/// Lo que hace cada resultado.
#[derive(Debug, Clone, PartialEq)]
pub enum Cmd {
    Play(String),
    Open(String),
    Go(Tab),
    NewInstance,
    SearchMods(String),
    Report,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub title: String,
    /// Texto gris a la derecha (p. ej. «Fabric 1.21.1»).
    pub detail: String,
    pub cmd: Cmd,
}

/// Minúsculas y sin tildes, para que «creacion» encuentre «Creación».
pub fn fold(s: &str) -> String {
    s.chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            c => c,
        })
        .collect()
}

/// Puntuación de un texto para la búsqueda (`None` = no coincide). Cada palabra buscada debe
/// aparecer; puntúa más si empieza el texto o una palabra, y si el texto es corto.
pub fn score(query: &str, text: &str) -> Option<i32> {
    let q = fold(query);
    let t = fold(text);
    let words: Vec<&str> = q.split_whitespace().collect();
    if words.is_empty() {
        return Some(0);
    }
    let mut total = 0;
    for w in words {
        let i = t.find(w)?;
        let at_word = i == 0 || !t[..i].chars().last().is_some_and(|c| c.is_alphanumeric());
        total += if i == 0 {
            30
        } else if at_word {
            20
        } else {
            5
        };
    }
    Some(total - (t.chars().count() as i32 / 8))
}

pub fn entries(mdl: &Model) -> Vec<Entry> {
    let mut v = vec![];
    let mut inst: Vec<_> = mdl.instances.iter().collect();
    inst.sort_by_key(|i| std::cmp::Reverse(i.last_played));
    for i in &inst {
        let d = format!("{} {}", i.loader.label(), i.mc_version);
        v.push(Entry { title: t!("Jugar a {}", i.name), detail: d.clone(), cmd: Cmd::Play(i.id.clone()) });
        v.push(Entry { title: t!("Abrir {}", i.name), detail: d, cmd: Cmd::Open(i.id.clone()) });
    }
    v.push(Entry { title: t!("Nueva instancia").into(), detail: t!("Crear").into(), cmd: Cmd::NewInstance });
    for (tab, name) in [
        (Tab::Home, t!("Inicio")),
        (Tab::Instances, t!("Instancias")),
        (Tab::Wardrobe, t!("Taquilla de cosméticos")),
        (Tab::Browse, t!("Explorar mods")),
        (Tab::Import, t!("Importar")),
        (Tab::Hosting, t!("Servidores")),
        (Tab::Accounts, t!("Cuentas")),
        (Tab::Settings, t!("Ajustes")),
        (Tab::Logs, t!("Consola")),
    ] {
        v.push(Entry { title: name.into(), detail: t!("Ir a").into(), cmd: Cmd::Go(tab) });
    }
    v.push(Entry { title: t!("Informar de un problema").into(), detail: t!("Ayuda").into(), cmd: Cmd::Report });
    v
}

/// Resultados para lo escrito, del mejor al peor. Si hay texto, siempre se ofrece buscarlo como mod.
pub fn results(all: &[Entry], query: &str, limit: usize) -> Vec<Entry> {
    let mut scored: Vec<(i32, usize, &Entry)> = all
        .iter()
        .enumerate()
        .filter_map(|(n, e)| {
            score(query, &e.title)
                .or_else(|| score(query, &format!("{} {}", e.title, e.detail)).map(|s| s - 10))
                .map(|s| (s + if matches!(e.cmd, Cmd::Play(_)) { 3 } else { 0 }, n, e))
        })
        .collect();
    // jugar es lo que más se busca: un pequeño empujón. A igualdad de puntos, el orden original (instancias recientes primero)
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut out: Vec<Entry> = scored.into_iter().take(limit).map(|(_, _, e)| e.clone()).collect();
    let q = query.trim();
    if !q.is_empty() {
        out.truncate(limit.saturating_sub(1));
        out.push(Entry { title: t!("Buscar «{q}» en mods", q = q), detail: t!("Explorar").into(), cmd: Cmd::SearchMods(q.to_string()) });
    }
    out
}

fn run(cmd: Cmd, vw: &mut ViewState, out: &mut Vec<Action>) {
    match cmd {
        Cmd::Play(id) => out.push(Action::Play(id)),
        Cmd::Open(id) => {
            vw.tab = Tab::Instances;
            out.push(Action::Select(id));
        }
        Cmd::Go(t) => vw.tab = t,
        Cmd::NewInstance => {
            vw.tab = Tab::Instances;
            out.push(Action::ShowCreate);
        }
        Cmd::SearchMods(q) => {
            vw.tab = Tab::Browse;
            vw.query = q;
            out.push(Action::Search);
        }
        Cmd::Report => crate::gui::view::report::open(vw),
    }
}

/// Atajo de teclado: Ctrl+K (⌘K en Mac) abre y cierra.
pub fn shortcut(ctx: &egui::Context, vw: &mut ViewState) {
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::K)) {
        vw.palette_open = !vw.palette_open;
        vw.palette_query.clear();
        vw.palette_sel = 0;
    }
}

pub fn draw(ctx: &egui::Context, mdl: &Model, vw: &mut ViewState, out: &mut Vec<Action>) {
    if !vw.palette_open {
        return;
    }
    let all = entries(mdl);
    let list = results(&all, &vw.palette_query, 8);
    let (down, up, enter, esc) = ctx
        .input(|i| (i.key_pressed(egui::Key::ArrowDown), i.key_pressed(egui::Key::ArrowUp), i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
    if esc {
        vw.palette_open = false;
        return;
    }
    if !list.is_empty() {
        if down {
            vw.palette_sel = (vw.palette_sel + 1) % list.len();
        }
        if up {
            vw.palette_sel = (vw.palette_sel + list.len() - 1) % list.len();
        }
        vw.palette_sel = vw.palette_sel.min(list.len() - 1);
    }
    if enter {
        if let Some(e) = list.get(vw.palette_sel) {
            vw.palette_open = false;
            run(e.cmd.clone(), vw, out);
            return;
        }
    }

    // velo sobre la ventana; un clic fuera cierra
    let screen = ctx.screen_rect();
    let veil = egui::Area::new(egui::Id::new("palette_veil")).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
        let (r, resp) = ui.allocate_exact_size(screen.size(), egui::Sense::click());
        ui.painter().rect_filled(r, 0.0, egui::Color32::from_black_alpha(150));
        resp
    });
    let mut picked: Option<Cmd> = None;
    let width = (screen.width() - 48.0).min(580.0);
    let panel = egui::Area::new(egui::Id::new("palette"))
        .order(egui::Order::Tooltip)
        .fixed_pos(egui::pos2(screen.center().x - width / 2.0, screen.min.y + screen.height() * 0.16))
        .show(ctx, |ui| {
            egui::Frame::none()
                .fill(CARD_HI)
                .rounding(12.0)
                .stroke(egui::Stroke::new(1.0_f32, BORDER))
                .shadow(egui::epaint::Shadow { offset: egui::vec2(0.0, 12.0), blur: 40.0, spread: 0.0, color: egui::Color32::from_black_alpha(140) })
                .inner_margin(egui::Margin::same(10.0))
                .show(ui, |ui| {
                    ui.set_width(width - 20.0);
                    let edit = egui::TextEdit::singleline(&mut vw.palette_query)
                        .hint_text(t!("Jugar, abrir o ir a…"))
                        .font(egui::FontId::proportional(18.0))
                        .frame(false)
                        .desired_width(f32::INFINITY)
                        .margin(egui::vec2(8.0, 10.0));
                    let r = ui.add(edit);
                    r.request_focus();
                    if r.changed() {
                        vw.palette_sel = 0;
                    }
                    ui.add_space(4.0);
                    ui.painter().hline(ui.min_rect().x_range(), ui.cursor().top(), egui::Stroke::new(1.0_f32, BORDER));
                    ui.add_space(6.0);
                    if list.is_empty() {
                        ui.label(RichText::new(t!("Sin resultados")).color(TEXT_DIM));
                    }
                    for (n, e) in list.iter().enumerate() {
                        let sel = n == vw.palette_sel;
                        let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 38.0), egui::Sense::click());
                        if resp.hovered() && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
                            vw.palette_sel = n;
                        }
                        let p = ui.painter();
                        if sel {
                            p.rect_filled(rect, 7.0, CARD);
                            p.rect_filled(egui::Rect::from_min_size(rect.min + egui::vec2(0.0, 9.0), egui::vec2(3.0, rect.height() - 18.0)), 0.0, ACCENT);
                        }
                        let fam = if sel { crate::gui::view::fonts::semibold_family() } else { egui::FontFamily::Proportional };
                        p.text(rect.left_center() + egui::vec2(14.0, 0.0), egui::Align2::LEFT_CENTER, &e.title, egui::FontId::new(15.0, fam), TEXT);
                        p.text(rect.right_center() - egui::vec2(12.0, 0.0), egui::Align2::RIGHT_CENTER, &e.detail, egui::FontId::proportional(12.5), TEXT_DIM);
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            picked = Some(e.cmd.clone());
                        }
                    }
                    ui.add_space(4.0);
                    ui.label(RichText::new(t!("↑ ↓ para elegir · Intro para abrir · Esc para cerrar")).size(11.5).color(TEXT_DIM));
                });
        });
    if let Some(c) = picked {
        vw.palette_open = false;
        run(c, vw, out);
    } else if veil.inner.clicked() && !panel.response.hovered() {
        vw.palette_open = false;
    }
}

/// El acceso visible en la barra lateral (para quien no conoce el atajo).
pub fn sidebar_hint(ui: &mut egui::Ui, vw: &mut ViewState) {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 34.0), egui::Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, 7.0, if resp.hovered() { CARD_HI } else { CARD.gamma_multiply(0.7) });
    p.text(rect.left_center() + egui::vec2(12.0, 0.0), egui::Align2::LEFT_CENTER, t!("Buscar…"), egui::FontId::proportional(13.5), TEXT_DIM);
    let key = if cfg!(target_os = "macos") { "⌘K" } else { t!("Ctrl K") };
    let g = p.layout_no_wrap(key.to_string(), egui::FontId::new(11.5, crate::gui::view::fonts::semibold_family()), TEXT_DIM);
    let kr = egui::Rect::from_min_size(egui::pos2(rect.max.x - g.size().x - 20.0, rect.center().y - 10.0), egui::vec2(g.size().x + 10.0, 20.0));
    p.rect_stroke(kr, 4.0, egui::Stroke::new(1.0_f32, BORDER));
    p.galley(kr.center() - g.size() / 2.0, g, TEXT_DIM);
    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        vw.palette_open = true;
        vw.palette_query.clear();
        vw.palette_sel = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Entry> {
        let e = |t: &str, d: &str, c: Cmd| Entry { title: t.into(), detail: d.into(), cmd: c };
        vec![
            e("Jugar a Create 1.20", "Forge 1.20.1", Cmd::Play("create".into())),
            e("Abrir Create 1.20", "Forge 1.20.1", Cmd::Open("create".into())),
            e("Jugar a Mi survival", "Fabric 1.21.1", Cmd::Play("surv".into())),
            e("Ajustes", "Ir a", Cmd::Go(Tab::Settings)),
            e("Importar", "Ir a", Cmd::Go(Tab::Import)),
        ]
    }

    #[test]
    fn accents_and_case_do_not_matter() {
        assert_eq!(fold("Creación ÑANDÚ"), "creacion nandu");
        assert!(score("creacion", "Creación de mundos").is_some());
        assert!(score("CREATE", "Jugar a Create").is_some());
    }

    #[test]
    fn every_word_must_match_and_word_starts_win() {
        assert!(score("create fabric", "Jugar a Create 1.20 Forge").is_none());
        let start = score("aju", "Ajustes").unwrap();
        let middle = score("tes", "Ajustes").unwrap();
        assert!(start > middle);
    }

    #[test]
    fn results_order_and_mod_search_at_the_end() {
        let all = sample();
        let r = results(&all, "create", 8);
        assert_eq!(r[0].cmd, Cmd::Play("create".into()), "jugar primero, por el orden original");
        assert_eq!(r.last().unwrap().cmd, Cmd::SearchMods("create".into()));
        let empty = results(&all, "", 3);
        assert_eq!(empty.len(), 3, "sin texto: lo primero de la lista, sin búsqueda de mods");
        assert!(empty.iter().all(|e| !matches!(e.cmd, Cmd::SearchMods(_))));
        let none = results(&all, "zzz", 8);
        assert_eq!(none, vec![Entry { title: "Buscar «zzz» en mods".into(), detail: "Explorar".into(), cmd: Cmd::SearchMods("zzz".into()) }]);
    }

    #[test]
    fn commands_do_what_they_say() {
        let mut vw = ViewState::new(&crate::config::Settings::default());
        let mut out = vec![];
        run(Cmd::SearchMods("sodium".into()), &mut vw, &mut out);
        assert!(vw.tab == Tab::Browse && vw.query == "sodium" && matches!(out.as_slice(), [Action::Search]));
        out.clear();
        run(Cmd::Open("x".into()), &mut vw, &mut out);
        assert!(vw.tab == Tab::Instances && matches!(out.as_slice(), [Action::Select(id)] if id == "x"));
    }
}
