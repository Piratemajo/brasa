//! Bienvenida del primer arranque: idioma, cuenta y primera instancia, con el aspecto del instalador.
//!
//! Sale cuando Brasa no tiene ni cuentas ni instancias y aún no se ha visto. Cada paso usa las
//! mismas acciones que el resto del launcher (añadir cuenta, crear instancia, importar): esto sólo
//! las ordena para quien empieza.

use crate::account::auth::AccountKind;
use crate::gui::action::Action;
use crate::gui::model::{Model, Session};
use crate::gui::state::{Tab, ViewState};
use crate::gui::view::art::{self, BigKind};
use crate::gui::view::fonts::Weight;
use crate::gui::view::theme::*;
use crate::gui::view::widgets::*;
use eframe::egui::{self, RichText};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Who {
    #[default]
    Premium,
    NoPremium,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum First {
    Vanilla,
    #[default]
    Perf,
    Import,
}

/// Skin para la cuenta no premium: ninguna (offline) o un servidor de `yggdrasil::PRESETS`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Skin {
    #[default]
    None,
    Preset(usize),
}

#[derive(Debug, Clone, Default)]
pub struct Onboard {
    pub step: usize,
    pub who: Who,
    pub skin: Skin,
    pub first: First,
    /// Esperando a que aparezca la cuenta nueva (había estas antes).
    pub waiting: Option<usize>,
    /// Instancias de otros launchers ya buscadas.
    pub scanned: bool,
    /// Cuáles importar (por índice en `Model::importable`).
    pub import_sel: Vec<bool>,
    /// Lo que se hizo, para el resumen.
    pub imported: usize,
    pub made: Option<First>,
}

/// ¿Toca enseñar la bienvenida? Sólo la primera vez y con Brasa vacío.
pub fn should_show(mdl: &Model) -> bool {
    !mdl.settings.onboarded && mdl.accounts.accounts.is_empty() && mdl.instances.is_empty()
}

pub fn start(vw: &mut ViewState) {
    vw.onboard = Some(Onboard::default());
    vw.offline_name.clear();
}

const STEPS: [&str; 4] = ["Idioma", "Cuenta", "Primera instancia", "Listo"];

/// Fuera de la bienvenida: jugar a la instancia nueva en cuanto termine de instalarse.
pub fn pending(s: &Session, vw: &mut ViewState, out: &mut Vec<Action>) {
    if !vw.play_after_quickstart {
        return;
    }
    if let Some(q) = &s.quickstart {
        if q.done {
            vw.play_after_quickstart = false;
            if let (None, Some(id)) = (&q.error, &q.id) {
                out.push(Action::Select(id.clone()));
                out.push(Action::Play(id.clone()));
            }
        }
    }
}

pub fn draw(ctx: &egui::Context, mdl: &Model, vw: &mut ViewState, s: &Session, out: &mut Vec<Action>) {
    let Some(mut ob) = vw.onboard.take() else { return };
    // la cuenta que se estaba añadiendo ya está: siguiente paso
    if let Some(before) = ob.waiting {
        if mdl.accounts.accounts.len() > before {
            ob.waiting = None;
            ob.step = 2;
        } else if s.error.is_some() && s.device_code.is_none() && s.tasks == 0 {
            ob.waiting = None;
        }
    }
    let mut close = false;
    egui::CentralPanel::default().frame(egui::Frame::none().fill(egui::Color32::TRANSPARENT)).show(ctx, |ui| {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            side(ui, ob.step);
            let w = ui.available_width();
            let pad = (w * 0.06).clamp(28.0, 64.0);
            ui.add_space(pad);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.set_width((w - pad * 2.0).min(760.0));
                ui.add_space(44.0);
                match ob.step {
                    0 => language(ui, mdl, vw, &mut ob, out),
                    1 => account(ui, mdl, vw, s, &mut ob, out),
                    2 => instance(ui, mdl, vw, s, &mut ob, out),
                    _ => close = ready(ui, mdl, vw, s, &mut ob),
                }
            });
        });
    });
    if close {
        out.push(Action::FinishOnboarding);
    } else {
        vw.onboard = Some(ob);
    }
}

