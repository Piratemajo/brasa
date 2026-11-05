//! Detalle de la instancia seleccionada: cabecera y pestañas Mods / Mundos / Ajustes.

use crate::actions;
use crate::config::Loader;
use crate::content::modrinth::ProjectType;
use crate::gui::action::Action;
use crate::gui::model::{Model, Session};
use crate::gui::state::{InstTab, Tab, ViewState};
use crate::gui::view::art::{caption, hero_background, instance_avatar};
use crate::gui::view::fmt::*;
use crate::gui::view::fonts::Weight;
use crate::gui::view::theme::*;
use crate::gui::view::widgets::*;
use eframe::egui::{self, Color32, RichText};

pub fn draw(ui: &mut egui::Ui, mdl: &Model, vw: &mut ViewState, s: &Session, out: &mut Vec<Action>) {
    let Some(e) = vw.edit.as_mut() else {
        ui.centered_and_justified(|ui| {
            ui.label(RichText::new(t!("Selecciona una instancia o crea una nueva")).size(16.0).color(TEXT_DIM));
        });
        return;
    };
    let id = e.id.clone();
    let game_dir = e.game_dir(&mdl.p);
    // lanzando o jugando a ESTA instancia; las demás tareas de fondo también bloquean instalar/reparar
    let playing = s.launch(&id).is_some();
    let busy = s.background_tasks() > 0 || playing;

    // ---------------- Cabecera: avatar, nombre, estado y el botón principal
    let loader_c = loader_color(e.loader);
    // ventana estrecha: cabecera más compacta y botones sólo con icono (el nombre va en la ayuda)
    let narrow = ui.available_width() < 580.0;
    let (avatar_px, play_w, title_px) = if narrow { (64.0, 164.0, 30.0) } else { (92.0, 188.0, 38.0) };
    let lbl = |icon: &str, text: &str| if narrow { icon.to_string() } else { format!("{icon}  {text}") };
    let (hero, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 130.0), egui::Sense::hover());
    hero_background(ui.painter(), hero, loader_c, &e.name);
    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(hero.shrink2(egui::vec2(24.0, 18.0))).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
        instance_avatar(ui, mdl.icon_uri(e), &e.name, e.loader, avatar_px);
        ui.add_space(12.0);
        // la columna del nombre nunca invade el botón (188 px + márgenes)
        let text_w = (ui.available_width() - play_w - 26.0).max(120.0);
        ui.vertical(|ui| {
            ui.set_max_width(text_w);
            ui.spacing_mut().item_spacing.y = 7.0;
            ui.add(egui::Label::new(RichText::new(&e.name).size(title_px).display().color(TEXT)).truncate());
            ui.horizontal(|ui| {
                let lv = e.loader_version.clone().map(|v| format!(" {v}")).unwrap_or_default();
                chip(ui, &format!("{}{lv}", e.loader.label()), loader_c);
                chip(ui, &if narrow { e.mc_version.clone() } else { t!("Minecraft {}", e.mc_version) }, TEXT_DIM);
                if e.saving_mode {
                    chip(ui, t!("Ahorro"), GREEN);
                }
            });
            let last = match e.last_played {
                _ if s.is_running(&id) => t!("Jugando ahora").to_string(),
                Some(t) => t!("{} jugadas · última {}", actions::fmt_duration(e.playtime_secs), ago(t)),
                None => t!("Aún no has jugado a esta instancia").to_string(),
            };
            ui.add(egui::Label::new(caption(&last)).truncate());
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // columna fija: botón y, debajo, el estado y la consola (nunca encima de los chips)
            ui.allocate_ui_with_layout(egui::vec2(play_w, ui.available_height()), egui::Layout::top_down(egui::Align::Center), |ui| {
                super::play::play_control(ui, s, &id, egui::vec2(play_w, 60.0), out);
                if let Some((txt, col)) = super::play::status_text(s, &id) {
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(txt).size(12.5).color(col));
                        if ui.add(ghost_button(t!("Consola"))).on_hover_text(t!("Ver lo que está haciendo")).clicked() {
                            vw.tab = Tab::Logs;
                        }
                    });
                }
            });
        });
    });

    // ---------------- Acciones: las frecuentes a la vista, el resto en «Más…»
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        if ui.add(secondary_button(&lbl("📁", t!("Carpeta")))).on_hover_text(t!("Abrir la carpeta de la instancia")).clicked() {
            out.push(Action::OpenDir(game_dir.clone()));
        }
        if ui.add_enabled(!busy, secondary_button(&lbl("⬇", t!("Instalar")))).on_hover_text(t!("Descarga todo sin abrir el juego")).clicked() {
            out.push(Action::Install(id.clone(), false));
        }
        if ui
            .add_enabled(!busy, secondary_button(&lbl("🛠", t!("Reparar"))))
            .on_hover_text(t!("Re-verifica todos los ficheros por SHA-1 y baja sólo lo roto"))
            .clicked()
        {
            out.push(Action::Install(id.clone(), true));
        }
        if ui
            .add(secondary_button(&lbl("⚡", t!("Rendimiento"))))
            .on_hover_text(t!("Instala Sodium, Lithium, FerriteCore y compañía si existen para esta versión"))
            .clicked()
        {
            out.push(Action::Perf(id.clone()));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.menu_button(RichText::new(t!("Más…")).color(TEXT), |ui| {
                ui.set_min_width(210.0);
                if ui.button(t!("Duplicar instancia")).clicked() {
                    out.push(Action::Duplicate(id.clone()));
                    ui.close_menu();
                }
                if ui.button(t!("Exportar como .mrpack")).clicked() {
                    out.push(Action::ExportPack(id.clone()));
                    ui.close_menu();
                }
                ui.checkbox(&mut vw.export_saves, t!("Incluir los mundos al exportar"));
                ui.separator();
                if ui.button(RichText::new(t!("Borrar instancia…")).color(RED)).clicked() {
                    vw.confirm_delete = true;
                    ui.close_menu();
                }
            });
        });
    });
    if vw.confirm_delete {
        ui.add_space(8.0);
        egui::Frame::none()
            .fill(RED.gamma_multiply(0.14))
            .rounding(7.0)
            .stroke(egui::Stroke::new(1.0_f32, RED.gamma_multiply(0.5)))
            .inner_margin(egui::Margin::symmetric(14.0, 10.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t!("¿Borrar «{}» con todos sus mundos? No se puede deshacer.", e.name)).color(TEXT));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let b = egui::Button::new(RichText::new(t!("Sí, borrar todo")).semibold().color(Color32::WHITE)).fill(RED).rounding(7.0);
                        if ui.add(b).clicked() {
                            out.push(Action::Delete(id.clone()));
                        }
                        if ui.add(ghost_button(t!("Cancelar"))).clicked() {
                            vw.confirm_delete = false;
                        }
                    });
                });
            });
    }

    ui.add_space(10.0);

    // ---------------- Modpack: versión y actualización
    if let Some(pack) = &mdl.pack {
        if vw.pack_checked_for.as_deref() != Some(id.as_str()) {
            vw.pack_checked_for = Some(id.clone());
            out.push(Action::CheckPackUpdate(id.clone()));
        }
        pack_strip(ui, pack, s.pack_checks.get(&id), &id, busy || playing, narrow, out);
        ui.add_space(8.0);
    }

    // ---------------- Pestañas
    let tabs = [
        (InstTab::Mods, t!("Mods"), Some(mdl.mods.len())),
        (InstTab::Worlds, t!("Mundos"), Some(mdl.worlds.len())),
        (InstTab::Servers, t!("Servidores"), Some(mdl.servers.len())),
        (InstTab::Packs, t!("Texturas"), Some(mdl.packs.len())),
        (InstTab::Shaders, t!("Shaders"), Some(mdl.shaders.len())),
        (InstTab::Shots, t!("Capturas"), Some(mdl.shots.len())),
        (InstTab::Settings, t!("Ajustes"), None),
    ];
    // con la ventana estrecha, las pestañas se desplazan en horizontal en vez de partirse
    egui::ScrollArea::horizontal().id_salt("inst_tabs").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (t, label, count) in &tabs {
                if sub_tab(ui, label, *count, vw.inst_tab == *t) {
                    vw.inst_tab = *t;
                }
            }
        });
    });
    // línea base bajo las pestañas: separa la navegación del contenido
    {
        let r = ui.min_rect();
        let y = ui.cursor().top() - 1.0;
        ui.painter().hline(r.x_range(), y, egui::Stroke::new(1.0_f32, BORDER.gamma_multiply(0.6)));
    }
    ui.add_space(8.0);

    match vw.inst_tab {
        InstTab::Mods => {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut vw.mod_filter).hint_text(t!("🔎 Filtrar mods…")).desired_width(if narrow { 150.0 } else { 240.0 }));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(secondary_button("📁")).on_hover_text(t!("Abrir carpeta de mods")).clicked() {
                        out.push(Action::OpenDir(game_dir.join("mods")));
                    }
                    if ui.add(secondary_button(&lbl("➕", t!("Añadir .jar")))).on_hover_text(t!("Añadir mods desde ficheros .jar")).clicked() {
                        out.push(Action::PickJars);
                    }
                    if ui.add(secondary_button(&lbl("🔍", t!("Buscar mods")))).on_hover_text(t!("Buscar mods para esta instancia")).clicked() {
                        vw.target = Some(id.clone());
                        vw.ptype = ProjectType::Mod;
                        vw.tab = Tab::Browse;
                    }
                    if ui
                        .add_enabled(!busy, secondary_button(&lbl("↻", t!("Buscar actualizaciones"))))
                        .on_hover_text(t!("Buscar versiones nuevas de los mods"))
                        .clicked()
                    {
                        out.push(Action::CheckUpdates(id.clone()));
                    }
                });
            });
            ui.add_space(6.0);

            if !s.updates.is_empty() {
                egui::Frame::none()
                    .fill(ACCENT.gamma_multiply(0.12))
                    .stroke(egui::Stroke::new(1.0_f32, ACCENT.gamma_multiply(0.6)))
                    .rounding(10.0)
                    .inner_margin(egui::Margin::same(12.0))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(t!("⬆  {} actualizaciones disponibles", s.updates.len())).semibold().color(ACCENT_HI));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.add(primary_button(t!("Actualizar todo"), 150.0, 30.0)).clicked() {
                                    out.push(Action::ApplyUpdates(id.clone()));
                                }
                            });
                        });
                        for u in s.updates.iter().take(8) {
                            ui.label(
                                RichText::new(format!("{}  →  {}", u.old_path.file_name().unwrap_or_default().to_string_lossy(), u.new_version.version_number))
                                    .size(12.5)
                                    .color(TEXT_DIM),
                            );
                        }
                        if s.updates.len() > 8 {
                            ui.label(RichText::new(t!("… y {} más", s.updates.len() - 8)).size(12.5).color(TEXT_DIM));
                        }
                    });
                ui.add_space(6.0);
            }

            let f = vw.mod_filter.to_lowercase();
            egui::Frame::none().fill(CARD).rounding(12.0).stroke(egui::Stroke::new(1.0_f32, BORDER)).inner_margin(egui::Margin::same(6.0)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.push_id("modlist", |ui| {
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        if mdl.mods.is_empty() {
                            crate::gui::view::art::empty_state(ui, t!("Sin mods todavía"), t!("Búscalos en Explorar o arrastra archivos .jar a la ventana"));
                            return;
                        }
                        let shown: Vec<_> = mdl.mods.iter().filter(|m| mod_matches(m, s.mod_meta.get(&m.path), &f)).collect();
                        if shown.is_empty() {
                            ui.add_space(16.0);
                            ui.vertical_centered(|ui| {
                                ui.label(RichText::new(t!("Ningún mod coincide")).color(TEXT_DIM));
                            });
                        }
                        for (n, m) in shown.into_iter().enumerate() {
                            mod_row(ui, m, s.mod_meta.get(&m.path), n, out);
                        }
                    });
                });
            });
        }
        InstTab::Worlds => {
            super::worlds::tab(ui, mdl, vw, &id, &game_dir, busy, out);
        }
        InstTab::Servers => super::servers::tab(ui, mdl, vw, s, &id, busy, out),
        InstTab::Packs => super::content::tab(ui, mdl, vw, &id, &game_dir, crate::content::resources::Kind::ResourcePack, out),
        InstTab::Shaders => super::content::tab(ui, mdl, vw, &id, &game_dir, crate::content::resources::Kind::Shader, out),
        InstTab::Shots => super::content::tab(ui, mdl, vw, &id, &game_dir, crate::content::resources::Kind::Screenshot, out),
        InstTab::Settings => {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                card(ui, |ui| {
                    section_title(ui, t!("General"));
                    egui::Grid::new("inst").num_columns(2).spacing([18.0, 10.0]).min_col_width(140.0).show(ui, |ui| {
                        grid_label(ui, t!("Nombre"));
                        ui.add(egui::TextEdit::singleline(&mut e.name).desired_width(320.0));
                        ui.end_row();
                        grid_label(ui, t!("Minecraft"));
                        ui.add(egui::TextEdit::singleline(&mut e.mc_version).desired_width(160.0));
                        ui.end_row();
                        grid_label(ui, t!("Loader"));
                        egui::ComboBox::new("instloader", "").selected_text(e.loader.label()).width(160.0).show_ui(ui, |ui| {
                            for l in Loader::ALL {
                                ui.selectable_value(&mut e.loader, l, l.label());
                            }
                        });
                        ui.end_row();
                        grid_label(ui, t!("Entrar a servidor"));
                        ui.add(egui::TextEdit::singleline(&mut vw.edit_server).hint_text("play.ejemplo.com:25565").desired_width(320.0));
                        ui.end_row();
                        grid_label(ui, t!("Grupo"));
                        ui.vertical(|ui| {
                            ui.add(egui::TextEdit::singleline(&mut vw.edit_group).hint_text(t!("Sin grupo (p. ej. «Con amigos»)")).desired_width(320.0));
                            let existing = crate::instance::organize::group_names(&mdl.instances);
                            if !existing.is_empty() {
                                ui.horizontal_wrapped(|ui| {
                                    for g in existing {
                                        if ui.small_button(&g).clicked() {
                                            vw.edit_group = g;
                                        }
                                    }
                                });
                            }
                        });
                        ui.end_row();
                        grid_label(ui, t!("Icono"));
                        ui.horizontal(|ui| {
                            instance_avatar(ui, mdl.icon_uri(e), &e.name, e.loader, 40.0);
                            if ui.add(secondary_button(t!("Elegir imagen…"))).clicked() {
                                out.push(Action::PickIcon(id.clone()));
                            }
                            if e.icon.is_some() && ui.add(ghost_button(t!("Quitar"))).clicked() {
                                out.push(Action::RemoveIcon(id.clone()));
                            }
                        });
                        ui.end_row();
                    });
                });
                ui.add_space(10.0);
                card(ui, |ui| {
                    section_title(ui, t!("Java y rendimiento"));
                    egui::Grid::new("inst_java").num_columns(2).spacing([18.0, 10.0]).min_col_width(140.0).show(ui, |ui| {
                        grid_label(ui, t!("Memoria"));
                        ui.vertical(|ui| {
                            let active_mods = mdl.mods.iter().filter(|m| m.enabled).count();
                            let mut auto = e.memory_mb.is_none();
                            if ui.checkbox(&mut auto, t!("Automática")).changed() {
                                let (now, _) = crate::game::perf::effective_memory(&vw.settings, e, mdl.total_ram_mb, active_mods);
                                e.memory_mb = if auto { None } else { Some(now) };
                            }
                            match e.memory_mb {
                                None => {
                                    let (mb, src) = crate::game::perf::effective_memory(&vw.settings, e, mdl.total_ram_mb, active_mods);
                                    let why = if src == crate::game::perf::MemorySource::Auto {
                                        t!("Minecraft {} con {active_mods} mods", e.mc_version, active_mods = active_mods)
                                    } else {
                                        t!("la memoria por defecto de Ajustes").to_string()
                                    };
                                    ui.label(caption(&t!("{} ({mb} MB) para {why}", crate::game::perf::fmt_gb(mb as u64), mb = mb, why = why)));
                                }
                                Some(mut mem) => {
                                    let max = mdl.total_ram_mb.map(|t| t as u32).unwrap_or(32768).max(2048);
                                    if ui.add(egui::Slider::new(&mut mem, 1024..=max).step_by(256.0).suffix(t!(" MB"))).changed() {
                                        e.memory_mb = Some(mem);
                                    }
                                    if let Some(w) = crate::game::perf::over_allocation_warning(mem, mdl.total_ram_mb) {
                                        ui.label(RichText::new(w).size(12.5).color(YELLOW));
                                    }
                                }
                            }
                        });
                        ui.end_row();
                        grid_label(ui, t!("Modo ahorro"));
                        ui.vertical(|ui| {
                            ui.checkbox(&mut e.saving_mode, t!("Gastar menos CPU, GPU y batería"));
                            ui.label(caption(t!(
                                "Al jugar limita la distancia de render a 8, los FPS a 60 con vsync, y quita sombras y parte de las partículas."
                            )));
                            if e.saving_mode
                                && e.loader != Loader::Vanilla
                                && ui.add(secondary_button(t!("⚡  Instalar también mods de rendimiento"))).clicked()
                            {
                                out.push(Action::Perf(id.clone()));
                            }
                        });
                        ui.end_row();
                        grid_label(ui, t!("Java"));
                        ui.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(&mut vw.edit_java).hint_text(t!("Automático (el del sistema)")).desired_width(280.0));
                            if ui.add(secondary_button("…")).on_hover_text(t!("Elegir ejecutable de Java")).clicked() {
                                out.push(Action::PickJava(false));
                            }
                        });
                        ui.end_row();
                        grid_label(ui, t!("Argumentos JVM"));
                        ui.add(egui::TextEdit::singleline(&mut vw.edit_jvm).hint_text("-Dejemplo=1").desired_width(320.0));
                        ui.end_row();
                        grid_label(ui, t!("Resolución"));
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut vw.edit_res, t!("Personalizada"));
                            ui.add_enabled_ui(vw.edit_res, |ui| {
                                let w = e.width.get_or_insert(1280);
                                ui.add(egui::DragValue::new(w).range(320..=7680));
                                ui.label("×");
                                let h = e.height.get_or_insert(720);
                                ui.add(egui::DragValue::new(h).range(240..=4320));
                            });
                        });
                        ui.end_row();
                    });
                });
                ui.add_space(10.0);
                card(ui, |ui| {
                    section_title(ui, t!("Opciones de lanzamiento"));
                    egui::Grid::new("inst_launch").num_columns(2).spacing([18.0, 10.0]).min_col_width(140.0).show(ui, |ui| {
                        grid_label(ui, t!("Pantalla completa"));
                        ui.checkbox(&mut e.fullscreen, "");
                        ui.end_row();
                        grid_label(ui, t!("Wrapper"));
                        ui.add(egui::TextEdit::singleline(&mut vw.edit_wrapper).hint_text(t!("gamemoderun  /  mangohud")).desired_width(320.0));
                        ui.end_row();
                        grid_label(ui, t!("Variables de entorno"));
                        ui.add(egui::TextEdit::multiline(&mut vw.edit_env).hint_text(t!("CLAVE=valor (una por línea)")).desired_rows(2).desired_width(320.0));
                        ui.end_row();
                        grid_label(ui, t!("Antes de jugar"));
                        ui.add(egui::TextEdit::singleline(&mut vw.edit_pre).hint_text(t!("Comando; si falla, no se lanza el juego")).desired_width(320.0));
                        ui.end_row();
                        grid_label(ui, t!("Al cerrar el juego"));
                        ui.add(egui::TextEdit::singleline(&mut vw.edit_post).hint_text(t!("Comando (opcional)")).desired_width(320.0));
                        ui.end_row();
                    });
                });
                ui.add_space(12.0);
                if ui.add(primary_button(t!("💾  Guardar cambios"), 200.0, 38.0)).clicked() {
                    out.push(Action::SaveInstance);
                }
            });
        }
    }
}

