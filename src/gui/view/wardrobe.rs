//! Taquilla: probarse y equiparse cosméticos con una vista previa 3D de la propia skin.
//!
//! - arrastrar gira al jugador; la rueda del ratón acerca
//! - pasar el ratón por una ficha se la prueba sin equiparla
//! - los emotes se reproducen en la vista; los de la rueda son los que se usan en el juego
//! - los teñibles tienen colores propuestos y selector libre

use super::render3d::{self, Camera, Scene, Tex};
use crate::cosmetics::loadout::{self, Loadout, WHEEL};
use crate::cosmetics::paint::Canvas;
use crate::cosmetics::pose::{self, Pose};
use crate::cosmetics::{catalog, support, Cosmetic, Model, Slot};
use crate::gui::action::Action;
use crate::gui::model::{Model as AppModel, Session};
use crate::gui::state::ViewState;
use crate::gui::view::fonts::Weight;
use crate::gui::view::theme::*;
use crate::gui::view::widgets::*;
use eframe::egui::{self, Color32, RichText};
use std::collections::HashMap;

/// Estado de la Taquilla (sólo visual: texturas en la GPU, cámara, emote que suena…).
pub struct WardrobeView {
    pub slot: Slot,
    yaw: f32,
    yaw_target: f32,
    pitch: f32,
    zoom: f32,
    pub walking: bool,
    /// Emote en la vista previa y cuándo empezó.
    playing: Option<(String, f64)>,
    textures: HashMap<String, egui::TextureHandle>,
    models: HashMap<String, Model>,
    dragging: bool,
}

impl Default for WardrobeView {
    fn default() -> Self {
        WardrobeView {
            slot: Slot::Cape,
            yaw: 150.0,
            yaw_target: 150.0,
            pitch: 10.0,
            zoom: 1.0,
            walking: false,
            playing: None,
            textures: HashMap::new(),
            models: HashMap::new(),
            dragging: false,
        }
    }
}

/// Desde dónde se mira mejor cada hueco.
fn slot_yaw(s: Slot) -> f32 {
    match s {
        Slot::Cape | Slot::Back => 150.0,
        Slot::Hat => -28.0,
        Slot::Pet => -40.0,
        Slot::Emote => -20.0,
    }
}

const SWATCHES: [[u8; 3]; 12] = [
    [200, 56, 56],
    [226, 112, 44],
    [238, 186, 58],
    [110, 200, 90],
    [60, 120, 90],
    [70, 170, 200],
    [60, 100, 210],
    [130, 80, 200],
    [220, 110, 170],
    [235, 235, 235],
    [120, 120, 126],
    [34, 32, 36],
];

fn canvas_image(c: &Canvas) -> egui::ColorImage {
    let flat: Vec<u8> = c.px.iter().flatten().copied().collect();
    egui::ColorImage::from_rgba_unmultiplied([c.w as usize, c.h as usize], &flat)
}

impl WardrobeView {
    fn texture(&mut self, ctx: &egui::Context, key: String, make: impl FnOnce() -> Option<egui::ColorImage>) -> Option<Tex> {
        if !self.textures.contains_key(&key) {
            let img = make()?;
            let h = ctx.load_texture(key.clone(), img, egui::TextureOptions::NEAREST);
            // no acumular cientos de fotogramas y colores viejos
            if self.textures.len() > 400 {
                self.textures.clear();
            }
            self.textures.insert(key.clone(), h);
        }
        let h = &self.textures[&key];
        let s = h.size();
        Some(Tex { id: h.id(), size: [s[0] as f32, s[1] as f32] })
    }

