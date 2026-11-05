//! Pestaña Servidores: crear un servidor, arrancarlo, ver su consola, gestionar jugadores,
//! plugins o mods y darles a los amigos la dirección para entrar.

use crate::gui::action::Action;
use crate::gui::model::{Model, Session};
use crate::gui::state::ViewState;
use crate::gui::view::art::{self, BigKind};
use crate::gui::view::fonts::Weight;
use crate::gui::view::theme::*;
use crate::gui::view::widgets::*;
use crate::hosting::props::Editable;
use crate::hosting::run::{Status, Tunnel};
use crate::hosting::{self, Kind, Server};
use eframe::egui::{self, Color32, RichText};

#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub enum HostTab {
    #[default]
    Console,
    Friends,
    Players,
    Content,
    Settings,
}

/// Formulario de «Nuevo servidor».
#[derive(Clone, Debug)]
pub struct Form {
    pub name: String,
    pub kind: Kind,
    pub version: String,
    /// Dejar entrar también a cuentas no premium.
    pub no_premium: bool,
    pub eula: bool,
}

impl Default for Form {
    fn default() -> Self {
        Form { name: String::new(), kind: Kind::Paper, version: String::new(), no_premium: true, eula: false }
    }
}

/// Borrador de los ajustes de un servidor (se aplica con «Guardar»).
#[derive(Clone, Debug, PartialEq)]
pub struct Draft {
    pub name: String,
    pub memory_mb: u32,
    pub port: u16,
    pub online_mode: bool,
    pub whitelist: bool,
    pub lan_open: bool,
    pub props: Editable,
}

impl Draft {
    pub fn of(p: &crate::paths::Paths, s: &Server) -> Draft {
        let pr = hosting::props::Properties::load(&s.dir(p).join("server.properties"));
        Draft {
            name: s.name.clone(),
            memory_mb: s.memory_mb,
            port: s.port,
            online_mode: s.online_mode,
            whitelist: s.whitelist,
            lan_open: s.lan_open,
            props: Editable::read(&pr),
        }
    }
}

#[derive(Default)]
pub struct HostView {
    pub selected: Option<String>,
    pub creating: bool,
    pub form: Form,
    pub tab: HostTab,
    pub cmd: String,
    pub query: String,
    pub searched_for: Option<String>,
    pub player: String,
    /// (id del servidor, borrador)
    pub draft: Option<(String, Draft)>,
    pub confirm_delete: bool,
    /// Bajar la consola al final en el próximo fotograma.
    pub jump: bool,
    /// Se está cerrando Brasa: esperando a que se apaguen los servidores.
    pub closing: bool,
    /// Servidor de un amigo seleccionado (clave `host/servidor`).
    pub friend_sel: Option<String>,
    /// Pantalla «Unirse a un amigo».
    pub joining: bool,
    pub join_code: String,
    /// Ventana de invitar.
    pub invite_open: bool,
    pub invite_name: String,
    pub invite_hours: u64,
}

/// Cuadro con la inicial del tipo y un punto con el estado (lista y cabecera).
fn server_tile(ui: &mut egui::Ui, kind: Kind, st: Option<Status>, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let p = ui.painter();
    let c = kind_color(kind);
    p.rect_filled(rect, size * 0.22, c.gamma_multiply(0.16));
    p.text(rect.center(), egui::Align2::CENTER_CENTER, &kind.label()[..1], egui::FontId::new(size * 0.45, crate::gui::view::fonts::semibold_family()), c);
    let dot = rect.right_bottom() - egui::vec2(size * 0.06, size * 0.06);
    p.circle_filled(dot, size * 0.13, CARD);
    p.circle_filled(dot, size * 0.1, status_color(st));
}

/// Los errores vienen en minúscula (para encadenarlos): en pantalla, con mayúscula.
fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

fn status_of(id: &str) -> Option<Status> {
    hosting::run::with(id, |l| l.status).flatten()
}

fn status_color(st: Option<Status>) -> Color32 {
    match st {
        Some(Status::Running) => GREEN,
        Some(Status::Preparing | Status::Starting | Status::Stopping) => YELLOW,
        _ => TEXT_DIM.gamma_multiply(0.6),
    }
}

fn status_text(st: Option<Status>) -> &'static str {
    match st {
        Some(Status::Preparing) => t!("Preparando"),
        Some(Status::Starting) => t!("Arrancando"),
        Some(Status::Running) => t!("En marcha"),
        Some(Status::Stopping) => t!("Deteniéndose"),
        _ => t!("Apagado"),
    }
}

fn kind_color(k: Kind) -> Color32 {
    match k {
        Kind::Vanilla => GREEN,
        Kind::Paper => BLUE,
        Kind::Fabric => ACCENT_HI,
        Kind::NeoForge => YELLOW,
        Kind::Forge => RED,
    }
}

fn kind_blurb(k: Kind) -> &'static str {
    match k {
        Kind::Vanilla => t!("Minecraft tal cual, sin nada más."),
        Kind::Paper => t!("Plugins de Bukkit, Spigot y Paper. El más usado."),
        Kind::Fabric => t!("Mods ligeros y de rendimiento."),
        Kind::NeoForge => t!("Modpacks grandes como Create."),
        Kind::Forge => t!("Mods clásicos y versiones antiguas."),
    }
}

/// Aviso a pantalla completa mientras se apagan los servidores al cerrar Brasa.
pub fn closing_overlay(ctx: &egui::Context) {
    let screen = ctx.screen_rect();
    egui::Area::new(egui::Id::new("host_closing")).order(egui::Order::Tooltip).fixed_pos(screen.min).show(ctx, |ui| {
        let (r, _) = ui.allocate_exact_size(screen.size(), egui::Sense::click());
        ui.painter().rect_filled(r, 0.0, Color32::from_black_alpha(200));
        let panel = egui::Rect::from_center_size(r.center(), egui::vec2(460.0, 150.0));
        ui.painter().rect_filled(panel, 14.0, CARD_HI);
        ui.painter().rect_stroke(panel, 14.0, egui::Stroke::new(1.0_f32, BORDER));
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(panel.shrink2(egui::vec2(20.0, 22.0))).layout(egui::Layout::top_down(egui::Align::Center)));
        c.add(egui::Spinner::new().size(28.0).color(ACCENT));
        c.add_space(10.0);
        c.label(RichText::new(t!("Apagando los servidores…")).size(18.0).semibold().color(TEXT));
        c.label(RichText::new(t!("Guardan el mundo antes de cerrar. Brasa se cierra solo al terminar.")).size(13.0).color(TEXT_DIM));
        let names = hosting::run::running_ids().len();
        c.label(RichText::new(t!("Quedan {n}", n = names)).size(12.5).color(TEXT_DIM));
    });
}

// ------------------------------------------------------------------------------ lista