/// Columna izquierda: la llama, los pasos y las brasas.
fn side(ui: &mut egui::Ui, step: usize) {
    let w = (ui.available_width() * 0.27).clamp(220.0, 300.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, ui.available_height()), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, BG_SIDE);
    art::gradient_v(&p, rect, EMBER.gamma_multiply(0.0), EMBER.gamma_multiply(0.18 + 0.08 * step as f32));
    art::sparks(&p, rect, "onboarding");
    let mut c = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(egui::vec2(28.0, 44.0))).layout(egui::Layout::top_down(egui::Align::Min)));
    c.horizontal(|ui| {
        art::flame(ui, 3.0, 0.0);
        ui.add_space(8.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = -2.0;
            ui.label(RichText::new("BRASA").size(32.0).display().color(TEXT));
            ui.label(RichText::new(format!("launcher v{}", env!("CARGO_PKG_VERSION"))).size(11.0).color(TEXT_DIM));
        });
    });
    c.add_space(44.0);
    for (i, name) in STEPS.iter().enumerate() {
        step_row(&mut c, i, crate::i18n::tr(name), step);
        c.add_space(4.0);
    }
}

fn step_row(ui: &mut egui::Ui, i: usize, label: &str, current: usize) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 40.0), egui::Sense::hover());
    let p = ui.painter();
    let on = i == current;
    if on {
        p.rect_filled(rect, 8.0, ACCENT.gamma_multiply(0.10));
    }
    let c = egui::pos2(rect.left() + 24.0, rect.center().y);
    if i < current {
        p.circle_filled(c, 11.0, GREEN);
        p.line_segment([c + egui::vec2(-5.0, 0.0), c + egui::vec2(-1.5, 3.5)], egui::Stroke::new(2.0_f32, BG));
        p.line_segment([c + egui::vec2(-1.5, 3.5), c + egui::vec2(5.0, -3.5)], egui::Stroke::new(2.0_f32, BG));
    } else {
        if on {
            p.circle_filled(c, 15.0, ACCENT.gamma_multiply(0.13));
        }
        p.circle_stroke(c, 11.0, egui::Stroke::new(1.5_f32, if on { ACCENT } else { BORDER }));
        p.text(c, egui::Align2::CENTER_CENTER, (i + 1).to_string(), egui::FontId::proportional(12.0), if on { ACCENT } else { TEXT_DIM });
    }
    p.text(
        egui::pos2(rect.left() + 48.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(14.5),
        if on || i < current { TEXT } else { TEXT_DIM },
    );
}

fn header(ui: &mut egui::Ui, title: &str, sub: &str) {
    ui.label(RichText::new(title).size(28.0).display().color(TEXT));
    ui.add_space(4.0);
    ui.label(RichText::new(sub).size(14.5).color(TEXT_DIM));
    ui.add_space(22.0);
}

/// Tarjeta que se elige con un clic (con su «radio» arriba a la derecha).
fn choice(ui: &mut egui::Ui, key: &str, selected: bool, add: impl FnOnce(&mut egui::Ui)) -> bool {
    let id = ui.id().with(("choice", key));
    let prev: Option<egui::Rect> = ui.ctx().data(|d| d.get_temp(id));
    let mut hovered = false;
    let mut clicked = false;
    if let Some(r) = prev {
        // se registra antes que el contenido: así los campos de dentro siguen recibiendo el ratón
        let resp = ui.interact(r, id, egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        hovered = resp.hovered();
        clicked = resp.clicked();
    }
    let hot = ui.ctx().animate_bool_with_time(id.with("hot"), hovered || selected, 0.12);
    let fill = if selected { art::mix(CARD, ACCENT, 0.07) } else { art::mix(CARD, CARD_HI, hot) };
    let stroke = if selected { ACCENT } else { art::mix(BORDER, TEXT_DIM, hot * 0.3) };
    let resp = egui::Frame::none()
        .fill(fill)
        .rounding(12.0)
        .stroke(egui::Stroke::new(1.5_f32, stroke))
        .inner_margin(egui::Margin { left: 18.0, right: 46.0, top: 16.0, bottom: 16.0 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            // textos no seleccionables: si no, se quedan el clic y la tarjeta no se elige
            ui.style_mut().interaction.selectable_labels = false;
            add(ui);
        })
        .response;
    let p = ui.painter();
    let c = egui::pos2(resp.rect.right() - 24.0, resp.rect.top() + 26.0);
    p.circle_stroke(c, 9.0, egui::Stroke::new(2.0_f32, if selected { ACCENT } else { BORDER }));
    if selected {
        p.circle_filled(c, 4.5, ACCENT);
    }
    ui.ctx().data_mut(|d| d.insert_temp(id, resp.rect));
    clicked
}

fn card_title(ui: &mut egui::Ui, icon: &str, title: &str, chips: &[(&str, egui::Color32)]) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(egui::vec2(40.0, 40.0), egui::Sense::hover());
        ui.painter().rect_filled(r, 10.0, CARD_HI);
        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(19.0), ACCENT_HI);
        ui.add_space(6.0);
        ui.label(RichText::new(title).size(17.0).semibold().color(TEXT));
        for (t, c) in chips {
            // en vertical para que la pastilla no se estire al alto del icono
            ui.vertical(|ui| {
                ui.add_space(10.0);
                chip(ui, t, *c);
            });
        }
    });
}