    fn skin(&mut self, ctx: &egui::Context, path: Option<&str>) -> Tex {
        let key = format!("skin:{}", path.unwrap_or("brasa"));
        let p = path.map(|s| s.to_string());
        self.texture(ctx, key, move || {
            if let Some(img) = p.and_then(|p| image::open(p).ok()).map(|i| render3d::upgrade_legacy_skin(&i.to_rgba8())) {
                return Some(egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()));
            }
            Some(canvas_image(&catalog::default_skin()))
        })
        .expect("skin por defecto")
    }

    fn cape(&mut self, ctx: &egui::Context, c: &Cosmetic, tint: Option<[u8; 3]>, t: f64) -> Option<Tex> {
        let frame = if c.frames > 1 { ((t * c.fps as f64) as u32) % c.frames } else { 0 };
        self.texture(ctx, format!("cape:{}:{:?}:{frame}", c.id, tint), || catalog::cape(c.id, tint, frame).map(|cv| canvas_image(&cv)))
    }

    fn model(&mut self, ctx: &egui::Context, id: &str, tint: Option<[u8; 3]>) -> Option<(Model, Tex)> {
        let key = format!("model:{id}:{tint:?}");
        if !self.models.contains_key(&key) {
            let (m, cv) = catalog::build(id, tint)?;
            self.models.insert(key.clone(), m);
            let img = canvas_image(&cv);
            self.texture(ctx, format!("mtex:{id}:{tint:?}"), || Some(img));
        }
        let tex = self.texture(ctx, format!("mtex:{id}:{tint:?}"), || None)?;
        Some((self.models[&key].clone(), tex))
    }

    fn emote_pose(&mut self, now: f64) -> Option<Pose> {
        let (id, start) = self.playing.clone()?;
        let e = catalog::emote(&id)?;
        let t = (now - start) as f32;
        if !e.looping && t > e.duration + 1.2 {
            self.playing = None;
            return None;
        }
        Some(e.sample(t))
    }

    /// El jugador con lo que lleva (o se está probando) puesto.
    #[allow(clippy::too_many_arguments)]
    fn dress(&mut self, ctx: &egui::Context, scene: &mut Scene, look: &Loadout, pose: &Pose, skin: Tex, slim: bool, now: f64, cape_angle: f32) {
        render3d::add_player(scene, pose, skin, slim);
        if let Some(c) = look.cape.as_deref().and_then(catalog::get) {
            if let Some(tex) = self.cape(ctx, c, look.tint(c.id), now) {
                render3d::add_cape(scene, pose, tex, cape_angle);
            }
        }
        for slot in [Slot::Hat, Slot::Back, Slot::Pet] {
            if let Some(id) = look.equipped(slot) {
                if let Some((m, tex)) = self.model(ctx, id, look.tint(id)) {
                    scene.add_model(&m, pose, tex, now as f32);
                }
            }
        }
    }
}