pub fn list(ui: &mut egui::Ui, mdl: &Model, vw: &mut ViewState, _out: &mut Vec<Action>) {
    let h = &mut vw.hosting;
    ui.add_space(14.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(t!("Servidores")).size(17.0).semibold().color(TEXT));
        ui.label(RichText::new(mdl.hosted.len().to_string()).color(TEXT_DIM));
    });
    ui.add_space(4.0);
    if ui.add(primary_button(t!("➕  Nuevo servidor"), ui.available_width(), 36.0)).clicked() {
        h.creating = true;
        h.joining = false;
        h.friend_sel = None;
        h.form = Form::default();
    }
    ui.add_space(4.0);
    let join_btn = egui::Button::new(RichText::new(t!("🔗  Unirse a un amigo")).color(TEXT))
        .fill(CARD_HI)
        .stroke(egui::Stroke::new(1.0_f32, if h.joining { ACCENT } else { BORDER }))
        .rounding(7.0)
        .min_size(egui::vec2(ui.available_width(), 34.0));
    if ui.add(join_btn).on_hover_text(t!("Pega el código que te ha pasado tu amigo")).clicked() {
        h.joining = true;
        h.creating = false;
        h.friend_sel = None;
        _out.push(Action::P2pJoinReset);
    }
    ui.add_space(8.0);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        if mdl.hosted.is_empty() {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| ui.label(RichText::new(t!("Aún no hay servidores")).color(TEXT_DIM)));
        }
        for s in &mdl.hosted {
            let sel = h.selected.as_deref() == Some(s.id.as_str()) && !h.creating && !h.joining && h.friend_sel.is_none();
            let st = status_of(&s.id);
            let players = hosting::run::with(&s.id, |l| l.players.len()).unwrap_or(0);
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 58.0), egui::Sense::click());
            let p = ui.painter();
            if sel || resp.hovered() {
                p.rect_filled(rect, 8.0, if sel { CARD_HI } else { CARD });
            }
            if sel {
                p.rect_filled(egui::Rect::from_min_size(rect.min + egui::vec2(0.0, 12.0), egui::vec2(3.0, rect.height() - 24.0)), 2.0, ACCENT);
            }
            let icon = egui::Rect::from_min_size(rect.min + egui::vec2(12.0, 11.0), egui::vec2(36.0, 36.0));
            p.rect_filled(icon, 8.0, kind_color(s.kind).gamma_multiply(0.16));
            p.text(
                icon.center(),
                egui::Align2::CENTER_CENTER,
                &s.kind.label()[..1],
                egui::FontId::new(17.0, crate::gui::view::fonts::semibold_family()),
                kind_color(s.kind),
            );
            p.circle_filled(icon.right_bottom() - egui::vec2(2.0, 2.0), 5.0, status_color(st));
            p.circle_stroke(icon.right_bottom() - egui::vec2(2.0, 2.0), 5.5, egui::Stroke::new(1.5_f32, if sel { CARD_HI } else { BG }));
            let x = icon.right() + 12.0;
            let name = crate::gui::view::fmt::elide(&s.name, 22);
            p.text(
                egui::pos2(x, rect.top() + 20.0),
                egui::Align2::LEFT_CENTER,
                name,
                egui::FontId::new(14.5, crate::gui::view::fonts::semibold_family()),
                TEXT,
            );
            let sub = match (st, players) {
                (Some(Status::Running), 0) => t!("En marcha · {v}", v = s.describe()),
                (Some(Status::Running), 1) => t!("1 conectado · {v}", v = s.describe()),
                (Some(Status::Running), n) => t!("{n} conectados · {v}", n = n, v = s.describe()),
                _ => s.describe(),
            };
            p.text(egui::pos2(x, rect.top() + 40.0), egui::Align2::LEFT_CENTER, sub, egui::FontId::proportional(12.0), TEXT_DIM);
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                h.selected = Some(s.id.clone());
                h.creating = false;
                h.joining = false;
                h.friend_sel = None;
                h.draft = None;
                h.confirm_delete = false;
                h.jump = true;
            }
        }
        if !mdl.joined.is_empty() {
            ui.add_space(12.0);
            ui.label(RichText::new(t!("DE AMIGOS")).size(11.5).semibold().color(TEXT_DIM));
            ui.add_space(2.0);
        }
        for j in &mdl.joined {
            let key = j.key();
            let sel = h.friend_sel.as_deref() == Some(key.as_str()) && !h.creating && !h.joining;
            let gs = hosting::p2p::guest_state(&key);
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 58.0), egui::Sense::click());
            let p = ui.painter();
            if sel || resp.hovered() {
                p.rect_filled(rect, 8.0, if sel { CARD_HI } else { CARD });
            }
            if sel {
                p.rect_filled(egui::Rect::from_min_size(rect.min + egui::vec2(0.0, 12.0), egui::vec2(3.0, rect.height() - 24.0)), 2.0, ACCENT);
            }
            let icon = egui::Rect::from_min_size(rect.min + egui::vec2(12.0, 11.0), egui::vec2(36.0, 36.0));
            p.rect_filled(icon, 8.0, GREEN.gamma_multiply(0.16));
            let initial: String = j.title.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
            p.text(icon.center(), egui::Align2::CENTER_CENTER, initial, egui::FontId::new(17.0, crate::gui::view::fonts::semibold_family()), GREEN);
            let dot = match gs {
                Some(hosting::p2p::GuestState::Ready { .. }) => GREEN,
                Some(hosting::p2p::GuestState::Connecting) => YELLOW,
                _ => TEXT_DIM.gamma_multiply(0.6),
            };
            p.circle_filled(icon.right_bottom() - egui::vec2(2.0, 2.0), 5.0, dot);
            let x = icon.right() + 12.0;
            p.text(
                egui::pos2(x, rect.top() + 20.0),
                egui::Align2::LEFT_CENTER,
                crate::gui::view::fmt::elide(&j.title, 22),
                egui::FontId::new(14.5, crate::gui::view::fonts::semibold_family()),
                TEXT,
            );
            p.text(
                egui::pos2(x, rect.top() + 40.0),
                egui::Align2::LEFT_CENTER,
                t!("tú: {n} · {v}", n = j.name, v = j.mc),
                egui::FontId::proportional(12.0),
                TEXT_DIM,
            );
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                h.friend_sel = Some(key);
                h.creating = false;
                h.joining = false;
            }
        }
    });
}

// ------------------------------------------------------------------------------ centro

pub fn draw(ui: &mut egui::Ui, mdl: &Model, vw: &mut ViewState, s: &Session, out: &mut Vec<Action>) {
    // sin selección válida: el primero, o el formulario si no hay ninguno
    if !vw.hosting.creating && !mdl.hosted.iter().any(|x| Some(&x.id) == vw.hosting.selected.as_ref()) {
        vw.hosting.selected = mdl.hosted.first().map(|x| x.id.clone());
        vw.hosting.draft = None;
    }
    let server = vw.hosting.selected.as_ref().and_then(|id| mdl.hosted.iter().find(|x| &x.id == id)).cloned();
    let friend = vw.hosting.friend_sel.as_ref().and_then(|k| mdl.joined.iter().find(|j| &j.key() == k)).cloned();
    if vw.hosting.joining {
        join_view(ui, vw, s, out);
    } else if vw.hosting.creating {
        create(ui, mdl, vw, s, out);
    } else if let Some(j) = friend {
        friend_detail(ui, mdl, vw, s, &j, out);
    } else if let Some(srv) = server {
        detail(ui, mdl, vw, s, &srv, out);
        if vw.hosting.invite_open {
            invite_dialog(ui.ctx(), &mut vw.hosting, s, &srv, out);
        }
    } else {
        welcome(ui, vw, out);
    }
    if hosting::run::running_ids().iter().any(|id| status_of(id) != Some(Status::Running)) {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
    }
}

/// Sin servidores todavía: qué es esto y las dos formas de empezar.
fn welcome(ui: &mut egui::Ui, vw: &mut ViewState, out: &mut Vec<Action>) {
    let h = &mut vw.hosting;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.add_space(18.0);
        page_header(ui, t!("Juega con tus amigos"), t!("Crea un servidor en tu equipo o entra al de un amigo. Sin abrir puertos y cifrado de punta a punta."));
        ui.columns(2, |c| {
            if big_tile(&mut c[0], "🖥", t!("Crear un servidor"), t!("Vanilla, con plugins o con mods")) {
                h.creating = true;
                h.joining = false;
                h.form = Form::default();
            }
            if big_tile(&mut c[1], "🔗", t!("Unirme al de un amigo"), t!("Pega el código que te ha pasado")) {
                h.joining = true;
                h.creating = false;
                out.push(Action::P2pJoinReset);
            }
        });
        ui.add_space(14.0);
        card(ui, |ui| {
            section_title(ui, t!("Cómo funciona"));
            for (n, title, text) in [
                ("1", t!("Creas el servidor"), t!("Brasa descarga el software y lo deja listo. Funciona mientras Brasa está abierto.")),
                ("2", t!("Invitas a tus amigos"), t!("Cada uno recibe un código personal y de un solo uso, atado a su nombre de Minecraft.")),
                (
                    "3",
                    t!("Entran desde su Brasa"),
                    t!("Conexión directa entre equipos y, si su red no la deja, por un relé cifrado. Nadie puede hacerse pasar por ellos."),
                ),
            ] {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::hover());
                    ui.painter().circle_filled(r.center(), 13.0, ACCENT.gamma_multiply(0.15));
                    ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, n, egui::FontId::new(13.0, crate::gui::view::fonts::semibold_family()), ACCENT);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(title).semibold().color(TEXT));
                        ui.label(RichText::new(text).size(12.5).color(TEXT_DIM));
                    });
                });
                ui.add_space(6.0);
            }
        });
    });
}