fn indent(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.add_space(54.0);
        ui.vertical(add);
    });
}

fn hint(ui: &mut egui::Ui, t: &str) {
    ui.label(RichText::new(t).size(12.5).color(TEXT_DIM));
}

/// Pie: «Saltar» a la izquierda, «Atrás» y el botón grande a la derecha. Devuelve (saltar, atrás, siguiente).
fn footer(ui: &mut egui::Ui, skip: Option<&str>, back: bool, next: &str, enabled: bool) -> (bool, bool, bool) {
    let h = ui.available_height();
    ui.add_space((h - 96.0).max(16.0));
    let mut r = (false, false, false);
    ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 54.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        if let Some(s) = skip {
            r.0 = ui
                .add(egui::Label::new(RichText::new(s).size(14.0).color(TEXT_DIM)).sense(egui::Sense::click()))
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked();
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let w = 60.0 + next.chars().count() as f32 * 13.0;
            r.2 = art::big_button(ui, BigKind::Play, next, egui::vec2(w.max(180.0), 50.0), enabled).clicked();
            ui.add_space(14.0);
            if back {
                r.1 = ui.add(ghost_button(t!("Atrás"))).clicked();
            }
        });
    });
    r
}

// ----------------------------------------------------------------------------- 1. idioma

fn language(ui: &mut egui::Ui, mdl: &Model, vw: &mut ViewState, ob: &mut Onboard, out: &mut Vec<Action>) {
    header(ui, t!("¡Bienvenido a Brasa!"), t!("Vamos a dejarlo listo en un minuto. Primero, ¿en qué idioma quieres usarlo?"));
    let system = crate::i18n::resolve("auto", crate::i18n::system_locale().as_deref());
    let current = crate::i18n::current();
    let _ = mdl;
    ui.columns(2, |cols| {
        for (col, (lang, code, name)) in cols.iter_mut().zip([(crate::i18n::Lang::Es, "es", "Español"), (crate::i18n::Lang::En, "en", "English")]) {
            let sub = if lang == system {
                t!("Detectado en tu sistema")
            } else if lang == crate::i18n::Lang::En {
                "Switches instantly, no restart"
            } else {
                "Cambia al momento, sin reiniciar"
            };
            if choice(col, code, current == lang, |ui| {
                ui.label(RichText::new(name).size(24.0).semibold().color(TEXT));
                ui.label(RichText::new(sub).size(13.0).color(TEXT_DIM));
            }) {
                // «auto» si coincide con el del sistema: así sigue al sistema si este cambia
                vw.settings.language = if lang == system { "auto".into() } else { code.into() };
                out.push(Action::ApplySettings({
                    let mut s = mdl.settings.clone();
                    s.language = vw.settings.language.clone();
                    s
                }));
            }
        }
    });
    ui.add_space(12.0);
    hint(ui, t!("Puedes cambiarlo cuando quieras en Ajustes → Launcher."));
    let (_, _, next) = footer(ui, None, false, t!("SIGUIENTE"), true);
    if next {
        ob.step = 1;
    }
}

// ----------------------------------------------------------------------------- 2. cuenta

