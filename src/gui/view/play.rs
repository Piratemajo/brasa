//! El control de JUGAR, igual en la ficha de la instancia y en Inicio: JUGAR → preparando
//! (fase y progreso) → CERRAR con el tiempo de partida.

use crate::gui::action::Action;
use crate::gui::model::{Launch, Session};
use crate::gui::view::art::{big_button, progress_button, BigKind};
use crate::gui::view::theme::*;
use crate::gui::view::widgets::*;
use eframe::egui::{self, RichText};

/// Resume en pocas palabras la fase en la que está el lanzamiento (el detalle va en el tooltip).
pub fn launch_phase(stage: &str) -> &'static str {
    let s = stage.to_lowercase();
    let has = |w: &str| s.contains(w);
    if has("sesión") {
        t!("Iniciando sesión")
    } else if has("skin") {
        t!("Preparando la skin")
    } else if has("java") && (has("descargando") || has("extrayendo")) {
        t!("Instalando Java")
    } else if has("java") {
        t!("Buscando Java")
    } else if has("instalador") || (has("instalando") && !has("java")) {
        t!("Instalando el loader")
    } else if has("descargando") {
        t!("Descargando el juego")
    } else if has("copia de seguridad") {
        t!("Copiando tus mundos")
    } else if has("comando previo") {
        t!("Comando previo")
    } else if has("jugando") {
        t!("Abriendo Minecraft")
    } else {
        t!("Preparando")
    }
}

/// «12 min», «1 h 05 min».
pub fn fmt_elapsed(secs: u64) -> String {
    let m = secs / 60;
    if m < 60 {
        t!("{m} min", m = m)
    } else {
        t!("{} h {:02} min", m / 60, m % 60)
    }
}

fn fraction(progress: &Option<(usize, usize)>) -> Option<f32> {
    progress.filter(|(_, t)| *t > 0).map(|(d, t)| d as f32 / t as f32)
}

/// El botón grande: JUGAR, el progreso mientras se prepara o CERRAR con el juego abierto.
pub fn play_control(ui: &mut egui::Ui, s: &Session, id: &str, size: egui::Vec2, out: &mut Vec<Action>) {
    match s.launch(id) {
        Some(Launch::Preparing { stage, progress }) => {
            progress_button(ui, launch_phase(stage), fraction(progress), size).on_hover_text(stage.as_str());
        }
        Some(Launch::Running { .. }) => {
            if big_button(ui, BigKind::Stop, t!("CERRAR"), size, true).on_hover_text(t!("Cierra el juego a la fuerza (lo no guardado se pierde)")).clicked() {
                out.push(Action::Kill(id.to_string()));
            }
        }
        None => {
            if big_button(ui, BigKind::Play, t!("JUGAR"), size, true).on_hover_text(t!("Descarga lo que falte y lanza el juego")).clicked() {
                out.push(Action::Play(id.to_string()));
            }
        }
    }
}

/// Texto corto de estado («En juego · 12 min», «Preparando…») o `None` si no se está lanzando.
pub fn status_text(s: &Session, id: &str) -> Option<(String, egui::Color32)> {
    match s.launch(id)? {
        Launch::Preparing { .. } => Some((t!("Preparando…").to_string(), ACCENT_HI)),
        Launch::Running { since } => Some((t!("En juego · {}", fmt_elapsed(since.elapsed().as_secs())), GREEN)),
    }
}

/// Versión pequeña para las tarjetas de Inicio.
pub fn small_play(ui: &mut egui::Ui, s: &Session, id: &str, out: &mut Vec<Action>) {
    match s.launch(id) {
        Some(Launch::Preparing { stage, progress }) => {
            let txt = match fraction(progress) {
                Some(f) => format!("{} %", (f * 100.0).floor() as u32),
                None => t!("Preparando…").into(),
            };
            ui.add_sized(egui::vec2(92.0, 30.0), egui::Label::new(RichText::new(txt).color(ACCENT_HI))).on_hover_text(stage.as_str());
        }
        Some(Launch::Running { .. }) => {
            let b = egui::Button::new(RichText::new(t!("■  Cerrar")).color(egui::Color32::WHITE)).fill(RED).rounding(7.0).min_size(egui::vec2(92.0, 30.0));
            if ui.add(b).clicked() {
                out.push(Action::Kill(id.to_string()));
            }
        }
        None => {
            if ui.add(primary_button(t!("▶  Jugar"), 92.0, 30.0)).clicked() {
                out.push(Action::Play(id.to_string()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phases_in_plain_words() {
        // los textos reales que reporta el lanzamiento
        assert_eq!(launch_phase("Comprobando sesión de Steve"), "Iniciando sesión");
        assert_eq!(launch_phase("Preparando skins de Ely.by"), "Preparando la skin");
        assert_eq!(launch_phase("Resolviendo 1.21.1"), "Preparando");
        assert_eq!(launch_phase("Descargando 3412 ficheros de 1.21.1"), "Descargando el juego");
        assert_eq!(launch_phase("Buscando Java 21 en el sistema"), "Buscando Java");
        assert_eq!(launch_phase("Descargando Java (java-runtime-delta) del servidor de Mojang"), "Instalando Java");
        assert_eq!(launch_phase("Extrayendo Java 21"), "Instalando Java");
        assert_eq!(launch_phase("Instalando Fabric 0.16.9"), "Instalando el loader");
        assert_eq!(launch_phase("Ejecutando instalador de Forge (puede tardar un par de minutos)"), "Instalando el loader");
        assert_eq!(launch_phase("Copia de seguridad de mundos"), "Copiando tus mundos");
        assert_eq!(launch_phase("Jugando: Create 1.20"), "Abriendo Minecraft");
        assert_eq!(launch_phase("algo nuevo"), "Preparando");
    }

    #[test]
    fn elapsed() {
        assert_eq!(fmt_elapsed(30), "0 min");
        assert_eq!(fmt_elapsed(12 * 60 + 5), "12 min");
        assert_eq!(fmt_elapsed(65 * 60), "1 h 05 min");
    }
}