fn create(ui: &mut egui::Ui, _mdl: &Model, vw: &mut ViewState, s: &Session, out: &mut Vec<Action>) {
    let f = &mut vw.hosting.form;
    if f.version.is_empty() {
        f.version = s.latest_release.clone();
    }
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.add_space(18.0);
        page_header(ui, t!("Nuevo servidor"), t!("Corre en este equipo mientras Brasa está abierto. Te damos la dirección para que entren tus amigos."));
        card(ui, |ui| {
            field_label(ui, t!("Nombre"));
            ui.add(egui::TextEdit::singleline(&mut f.name).hint_text(t!("Por ejemplo: Survival con amigos")).desired_width(f32::INFINITY));
            ui.add_space(12.0);
            field_label(ui, t!("Tipo"));
            ui.add_space(2.0);
            let w = ui.available_width();
            for (label, row) in [
                (t!("Sin mods: tus amigos entran con Minecraft normal"), &[Kind::Vanilla, Kind::Paper][..]),
                (t!("Con mods: tus amigos necesitan los mismos (Brasa se los pone)"), &[Kind::Fabric, Kind::NeoForge, Kind::Forge][..]),
            ] {
                ui.label(RichText::new(label).size(12.0).color(TEXT_DIM));
                let cw = (w - 10.0 * (row.len() as f32 - 1.0)) / row.len() as f32;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    for k in row {
                        if kind_card(ui, *k, f.kind == *k, cw) {
                            f.kind = *k;
                        }
                    }
                });
                ui.add_space(8.0);
            }
            ui.add_space(4.0);
            field_label(ui, t!("Versión de Minecraft"));
            let releases: Vec<&str> = s.manifest.iter().filter(|v| v.kind == "release").map(|v| v.id.as_str()).take(40).collect();
            egui::ComboBox::from_id_salt("host_version").selected_text(if f.version.is_empty() { t!("Elige una versión").to_string() } else { f.version.clone() }).width(200.0).show_ui(ui, |ui| {
                for v in &releases {
                    ui.selectable_value(&mut f.version, v.to_string(), *v);
                }
            });
            if releases.is_empty() {
                ui.label(RichText::new(t!("Cargando la lista de versiones…")).size(12.5).color(TEXT_DIM));
            }
            if f.kind != Kind::Vanilla {
                ui.label(RichText::new(t!("Si {k} aún no está para esa versión, te lo diremos al arrancar.", k = f.kind.label())).size(12.0).color(TEXT_DIM));
            }
            ui.add_space(12.0);
            field_label(ui, t!("¿Quién puede entrar?"));
            let mut mode = f.no_premium;
            segmented(ui, &mut mode, &[(true, t!("Premium y no premium")), (false, t!("Sólo premium"))]);
            f.no_premium = mode;
            ui.label(
                RichText::new(if f.no_premium {
                    t!("Sin premium, Minecraft no comprueba las cuentas: cualquiera podría entrar con el nombre de otro. Por eso la lista blanca va activada: sólo entran los que añadas.")
                } else {
                    t!("Sólo cuentas de Microsoft con Minecraft comprado.")
                })
                .size(12.5)
                .color(if f.no_premium { YELLOW } else { TEXT_DIM }),
            );
            ui.add_space(12.0);
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut f.eula, "");
                ui.label(RichText::new(t!("Acepto el")).color(TEXT));
                ui.hyperlink_to(t!("EULA de Minecraft"), "https://aka.ms/MinecraftEULA");
                ui.label(RichText::new(t!("(obligatorio para cualquier servidor)")).size(12.5).color(TEXT_DIM));
            });
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                let ok = !f.name.trim().is_empty() && !f.version.is_empty() && f.eula;
                if ui.add_enabled(ok, primary_button(t!("Crear servidor"), 160.0, 38.0)).clicked() {
                    out.push(Action::HostCreate);
                }
                if ui.add(ghost_button(t!("Cancelar"))).clicked() {
                    vw.hosting.creating = false;
                }
                if !ok {
                    let why = if f.name.trim().is_empty() {
                        t!("Falta el nombre")
                    } else if !f.eula {
                        t!("Falta aceptar el EULA")
                    } else {
                        t!("Falta la versión")
                    };
                    ui.label(RichText::new(why).size(12.5).color(TEXT_DIM));
                }
            });
        });
        ui.add_space(20.0);
    });
}

fn kind_card(ui: &mut egui::Ui, k: Kind, sel: bool, w: f32) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 74.0), egui::Sense::click());
    let p = ui.painter();
    let c = kind_color(k);
    let hot = resp.hovered();
    p.rect_filled(
        rect,
        10.0,
        if sel {
            art::mix(CARD, c, 0.10)
        } else if hot {
            CARD_HI
        } else {
            INPUT
        },
    );
    p.rect_stroke(rect, 10.0, egui::Stroke::new(1.5_f32, if sel { c } else { BORDER }));
    p.text(
        rect.min + egui::vec2(14.0, 22.0),
        egui::Align2::LEFT_CENTER,
        k.label(),
        egui::FontId::new(15.5, crate::gui::view::fonts::semibold_family()),
        if sel { c } else { TEXT },
    );
    let g = p.layout(kind_blurb(k).to_string(), egui::FontId::proportional(12.0), TEXT_DIM, w - 28.0);
    p.galley(rect.min + egui::vec2(14.0, 36.0), g, TEXT_DIM);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

fn detail(ui: &mut egui::Ui, mdl: &Model, vw: &mut ViewState, s: &Session, srv: &Server, out: &mut Vec<Action>) {
    let st = status_of(&srv.id);
    let up = matches!(st, Some(Status::Preparing | Status::Starting | Status::Running | Status::Stopping));
    let (players, error, tunnel) = hosting::run::with(&srv.id, |l| (l.players.clone(), l.error.clone(), l.tunnel.clone())).unwrap_or_default();
    ui.add_space(16.0);
    // ---- cabecera: estado, botón grande y, debajo, la franja de amigos
    egui::Frame::none().fill(CARD).rounding(12.0).stroke(egui::Stroke::new(1.0_f32, BORDER.gamma_multiply(0.6))).inner_margin(egui::Margin::symmetric(16.0, 14.0)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let narrow = ui.available_width() < 640.0;
        ui.horizontal(|ui| {
            server_tile(ui, srv.kind, st, 48.0);
            ui.add_space(6.0);
            let text_w = (ui.available_width() - if narrow { 200.0 } else { 330.0 }).max(140.0);
            ui.vertical(|ui| {
                ui.set_max_width(text_w);
                ui.spacing_mut().item_spacing.y = 3.0;
                ui.add(egui::Label::new(RichText::new(&srv.name).size(22.0).display().color(TEXT)).truncate());
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let running = st == Some(Status::Running);
                    let label = match players.len() {
                        n if running && n > 1 => t!("{} · {n} conectados", status_text(st), n = n),
                        1 if running => t!("{} · 1 conectado", status_text(st)),
                        _ => status_text(st).to_string(),
                    };
                    ui.label(RichText::new(label).size(13.0).color(if running { GREEN } else { status_color(st) }));
                    ui.label(RichText::new("·").size(13.0).color(TEXT_DIM));
                    ui.add(egui::Label::new(RichText::new(srv.describe()).size(13.0).color(TEXT_DIM)).truncate())
                        .on_hover_text(if srv.online_mode { t!("Sólo premium") } else { t!("Premium y no premium") });
                });
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let size = egui::vec2(150.0, 46.0);
                match st {
                    Some(Status::Running) | Some(Status::Starting) => {
                        if art::big_button(ui, BigKind::Stop, t!("DETENER"), size, true).on_hover_text(t!("Guarda el mundo y apaga el servidor")).clicked() {
                            out.push(Action::HostStop(srv.id.clone()));
                        }
                    }
                    Some(Status::Preparing) => {
                        art::progress_button(ui, t!("PREPARANDO"), None, size);
                    }
                    Some(Status::Stopping) => {
                        art::progress_button(ui, t!("APAGANDO"), None, size);
                        if ui.add(ghost_button(t!("Forzar cierre"))).on_hover_text(t!("Cierra sin guardar: sólo si no responde")).clicked() {
                            out.push(Action::HostKill(srv.id.clone()));
                        }
                    }
                    _ => {
                        if art::big_button(ui, BigKind::Play, t!("INICIAR"), size, srv.eula_accepted).clicked() {
                            out.push(Action::HostStart(srv.id.clone()));
                            vw.hosting.tab = HostTab::Console;
                            vw.hosting.jump = true;
                        }
                    }
                }
                ui.add_space(6.0);
                let join_tip = if srv.kind.mods() {
                    t!("Crea una instancia con la misma versión y los mods que hacen falta, con el servidor en su lista. Para tus amigos: expórtala (.mrpack) y pásasela.")
                } else {
                    t!("Crea una instancia con la misma versión y el servidor ya en su lista")
                };
                let label = if narrow { t!("🎮  Jugar") } else { t!("🎮  Jugar aquí") };
                if ui.add(secondary_button(label).min_size(egui::vec2(0.0, 36.0))).on_hover_text(join_tip).clicked() {
                    out.push(Action::HostJoin(srv.id.clone()));
                }
            });
        });
        if let Some(e) = error.filter(|_| !up || st == Some(Status::Starting)) {
            ui.add_space(8.0);
            ui.label(RichText::new(capitalize(&e)).size(13.0).color(RED));
        }
        // franja de amigos
        ui.add_space(10.0);
        let r = ui.available_rect_before_wrap();
        ui.painter().hline(r.x_range(), r.top(), egui::Stroke::new(1.0_f32, BORDER.gamma_multiply(0.6)));
        ui.add_space(10.0);
        friends_strip(ui, mdl, &mut vw.hosting, srv, out);
    });
    ui.add_space(8.0);
    // ---- pestañas
    let content_n = hosting::content::installed(&mdl.p, srv).len();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let h = &mut vw.hosting;
        if sub_tab(ui, t!("Consola"), None, h.tab == HostTab::Console) {
            h.tab = HostTab::Console;
            h.jump = true;
        }
        if sub_tab(ui, t!("Amigos"), Some(srv.friends.len()), h.tab == HostTab::Friends) {
            h.tab = HostTab::Friends;
        }
        if sub_tab(ui, t!("Jugadores"), Some(players.len()), h.tab == HostTab::Players) {
            h.tab = HostTab::Players;
        }
        if srv.kind.content_dir().is_some() {
            let label = if srv.kind.plugins() { t!("Plugins") } else { t!("Mods") };
            if sub_tab(ui, label, Some(content_n), h.tab == HostTab::Content) {
                h.tab = HostTab::Content;
            }
        }
        if sub_tab(ui, t!("Ajustes"), None, h.tab == HostTab::Settings) {
            h.tab = HostTab::Settings;
        }
    });
    ui.add_space(6.0);
    match vw.hosting.tab {
        HostTab::Console => console(ui, vw, srv, st, out),
        HostTab::Players => players_tab(ui, mdl, vw, srv, st, &players, out),
        HostTab::Friends => friends_tab(ui, mdl, srv, st, &tunnel, s, out),
        HostTab::Content if srv.kind.content_dir().is_some() => content_tab(ui, mdl, vw, s, srv, up, out),
        _ => settings_tab(ui, mdl, vw, srv, up, out),
    }
}

