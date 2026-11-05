//! Qué lleva puesto cada cuenta. Se guarda en `cosmetics.toml`, en la carpeta de Brasa.

use super::{catalog, Slot};
use crate::paths::Paths;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Cuántos emotes caben en la rueda del juego.
pub const WHEEL: usize = 8;

#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Debug)]
pub struct Loadout {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cape: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hat: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub back: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pet: Option<String>,
    /// Emotes en la rueda, en orden.
    #[serde(default)]
    pub emotes: Vec<String>,
    /// Color elegido para los teñibles (por id).
    #[serde(default)]
    pub tints: BTreeMap<String, [u8; 3]>,
}

impl Loadout {
    pub fn equipped(&self, slot: Slot) -> Option<&str> {
        match slot {
            Slot::Cape => self.cape.as_deref(),
            Slot::Hat => self.hat.as_deref(),
            Slot::Back => self.back.as_deref(),
            Slot::Pet => self.pet.as_deref(),
            Slot::Emote => None,
        }
    }

    pub fn is_on(&self, id: &str) -> bool {
        match catalog::get(id).map(|c| c.slot) {
            Some(Slot::Emote) => self.emotes.iter().any(|e| e == id),
            Some(s) => self.equipped(s) == Some(id),
            None => false,
        }
    }

    /// Pone o quita: un objeto por hueco; los emotes entran o salen de la rueda.
    /// Devuelve `false` si no se pudo (id desconocido o rueda llena).
    pub fn toggle(&mut self, id: &str) -> bool {
        let Some(c) = catalog::get(id) else { return false };
        let on = self.is_on(id);
        let new = (!on).then(|| id.to_string());
        match c.slot {
            Slot::Cape => self.cape = new,
            Slot::Hat => self.hat = new,
            Slot::Back => self.back = new,
            Slot::Pet => self.pet = new,
            Slot::Emote => {
                if on {
                    self.emotes.retain(|e| e != id);
                } else if self.emotes.len() < WHEEL {
                    self.emotes.push(id.to_string());
                } else {
                    return false;
                }
            }
        }
        true
    }

    pub fn tint(&self, id: &str) -> Option<[u8; 3]> {
        self.tints.get(id).copied().or_else(|| catalog::get(id).and_then(|c| c.tint))
    }

    pub fn is_empty(&self) -> bool {
        self.cape.is_none() && self.hat.is_none() && self.back.is_none() && self.pet.is_none() && self.emotes.is_empty()
    }

    /// Quita lo que ya no existe en el catálogo (p. ej. tras actualizar Brasa).
    pub fn clean(&mut self) {
        let keep = |x: &mut Option<String>| {
            if x.as_deref().is_some_and(|id| catalog::get(id).is_none()) {
                *x = None;
            }
        };
        keep(&mut self.cape);
        keep(&mut self.hat);
        keep(&mut self.back);
        keep(&mut self.pet);
        self.emotes.retain(|e| catalog::get(e).is_some_and(|c| c.slot == Slot::Emote));
        self.emotes.truncate(WHEEL);
    }

    pub fn count(&self) -> usize {
        [&self.cape, &self.hat, &self.back, &self.pet].iter().filter(|x| x.is_some()).count() + self.emotes.len()
    }
}

/// Todas las cuentas. La clave es el UUID de la cuenta (o «default» sin cuenta).
#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Debug)]
pub struct Wardrobe {
    #[serde(default)]
    pub accounts: BTreeMap<String, Loadout>,
}

pub fn file(p: &Paths) -> std::path::PathBuf {
    p.root.join("cosmetics.toml")
}

impl Wardrobe {
    pub fn load(p: &Paths) -> Wardrobe {
        let mut w: Wardrobe = std::fs::read_to_string(file(p)).ok().and_then(|s| toml::from_str(&s).ok()).unwrap_or_default();
        w.accounts.values_mut().for_each(Loadout::clean);
        w
    }

    pub fn save(&self, p: &Paths) -> Result<()> {
        crate::paths::write_atomic(&file(p), toml::to_string_pretty(self)?)?;
        Ok(())
    }

    /// Lo de una cuenta; una cuenta nueva empieza con la capa de Brasa y tres emotes.
    pub fn of(&self, key: &str) -> Loadout {
        self.accounts.get(key).cloned().unwrap_or_else(starter)
    }

    pub fn set(&mut self, key: &str, l: Loadout) {
        self.accounts.insert(key.to_string(), l);
    }
}

pub fn starter() -> Loadout {
    Loadout { cape: Some("cape.brasa".into()), emotes: vec!["emote.saludar".into(), "emote.bailar".into(), "emote.aplaudir".into()], ..Default::default() }
}

pub fn key_for(acc: Option<&crate::account::auth::Account>) -> String {
    acc.map(|a| a.uuid.clone()).unwrap_or_else(|| "default".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_item_per_slot_and_a_wheel_of_emotes() {
        let mut l = Loadout::default();
        assert!(l.toggle("hat.corona"));
        assert!(l.toggle("hat.gorro"));
        assert_eq!(l.hat.as_deref(), Some("hat.gorro"), "el segundo sombrero sustituye al primero");
        assert!(l.toggle("hat.gorro"));
        assert_eq!(l.hat, None, "pulsar otra vez lo quita");
        let emotes: Vec<_> = catalog::of(Slot::Emote).map(|c| c.id).collect();
        for e in &emotes {
            l.toggle(e);
        }
        assert_eq!(l.emotes.len(), WHEEL.min(emotes.len()));
        assert!(!l.toggle("nada.raro"));
    }

    #[test]
    fn tints_default_to_the_catalog_and_unknown_items_are_dropped() {
        let mut l = Loadout {
            hat: Some("hat.gorro".into()),
            cape: Some("cape.ya_no_existe".into()),
            emotes: vec!["emote.bailar".into(), "hat.corona".into()],
            ..Default::default()
        };
        assert_eq!(l.tint("hat.gorro"), catalog::get("hat.gorro").unwrap().tint);
        l.tints.insert("hat.gorro".into(), [1, 2, 3]);
        assert_eq!(l.tint("hat.gorro"), Some([1, 2, 3]));
        l.clean();
        assert_eq!(l.cape, None);
        assert_eq!(l.emotes, vec!["emote.bailar".to_string()]);
    }

    #[test]
    fn wardrobe_roundtrip() {
        let root = std::env::temp_dir().join(format!("brasa-cos-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let p = Paths { root: root.clone() };
        let mut w = Wardrobe::load(&p);
        assert_eq!(w.of("abc"), starter());
        let mut l = starter();
        l.toggle("pet.gato");
        l.tints.insert("pet.gato".into(), [9, 9, 9]);
        w.set("abc", l.clone());
        w.save(&p).unwrap();
        assert_eq!(Wardrobe::load(&p).of("abc"), l);
        let _ = std::fs::remove_dir_all(root);
    }
}