fn valid_name(n: &str) -> bool {
    (3..=16).contains(&n.len()) && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn account(ui: &mut egui::Ui, mdl: &Model, vw: &mut ViewState, s: &Session, ob: &mut Onboard, out: &mut Vec<Action>) {
    header(ui, t!("¿Con qué cuenta vas a jugar?"), t!("Tus cuentas se guardan sólo en este equipo. Puedes añadir más luego."));
    let busy = ob.waiting.is_some();
    let has_msa = !mdl.settings.msa_client_id.trim().is_empty();
    ui.add_enabled_ui(!busy, |ui| {
        if choice(ui, "premium", ob.who == Who::Premium, |ui| {
            card_title(ui, "🔑", t!("Tengo Minecraft"), &[("Premium", GREEN)]);
            indent(ui, |ui| {
                hint(ui, t!("Inicia sesión con tu cuenta de Microsoft. Skins, servidores premium y Realms."));
                if !has_msa {
                    ui.label(
                        RichText::new(t!("Esta copia de Brasa no trae el inicio de sesión con Microsoft: usa la versión oficial de las releases."))
                            .size(12.5)
                            .color(YELLOW),
                    );
                }
            });
        }) {
            ob.who = Who::Premium;
        }
        ui.add_space(12.0);
        if choice(ui, "nopremium", ob.who == Who::NoPremium, |ui| {
            card_title(ui, "🎮", t!("Jugar sin premium"), &[]);
            indent(ui, |ui| {
                hint(ui, t!("Elige un nombre. Puedes usar skin con Ely.by o LittleSkin."));
                if ob.who != Who::NoPremium {
                    return;
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let mut pill = |ui: &mut egui::Ui, label: &str, v: Skin| {
                        let on = ob.skin == v;
                        let b = egui::Button::new(RichText::new(label).size(13.0).color(if on { TEXT } else { TEXT_DIM }))
                            .fill(if on { ACCENT.gamma_multiply(0.14) } else { egui::Color32::TRANSPARENT })
                            .stroke(egui::Stroke::new(1.0_f32, if on { ACCENT } else { BORDER }))
                            .rounding(20.0);
                        if ui.add(b).clicked() {
                            ob.skin = v;
                        }
                    };
                    pill(ui, t!("Sin skin (offline)"), Skin::None);
                    for (i, (_, name, _)) in crate::account::yggdrasil::PRESETS.iter().enumerate() {
                        pill(ui, name, Skin::Preset(i));
                    }
                });
                ui.add_space(8.0);
                match ob.skin {
                    Skin::None => {
                        let name = vw.offline_name.trim().to_string();
                        ui.horizontal(|ui| {
                            let field = ui.add(egui::TextEdit::singleline(&mut vw.offline_name).hint_text(t!("Tu nombre en el juego")).desired_width(240.0));
                            // al elegir esta opción, el cursor ya está en el nombre
                            if vw.offline_name.is_empty() && ui.memory(|m| m.focused().is_none()) {
                                field.request_focus();
                            }
                            if !name.is_empty() {
                                if valid_name(&name) {
                                    ui.label(RichText::new(t!("✔ nombre válido")).size(12.5).color(GREEN));
                                } else {
                                    ui.label(RichText::new(t!("3 a 16 letras, números o _")).size(12.5).color(RED));
                                }
                            }
                        });
                    }
                    Skin::Preset(i) => {
                        vw.skin_server = i;
                        let name = crate::account::yggdrasil::PRESETS[i].1;
                        ui.add(egui::TextEdit::singleline(&mut vw.skin_user).hint_text(t!("Usuario o correo de {}", name)).desired_width(300.0));
                        ui.add(egui::TextEdit::singleline(&mut vw.skin_pass).password(true).hint_text(t!("Contraseña")).desired_width(300.0));
                        hint(ui, t!("La contraseña no se guarda: sólo se usa para entrar."));
                    }
                }
            });
        }) {
            ob.who = Who::NoPremium;
        }
    });
    // código de Microsoft mientras se espera
    if busy {
        ui.add_space(14.0);
        egui::Frame::none()
            .fill(INPUT)
            .rounding(10.0)
            .stroke(egui::Stroke::new(1.0_f32, ACCENT.gamma_multiply(0.5)))
            .inner_margin(egui::Margin::same(14.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                match &s.device_code {
                    Some((code, uri)) => {
                        ui.label(RichText::new(t!("Se ha abierto {} en el navegador. Escribe este código:", uri)).color(TEXT));
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(code).monospace().size(24.0).color(ACCENT_HI));
                            ui.add_space(10.0);
                            if ui.add(secondary_button(t!("📋  Copiar código"))).clicked() {
                                out.push(Action::Copy(code.clone(), t!("Código copiado").into()));
                            }
                            if ui.add(ghost_button(t!("Abrir la página"))).clicked() {
                                out.push(Action::OpenUrl(uri.clone()));
                            }
                        });
                    }
                    None => {
                        ui.horizontal(|ui| {
                            ui.add(egui::Spinner::new().size(16.0).color(ACCENT));
                            ui.label(RichText::new(t!("Iniciando sesión…")).color(TEXT_DIM));
                        });
                    }
                }
            });
    }
    let ready = match (ob.who, ob.skin) {
        (Who::Premium, _) => has_msa,
        (Who::NoPremium, Skin::None) => valid_name(vw.offline_name.trim()),
        (Who::NoPremium, Skin::Preset(_)) => !vw.skin_user.trim().is_empty() && !vw.skin_pass.is_empty(),
    };
    let label = if busy { t!("ESPERANDO…") } else { t!("SIGUIENTE") };
    let (skip, back, next) = footer(ui, Some(t!("Saltar este paso")), true, label, ready && !busy);
    if skip {
        ob.waiting = None;
        ob.step = 2;
    }
    if back {
        ob.waiting = None;
        ob.step = 0;
    }
    if next {
        ob.waiting = Some(mdl.accounts.accounts.len());
        out.push(match (ob.who, ob.skin) {
            (Who::Premium, _) => Action::MsLogin,
            (Who::NoPremium, Skin::None) => Action::AddOffline,
            (Who::NoPremium, Skin::Preset(_)) => Action::SkinLogin,
        });
    }
}