/// Franja de la cabecera: Brasa P2P de un vistazo y el botón de invitar.
fn friends_strip(ui: &mut egui::Ui, mdl: &Model, h: &mut HostView, srv: &Server, out: &mut Vec<Action>) {
    let online = hosting::p2p::peers().iter().filter(|p| p.server == srv.id).count();
    ui.horizontal(|ui| {
        ui.label(RichText::new("🔒").size(14.0).color(ACCENT_HI));
        ui.label(RichText::new(t!("Amigos con Brasa")).size(14.0).semibold().color(TEXT));
        let mut txt = match srv.friends.len() {
            0 => t!("nadie emparejado todavía").to_string(),
            1 => t!("1 emparejado").to_string(),
            n => t!("{n} emparejados", n = n),
        };
        if online > 0 {
            txt.push_str(" · ");
            txt.push_str(&if online == 1 { t!("1 conectado").to_string() } else { t!("{n} conectados", n = online) });
        }
        if mdl.settings.p2p_hide_ip {
            txt.push_str(" · ");
            txt.push_str(t!("IP oculta"));
        }
        ui.add(egui::Label::new(RichText::new(txt).size(13.0).color(if online > 0 { GREEN } else { TEXT_DIM })).truncate());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(primary_button(t!("+  Invitar amigo"), 0.0, 32.0))
                .on_hover_text(t!("Código personal y de un solo uso: sin abrir puertos, cifrado de punta a punta"))
                .clicked()
            {
                h.invite_open = true;
                h.invite_name.clear();
                if h.invite_hours == 0 {
                    h.invite_hours = 24;
                }
                out.push(Action::P2pInviteReset);
            }
            if ui.add(ghost_button(t!("Ver amigos"))).clicked() {
                h.tab = HostTab::Friends;
            }
        });
    });
}

/// Otras formas de entrar (pestaña Amigos): red de casa sin Brasa y playit.gg.
fn other_ways(ui: &mut egui::Ui, mdl: &Model, srv: &Server, st: Option<Status>, tunnel: &Tunnel, s: &Session, out: &mut Vec<Action>) {
    card(ui, |ui| {
        section_title(ui, t!("Otras formas de entrar"));
        let addr_row = |ui: &mut egui::Ui, label: &str, addr: &str, out: &mut Vec<Action>| {
            ui.horizontal(|ui| {
                key_label(ui, label);
                ui.label(RichText::new(addr).monospace().size(14.5).color(ACCENT_HI));
                if ui.add(ghost_button(t!("📋 Copiar"))).clicked() {
                    out.push(Action::Copy(addr.to_string(), t!("Dirección copiada: pásasela a tus amigos").into()));
                }
            });
        };
        if srv.lan_open {
            ui.add_space(4.0);
            match hosting::lan_ip() {
                Some(ip) => addr_row(ui, t!("En tu casa, sin Brasa"), &hosting::address(&ip, srv.port), out),
                None => {
                    ui.label(RichText::new(t!("Sin red local detectada")).size(13.0).color(TEXT_DIM));
                }
            }
        }
        if !srv.lan_open {
            ui.horizontal_wrapped(|ui| {
                key_label(ui, t!("En tu casa, sin Brasa"));
                ui.label(RichText::new(t!("Desactivado: actívalo en Ajustes → Acceso si alguien no usa Brasa.")).size(13.0).color(TEXT_DIM));
            });
        }
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            key_label(ui, t!("Plan B: playit.gg"));
            if let Some(url) = &s.playit_claim {
                ui.add(egui::Spinner::new().size(14.0).color(ACCENT));
                ui.label(RichText::new(t!("Apruébalo en la página de playit.gg que se ha abierto")).size(13.0).color(TEXT));
                if ui.add(ghost_button(t!("Abrir otra vez"))).clicked() {
                    out.push(Action::OpenUrl(url.clone()));
                }
                if ui.add(ghost_button(t!("Cancelar"))).clicked() {
                    out.push(Action::PlayitCancel);
                }
                return;
            }
            if !mdl.playit_linked {
                ui.label(RichText::new(t!("Con playit.gg (gratis): sin tocar el router.")).size(13.0).color(TEXT_DIM));
                if ui
                    .add(secondary_button(t!("Enlazar playit.gg")))
                    .on_hover_text(t!("Abre playit.gg para crear tu cuenta o entrar, y autorizar a Brasa"))
                    .clicked()
                {
                    out.push(Action::PlayitLink);
                }
                return;
            }
            match tunnel {
                Tunnel::Ready(a) => {
                    ui.label(RichText::new(a).monospace().size(14.5).color(GREEN));
                    if ui.add(ghost_button(t!("📋 Copiar"))).clicked() {
                        out.push(Action::Copy(a.clone(), t!("Dirección copiada: pásasela a tus amigos").into()));
                    }
                }
                Tunnel::Connecting(m) => {
                    ui.add(egui::Spinner::new().size(14.0).color(ACCENT));
                    ui.label(RichText::new(m).size(13.0).color(TEXT_DIM));
                }
                Tunnel::Manual { port, why } => {
                ui.add(egui::Spinner::new().size(14.0).color(ACCENT));
                ui.label(
                    RichText::new(t!("Crea el túnel en playit.gg: «Add Tunnel» → Minecraft Java → este agente, dirección local 127.0.0.1 y puerto {p}. Brasa lo detecta solo.", p = port))
                        .size(13.0)
                        .color(TEXT),
                )
                .on_hover_text(why);
                if ui.add(secondary_button(t!("Abrir playit.gg"))).clicked() {
                    out.push(Action::OpenUrl("https://playit.gg/account/tunnels".into()));
                }
            }
            Tunnel::Failed(e) => {
                    ui.label(RichText::new(e).size(13.0).color(RED));
                }
                Tunnel::Off => {
                    let on = srv.playit;
                    if toggle_switch(ui, on).clicked() {
                        out.push(Action::HostSetPlayit(srv.id.clone(), !on));
                    }
                    let txt = match (on, st) {
                        (true, Some(Status::Running | Status::Starting)) => t!("Se activa al reiniciar el servidor"),
                        (true, _) => t!("Túnel de playit.gg al iniciar"),
                        (false, _) => t!("Túnel de playit.gg desactivado"),
                    };
                    ui.label(RichText::new(txt).size(13.0).color(if on { TEXT } else { TEXT_DIM }));
                    if ui.add(ghost_button(t!("Desenlazar"))).on_hover_text(t!("Olvida la clave de playit.gg en este equipo")).clicked() {
                        out.push(Action::PlayitUnlink);
                    }
                }
            }
        });
    });
}

/// Etiqueta de ancho fijo, alineada a la izquierda.
fn key_label(ui: &mut egui::Ui, t: &str) {
    ui.allocate_ui_with_layout(egui::vec2(150.0, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_width(150.0);
        ui.label(RichText::new(t).size(13.0).color(TEXT_DIM));
    });
}

fn line_color(l: &str) -> Color32 {
    if l.starts_with("==>") {
        ACCENT_HI
    } else if l.starts_with("> ") {
        BLUE
    } else if l.starts_with("[error]") || l.contains("/ERROR]") || l.contains(" ERROR]") || l.contains("SEVERE") || l.contains("Exception") {
        RED
    } else if l.contains("/WARN]") || l.contains(" WARN]") {
        YELLOW
    } else {
        TEXT_DIM
    }
}

fn console(ui: &mut egui::Ui, vw: &mut ViewState, srv: &Server, st: Option<Status>, out: &mut Vec<Action>) {
    let h = &mut vw.hosting;
    let input_h = 44.0;
    let avail = (ui.available_height() - input_h - 8.0).max(160.0);
    egui::Frame::none().fill(INPUT).rounding(10.0).stroke(egui::Stroke::new(1.0_f32, BORDER.gamma_multiply(0.6))).inner_margin(egui::Margin::same(10.0)).show(
        ui,
        |ui| {
            ui.set_width(ui.available_width());
            let rows = hosting::run::with(&srv.id, |l| l.lines.len()).unwrap_or(0);
            let row_h = 17.0;
            let mut sa = egui::ScrollArea::vertical().max_height(avail).auto_shrink([false, false]).stick_to_bottom(true);
            if std::mem::take(&mut h.jump) {
                sa = sa.vertical_scroll_offset(rows as f32 * (row_h + 4.0));
            }
            sa.show_rows(ui, row_h, rows.max(1), |ui, range| {
                if rows == 0 {
                    ui.label(RichText::new(t!("La consola del servidor sale aquí al iniciarlo.")).color(TEXT_DIM));
                    return;
                }
                hosting::run::with(&srv.id, |l| {
                    for i in range {
                        if let Some(line) = l.lines.get(i) {
                            ui.add(egui::Label::new(RichText::new(line).monospace().size(12.0).color(line_color(line))).truncate());
                        }
                    }
                });
            });
        },
    );
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        let running = matches!(st, Some(Status::Running | Status::Starting));
        let w = ui.available_width() - 110.0;
        let r = ui.add_enabled(
            running,
            egui::TextEdit::singleline(&mut h.cmd)
                .hint_text(if running { t!("Escribe un comando (op, whitelist add, say…)") } else { t!("Inicia el servidor para escribir comandos") })
                .desired_width(w),
        );
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (ui.add_enabled(running && !h.cmd.trim().is_empty(), primary_button(t!("Enviar"), 96.0, 32.0)).clicked() || enter) && !h.cmd.trim().is_empty() {
            out.push(Action::HostCommand(srv.id.clone(), std::mem::take(&mut h.cmd)));
            h.jump = true;
            r.request_focus();
        }
    });
}