pub fn draw(ui: &mut egui::Ui, mdl: &AppModel, vw: &mut ViewState, _s: &Session, out: &mut Vec<Action>) {
    let ctx = ui.ctx().clone();
    let now = ui.input(|i| i.time);
    ctx.request_repaint_after(std::time::Duration::from_millis(16));
    let acc = mdl.accounts.active();
    let key = loadout::key_for(acc);
    let saved = mdl.wardrobe.of(&key);
    let mut look = saved.clone();
    let skin_path = acc.and_then(|a| a.skin_path.clone());
    let slim = acc.is_some_and(|a| a.skin_slim);

    // ---------------- Cabecera
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(t!("Taquilla")).size(26.0).display().color(TEXT));
            let who = acc.map(|a| a.username.as_str()).unwrap_or(t!("sin cuenta"));
            let n = look.count();
            ui.label(
                RichText::new(t!("Cosméticos de {who} · {}", if n == 1 { t!("1 puesto").to_string() } else { t!("{n} puestos", n = n) }, who = who))
                    .color(TEXT_DIM),
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if !look.is_empty() && ui.add(ghost_button(t!("Quitarlo todo"))).clicked() {
                look = Loadout { tints: look.tints.clone(), ..Default::default() };
            }
        });
    });
    ui.add_space(10.0);

    let wide = ui.available_width() > 760.0;
    let pw = if wide { (ui.available_width() * 0.40).clamp(280.0, 440.0) } else { ui.available_width() };
    let ph = if wide { (ui.available_height() - 4.0).clamp(320.0, 640.0) } else { 340.0 };

    // la ficha bajo el ratón se prueba en la vista previa (se decide en este fotograma, se ve en el siguiente)
    let hover_id = ui.data(|d| d.get_temp::<String>(egui::Id::new("wardrobe_hover")));
    ui.data_mut(|d| d.remove::<String>(egui::Id::new("wardrobe_hover")));

    let body = |ui: &mut egui::Ui, vw: &mut ViewState, look: &mut Loadout, out: &mut Vec<Action>| {
        // ---- Vista previa
        ui.allocate_ui_with_layout(egui::vec2(pw, ph), egui::Layout::top_down(egui::Align::Min), |ui| {
            let mut trying = look.clone();
            if let Some(id) = hover_id.as_deref() {
                if !trying.is_on(id) && catalog::get(id).is_some_and(|c| c.slot != Slot::Emote) {
                    trying.toggle(id);
                }
            }
            preview(ui, &ctx, vw, &trying, skin_path.as_deref(), slim, now, ph - 96.0);
            ui.add_space(8.0);
            wheel(ui, vw, look, now);
        });
        if wide {
            ui.add_space(18.0);
        } else {
            ui.add_space(12.0);
        }
        // ---- Catálogo
        ui.vertical(|ui| {
            compat(ui, mdl);
            ui.add_space(8.0);
            egui::ScrollArea::horizontal().id_salt("wardrobe_tabs").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(
                ui,
                |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        for s in Slot::ALL {
                            let on = match s {
                                Slot::Emote => look.emotes.len(),
                                _ => look.equipped(s).is_some() as usize,
                            };
                            if sub_tab(ui, s.label(), Some(on), vw.wardrobe.slot == s) {
                                vw.wardrobe.slot = s;
                                vw.wardrobe.yaw_target = slot_yaw(s);
                            }
                        }
                    });
                },
            );
            {
                let r = ui.min_rect();
                ui.painter().hline(r.x_range(), ui.cursor().top() - 1.0, egui::Stroke::new(1.0_f32, BORDER.gamma_multiply(0.6)));
            }
            ui.add_space(10.0);
            tint_row(ui, vw, look);
            egui::ScrollArea::vertical().id_salt("wardrobe_grid").auto_shrink([false, false]).show(ui, |ui| {
                grid(ui, &ctx, vw, look, skin_path.as_deref(), slim, now, out);
            });
        });
    };
    if wide {
        ui.horizontal_top(|ui| body(ui, vw, &mut look, out));
    } else {
        egui::ScrollArea::vertical().id_salt("wardrobe_narrow").show(ui, |ui| body(ui, vw, &mut look, out));
    }

    if look != saved {
        out.push(Action::SetLoadout(key, look));
    }
}