/// Franja con el modpack de origen: versión instalada y, si la hay, la nueva para actualizar.
fn pack_strip(
    ui: &mut egui::Ui,
    pack: &crate::content::modpacks::PackInfo,
    check: Option<&crate::gui::model::PackCheck>,
    id: &str,
    busy: bool,
    narrow: bool,
    out: &mut Vec<Action>,
) {
    use crate::gui::model::PackCheck;
    let available = matches!(check, Some(PackCheck::Available(_)));
    let (fill, stroke) = if available { (ACCENT.gamma_multiply(0.10), ACCENT.gamma_multiply(0.5)) } else { (CARD, BORDER.gamma_multiply(0.55)) };
    egui::Frame::none().fill(fill).rounding(7.0).stroke(egui::Stroke::new(1.0_f32, stroke)).inner_margin(egui::Margin::symmetric(14.0, 8.0)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        // primero el estado (derecha); el nombre y la versión ocupan lo que quede y se recortan
        // altura fija: un diseño de derecha a izquierda dentro del marco se estiraría hacia abajo
        ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 30.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
            match check {
                Some(PackCheck::Available(v)) => {
                    let b = ui.add_enabled(!busy, primary_button(&t!("Actualizar a {v}", v = v), 0.0, 30.0));
                    if b.on_hover_text(t!("Copia tus mundos y actualiza; tus mods añadidos, opciones y servidores se conservan")).clicked() {
                        out.push(Action::UpdatePack(id.to_string()));
                    }
                }
                Some(PackCheck::Checking) => {
                    ui.label(RichText::new(t!("buscando versión nueva…")).color(TEXT_DIM));
                }
                Some(PackCheck::Updating) => {
                    ui.label(RichText::new("actualizando…").color(ACCENT_HI));
                }
                Some(PackCheck::UpToDate) => {
                    ui.label(RichText::new(t!("✔ al día")).color(GREEN));
                }
                Some(PackCheck::Failed(e)) => {
                    if ui.add(ghost_button(t!("Reintentar"))).clicked() {
                        out.push(Action::CheckPackUpdate(id.to_string()));
                    }
                    ui.label(RichText::new(if narrow { t!("sin comprobar") } else { t!("no se pudo comprobar") }).color(YELLOW)).on_hover_text(e.as_str());
                }
                None => {
                    if ui.add(ghost_button(if narrow { t!("Comprobar") } else { t!("Buscar actualización") })).clicked() {
                        out.push(Action::CheckPackUpdate(id.to_string()));
                    }
                }
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                let name = if narrow { pack.source.label().to_string() } else { t!("Modpack de {}", pack.source.label()) };
                ui.label(RichText::new(name).semibold().color(TEXT));
                ui.add(egui::Label::new(RichText::new(t!("versión {}", pack.version_name)).color(TEXT_DIM)).truncate());
            });
        });
    });
}