fn read_names(path: &std::path::Path) -> Vec<String> {
    let v: serde_json::Value = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let mut names: Vec<String> = v.as_array().map(|a| a.iter().filter_map(|x| x["name"].as_str().map(String::from)).collect()).unwrap_or_default();
    names.sort_by_key(|n| n.to_lowercase());
    names
}

fn players_tab(ui: &mut egui::Ui, mdl: &Model, vw: &mut ViewState, srv: &Server, st: Option<Status>, online: &[String], out: &mut Vec<Action>) {
    let running = st == Some(Status::Running);
    let dir = srv.dir(&mdl.p);
    let wl = read_names(&dir.join("whitelist.json"));
    let ops = read_names(&dir.join("ops.json"));
    let h = &mut vw.hosting;
    let cmd = |out: &mut Vec<Action>, c: String| out.push(Action::HostCommand(srv.id.clone(), c));
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        if !running {
            ui.label(RichText::new(t!("Inicia el servidor para añadir o quitar jugadores: los cambios se hacen con sus comandos.")).size(13.0).color(TEXT_DIM));
            ui.add_space(6.0);
        }
        card(ui, |ui| {
            section_title(ui, &t!("Conectados ({n})", n = online.len()));
            if online.is_empty() {
                ui.label(RichText::new(t!("Nadie ahora mismo")).color(TEXT_DIM));
            }
            for p in online {
                ui.horizontal(|ui| {
                    avatar(ui, p, 24.0, GREEN);
                    ui.label(RichText::new(p).color(TEXT));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(ghost_button(t!("Expulsar"))).clicked() {
                            cmd(out, format!("kick {p}"));
                        }
                        if !ops.contains(p) && ui.add(ghost_button(t!("Hacer operador"))).on_hover_text(t!("Puede usar todos los comandos")).clicked() {
                            cmd(out, format!("op {p}"));
                        }
                    });
                });
            }
        });
        ui.add_space(8.0);
        card(ui, |ui| {
            section_title(ui, &t!("Lista blanca ({n})", n = wl.len()));
            ui.label(
                RichText::new(if srv.whitelist {
                    t!("Activada: sólo entran estos jugadores.")
                } else {
                    t!("Desactivada: entra cualquiera. Actívala en Ajustes.")
                })
                .size(12.5)
                .color(if srv.whitelist { TEXT_DIM } else { YELLOW }),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let ok = hosting::valid_player(h.player.trim());
                let r = ui.add_enabled(running, egui::TextEdit::singleline(&mut h.player).hint_text(t!("Nombre en Minecraft")).desired_width(220.0));
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if (ui.add_enabled(running && ok, secondary_button(t!("Añadir"))).clicked() || (enter && ok)) && running {
                    cmd(out, format!("whitelist add {}", h.player.trim()));
                    h.player.clear();
                }
                if !h.player.trim().is_empty() && !ok {
                    ui.label(RichText::new(t!("3 a 16 letras, números o _")).size(12.5).color(RED));
                }
            });
            ui.add_space(4.0);
            for p in &wl {
                ui.horizontal(|ui| {
                    avatar(ui, p, 22.0, ACCENT);
                    ui.label(RichText::new(p).color(TEXT));
                    if ops.contains(p) {
                        chip(ui, t!("Operador"), ACCENT_HI);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_enabled(running, ghost_button(t!("Quitar"))).clicked() {
                            cmd(out, format!("whitelist remove {p}"));
                        }
                    });
                });
            }
        });
        if !ops.is_empty() {
            ui.add_space(8.0);
            card(ui, |ui| {
                section_title(ui, &t!("Operadores ({n})", n = ops.len()));
                for p in &ops {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(p).color(TEXT));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add_enabled(running, ghost_button(t!("Quitar operador"))).clicked() {
                                cmd(out, format!("deop {p}"));
                            }
                        });
                    });
                }
            });
        }
    });
}

fn content_tab(ui: &mut egui::Ui, mdl: &Model, vw: &mut ViewState, s: &Session, srv: &Server, up: bool, out: &mut Vec<Action>) {
    let h = &mut vw.hosting;
    let items = hosting::content::installed(&mdl.p, srv);
    let what = if srv.kind.plugins() { t!("plugins") } else { t!("mods") };
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        card(ui, |ui| {
            section_title(ui, &t!("Instalados ({n})", n = items.len()));
            if items.is_empty() {
                ui.label(RichText::new(t!("Ninguno todavía: búscalos aquí abajo.")).color(TEXT_DIM));
            }
            if up && !items.is_empty() {
                ui.label(RichText::new(t!("Los cambios se notan al reiniciar el servidor.")).size(12.5).color(TEXT_DIM));
            }
            for c in &items {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&c.title).color(TEXT));
                    ui.label(RichText::new(&c.file).size(12.0).color(TEXT_DIM));
                    if hosting::content::for_players(srv, c) {
                        chip(ui, t!("También en los jugadores"), BLUE);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_enabled(!up, ghost_button(t!("Quitar"))).on_disabled_hover_text(t!("Detén el servidor para quitarlo")).clicked() {
                            out.push(Action::HostRemove(srv.id.clone(), c.file.clone()));
                        }
                    });
                });
            }
        });
        ui.add_space(10.0);
        section_title(ui, &t!("Buscar {what} en Modrinth", what = what));
        ui.horizontal(|ui| {
            let r = ui.add(
                egui::TextEdit::singleline(&mut h.query)
                    .hint_text(if srv.kind.plugins() { t!("LuckPerms, EssentialsX, WorldEdit…") } else { t!("Lithium, Chunky, Create…") })
                    .desired_width(ui.available_width() - 110.0),
            );
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.add(primary_button(t!("Buscar"), 96.0, 32.0)).clicked() || enter {
                out.push(Action::HostSearch(srv.id.clone(), h.query.clone()));
                h.searched_for = Some(srv.id.clone());
            }
        });
        // la primera vez, los más descargados
        if h.searched_for.as_deref() != Some(srv.id.as_str()) {
            h.searched_for = Some(srv.id.clone());
            out.push(Action::HostSearch(srv.id.clone(), String::new()));
        }
        ui.add_space(8.0);
        if s.host_searching {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().color(ACCENT));
                ui.label(RichText::new(t!("Buscando…")).color(TEXT_DIM));
            });
        }
        let have: Vec<&str> = srv.content.iter().map(|c| c.project_id.as_str()).collect();
        for hit in &s.host_hits {
            let installed = have.contains(&hit.project_id.as_str());
            let label = if installed { t!("Actualizar") } else { t!("Instalar") };
            if result_card(
                ui,
                hit.icon_url.clone(),
                &hit.title,
                &hit.author,
                &hit.description,
                hit.downloads,
                Some(format!("https://modrinth.com/project/{}", hit.slug)),
                label,
            ) {
                out.push(Action::HostInstall(srv.id.clone(), hit.project_id.clone()));
            }
        }
    });
}