#[allow(clippy::too_many_arguments)]
fn preview(ui: &mut egui::Ui, ctx: &egui::Context, vw: &mut ViewState, look: &Loadout, skin_path: Option<&str>, slim: bool, now: f64, h: f32) {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, h.max(220.0)), egui::Sense::click_and_drag());
    let wv = &mut vw.wardrobe;

    // cámara: arrastrar gira; soltar vuelve suavemente al ángulo del hueco elegido... salvo que se haya girado a mano
    if resp.dragged() {
        let d = resp.drag_delta();
        wv.yaw -= d.x * 0.6;
        wv.pitch = (wv.pitch + d.y * 0.35).clamp(-25.0, 45.0);
        wv.yaw_target = wv.yaw;
        wv.dragging = true;
    } else {
        wv.dragging = false;
        let diff = ((wv.yaw_target - wv.yaw + 540.0).rem_euclid(360.0)) - 180.0;
        wv.yaw += diff * 0.12;
    }
    if resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            wv.zoom = (wv.zoom * (1.0 + scroll * 0.002)).clamp(0.7, 2.2);
        }
    }
    let resp = resp.on_hover_cursor(if wv.dragging { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });

    // escenario: fondo cálido, suelo y resplandor
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 14.0, art_mix(BG_SIDE, CARD, 0.5));
    super::art::gradient_v(
        &p,
        egui::Rect::from_min_max(egui::pos2(rect.min.x, rect.center().y), rect.max).shrink2(egui::vec2(1.0, 1.0)),
        EMBER.gamma_multiply(0.0),
        EMBER.gamma_multiply(0.16),
    );
    let floor = egui::pos2(rect.center().x, rect.max.y - rect.height() * 0.13);
    let pts: Vec<egui::Pos2> = (0..32)
        .map(|k| {
            let a = k as f32 / 32.0 * std::f32::consts::TAU;
            floor + egui::vec2(a.cos() * rect.width() * 0.2, a.sin() * rect.height() * 0.035)
        })
        .collect();
    p.add(egui::Shape::convex_polygon(pts, Color32::from_black_alpha(90), egui::Stroke::NONE));
    p.rect_stroke(rect, 14.0, egui::Stroke::new(1.0_f32, BORDER));

    // pose: emote, andar o reposo
    let t = now as f32;
    let pose = wv.emote_pose(now).unwrap_or_else(|| pose::walk(t, if wv.walking { 1.0 } else { 0.0 }));
    let cape_angle = 6.0 + if wv.walking { 24.0 + (t * 5.6).sin() * 4.0 } else { (t * 1.7).sin() * 2.5 } + pose.lift.max(0.0) * 3.0;

    let skin = wv.skin(ctx, skin_path);
    let mut scene = Scene::default();
    wv.dress(ctx, &mut scene, look, &pose, skin, slim, now, cape_angle);
    let pet = look.pet.is_some();
    let scale = (rect.height() / 46.0).min(rect.width() / if pet { 46.0 } else { 30.0 }) * wv.zoom;
    let target = [if pet { -5.0 } else { 0.0 }, 17.5, 0.0];
    // los pies sobre el suelo pintado
    let center_y = floor.y - (target[1]) * scale;
    let view_rect = egui::Rect::from_center_size(egui::pos2(rect.center().x, center_y), rect.size());
    let painter = ui.painter_at(rect);
    scene.paint(&painter, view_rect, &Camera { yaw: wv.yaw, pitch: wv.pitch, target, scale });

    // controles sobre la vista
    let bar = egui::Rect::from_min_size(rect.left_top() + egui::vec2(10.0, 10.0), egui::vec2(rect.width() - 20.0, 30.0));
    // hijo sin reservar sitio: los botones flotan sobre la vista
    {
        let ui = &mut ui.new_child(egui::UiBuilder::new().max_rect(bar).layout(egui::Layout::left_to_right(egui::Align::Center)));
        ui.spacing_mut().item_spacing.x = 6.0;
        let walk_label = if wv.walking { t!("Parar") } else { t!("Andar") };
        if ui.add(secondary_button(walk_label)).on_hover_text(t!("Ver cómo se mueve la capa al andar")).clicked() {
            wv.walking = !wv.walking;
            wv.playing = None;
        }
        if ui.add(secondary_button(t!("De frente"))).clicked() {
            wv.yaw_target = -25.0;
        }
        if ui.add(secondary_button(t!("De espaldas"))).clicked() {
            wv.yaw_target = 155.0;
        }
    }
    if !wv.dragging && resp.hovered() {
        p.text(
            rect.center_bottom() - egui::vec2(0.0, 12.0),
            egui::Align2::CENTER_BOTTOM,
            t!("Arrastra para girar · rueda para acercar"),
            egui::FontId::proportional(11.5),
            TEXT_DIM,
        );
    }
}

fn art_mix(a: Color32, b: Color32, t: f32) -> Color32 {
    super::art::mix(a, b, t)
}

/// La rueda de emotes del juego (tecla B), con cada uno para reproducirlo.
fn wheel(ui: &mut egui::Ui, vw: &mut ViewState, look: &mut Loadout, now: f64) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(t!("Rueda de emotes")).semibold().color(TEXT));
        ui.label(RichText::new(t!("{}/{WHEEL} · tecla B en el juego", look.emotes.len(), WHEEL = WHEEL)).size(12.0).color(TEXT_DIM));
    });
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
        if look.emotes.is_empty() {
            ui.label(RichText::new(t!("Vacía: añade emotes desde la pestaña Emotes")).size(12.5).color(TEXT_DIM));
        }
        for id in look.emotes.clone() {
            let Some(c) = catalog::get(&id) else { continue };
            let on = vw.wardrobe.playing.as_ref().is_some_and(|(p, _)| *p == id);
            let b = egui::Button::new(RichText::new(crate::i18n::tr(c.name)).size(12.5).color(if on { ON_ACCENT } else { TEXT }))
                .fill(if on { ACCENT } else { CARD_HI })
                .rounding(14.0)
                .min_size(egui::vec2(0.0, 26.0));
            if ui.add(b).on_hover_text(t!("Reproducir")).clicked() {
                vw.wardrobe.playing = if on { None } else { Some((id.clone(), now)) };
                vw.wardrobe.walking = false;
            }
        }
    });
}

