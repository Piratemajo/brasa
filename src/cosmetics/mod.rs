//! Cosméticos: capas, sombreros, cosas a la espalda, emotes y mascotas.
//!
//! El launcher define el catálogo, enseña la vista previa en 3D y guarda qué lleva cada cuenta.
//! Antes de jugar escribe en la instancia (`config/brasa/cosmetics/`) todo lo equipado, ya
//! resuelto: modelos, texturas PNG y animaciones. El mod del juego sólo tiene que leerlo y
//! dibujarlo, así que cada versión de Minecraft necesita un adaptador pequeño y nada más.
//!
//! # Sistema de coordenadas (el mismo en el launcher y en el mod)
//!
//! Unidades: píxeles de skin (1/16 de bloque). Origen en los pies del jugador.
//! - `x` hacia la **izquierda del jugador** (su brazo derecho está en `x < 0`)
//! - `y` hacia arriba (la cabeza va de `y = 24` a `y = 32`)
//! - `z` hacia **delante** (hacia donde mira)
//!
//! Rotaciones en grados, aplicadas como en Minecraft: primero X, luego Y, luego Z
//! (`R = Rz · Ry · Rx`). Para pasar al espacio de modelo de Minecraft (y hacia abajo desde el
//! cuello, mirando hacia -z): `(x, 24 - y, -z)` y rotaciones `(rx, -ry, -rz)`.
//!
//! Las texturas usan el «box UV» de Minecraft: un cubo de `w × h × d` con origen `uv` ocupa
//! `2·(w + d) × (h + d)` píxeles, igual que las partes de una skin.

pub mod catalog;
pub mod export;
pub mod install;
pub mod loadout;
pub mod paint;
pub mod pose;
pub mod support;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Versión del formato que se escribe en la instancia. El mod rechaza versiones que no conoce.
pub const FORMAT: u32 = 1;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Slot {
    Cape,
    Hat,
    Back,
    Emote,
    Pet,
}

impl Slot {
    pub const ALL: [Slot; 5] = [Slot::Cape, Slot::Hat, Slot::Back, Slot::Emote, Slot::Pet];
    pub fn label(self) -> &'static str {
        match self {
            Slot::Cape => t!("Capas"),
            Slot::Hat => t!("Sombreros"),
            Slot::Back => t!("Espalda"),
            Slot::Emote => "Emotes",
            Slot::Pet => t!("Mascotas"),
        }
    }
}

/// A qué parte del jugador va pegado un modelo.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Attach {
    /// Sigue a la cabeza (gira y se inclina con ella).
    Head,
    /// Sigue al cuerpo (espalda).
    Body,
    /// Flota junto al jugador, sin seguir sus huesos (mascotas).
    Beside,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    X,
    Y,
    Z,
}

/// Movimiento continuo de un hueso. `t` en segundos; los ángulos en grados.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Motion {
    /// Oscila alrededor de un eje: `amp · sin(2π · speed · t + phase)` (alas, colas).
    Swing {
        axis: Axis,
        amp: f32,
        speed: f32,
        #[serde(default)]
        phase: f32,
    },
    /// Sube y baja: `amp · sin(2π · speed · t)` píxeles (aureolas, mascotas).
    Bob { amp: f32, speed: f32 },
    /// Gira sin parar alrededor de Y, `speed` grados por segundo.
    Spin { speed: f32 },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Cube {
    pub from: [f32; 3],
    pub size: [f32; 3],
    pub uv: [u32; 2],
    #[serde(default, skip_serializing_if = "is_zero")]
    pub inflate: f32,
}

fn is_zero(v: &f32) -> bool {
    *v == 0.0
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Bone {
    pub name: String,
    pub pivot: [f32; 3],
    #[serde(default)]
    pub rot: [f32; 3],
    #[serde(default)]
    pub cubes: Vec<Cube>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion: Option<Motion>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Bone>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Model {
    pub texture_size: [u32; 2],
    pub attach: Attach,
    pub bones: Vec<Bone>,
}

/// Fotograma clave de un emote: rotación de los huesos del jugador en ese instante.
/// Huesos: `head`, `body`, `right_arm`, `left_arm`, `right_leg`, `left_leg`.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Key {
    pub t: f32,
    #[serde(default)]
    pub bones: BTreeMap<String, [f32; 3]>,
    /// Altura extra de todo el cuerpo, en píxeles (saltos, sentarse).
    #[serde(default)]
    pub lift: f32,
    /// Giro de todo el cuerpo alrededor de Y, en grados.
    #[serde(default)]
    pub yaw: f32,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Emote {
    /// Segundos.
    pub duration: f32,
    pub looping: bool,
    pub keys: Vec<Key>,
}

/// Un cosmético del catálogo (sin la textura: se genera aparte, según el color elegido).
#[derive(Clone, Debug)]
pub struct Cosmetic {
    pub id: &'static str,
    pub name: &'static str,
    pub slot: Slot,
    pub description: &'static str,
    /// Color por defecto si se puede teñir.
    pub tint: Option<[u8; 3]>,
    /// Fotogramas de la textura (capas animadas) y a cuántos por segundo.
    pub frames: u32,
    pub fps: f32,
}

impl Cosmetic {
    pub fn tintable(&self) -> bool {
        self.tint.is_some()
    }
    pub fn animated(&self) -> bool {
        self.frames > 1
    }
}