fn settings_tab(ui: &mut egui::Ui, mdl: &Model, vw: &mut ViewState, srv: &Server, up: bool, out: &mut Vec<Action>) {
    let h = &mut vw.hosting;
    if h.draft.as_ref().map(|(id, _)| id != &srv.id).unwrap_or(true) {
        h.draft = Some((srv.id.clone(), Draft::of(&mdl.p, srv)));
    }
    let original = Draft::of(&mdl.p, srv);
    let Some((_, d)) = h.draft.as_mut() else { return };
    let ram = mdl.total_ram_mb.unwrap_or(8192) as u32;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        card(ui, |ui| {
            section_title(ui, t!("General"));
            egui::Grid::new("host_general").num_columns(2).spacing([18.0, 10.0]).show(ui, |ui| {
                grid_label(ui, t!("Nombre"));
                ui.add(egui::TextEdit::singleline(&mut d.name).desired_width(280.0));
                ui.end_row();
                grid_label(ui, t!("Mensaje en la lista (MOTD)"));
                ui.add(egui::TextEdit::singleline(&mut d.props.motd).desired_width(280.0));
                ui.end_row();
                grid_label(ui, t!("Memoria"));
                ui.horizontal(|ui| {
                    ui.add(egui::Slider::new(&mut d.memory_mb, 1024..=ram.clamp(2048, 32768)).step_by(512.0).suffix(" MB"));
                });
                ui.end_row();
                grid_label(ui, t!("Puerto"));
                ui.add(egui::DragValue::new(&mut d.port).range(1024..=65535));
                ui.end_row();
                grid_label(ui, t!("Máximo de jugadores"));
                ui.add(egui::DragValue::new(&mut d.props.max_players).range(1..=1000));
                ui.end_row();
            });
        });
        ui.add_space(8.0);
        card(ui, |ui| {
            section_title(ui, t!("Acceso"));
            let mut no_premium = !d.online_mode;
            segmented(ui, &mut no_premium, &[(true, t!("Premium y no premium")), (false, t!("Sólo premium"))]);
            d.online_mode = !no_premium;
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if toggle_switch(ui, d.whitelist).clicked() {
                    d.whitelist = !d.whitelist;
                }
                ui.label(RichText::new(t!("Lista blanca: sólo entran los jugadores que añadas")).color(TEXT));
            });
            ui.horizontal(|ui| {
                if toggle_switch(ui, d.lan_open).clicked() {
                    d.lan_open = !d.lan_open;
                }
                ui.label(RichText::new(t!("Abierto a tu red de casa sin Brasa")).color(TEXT));
            });
            ui.label(
                RichText::new(if d.lan_open {
                    t!("Cualquiera en tu wifi puede conectarse a tu IP local, sin invitación.")
                } else {
                    t!("Recomendado apagado: el servidor sólo escucha en este equipo y se entra con Brasa (invitación y cifrado).")
                })
                .size(12.5)
                .color(if d.lan_open { YELLOW } else { TEXT_DIM }),
            );
            if !d.online_mode && !d.whitelist {
                ui.label(
                    RichText::new(t!("Sin premium y sin lista blanca, cualquiera que tenga la dirección puede entrar con cualquier nombre (también el tuyo)."))
                        .size(12.5)
                        .color(YELLOW),
                );
            }
        });
        ui.add_space(8.0);
        card(ui, |ui| {
            section_title(ui, t!("Partida"));
            egui::Grid::new("host_game").num_columns(2).spacing([18.0, 10.0]).show(ui, |ui| {
                grid_label(ui, t!("Modo de juego"));
                egui::ComboBox::from_id_salt("host_gm").selected_text(gamemode_label(&d.props.gamemode)).show_ui(ui, |ui| {
                    for g in ["survival", "creative", "adventure", "spectator"] {
                        ui.selectable_value(&mut d.props.gamemode, g.to_string(), gamemode_label(g));
                    }
                });
                ui.end_row();
                grid_label(ui, t!("Dificultad"));
                egui::ComboBox::from_id_salt("host_diff").selected_text(difficulty_label(&d.props.difficulty)).show_ui(ui, |ui| {
                    for g in ["peaceful", "easy", "normal", "hard"] {
                        ui.selectable_value(&mut d.props.difficulty, g.to_string(), difficulty_label(g));
                    }
                });
                ui.end_row();
                grid_label(ui, t!("PvP"));
                if toggle_switch(ui, d.props.pvp).clicked() {
                    d.props.pvp = !d.props.pvp;
                }
                ui.end_row();
                grid_label(ui, t!("Extremo (hardcore)"));
                if toggle_switch(ui, d.props.hardcore).clicked() {
                    d.props.hardcore = !d.props.hardcore;
                }
                ui.end_row();
                grid_label(ui, t!("Permitir volar"));
                if toggle_switch(ui, d.props.allow_flight).clicked() {
                    d.props.allow_flight = !d.props.allow_flight;
                }
                ui.end_row();
                grid_label(ui, t!("Distancia de visión"));
                ui.add(egui::Slider::new(&mut d.props.view_distance, 2..=32).suffix(" chunks"));
                ui.end_row();
                grid_label(ui, t!("Semilla del mundo"));
                ui.add(egui::TextEdit::singleline(&mut d.props.seed).hint_text(t!("Vacía: aleatoria")).desired_width(220.0));
                ui.end_row();
            });
            ui.label(RichText::new(t!("La semilla sólo cuenta al crear el mundo (el primer arranque).")).size(12.0).color(TEXT_DIM));
        });
        ui.add_space(10.0);
        let changed = *d != original;
        ui.horizontal(|ui| {
            if ui.add_enabled(changed, primary_button(t!("Guardar"), 120.0, 36.0)).clicked() {
                out.push(Action::HostSave(srv.id.clone(), d.clone()));
            }
            if changed && ui.add(ghost_button(t!("Descartar"))).clicked() {
                *d = original.clone();
            }
            if up && changed {
                ui.label(RichText::new(t!("Se aplica al reiniciar el servidor")).size(12.5).color(TEXT_DIM));
            }
        });
        ui.add_space(14.0);
        card(ui, |ui| {
            section_title(ui, t!("Carpeta y borrado"));
            ui.horizontal_wrapped(|ui| {
                if ui.add(secondary_button(t!("📂  Abrir carpeta"))).on_hover_text(t!("Mundo, plugins, mods y configuración")).clicked() {
                    out.push(Action::OpenDir(srv.dir(&mdl.p)));
                }
                if !h.confirm_delete {
                    if ui.add_enabled(!up, danger_ghost_button(t!("Borrar servidor"))).on_disabled_hover_text(t!("Detén el servidor antes")).clicked() {
                        h.confirm_delete = true;
                    }
                } else {
                    ui.label(RichText::new(t!("Se borra todo, mundo incluido. ¿Seguro?")).color(RED));
                    if ui.add(danger_ghost_button(t!("Sí, borrar"))).clicked() {
                        out.push(Action::HostDelete(srv.id.clone()));
                        h.confirm_delete = false;
                    }
                    if ui.add(ghost_button(t!("No"))).clicked() {
                        h.confirm_delete = false;
                    }
                }
            });
        });
        ui.add_space(20.0);
    });
}

fn gamemode_label(g: &str) -> &'static str {
    match g {
        "creative" => t!("Creativo"),
        "adventure" => t!("Aventura"),
        "spectator" => t!("Espectador"),
        _ => t!("Supervivencia"),
    }
}

fn difficulty_label(g: &str) -> &'static str {
    match g {
        "peaceful" => t!("Pacífico"),
        "normal" => t!("Normal"),
        "hard" => t!("Difícil"),
        _ => t!("Fácil"),
    }
}

// ------------------------------------------------------------------------------ Brasa P2P

fn ago(secs: u64) -> String {
    let d = crate::config::now_secs().saturating_sub(secs);
    if d < 3600 {
        t!("hace {n} min", n = (d / 60).max(1))
    } else if d < 86400 {
        t!("hace {n} h", n = d / 3600)
    } else {
        t!("hace {n} días", n = d / 86400)
    }
}

#[allow(clippy::too_many_arguments)]
fn friends_tab(ui: &mut egui::Ui, mdl: &Model, srv: &Server, st: Option<Status>, tunnel: &Tunnel, s: &Session, out: &mut Vec<Action>) {
    let live = hosting::p2p::peers();
    let now = crate::config::now_secs();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if toggle_switch(ui, srv.gatekeeper).clicked() {
                    out.push(Action::HostSetGatekeeper(srv.id.clone(), !srv.gatekeeper));
                }
                ui.label(RichText::new(t!("Portero")).color(TEXT)).on_hover_text(t!("Te pregunta antes de emparejar a alguien con una invitación"));
                ui.add_space(18.0);
                let hide = mdl.settings.p2p_hide_ip;
                if toggle_switch(ui, hide).clicked() {
                    out.push(Action::SetHideIp(!hide));
                }
                ui.label(RichText::new(t!("Ocultar mi IP")).color(TEXT)).on_hover_text(t!("Todo pasa por el relé cifrado: nadie ve tu IP. Algo más lento. Vale para todos tus servidores."));
            });
            ui.label(
                RichText::new(t!("Cada amigo entra con un código personal. Su equipo queda atado a su nombre: nadie puede entrar haciéndose pasar por él, aunque el servidor sea no premium."))
                    .size(12.5)
                    .color(TEXT_DIM),
            );
        });
        ui.add_space(8.0);
        card(ui, |ui| {
            section_title(ui, &t!("Emparejados ({n})", n = srv.friends.len()));
            if srv.friends.is_empty() {
                ui.label(RichText::new(t!("Nadie todavía: pulsa «Invitar amigo».")).color(TEXT_DIM));
            }
            for f in &srv.friends {
                ui.horizontal(|ui| {
                    let on = live.iter().find(|p| p.server == srv.id && p.name == f.name);
                    avatar(ui, &f.name, 26.0, if on.is_some() { GREEN } else { TEXT_DIM });
                    ui.allocate_ui_with_layout(egui::vec2(140.0, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.set_min_width(140.0);
                        ui.label(RichText::new(&f.name).semibold().color(TEXT)).on_hover_text(t!("equipo {k} · {a}", k = hosting::p2p::short(&f.id), a = ago(f.paired)));
                    });
                    match on {
                        Some(p) if p.direct => {
                            ui.label(RichText::new(t!("• conexión directa · {ms} ms", ms = p.rtt_ms)).size(13.0).color(GREEN));
                        }
                        Some(p) => {
                            ui.label(RichText::new(t!("• por relé · {ms} ms", ms = p.rtt_ms)).size(13.0).color(BLUE));
                        }
                        None => {
                            ui.label(RichText::new(t!("desconectado")).size(13.0).color(TEXT_DIM));
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(ghost_button(t!("Revocar"))).on_hover_text(t!("Deja de poder entrar al momento")).clicked() {
                            out.push(Action::P2pRevoke(srv.id.clone(), f.id.clone()));
                        }
                    });
                });
            }
        });
        let pending: Vec<&hosting::Invite> = srv.invites.iter().filter(|i| i.pending(now)).collect();
        if !pending.is_empty() {
            ui.add_space(8.0);
            card(ui, |ui| {
                section_title(ui, &t!("Invitaciones pendientes ({n})", n = pending.len()));
                for i in pending {
                    ui.horizontal(|ui| {
                        avatar(ui, &i.name, 26.0, YELLOW);
                        ui.label(RichText::new(&i.name).semibold().color(TEXT));
                        let left = i.expires.saturating_sub(now);
                        let txt = if left >= 3600 { t!("caduca en {n} h", n = left / 3600) } else { t!("caduca en {n} min", n = (left / 60).max(1)) };
                        ui.label(RichText::new(format!("⏳ {txt}")).size(13.0).color(YELLOW));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(ghost_button(t!("Anular"))).clicked() {
                                out.push(Action::P2pCancelInvite(srv.id.clone(), i.id.clone()));
                            }
                            if !i.code.is_empty() && ui.add(ghost_button(t!("📋 Copiar código"))).clicked() {
                                out.push(Action::Copy(i.code.clone(), t!("Código copiado").into()));
                            }
                        });
                    });
                }
            });
        }
        ui.add_space(8.0);
        other_ways(ui, mdl, srv, st, tunnel, s, out);
    });
}