/// Dónde se verán: según la instancia elegida y las que ya tienen adaptador.
fn compat(ui: &mut egui::Ui, mdl: &AppModel) {
    let ready = mdl.instances.iter().filter(|i| support::status(i).in_game()).count();
    let (txt, col) = match mdl.selected_instance() {
        Some(i) => {
            let st = support::status(i);
            (format!("{} ({} {}): {}", i.name, i.loader.label(), i.mc_version, st.short()), if st.in_game() { GREEN } else { YELLOW })
        }
        None => (t!("Elige una instancia para saber si allí se verán").to_string(), TEXT_DIM),
    };
    egui::Frame::none()
        .fill(col.gamma_multiply(0.10))
        .rounding(10.0)
        .stroke(egui::Stroke::new(1.0_f32, col.gamma_multiply(0.35)))
        .inner_margin(egui::Margin::symmetric(12.0, 8.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                ui.painter().rect_filled(r, 0.0, col);
                ui.label(RichText::new(txt).size(12.5).color(TEXT));
                let n = match ready {
                    0 => t!("Ninguna instancia tuya los enseña aún en el juego (hace falta Fabric 1.21.1)").to_string(),
                    1 => t!("1 instancia tuya los enseña en el juego").to_string(),
                    n => t!("{n} instancias tuyas los enseñan en el juego", n = n),
                };
                ui.label(RichText::new(n).size(12.0).color(TEXT_DIM));
            });
        });
}

/// Colores para lo puesto en este hueco, si se puede teñir.
fn tint_row(ui: &mut egui::Ui, vw: &mut ViewState, look: &mut Loadout) {
    let slot = vw.wardrobe.slot;
    let Some(c) = look.equipped(slot).and_then(catalog::get).filter(|c| c.tintable()) else { return };
    let cur = look.tint(c.id).unwrap_or([255, 255, 255]);
    ui.horizontal(|ui| {
        ui.label(RichText::new(t!("Color")).size(13.0).color(TEXT_DIM)).on_hover_text(t!("Color de {}", crate::i18n::tr(c.name).to_lowercase()));
        ui.add_space(4.0);
        for sw in SWATCHES {
            let (r, resp) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
            let p = ui.painter();
            p.circle_filled(r.center(), 9.0, Color32::from_rgb(sw[0], sw[1], sw[2]));
            if sw == cur {
                p.circle_stroke(r.center(), 10.5, egui::Stroke::new(2.0_f32, TEXT));
            } else if resp.hovered() {
                p.circle_stroke(r.center(), 10.5, egui::Stroke::new(1.0_f32, TEXT_DIM));
            }
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                look.tints.insert(c.id.to_string(), sw);
            }
        }
        let mut rgb = cur;
        if ui.color_edit_button_srgb(&mut rgb).on_hover_text(t!("Otro color")).changed() {
            look.tints.insert(c.id.to_string(), rgb);
        }
    });
    ui.add_space(8.0);
}

#[allow(clippy::too_many_arguments)]
fn grid(ui: &mut egui::Ui, ctx: &egui::Context, vw: &mut ViewState, look: &mut Loadout, skin_path: Option<&str>, slim: bool, now: f64, _out: &mut Vec<Action>) {
    let slot = vw.wardrobe.slot;
    let (cw, ch) = (136.0, 168.0);
    let cols = ((ui.available_width() + 10.0) / (cw + 10.0)).floor().max(1.0) as usize;
    let items: Vec<&'static Cosmetic> = catalog::of(slot).collect();
    for row in items.chunks(cols) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            for c in row {
                card(ui, ctx, vw, look, c, skin_path, slim, now, egui::vec2(cw, ch));
            }
        });
        ui.add_space(10.0);
    }
    if slot == Slot::Emote {
        ui.label(RichText::new(t!("Pulsa una ficha para verla; «A la rueda» la pone en la rueda del juego.")).size(12.0).color(TEXT_DIM));
    } else {
        ui.label(RichText::new(t!("Pasa el ratón por una ficha para probártela; pulsa para ponértela o quitártela.")).size(12.0).color(TEXT_DIM));
    }
}