fn mod_matches(m: &crate::content::modrinth::LocalMod, meta: Option<&crate::content::modmeta::ModMeta>, f: &str) -> bool {
    if f.is_empty() {
        return true;
    }
    let mut hay = m.file_name.to_lowercase();
    if let Some(x) = meta {
        hay.push(' ');
        hay.push_str(&format!("{} {} {}", x.name, x.authors.join(" "), x.description).to_lowercase());
    }
    f.split_whitespace().all(|w| hay.contains(w))
}

/// Una fila de la lista de mods: icono, nombre y versión reales; autores y descripción debajo.
fn mod_row(ui: &mut egui::Ui, m: &crate::content::modrinth::LocalMod, meta: Option<&crate::content::modmeta::ModMeta>, n: usize, out: &mut Vec<Action>) {
    let fill = if n.is_multiple_of(2) { Color32::TRANSPARENT } else { Color32::from_rgba_unmultiplied(255, 255, 255, 5) };
    let file = m.file_name.trim_end_matches(".disabled");
    let (name, version) = match meta {
        Some(x) => (x.name.clone(), x.version.clone()),
        None => (file.trim_end_matches(".jar").to_string(), String::new()),
    };
    let sub = match meta {
        Some(x) => {
            let by = if x.authors.is_empty() { String::new() } else { t!("de {}", x.authors.iter().take(3).cloned().collect::<Vec<_>>().join(", ")) };
            match (by.is_empty(), x.description.is_empty()) {
                (false, false) => format!("{by} · {}", x.description),
                (false, true) => by,
                (true, false) => x.description.clone(),
                (true, true) => file.to_string(),
            }
        }
        None => file.to_string(),
    };
    let (main, dim) = if m.enabled { (TEXT, TEXT_DIM) } else { (TEXT_DIM, TEXT_DIM.gamma_multiply(0.75)) };
    egui::Frame::none().fill(fill).rounding(8.0).inner_margin(egui::Margin::symmetric(10.0, 7.0)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            if toggle_switch(ui, m.enabled).on_hover_text(if m.enabled { t!("Desactivar") } else { t!("Activar") }).clicked() {
                out.push(Action::ToggleMod(m.clone()));
            }
            ui.add_space(4.0);
            let tint = if m.enabled { Color32::WHITE } else { Color32::from_white_alpha(110) };
            match meta.and_then(|x| x.icon.as_ref()).filter(|p| p.is_file()) {
                Some(icon) => {
                    ui.add(egui::Image::new(format!("file://{}", icon.display())).fit_to_exact_size(egui::vec2(36.0, 36.0)).rounding(6.0).tint(tint));
                }
                None => {
                    crate::gui::view::art::identicon(ui, &name, Loader::Vanilla, 36.0);
                }
            }
            ui.add_space(4.0);
            let text_w = (ui.available_width() - 110.0).max(120.0);
            ui.vertical(|ui| {
                ui.set_max_width(text_w);
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.horizontal(|ui| {
                    ui.add(egui::Label::new(RichText::new(&name).semibold().color(main)).truncate());
                    if !version.is_empty() {
                        ui.add(egui::Label::new(RichText::new(&version).size(12.0).color(dim)).truncate());
                    }
                    if !m.enabled {
                        chip(ui, t!("Desactivado"), TEXT_DIM);
                    }
                });
                ui.add(egui::Label::new(RichText::new(&sub).size(12.0).color(dim)).truncate()).on_hover_text(match meta {
                    Some(x) if !x.description.is_empty() => format!("{}\n\n{file}", x.description),
                    _ => file.to_string(),
                });
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(icon_button("🗑")).on_hover_text(t!("Borrar mod")).clicked() {
                    out.push(Action::DeleteMod(m.path.clone()));
                }
                ui.label(RichText::new(fmt_size(m.size)).size(12.0).color(TEXT_DIM));
            });
        });
    });
}