fn invite_dialog(ctx: &egui::Context, h: &mut HostView, s: &Session, srv: &Server, out: &mut Vec<Action>) {
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        h.invite_open = false;
        return;
    }
    let screen = ctx.screen_rect();
    egui::Area::new(egui::Id::new("invite_veil")).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
        let (r, _) = ui.allocate_exact_size(screen.size(), egui::Sense::click());
        ui.painter().rect_filled(r, 0.0, Color32::from_black_alpha(160));
    });
    let width = (screen.width() - 48.0).min(600.0);
    egui::Area::new(egui::Id::new("invite")).order(egui::Order::Tooltip).fixed_pos(egui::pos2(screen.center().x - width / 2.0, screen.min.y + (screen.height() * 0.1).max(20.0))).show(ctx, |ui| {
        egui::Frame::none()
            .fill(CARD_HI)
            .rounding(14.0)
            .stroke(egui::Stroke::new(1.0_f32, BORDER))
            .shadow(egui::epaint::Shadow { offset: egui::vec2(0.0, 12.0), blur: 40.0, spread: 0.0, color: Color32::from_black_alpha(140) })
            .inner_margin(egui::Margin::same(20.0))
            .show(ui, |ui| {
                ui.set_width(width - 40.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t!("Invitar amigo")).size(20.0).display().color(TEXT));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if close_button(ui).clicked() {
                            h.invite_open = false;
                        }
                    });
                });
                ui.label(RichText::new(t!("El código es personal y de un solo uso. Su equipo quedará atado al nombre que pongas.")).size(12.5).color(TEXT_DIM));
                ui.add_space(10.0);
                let done = s.p2p_code.clone();
                field_label(ui, t!("Su nombre en Minecraft"));
                ui.horizontal(|ui| {
                    let r = ui.add_enabled(done.is_none(), egui::TextEdit::singleline(&mut h.invite_name).hint_text(t!("Exactamente como se llama en el juego")).desired_width(280.0));
                    if done.is_none() && h.invite_name.is_empty() && ui.memory(|m| m.focused().is_none()) {
                        r.request_focus();
                    }
                    let n = h.invite_name.trim();
                    if !n.is_empty() {
                        if hosting::valid_player(n) {
                            ui.label(RichText::new(t!("✔ nombre válido")).size(12.5).color(GREEN));
                        } else {
                            ui.label(RichText::new(t!("3 a 16 letras, números o _")).size(12.5).color(RED));
                        }
                    }
                });
                ui.add_space(8.0);
                field_label(ui, t!("El código caduca en"));
                ui.add_enabled_ui(done.is_none(), |ui| segmented(ui, &mut h.invite_hours, &[(1, t!("1 hora")), (24, t!("24 horas")), (168, t!("7 días"))]));
                ui.add_space(12.0);
                match done {
                    None => {
                        ui.horizontal(|ui| {
                            let ok = hosting::valid_player(h.invite_name.trim()) && !s.p2p_inviting;
                            if ui.add_enabled(ok, primary_button(t!("Crear invitación"), 160.0, 36.0)).clicked() {
                                out.push(Action::P2pInvite(srv.id.clone(), h.invite_name.trim().to_string(), h.invite_hours.max(1)));
                            }
                            if s.p2p_inviting {
                                ui.add(egui::Spinner::new().color(ACCENT));
                                ui.label(RichText::new(t!("Preparando la conexión…")).size(12.5).color(TEXT_DIM));
                            }
                        });
                    }
                    Some(code) => {
                        field_label(ui, &t!("Código para {n}", n = h.invite_name.trim()));
                        egui::Frame::none().fill(INPUT).rounding(9.0).stroke(egui::Stroke::new(1.0_f32, ACCENT.gamma_multiply(0.5))).inner_margin(egui::Margin::same(12.0)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.add(egui::Label::new(RichText::new(crate::gui::view::fmt::elide(&code, 64)).monospace().size(14.0).color(ACCENT_HI)).truncate());
                        });
                        ui.add_space(10.0);
                        ui.horizontal_wrapped(|ui| {
                            if ui.add(primary_button(t!("📋  Copiar código"), 150.0, 34.0)).clicked() {
                                out.push(Action::Copy(code.clone(), t!("Código copiado: pásaselo sólo a esa persona").into()));
                            }
                            if ui.add(secondary_button(t!("Copiar mensaje para WhatsApp"))).clicked() {
                                let msg = t!(
                                    "¡Te invito a mi servidor de Minecraft «{s}»! Abre Brasa → Servidores → «Unirse a un amigo» y pega este código (es sólo para ti y caduca):\n\n{c}",
                                    s = srv.name,
                                    c = code
                                );
                                out.push(Action::Copy(msg, t!("Mensaje copiado").into()));
                            }
                        });
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(t!("🔒 El código lleva la identidad de tu servidor y un secreto. Si alguien lo usa antes que tu amigo, te saldrá en el portero."))
                                .size(12.0)
                                .color(TEXT_DIM),
                        );
                    }
                }
            });
    });
}

/// Avisos del portero (en cualquier pestaña).
pub fn approvals(ctx: &egui::Context, out: &mut Vec<Action>) {
    let list = hosting::p2p::pending();
    let Some((key, server, name, peer)) = list.first().cloned() else { return };
    egui::Area::new(egui::Id::new("p2p_gate")).order(egui::Order::Tooltip).anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-20.0, -20.0)).show(ctx, |ui| {
        egui::Frame::none().fill(CARD_HI).rounding(12.0).stroke(egui::Stroke::new(1.5_f32, ACCENT)).inner_margin(egui::Margin::same(16.0)).show(ui, |ui| {
            ui.set_width(360.0);
            ui.label(RichText::new(t!("🚪 {n} quiere unirse", n = name)).size(15.5).semibold().color(TEXT));
            ui.label(
                RichText::new(t!("A «{s}», con la invitación que creaste para {n}. Equipo {k}.", s = server, n = name, k = peer)).size(12.5).color(TEXT_DIM),
            );
            if list.len() > 1 {
                ui.label(RichText::new(t!("y {n} más esperando", n = list.len() - 1)).size(12.0).color(TEXT_DIM));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add(primary_button(t!("Aceptar"), 100.0, 32.0)).clicked() {
                    out.push(Action::P2pDecide(key, true));
                }
                if ui.add(secondary_button(t!("Rechazar"))).clicked() {
                    out.push(Action::P2pDecide(key, false));
                }
            });
        });
    });
    ctx.request_repaint_after(std::time::Duration::from_millis(500));
}

fn step_line(ui: &mut egui::Ui, state: i8, text: &str) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
        let p = ui.painter();
        match state {
            1 => {
                p.circle_filled(r.center(), 10.0, GREEN);
                p.line_segment([r.center() + egui::vec2(-4.5, 0.0), r.center() + egui::vec2(-1.0, 3.5)], egui::Stroke::new(2.0_f32, BG));
                p.line_segment([r.center() + egui::vec2(-1.0, 3.5), r.center() + egui::vec2(5.0, -3.5)], egui::Stroke::new(2.0_f32, BG));
            }
            0 => {
                ui.put(r, egui::Spinner::new().size(18.0).color(ACCENT));
            }
            -1 => {
                p.circle_filled(r.center(), 10.0, RED);
                p.text(r.center(), egui::Align2::CENTER_CENTER, "!", egui::FontId::proportional(13.0), BG);
            }
            _ => {
                p.circle_stroke(r.center(), 9.0, egui::Stroke::new(1.5_f32, BORDER));
            }
        }
        ui.label(RichText::new(text).size(14.0).color(if state == 2 { TEXT_DIM } else { TEXT }));
    });
}