#[allow(clippy::too_many_arguments)]
fn card(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    vw: &mut ViewState,
    look: &mut Loadout,
    c: &Cosmetic,
    skin_path: Option<&str>,
    slim: bool,
    now: f64,
    size: egui::Vec2,
) {
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let on = look.is_on(c.id);
    let hovered = resp.hovered();
    if hovered {
        ui.data_mut(|d| d.insert_temp(egui::Id::new("wardrobe_hover"), c.id.to_string()));
    }
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 12.0, if hovered { CARD_HI } else { CARD });
    let stroke = if on { egui::Stroke::new(2.0_f32, ACCENT) } else { egui::Stroke::new(1.0_f32, BORDER.gamma_multiply(if hovered { 1.0 } else { 0.6 })) };
    p.rect_stroke(rect, 12.0, stroke);

    // miniatura
    let thumb = egui::Rect::from_min_size(rect.min + egui::vec2(8.0, 8.0), egui::vec2(size.x - 16.0, 104.0));
    p.rect_filled(thumb, 8.0, art_mix(BG_SIDE, CARD, 0.35));
    let wv = &mut vw.wardrobe;
    let anim_t = if hovered || on { now } else { 0.6 };
    match c.slot {
        Slot::Cape => {
            if let Some(tex) = wv.cape(ctx, c, look.tint(c.id), anim_t) {
                // la cara de fuera de la capa, en grande y nítida
                let r = egui::Rect::from_center_size(thumb.center(), egui::vec2(50.0, 80.0));
                let uv = egui::Rect::from_min_max(egui::pos2(1.0 / 64.0, 1.0 / 32.0), egui::pos2(11.0 / 64.0, 17.0 / 32.0));
                p.image(tex.id, r, uv, Color32::WHITE);
                p.rect_stroke(r, 0.0, egui::Stroke::new(1.0_f32, Color32::from_black_alpha(80)));
            }
        }
        Slot::Emote => {
            let pose =
                catalog::emote(c.id).map(|e| e.sample(if hovered { (now % (e.duration as f64 + 0.4)) as f32 } else { e.duration * 0.3 })).unwrap_or_default();
            let skin = wv.skin(ctx, skin_path);
            let mut s = Scene::default();
            render3d::add_player(&mut s, &pose, skin, slim);
            paint_fit(&s, &p, thumb, -25.0, 8.0);
        }
        _ => {
            if let Some((m, tex)) = wv.model(ctx, c.id, look.tint(c.id)) {
                let mut s = Scene::default();
                s.add_model(&m, &Pose::default(), tex, anim_t as f32);
                let yaw = if c.slot == Slot::Back { 155.0 + (anim_t as f32 * 20.0).sin() * 10.0 } else { -30.0 + (anim_t as f32 * 0.8).sin() * 12.0 };
                paint_fit(&s, &p, thumb, yaw, 16.0);
            }
        }
    }
    if c.animated() {
        p.text(thumb.right_top() + egui::vec2(-8.0, 6.0), egui::Align2::RIGHT_TOP, t!("animada"), egui::FontId::proportional(10.5), ACCENT_HI);
    }
    if catalog::NEW.contains(&c.id) {
        let g = p.layout_no_wrap(t!("nuevo").to_string(), egui::FontId::new(10.5, crate::gui::view::fonts::semibold_family()), ON_ACCENT);
        let r = egui::Rect::from_min_size(thumb.left_bottom() + egui::vec2(6.0, -g.size().y - 10.0), g.size() + egui::vec2(10.0, 4.0));
        p.rect_filled(r, 6.0, GREEN);
        p.galley(r.min + egui::vec2(5.0, 2.0), g, ON_ACCENT);
    }

    // nombre y estado
    let name_y = thumb.max.y + 8.0;
    p.text(
        egui::pos2(rect.min.x + 10.0, name_y),
        egui::Align2::LEFT_TOP,
        crate::i18n::tr(c.name),
        egui::FontId::new(13.5, crate::gui::view::fonts::semibold_family()),
        TEXT,
    );
    let (sub, col) = match (c.slot, on) {
        (Slot::Emote, true) => (t!("En la rueda").to_string(), ACCENT_HI),
        (_, true) => (t!("Puesto").to_string(), ACCENT_HI),
        (_, false) if c.tintable() => (t!("Se puede teñir").to_string(), TEXT_DIM),
        _ => (short(crate::i18n::tr(c.description), 22), TEXT_DIM),
    };
    if c.slot != Slot::Emote {
        p.text(egui::pos2(rect.min.x + 10.0, name_y + 20.0), egui::Align2::LEFT_TOP, sub, egui::FontId::proportional(11.5), col);
    }
    if on {
        // marca de puesto, pintada
        let cpos = thumb.left_top() + egui::vec2(12.0, 12.0);
        p.circle_filled(cpos, 8.0, ACCENT);
        p.line_segment([cpos + egui::vec2(-3.5, 0.0), cpos + egui::vec2(-1.0, 2.5)], egui::Stroke::new(2.0_f32, ON_ACCENT));
        p.line_segment([cpos + egui::vec2(-1.0, 2.5), cpos + egui::vec2(3.5, -2.5)], egui::Stroke::new(2.0_f32, ON_ACCENT));
    }

    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(crate::i18n::tr(c.description));
    if c.slot == Slot::Emote {
        // botón propio para la rueda; la ficha reproduce
        // (pintado a mano: un widget aquí descolocaría la fila de fichas)
        let br = egui::Rect::from_min_size(egui::pos2(rect.min.x + 10.0, name_y + 20.0), egui::vec2(size.x - 20.0, 22.0));
        let full = look.emotes.len() >= WHEEL && !on;
        let r = ui.interact(br, egui::Id::new(("wheel_btn", c.id)), egui::Sense::click());
        let (label, fill, fg, stroke) = if on {
            (t!("En la rueda · quitar"), Color32::TRANSPARENT, ACCENT_HI, ACCENT.gamma_multiply(0.5))
        } else if full {
            (t!("Rueda llena"), Color32::TRANSPARENT, TEXT_DIM, BORDER)
        } else {
            (t!("Añadir a la rueda"), if r.hovered() { ACCENT_HI } else { ACCENT }, ON_ACCENT, Color32::TRANSPARENT)
        };
        p.rect(br, 11.0, fill, egui::Stroke::new(1.0_f32, stroke));
        p.text(br.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::new(11.5, crate::gui::view::fonts::semibold_family()), fg);
        let r = r.on_hover_cursor(egui::CursorIcon::PointingHand);
        if r.clicked() && !full {
            look.toggle(c.id);
        } else if resp.clicked() {
            vw.wardrobe.playing = Some((c.id.to_string(), now));
            vw.wardrobe.walking = false;
            vw.wardrobe.yaw_target = slot_yaw(Slot::Emote);
        }
        if full {
            r.on_hover_text(t!("La rueda tiene sitio para {WHEEL}: quita uno antes", WHEEL = WHEEL));
        }
    } else if resp.clicked() {
        look.toggle(c.id);
        vw.wardrobe.yaw_target = slot_yaw(c.slot);
    }
}

fn short(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n - 1).collect::<String>() + "…"
    }
}

/// Dibuja la escena encuadrada en el rectángulo.
fn paint_fit(s: &Scene, p: &egui::Painter, r: egui::Rect, yaw: f32, pitch: f32) {
    let Some((lo, hi)) = s.bounds() else { return };
    let center = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
    let ext = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]).max(1.0);
    let scale = (r.height().min(r.width()) * 0.78) / ext;
    s.paint(p, r, &Camera { yaw, pitch, target: center, scale });
}