// ----------------------------------------------------------------------------- 3. instancia

fn instance(ui: &mut egui::Ui, mdl: &Model, _vw: &mut ViewState, s: &Session, ob: &mut Onboard, out: &mut Vec<Action>) {
    if !ob.scanned {
        ob.scanned = true;
        out.push(Action::ScanImport);
    }
    if ob.import_sel.len() != mdl.importable.len() {
        ob.import_sel = vec![true; mdl.importable.len()];
    }
    header(ui, t!("Tu primera instancia"), t!("Una instancia es una versión de Minecraft con sus propios mods, mundos y ajustes."));
    let latest = s.latest_release.clone();
    let vanilla_chip = if latest.is_empty() { "Vanilla".to_string() } else { format!("Vanilla {latest}") };
    if choice(ui, "vanilla", ob.first == First::Vanilla, |ui| {
        card_title(ui, "🌍", t!("La última versión"), &[(&vanilla_chip, TEXT_DIM)]);
        indent(ui, |ui| hint(ui, t!("Minecraft tal cual, sin mods.")));
    }) {
        ob.first = First::Vanilla;
    }
    ui.add_space(12.0);
    if choice(ui, "perf", ob.first == First::Perf, |ui| {
        card_title(ui, "⚡", t!("Con mods de rendimiento"), &[("Fabric", ACCENT_HI), (t!("Recomendado"), GREEN)]);
        indent(ui, |ui| {
            hint(ui, t!("Más FPS y menos tirones, con la misma pinta de siempre."));
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for m in ["Sodium", "Lithium", "FerriteCore", "EntityCulling", "ImmediatelyFast", "ModernFix", "Krypton"] {
                    chip(ui, m, TEXT_DIM);
                }
            });
        });
    }) {
        ob.first = First::Perf;
    }
    ui.add_space(12.0);
    let found = mdl.importable.len();
    if choice(ui, "import", ob.first == First::Import, |ui| {
        card_title(ui, "📥", t!("Traer las de otro launcher"), &[]);
        indent(ui, |ui| {
            if found == 0 {
                hint(ui, t!("No hemos encontrado otros launchers en este equipo. Puedes elegir la carpeta de una instancia."));
                return;
            }
            if ob.first != First::Import {
                hint(ui, t!("Hemos encontrado instancias en tu equipo:"));
                ui.horizontal_wrapped(|ui| {
                    let mut by: Vec<(&str, usize)> = vec![];
                    for f in &mdl.importable {
                        match by.iter_mut().find(|(s, _)| *s == f.source) {
                            Some(e) => e.1 += 1,
                            None => by.push((f.source, 1)),
                        }
                    }
                    for (src, n) in by {
                        chip(ui, &format!("{src} · {n}"), BLUE);
                    }
                });
                return;
            }
            hint(ui, t!("Marca las que quieras traer (se copian: las originales no se tocan):"));
            egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
                for (i, f) in mdl.importable.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut ob.import_sel[i], RichText::new(&f.name).color(TEXT));
                        ui.label(RichText::new(format!("{} {} · {}", f.loader.label(), f.mc, f.source)).size(12.0).color(TEXT_DIM));
                    });
                }
            });
        });
    }) {
        ob.first = First::Import;
    }
    let chosen = ob.import_sel.iter().filter(|b| **b).count();
    let ready = match ob.first {
        First::Import => found == 0 || chosen > 0,
        _ => !latest.is_empty(),
    };
    if !latest.is_empty() || ob.first == First::Import {
        ui.add_space(0.0);
    } else {
        ui.add_space(8.0);
        hint(ui, t!("Cargando la lista de versiones…"));
    }
    let label = if ob.first == First::Import && found == 0 { t!("ELEGIR CARPETA") } else { t!("SIGUIENTE") };
    let (skip, back, next) = footer(ui, Some(t!("Saltar este paso")), true, label, ready);
    if skip {
        ob.made = None;
        ob.step = 3;
    }
    if back {
        ob.step = 1;
    }
    if next {
        match ob.first {
            First::Import if found == 0 => {
                out.push(Action::ImportFolder);
                ob.imported = 1;
            }
            First::Import => {
                for (i, on) in ob.import_sel.iter().enumerate() {
                    if *on {
                        out.push(Action::ImportFound(i));
                    }
                }
                ob.imported = chosen;
            }
            f => out.push(Action::QuickStart(f == First::Perf)),
        }
        ob.made = Some(ob.first);
        ob.step = 3;
    }
}

