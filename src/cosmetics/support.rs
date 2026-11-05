//! En qué instancias se ven los cosméticos dentro del juego.
//!
//! Dibujar dentro del juego necesita el mod de Brasa, y el código de dibujo de Minecraft cambia
//! entre familias de versiones, así que hay un adaptador por familia. Aquí está la lista: las
//! que ya existen y las que faltan. Todas leen el mismo `loadout.json` (ver `export.rs`).

use crate::config::{Instance, Loader};

/// Versión de Minecraft como números comparables: «1.20.1» → [1, 20, 1]; «26.1» → [26, 1, 0].
pub fn mc_key(v: &str) -> [u32; 3] {
    let mut out = [0; 3];
    for (i, part) in v.split(['.', '-', ' ']).take(3).enumerate() {
        out[i] = part.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0);
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Family {
    /// Nombre del adaptador (y del jar: `brasa-cosmetics-<id>.jar`).
    pub id: &'static str,
    pub label: &'static str,
    pub fabric: bool,
    pub forge: bool,
    /// Primera y última versión (incluidas).
    pub from: [u32; 3],
    pub to: [u32; 3],
    /// `true` si el adaptador ya está escrito.
    pub ready: bool,
}

/// Todas las familias, de la más nueva a la más antigua.
pub const FAMILIES: &[Family] = &[
    Family { id: "fabric-1.21.2", label: "Fabric 1.21.2 o posterior", fabric: true, forge: false, from: [1, 21, 2], to: [99, 99, 99], ready: false },
    Family { id: "fabric-1.21.1", label: "Fabric 1.21–1.21.1", fabric: true, forge: false, from: [1, 21, 0], to: [1, 21, 1], ready: true },
    Family { id: "fabric-1.20", label: "Fabric 1.20–1.20.6", fabric: true, forge: false, from: [1, 20, 0], to: [1, 20, 6], ready: false },
    Family { id: "fabric-1.16", label: "Fabric 1.16.5–1.19.4", fabric: true, forge: false, from: [1, 16, 5], to: [1, 19, 4], ready: false },
    Family { id: "neoforge-1.21.1", label: "NeoForge 1.21.1", fabric: false, forge: true, from: [1, 21, 0], to: [1, 21, 1], ready: false },
    Family { id: "forge-1.20.1", label: "Forge 1.20.1", fabric: false, forge: true, from: [1, 20, 1], to: [1, 20, 1], ready: false },
    Family { id: "forge-1.12.2", label: "Forge 1.12.2", fabric: false, forge: true, from: [1, 12, 2], to: [1, 12, 2], ready: false },
    Family { id: "forge-1.8.9", label: "Forge 1.8.9", fabric: false, forge: true, from: [1, 8, 9], to: [1, 8, 9], ready: false },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Se ven en el juego con este adaptador.
    Ready(Family),
    /// Hay adaptador previsto, aún no escrito.
    Planned(Family),
    /// Vanilla no carga mods.
    Vanilla,
    /// Ninguna familia cubre esa versión o loader.
    Unsupported,
}

pub fn family_for(loader: Loader, mc: &str) -> Option<Family> {
    let k = mc_key(mc);
    let fabricish = matches!(loader, Loader::Fabric | Loader::Quilt);
    let forgeish = matches!(loader, Loader::Forge | Loader::NeoForge);
    FAMILIES.iter().copied().find(|f| {
        ((f.fabric && fabricish) || (f.forge && forgeish))
            && k >= f.from
            && k <= f.to
            && (loader != Loader::NeoForge || f.id.starts_with("neoforge"))
            && (loader != Loader::Forge || f.id.starts_with("forge"))
    })
}

pub fn status(inst: &Instance) -> Status {
    status_for(inst.loader, &inst.mc_version)
}

pub fn status_for(loader: Loader, mc: &str) -> Status {
    if loader == Loader::Vanilla {
        return Status::Vanilla;
    }
    match family_for(loader, mc) {
        Some(f) if f.ready => Status::Ready(f),
        Some(f) => Status::Planned(f),
        None => Status::Unsupported,
    }
}

impl Status {
    pub fn in_game(&self) -> bool {
        matches!(self, Status::Ready(_))
    }

    /// Lo mismo en pocas palabras (tras el nombre de la instancia).
    pub fn short(&self) -> String {
        match self {
            Status::Ready(_) => t!("se ven en el juego").to_string(),
            Status::Planned(_) => t!("aún no se ven en el juego; su adaptador está en camino").to_string(),
            Status::Vanilla => t!("Vanilla no carga mods; sólo se ven aquí").to_string(),
            Status::Unsupported => t!("esta versión no tiene adaptador").to_string(),
        }
    }

    /// Frase para la consola y la Taquilla.
    pub fn describe(&self) -> String {
        match self {
            Status::Ready(f) => format!("Los cosméticos se ven en el juego (adaptador {})", f.label),
            Status::Planned(f) => format!("En esta instancia aún no se ven en el juego: el adaptador para {} está en camino", f.label),
            Status::Vanilla => "Las instancias Vanilla no cargan mods: los cosméticos sólo se ven en la Taquilla".to_string(),
            Status::Unsupported => "Esta versión no tiene adaptador de cosméticos".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_as_numbers() {
        assert!(mc_key("1.21.1") > mc_key("1.21"));
        assert!(mc_key("1.21.10") > mc_key("1.21.2"));
        assert!(mc_key("26.1") > mc_key("1.21.10"));
        assert_eq!(mc_key("1.20.1-pre2"), [1, 20, 1]);
    }

    #[test]
    fn every_instance_finds_its_family() {
        assert!(matches!(status_for(Loader::Fabric, "1.21.1"), Status::Ready(f) if f.id == "fabric-1.21.1"));
        assert!(matches!(status_for(Loader::Quilt, "1.21"), Status::Ready(_)), "Quilt carga mods de Fabric");
        assert!(matches!(status_for(Loader::Fabric, "1.21.4"), Status::Planned(f) if f.id == "fabric-1.21.2"));
        assert!(matches!(status_for(Loader::Fabric, "1.20.1"), Status::Planned(f) if f.id == "fabric-1.20"));
        assert!(matches!(status_for(Loader::Forge, "1.20.1"), Status::Planned(f) if f.id == "forge-1.20.1"));
        assert!(matches!(status_for(Loader::NeoForge, "1.21.1"), Status::Planned(f) if f.id == "neoforge-1.21.1"));
        assert!(matches!(status_for(Loader::Forge, "1.21.1"), Status::Unsupported));
        assert_eq!(status_for(Loader::Vanilla, "1.21.1"), Status::Vanilla);
        assert!(status_for(Loader::Fabric, "1.21.1").in_game());
    }
}
