//! Cuentas: premium, offline y skins.

use crate::account::auth::AccountKind;
use crate::account::skins;
use crate::gui::action::Action;
use crate::gui::model::{Model, Session};
use crate::gui::state::ViewState;
use crate::gui::view::fmt::*;
use crate::gui::view::fonts::Weight;
use crate::gui::view::theme::*;
use crate::gui::view::widgets::*;
use eframe::egui::{self, RichText};

pub fn draw(ui: &mut egui::Ui, mdl: &Model, vw: &mut ViewState, s: &Session, out: &mut Vec<Action>) {
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        page_header(ui, t!("Cuentas"), t!("Premium (Microsoft), no premium o no premium con skin."));

        if mdl.accounts.accounts.is_empty() {
            card(ui, |ui| {
                ui.label(RichText::new(t!("Todavía no hay cuentas. Añade una abajo para poder jugar.")).color(YELLOW));
            });
        }
        let active = mdl.accounts.active.clone();
        for a in &mdl.accounts.accounts {
            let is_active = active.as_deref() == Some(a.uuid.as_str());
            let color = if a.is_premium() {
                GREEN
            } else if a.kind == AccountKind::Yggdrasil {
                BLUE
            } else {
                TEXT_DIM
            };
            egui::Frame::none()
                .fill(if is_active { CARD_HI } else { CARD })
                .rounding(12.0)
                .stroke(egui::Stroke::new(if is_active { 1.5_f32 } else { 1.0 }, if is_active { ACCENT } else { BORDER }))
                .inner_margin(egui::Margin::same(12.0))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let _ = avatar(ui, &a.username, 40.0, color);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            ui.label(RichText::new(&a.username).size(16.0).semibold().color(TEXT));
                            chip(ui, &a.label(), color);
                        });
                        if a.kind == AccountKind::Yggdrasil && a.profiles.len() > 1 {
                            ui.add_space(12.0);
                            egui::ComboBox::new(format!("char-{}", a.uuid), "").selected_text(t!("Personaje: {}", a.username)).show_ui(ui, |ui| {
                                for pr in &a.profiles {
                                    if ui.selectable_label(pr.name == a.username, pr.name.as_str()).clicked() && pr.name != a.username {
                                        out.push(Action::SelectCharacter(a.uuid.clone(), pr.id.clone()));
                                    }
                                }
                            });
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(danger_ghost_button(t!("Eliminar"))).clicked() {
                                out.push(Action::RemoveAccount(a.uuid.clone()));
                            }
                            if is_active {
                                chip(ui, t!("✔ En uso"), ACCENT);
                            } else if ui.add(secondary_button(t!("Usar esta"))).clicked() {
                                out.push(Action::SetActive(a.uuid.clone()));
                            }
                        });
                    });
                });
            ui.add_space(6.0);
        }

        ui.add_space(10.0);
        section_title(ui, t!("Añadir cuenta"));
        ui.columns(2, |cols| {
            // ---------------- PREMIUM
            let ui = &mut cols[0];
            card(ui, |ui| {
                ui.label(RichText::new(t!("⭐ Premium")).size(18.0).semibold().color(GREEN));
                ui.label(RichText::new(t!("Cuenta de Microsoft con Minecraft comprado: skins oficiales, Realms y cualquier servidor.")).color(TEXT_DIM));
                ui.add_space(8.0);
                if let Some((code, uri)) = &s.device_code {
                    ui.label(t!("Abre el enlace e introduce este código:"));
                    egui::Frame::none().fill(INPUT).rounding(10.0).inner_margin(egui::Margin::symmetric(16.0, 10.0)).show(ui, |ui| {
                        ui.label(RichText::new(code).size(30.0).monospace().semibold().color(ACCENT));
                    });
                    ui.horizontal(|ui| {
                        if ui.add(secondary_button(t!("📋  Copiar código"))).clicked() {
                            ui.ctx().output_mut(|o| o.copied_text = code.clone());
                        }
                        ui.hyperlink_to(uri.as_str(), uri.as_str());
                    });
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new(t!("Esperando confirmación…")).color(TEXT_DIM));
                    });
                } else if mdl.settings.microsoft_available() {
                    if ui.add(primary_button(t!("Iniciar sesión con Microsoft"), 250.0, 38.0)).clicked() {
                        out.push(Action::MsLogin);
                    }
                } else {
                    ui.add_enabled(false, primary_button(t!("Iniciar sesión con Microsoft"), 250.0, 38.0));
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(t!(
                            "Esta copia de Brasa no incluye el inicio de sesión con Microsoft. Mientras tanto puedes jugar con una cuenta no premium."
                        ))
                        .size(12.5)
                        .color(TEXT_DIM),
                    );
                }
                if mdl.settings.microsoft_available() {
                    ui.label(RichText::new(t!("Si la cuenta no tiene el juego, se abre en modo demo.")).size(12.5).color(TEXT_DIM));
                }
            });

            // ---------------- NO PREMIUM
            let ui = &mut cols[1];
            card(ui, |ui| {
                ui.label(RichText::new(t!("No premium")).size(18.0).semibold().color(TEXT));
                ui.add_space(4.0);
                field_label(ui, t!("Rápida: sólo nombre"));
                ui.horizontal(|ui| {
                    let r = ui.add(egui::TextEdit::singleline(&mut vw.offline_name).hint_text(t!("Nombre (3-16)")).desired_width(190.0));
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.add(secondary_button(t!("Añadir"))).clicked() || enter {
                        out.push(Action::AddOffline);
                    }
                });
                ui.label(RichText::new(t!("Un jugador, LAN y servidores no premium (online-mode=false).")).size(12.5).color(TEXT_DIM));
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(6.0);
                field_label(ui, t!("Con skin: Ely.by / LittleSkin"));
                let mut opts: Vec<(usize, &str)> = crate::account::yggdrasil::PRESETS.iter().enumerate().map(|(i, (_, n, _))| (i, *n)).collect();
                opts.push((crate::account::yggdrasil::PRESETS.len(), t!("Otro servidor")));
                segmented(ui, &mut vw.skin_server, &opts);
                if vw.skin_server >= crate::account::yggdrasil::PRESETS.len() {
                    ui.add(egui::TextEdit::singleline(&mut vw.skin_custom).hint_text("https://tu-servidor/api/yggdrasil").desired_width(f32::INFINITY));
                }
                ui.add(egui::TextEdit::singleline(&mut vw.skin_user).hint_text(t!("Usuario o email")).desired_width(f32::INFINITY));
                let r = ui.add(egui::TextEdit::singleline(&mut vw.skin_pass).password(true).hint_text(t!("Contraseña")).desired_width(f32::INFINITY));
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.add(secondary_button(t!("Iniciar sesión"))).clicked() || enter {
                    out.push(Action::SkinLogin);
                }
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t!("Cuenta gratis:")).size(12.5).color(TEXT_DIM));
                    ui.hyperlink_to(t!("Ely.by"), "https://account.ely.by/register");
                    ui.hyperlink_to(t!("LittleSkin"), "https://littleskin.cn/auth/register");
                });
                ui.label(RichText::new(t!("La contraseña no se guarda.")).size(12.5).color(TEXT_DIM));
            });
        });

        // ---------------- SKIN de la cuenta activa
        if let Some(a) = mdl.accounts.active() {
            ui.add_space(12.0);
            card(ui, |ui| {
                ui.label(RichText::new(t!("👕 Skin de {}", a.username)).size(16.0).semibold().color(TEXT));
                ui.add_space(4.0);
                match a.kind {
                    AccountKind::Microsoft if a.is_premium() => {
                        ui.horizontal(|ui| {
                            if ui.add(secondary_button(t!("Subir skin (clásica)…"))).clicked() {
                                out.push(Action::PickPremiumSkin(false));
                            }
                            if ui.add(secondary_button(t!("Subir skin (slim)…"))).clicked() {
                                out.push(Action::PickPremiumSkin(true));
                            }
                            if ui.add(danger_ghost_button(t!("Quitar skin"))).clicked() {
                                out.push(Action::ResetPremiumSkin);
                            }
                        });
                        ui.label(RichText::new(t!("Se cambia en tu cuenta oficial: la ve todo el mundo.")).size(12.5).color(TEXT_DIM));
                    }
                    AccountKind::Microsoft => {
                        ui.label(RichText::new(t!("Las cuentas en modo demo no pueden cambiar la skin.")).color(TEXT_DIM));
                    }
                    AccountKind::Yggdrasil => {
                        let page = skins::yggdrasil_skin_page(a.auth_server.as_deref().unwrap_or_default());
                        if ui.add(secondary_button(t!("Cambiar skin en la web"))).clicked() {
                            out.push(Action::OpenUrl(page));
                        }
                        ui.label(
                            RichText::new(t!("La skin se gestiona en la web de tu servidor y se ve en el juego automáticamente.")).size(12.5).color(TEXT_DIM),
                        );
                    }
                    AccountKind::Offline => {
                        ui.horizontal(|ui| {
                            match &a.skin_path {
                                Some(path) => {
                                    ui.label(RichText::new(elide(path, 60)).monospace().size(12.0).color(TEXT_DIM));
                                    if ui.add(danger_ghost_button(t!("Quitar"))).clicked() {
                                        out.push(Action::ClearOfflineSkin);
                                    }
                                }
                                None => {
                                    ui.label(RichText::new(t!("Sin skin local.")).color(TEXT_DIM));
                                }
                            }
                            if ui.add(secondary_button(t!("Elegir PNG…"))).clicked() {
                                out.push(Action::PickOfflineSkin);
                            }
                        });
                        ui.label(RichText::new(t!("Skin local: sólo la ves tú. Para que la vean otros usa Ely.by o LittleSkin.")).size(12.5).color(TEXT_DIM));
                    }
                }
            });
        }
    });
}