// ----------------------------------------------------------------------------- 4. listo

fn row(ui: &mut egui::Ui, k: &str, v: impl FnOnce(&mut egui::Ui), last: bool) {
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(egui::vec2(140.0, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_width(140.0);
            ui.label(RichText::new(k).size(13.5).color(TEXT_DIM));
        });
        v(ui);
    });
    ui.add_space(12.0);
    if !last {
        let r = ui.available_rect_before_wrap();
        ui.painter().hline(r.x_range(), r.top(), egui::Stroke::new(1.0_f32, BORDER.gamma_multiply(0.6)));
    }
}

/// Devuelve `true` cuando hay que cerrar la bienvenida.
fn ready(ui: &mut egui::Ui, mdl: &Model, vw: &mut ViewState, s: &Session, ob: &mut Onboard) -> bool {
    let q = s.quickstart.as_ref();
    let working = s.tasks > 0;
    let failed = q.and_then(|q| q.error.clone());
    let sub = match (ob.made, working) {
        (Some(First::Import), true) => t!("Esto es lo que hemos preparado. Tus instancias se están copiando."),
        (Some(_), true) => t!("Esto es lo que hemos preparado. Minecraft se está descargando en segundo plano."),
        _ => t!("Esto es lo que hemos preparado."),
    };
    header(ui, t!("¡Todo listo!"), sub);
    egui::Frame::none().fill(CARD).rounding(12.0).stroke(egui::Stroke::new(1.0_f32, BORDER)).inner_margin(egui::Margin::symmetric(20.0, 2.0)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        row(
            ui,
            t!("Idioma"),
            |ui| {
                ui.label(RichText::new(if crate::i18n::en() { "English" } else { "Español" }).size(15.0).color(TEXT));
            },
            false,
        );
        row(
            ui,
            t!("Cuenta"),
            |ui| match mdl.accounts.active() {
                Some(a) => {
                    avatar(ui, &a.username, 24.0, if a.kind == AccountKind::Microsoft { GREEN } else { ACCENT });
                    ui.label(RichText::new(&a.username).size(15.0).color(TEXT));
                    chip(ui, &a.label(), TEXT_DIM);
                }
                None => {
                    ui.label(RichText::new(t!("Ninguna todavía: añádela en Cuentas antes de jugar")).size(14.0).color(TEXT_DIM));
                }
            },
            false,
        );
        row(
            ui,
            t!("Instancia"),
            |ui| match (ob.made, q) {
                (Some(First::Import), _) => {
                    ui.label(RichText::new(t!("{n} importadas de otros launchers", n = ob.imported)).size(15.0).color(TEXT));
                }
                (Some(_), Some(q)) => {
                    let name = if q.name.is_empty() { t!("Preparando…").to_string() } else { q.name.clone() };
                    ui.label(RichText::new(name).size(15.0).color(TEXT));
                    chip(ui, if q.perf { "Fabric" } else { "Vanilla" }, if q.perf { ACCENT_HI } else { TEXT_DIM });
                    if q.perf && q.mods > 0 {
                        chip(ui, &t!("{n} mods", n = q.mods), TEXT_DIM);
                    }
                }
                _ => {
                    ui.label(RichText::new(t!("Ninguna: puedes crearla luego en Instancias")).size(14.0).color(TEXT_DIM));
                }
            },
            !working && failed.is_none(),
        );
        if working {
            ui.add_space(12.0);
            let frac = s.progress.filter(|(_, t)| *t > 0).map(|(d, t)| d as f32 / t as f32);
            ui.horizontal(|ui| {
                ui.label(RichText::new(&s.stage).size(13.0).color(TEXT_DIM));
                if let Some(f) = frac {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(format!("{:.0} %", f * 100.0)).size(13.0).semibold().color(ACCENT_HI));
                    });
                }
            });
            ui.add_space(4.0);
            let (r, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 8.0), egui::Sense::hover());
            let p = ui.painter();
            p.rect_filled(r, 4.0, INPUT);
            let f = frac.unwrap_or_else(|| ((ui.input(|i| i.time) * 0.6).fract()) as f32 * 0.2 + 0.05);
            let mut fill = r;
            fill.set_width(r.width() * f.clamp(0.02, 1.0));
            art::gradient_h(p, fill, EMBER, ACCENT);
            ui.add_space(14.0);
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
        }
        if let Some(e) = &failed {
            ui.add_space(10.0);
            ui.label(RichText::new(t!("No se pudo preparar la instancia: {e}", e = e)).size(13.0).color(RED));
            ui.add_space(12.0);
        }
    });
    ui.add_space(12.0);
    hint(ui, t!("Puedes repetir esta bienvenida desde Ajustes → Acerca de."));

    let (primary, enabled) = match ob.made {
        Some(First::Import) => (t!("VER MIS INSTANCIAS"), true),
        Some(_) if failed.is_none() && q.is_some() && mdl.accounts.active().is_some() => (t!("EMPEZAR A JUGAR"), true),
        _ => (t!("IR AL LAUNCHER"), true),
    };
    let skip = (primary != t!("IR AL LAUNCHER")).then_some(t!("Ir al launcher"));
    let (go_launcher, back, next) = footer(ui, skip, !working, primary, enabled);
    if back {
        ob.step = 2;
        return false;
    }
    if go_launcher {
        vw.tab = Tab::Home;
        return true;
    }
    if next {
        match ob.made {
            Some(First::Import) => vw.tab = Tab::Instances,
            Some(_) if primary == t!("EMPEZAR A JUGAR") => {
                vw.tab = Tab::Home;
                vw.play_after_quickstart = true;
            }
            _ => vw.tab = Tab::Home,
        }
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_names_follow_minecraft_rules() {
        assert!(valid_name("Javi_Brasa") && valid_name("abc"));
        assert!(!valid_name("ab") && !valid_name("con espacio") && !valid_name("ñandú") && !valid_name("12345678901234567"));
        // las mismas reglas que al crear la cuenta
        for n in ["Javi_Brasa", "ab", "con espacio", "ñandú"] {
            assert_eq!(valid_name(n), crate::account::auth::Account::offline(n).is_ok(), "{n}");
        }
    }
}