fn join_view(ui: &mut egui::Ui, vw: &mut ViewState, s: &Session, out: &mut Vec<Action>) {
    use hosting::p2p::JoinStep;
    let h = &mut vw.hosting;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.add_space(18.0);
        page_header(ui, t!("Unirse a un amigo"), t!("Pega el código que te ha pasado. Sólo hace falta la primera vez."));
        card(ui, |ui| {
            let busy = matches!(&s.p2p_join, Some((st, None)) if *st != JoinStep::Done);
            field_label(ui, t!("Código"));
            let r = ui.add_enabled(
                !busy,
                egui::TextEdit::singleline(&mut h.join_code).hint_text("brasa1.…").desired_width(f32::INFINITY).font(egui::TextStyle::Monospace),
            );
            if h.join_code.is_empty() && !busy && ui.memory(|m| m.focused().is_none()) {
                r.request_focus();
            }
            let ticket = hosting::p2p::decode_ticket(&h.join_code);
            ui.add_space(6.0);
            match (&ticket, h.join_code.trim().is_empty()) {
                (_, true) => {}
                (Ok(t), _) => {
                    ui.label(RichText::new(t!("Invitación a «{s}» para {n}", s = t.title, n = t.name)).size(13.0).color(GREEN));
                    ui.label(RichText::new(t!("Entrarás con ese nombre: tu cuenta en Brasa tiene que llamarse igual.")).size(12.0).color(TEXT_DIM));
                }
                (Err(e), _) => {
                    ui.label(RichText::new(capitalize(&format!("{e:#}"))).size(13.0).color(RED));
                }
            }
            ui.add_space(10.0);
            if let Some((st, err)) = &s.p2p_join {
                let order = [JoinStep::Code, JoinStep::Found, JoinStep::Waiting, JoinStep::Paired, JoinStep::Preparing];
                let cur = order.iter().position(|x| x == st).unwrap_or(order.len());
                let labels = [
                    t!("Código válido"),
                    t!("Servidor encontrado e identidad comprobada"),
                    t!("Esperando a que tu amigo te acepte…"),
                    t!("Emparejado: tu equipo queda atado a tu nombre"),
                    t!("Preparando Minecraft (versión y mods del servidor)"),
                ];
                for (i, l) in labels.iter().enumerate() {
                    let state = if *st == JoinStep::Done || i < cur {
                        1
                    } else if i == cur {
                        if err.is_some() {
                            -1
                        } else {
                            0
                        }
                    } else {
                        2
                    };
                    step_line(ui, state, l);
                }
                if let Some(e) = err {
                    ui.add_space(6.0);
                    ui.label(RichText::new(capitalize(e)).size(13.0).color(RED));
                    if *st == JoinStep::Preparing {
                        ui.label(
                            RichText::new(t!("Ya estás emparejado: lo tienes en «De amigos» y se volverá a preparar al pulsar JUGAR."))
                                .size(12.5)
                                .color(TEXT_DIM),
                        );
                    }
                }
                if *st == JoinStep::Done {
                    ui.add_space(8.0);
                    ui.label(RichText::new(t!("¡Listo! Ya lo tienes en «De amigos». Pulsa JUGAR cuando quieras.")).color(GREEN));
                    if !s.p2p_missing.is_empty() {
                        ui.label(
                            RichText::new(t!("No se encontraron en Modrinth: {m}. Pídeselos a tu amigo.", m = s.p2p_missing.join(", ")))
                                .size(12.5)
                                .color(YELLOW),
                        );
                    }
                }
                ui.add_space(8.0);
            }
            ui.horizontal(|ui| {
                let ok = ticket.is_ok() && !busy;
                if ui.add_enabled(ok, primary_button(t!("Unirse"), 140.0, 38.0)).clicked() {
                    out.push(Action::P2pJoin(h.join_code.trim().to_string()));
                }
                if ui.add(ghost_button(t!("Cancelar"))).clicked() {
                    h.joining = false;
                    h.join_code.clear();
                }
            });
        });
        ui.add_space(8.0);
        ui.label(
            RichText::new(t!("🔒 Cifrado de punta a punta. Brasa probará la conexión directa; si tu red no la deja, irá por relé, también cifrado."))
                .size(12.5)
                .color(TEXT_DIM),
        );
    });
}

fn friend_detail(ui: &mut egui::Ui, mdl: &Model, _vw: &mut ViewState, s: &Session, j: &hosting::p2p::Joined, out: &mut Vec<Action>) {
    use hosting::p2p::GuestState;
    let key = j.key();
    let gs = hosting::p2p::guest_state(&key);
    let playing = j.instance.as_deref().map(|i| s.is_running(i) || s.launches.contains_key(i)).unwrap_or(false);
    ui.add_space(16.0);
    egui::Frame::none()
        .fill(CARD)
        .rounding(12.0)
        .stroke(egui::Stroke::new(1.0_f32, BORDER.gamma_multiply(0.6)))
        .inner_margin(egui::Margin::symmetric(18.0, 14.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let live = if matches!(gs, Some(GuestState::Ready { .. })) { Some(Status::Running) } else { None };
                server_tile(ui, j.kind, live, 48.0);
                ui.add_space(6.0);
                let text_w = (ui.available_width() - 200.0).max(160.0);
                ui.vertical(|ui| {
                    ui.set_max_width(text_w);
                    ui.add(egui::Label::new(RichText::new(&j.title).size(22.0).display().color(TEXT)).truncate());
                    ui.horizontal_wrapped(|ui| {
                        chip(ui, &format!("{} {}", j.kind.label(), j.mc), kind_color(j.kind));
                        chip(ui, &t!("🔒 Emparejado como {n}", n = j.name), GREEN);
                    });
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let size = egui::vec2(170.0, 50.0);
                    if playing {
                        art::progress_button(ui, t!("JUGANDO"), None, size);
                    } else if matches!(gs, Some(GuestState::Connecting)) {
                        art::progress_button(ui, t!("CONECTANDO"), None, size);
                    } else if art::big_button(ui, BigKind::Play, t!("JUGAR"), size, true).clicked() {
                        out.push(Action::P2pPlay(key.clone()));
                    }
                });
            });
        });
    ui.add_space(10.0);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        card(ui, |ui| {
            section_title(ui, t!("Conexión"));
            let row = |ui: &mut egui::Ui, k: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(egui::vec2(160.0, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.set_min_width(160.0);
                        ui.label(RichText::new(k).size(13.0).color(TEXT_DIM));
                    });
                    add(ui);
                });
            };
            row(ui, t!("Estado"), &mut |ui| match &gs {
                Some(GuestState::Ready { direct: true, rtt_ms, .. }) => {
                    ui.label(RichText::new(t!("• Conexión directa · {ms} ms", ms = rtt_ms)).color(GREEN));
                }
                Some(GuestState::Ready { rtt_ms, .. }) => {
                    ui.label(RichText::new(t!("• Por relé · {ms} ms", ms = rtt_ms)).color(BLUE));
                }
                Some(GuestState::Connecting) => {
                    ui.label(RichText::new(t!("Conectando…")).color(YELLOW));
                }
                Some(GuestState::Failed(e)) => {
                    ui.label(RichText::new(e).color(RED));
                }
                None => {
                    ui.label(RichText::new(t!("Sin conectar: se conecta al pulsar JUGAR")).color(TEXT_DIM));
                }
            });
            row(ui, t!("Cifrado"), &mut |ui| {
                ui.label(RichText::new(t!("QUIC + TLS 1.3 · identidad {k}", k = hosting::p2p::short(&j.host_id))).color(TEXT));
            });
            if let Some(GuestState::Ready { port, .. }) = &gs {
                let addr = format!("127.0.0.1:{port}");
                row(ui, t!("En tu Minecraft"), &mut |ui| {
                    ui.label(RichText::new(&addr).monospace().color(ACCENT_HI));
                    ui.label(RichText::new(t!("(Brasa entra solo)")).size(12.5).color(TEXT_DIM));
                });
            }
        });
        ui.add_space(8.0);
        card(ui, |ui| {
            section_title(ui, t!("Instancia"));
            let inst = j.instance.as_deref().and_then(|id| mdl.instances.iter().find(|i| i.id == id));
            match inst {
                Some(i) => {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&i.name).color(TEXT));
                        ui.label(RichText::new(format!("{} {}", i.loader.label(), i.mc_version)).size(12.5).color(TEXT_DIM));
                    });
                }
                None => {
                    ui.label(RichText::new(t!("Se crea al pulsar JUGAR, con la versión y los mods del servidor.")).color(TEXT_DIM));
                }
            }
            ui.label(RichText::new(t!("Si el servidor cambia de versión o de mods, Brasa los pone igual antes de entrar.")).size(12.5).color(TEXT_DIM));
            if !s.p2p_missing.is_empty() {
                ui.label(RichText::new(t!("No se encontraron en Modrinth: {m}. Pídeselos a tu amigo.", m = s.p2p_missing.join(", "))).size(12.5).color(YELLOW));
            }
        });
        ui.add_space(10.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(t!("Tu amigo puede quitarte el acceso cuando quiera.")).size(12.5).color(TEXT_DIM));
            if ui.add(danger_ghost_button(t!("Olvidar este servidor"))).clicked() {
                out.push(Action::P2pForget(key.clone()));
            }
        });
    });
}
