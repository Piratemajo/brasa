//! Ajustes globales del launcher.

use crate::gui::action::Action;
use crate::gui::model::{Model, Session};
use crate::gui::state::ViewState;
use crate::gui::view::fmt::*;
use crate::gui::view::fonts::Weight;
use crate::gui::view::theme::*;
use crate::gui::view::widgets::*;
use eframe::egui::{self, RichText};

pub fn draw(ui: &mut egui::Ui, mdl: &Model, vw: &mut ViewState, s: &Session, out: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(t!("Ajustes")).size(24.0).semibold().color(TEXT));
            ui.label(RichText::new(t!("Se aplican a todas las instancias que no tengan su propio valor.")).color(TEXT_DIM));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add(primary_button(t!("💾  Guardar"), 130.0, 36.0)).clicked() {
                out.push(Action::SaveSettings);
            }
        });
    });
    ui.add_space(10.0);

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        card(ui, |ui| {
            section_title(ui, t!("Rendimiento"));
            egui::Grid::new("set_perf").num_columns(2).spacing([18.0, 10.0]).min_col_width(190.0).show(ui, |ui| {
                grid_label(ui, t!("Memoria del juego"));
                ui.vertical(|ui| {
                    ui.checkbox(&mut vw.settings.auto_memory, t!("Automática según el equipo, la versión y los mods"));
                    let ram = match mdl.total_ram_mb {
                        Some(t) => t!("Tu equipo tiene {} de RAM. ", crate::game::perf::fmt_gb(t)),
                        None => String::new(),
                    };
                    if vw.settings.auto_memory {
                        ui.label(crate::gui::view::art::caption(&t!("{ram}Una instancia vanilla recibe unos 2,5 GB; cada mod suma un poco.", ram = ram)));
                    } else {
                        let max = mdl.total_ram_mb.map(|t| t as u32).unwrap_or(32768).max(2048);
                        ui.add(egui::Slider::new(&mut vw.settings.default_memory_mb, 1024..=max).step_by(512.0).suffix(t!(" MB")));
                        if let Some(w) = crate::game::perf::over_allocation_warning(vw.settings.default_memory_mb, mdl.total_ram_mb) {
                            ui.label(RichText::new(w).size(12.5).color(YELLOW));
                        }
                    }
                });
                ui.end_row();
                grid_label(ui, t!("Descargas simultáneas"));
                ui.add(egui::Slider::new(&mut vw.settings.concurrency, 4..=128));
                ui.end_row();
                grid_label(ui, t!("GC optimizado (G1 afinado)"));
                ui.checkbox(&mut vw.settings.optimized_gc, "");
                ui.end_row();
                grid_label(ui, t!("Copia de mundos al jugar"));
                ui.horizontal(|ui| {
                    ui.checkbox(&mut vw.settings.auto_backup_worlds, t!("Automática"));
                    ui.add_enabled_ui(vw.settings.auto_backup_worlds, |ui| {
                        ui.add(egui::Slider::new(&mut vw.settings.backups_to_keep, 1..=20).suffix(t!(" por mundo")));
                    });
                });
                ui.end_row();
                grid_label(ui, t!("Argumentos JVM globales"));
                ui.add(egui::TextEdit::singleline(&mut vw.settings_jvm).desired_width(360.0));
                ui.end_row();
            });
        });
        ui.add_space(10.0);

        card(ui, |ui| {
            section_title(ui, t!("Java"));
            egui::Grid::new("set_java").num_columns(2).spacing([18.0, 10.0]).min_col_width(190.0).show(ui, |ui| {
                grid_label(ui, t!("Java global"));
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut vw.settings.java_path).hint_text(t!("Automático: usa el instalado en el sistema")).desired_width(300.0));
                    if ui.add(secondary_button("…")).on_hover_text(t!("Elegir ejecutable de Java")).clicked() {
                        out.push(Action::PickJava(true));
                    }
                    if ui.add(secondary_button(t!("Detectar"))).clicked() {
                        out.push(Action::DetectJava);
                    }
                });
                ui.end_row();
                ui.label("");
                ui.checkbox(&mut vw.settings.java_auto_download, t!("Descargar Java sólo si no hay ninguno compatible instalado"));
                ui.end_row();
            });
            if let Some(list) = &s.javas {
                ui.add_space(6.0);
                egui::Frame::none().fill(INPUT).rounding(8.0).inner_margin(egui::Margin::same(10.0)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if list.is_empty() {
                        ui.label(RichText::new(t!("No se ha encontrado ningún Java instalado.")).color(YELLOW));
                    }
                    for j in list {
                        ui.horizontal(|ui| {
                            chip(ui, &t!("Java {}", j.major), ACCENT);
                            ui.label(RichText::new(format!("{} · {}", j.version, j.source)).color(TEXT_DIM));
                            ui.label(RichText::new(elide(&j.exe.display().to_string(), 70)).monospace().size(12.0).color(TEXT_DIM));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.add(ghost_button(t!("Usar siempre"))).clicked() {
                                    vw.settings.java_path = j.exe.to_string_lossy().into_owned();
                                }
                            });
                        });
                    }
                });
            }
            ui.label(
                RichText::new(t!("Con el campo vacío, Brasa elige el adecuado para cada versión (8 para ≤1.16, 17 para 1.18–1.20.4, 21 para 1.20.5+, 25 para las más nuevas)."))
                    .size(12.5)
                    .color(TEXT_DIM),
            );
        });
        ui.add_space(10.0);

        card(ui, |ui| {
            ui.horizontal(|ui| {
                section_title(ui, t!("Red"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(secondary_button(t!("🔎  Probar conexión"))).clicked() {
                        out.push(Action::NetTest);
                    }
                });
            });
            egui::Grid::new("set_net").num_columns(2).spacing([18.0, 10.0]).min_col_width(190.0).show(ui, |ui| {
                grid_label(ui, t!("Proxy"));
                ui.add(egui::TextEdit::singleline(&mut vw.settings.proxy).hint_text(t!("vacío = sistema · p. ej. http://proxy.empresa:8080")).desired_width(360.0));
                ui.end_row();
                grid_label(ui, t!("Fuente de descargas"));
                let mut src = vw.settings.download_source.clone();
                segmented(ui, &mut src, &[("auto".to_string(), t!("Automática")), ("official".to_string(), t!("Sólo oficial")), ("mirror".to_string(), t!("Espejo primero"))]);
                vw.settings.download_source = src;
                ui.end_row();
            });
            if let Some(r) = &s.net_report {
                ui.add_space(6.0);
                egui::Frame::none().fill(INPUT).rounding(8.0).inner_margin(egui::Margin::same(10.0)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    for line in r.lines() {
                        let color = if line.starts_with('✔') {
                            GREEN
                        } else if line.starts_with('✖') {
                            RED
                        } else if line.starts_with('⚠') {
                            YELLOW
                        } else {
                            TEXT_DIM
                        };
                        ui.label(RichText::new(line).monospace().color(color));
                    }
                });
            }
            ui.label(
                RichText::new(t!(
                    "Brasa reintenta solo por IPv4 y por espejos (BMCLAPI) si un servidor no responde. Tokens guardados en {}.",
                    if crate::account::secrets::AVAILABLE { t!("el almacén seguro del sistema") } else { t!("accounts.json (permisos 600)") }
                ))
                .size(12.5)
                .color(TEXT_DIM),
            );
        });
        ui.add_space(10.0);

        // Sólo lo que decide el usuario: los datos de distribución (IDs, claves, repo) van en el binario.
        if mdl.settings.discord_available() {
            card(ui, |ui| {
                section_title(ui, t!("Discord"));
                egui::Grid::new("set_int").num_columns(2).spacing([18.0, 10.0]).min_col_width(190.0).show(ui, |ui| {
                    grid_label(ui, t!("Estado en el perfil"));
                    ui.vertical(|ui| {
                        ui.checkbox(&mut vw.settings.discord_presence, t!("Mostrar en mi perfil que uso Brasa"));
                        let color = match s.discord {
                            crate::discord::Status::Connected => GREEN,
                            crate::discord::Status::Error(_) => RED,
                            _ => TEXT_DIM,
                        };
                        ui.label(RichText::new(s.discord.describe()).size(12.5).color(color));
                    });
                    ui.end_row();
                });
            });
            ui.add_space(10.0);
        }

        card(ui, |ui| {
            section_title(ui, t!("Launcher"));
            egui::Grid::new("set_upd").num_columns(2).spacing([18.0, 10.0]).min_col_width(190.0).show(ui, |ui| {
                grid_label(ui, t!("Idioma · Language"));
                segmented(ui, &mut vw.settings.language, &[("auto".to_string(), t!("Automático")), ("es".to_string(), t!("Español")), ("en".to_string(), t!("English"))]);
                ui.end_row();
                grid_label(ui, t!("Actualizaciones"));
                if mdl.settings.updates_available() {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut vw.settings.check_updates, t!("Buscar al arrancar"));
                        if ui.add(secondary_button(t!("Buscar ahora"))).clicked() {
                            out.push(Action::CheckUpdate);
                        }
                        ui.label(RichText::new(t!("en {}", mdl.settings.update_repo)).size(12.5).color(TEXT_DIM));
                    });
                } else {
                    let repo = mdl.settings.update_repo.trim();
                    let why = if repo.is_empty() {
                        t!("Esta copia no sabe de dónde actualizarse: se compiló sin repositorio (las del CI sí lo llevan)").to_string()
                    } else if crate::distro::looks_like_secret(repo) {
                        t!("Esta copia no sabe de dónde actualizarse: el valor del repositorio parece un token (no se muestra)").to_string()
                    } else {
                        t!("Esta copia no sabe de dónde actualizarse: «{repo}» no tiene la forma usuario/repo", repo = repo)
                    };
                    ui.label(RichText::new(why).color(TEXT_DIM));
                }
                ui.end_row();
                grid_label(ui, t!("Carpeta de datos"));
                ui.horizontal(|ui| {
                    ui.label(RichText::new(mdl.p.root.display().to_string()).monospace().size(12.5).color(TEXT_DIM));
                    if ui.add(secondary_button(t!("Abrir"))).clicked() {
                        out.push(Action::OpenDir(mdl.p.root.clone()));
                    }
                });
                ui.end_row();
            });
        });

        card(ui, |ui| {
            section_title(ui, t!("Acerca de"));
            ui.label(RichText::new(t!("Brasa Launcher {} · licencia MIT", env!("CARGO_PKG_VERSION"))).color(TEXT));
            ui.label(
                RichText::new(t!("No es un producto oficial de Minecraft. No está aprobado por Mojang ni Microsoft ni asociado a ellos. «Minecraft» es una marca de Mojang Synergies AB."))
                    .size(12.5)
                    .color(TEXT_DIM),
            );
            ui.label(RichText::new(t!("Brasa no recoge datos: no hay telemetría ni servidores propios.")).size(12.5).color(TEXT_DIM));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.add(secondary_button(t!("Avisos de terceros"))).on_hover_text(t!("Fuentes, librerías y lo que se descarga al usarlo")).clicked() {
                    vw.notice = Some(crate::gui::action::Notice::ThirdParty);
                }
                if ui.add(secondary_button(t!("Privacidad"))).clicked() {
                    vw.notice = Some(crate::gui::action::Notice::Privacy);
                }
                if ui.add(secondary_button(t!("Licencia"))).clicked() {
                    vw.notice = Some(crate::gui::action::Notice::License);
                }
                if ui.add(secondary_button(t!("Informar de un problema"))).clicked() {
                    crate::gui::view::report::open(vw);
                }
                if ui.add(ghost_button(t!("Ver la bienvenida otra vez"))).clicked() {
                    crate::gui::view::onboarding::start(vw);
                }
            });
        });
        ui.add_space(20.0);
    });
    // El borrador pasa al modelo en cuanto cambia (la vista nunca lo toca directamente).
    if vw.settings != mdl.settings {
        out.push(Action::ApplySettings(vw.settings.clone()));
    }
}
