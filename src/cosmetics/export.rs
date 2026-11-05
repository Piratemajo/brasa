//! Antes de jugar: escribe en la instancia lo que lleva puesto la cuenta, ya resuelto, para que
//! el mod lo dibuje sin conocer el catálogo.
//!
//! ```text
//! <instancia>/.minecraft/config/brasa/cosmetics/
//!   loadout.json        formato, jugador, capa, objetos y emotes
//!   cape_0.png …        fotogramas de la capa (64×32)
//!   hat.png, back.png, pet.png
//! ```

use super::loadout::Loadout;
use super::{catalog, Emote, Model, Slot, FORMAT};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct CapeOut {
    pub id: String,
    pub name: String,
    pub frames: Vec<String>,
    pub fps: f32,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct ItemOut {
    pub id: String,
    pub name: String,
    pub slot: Slot,
    pub texture: String,
    pub model: Model,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct EmoteOut {
    pub id: String,
    pub name: String,
    pub emote: Emote,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Export {
    pub format: u32,
    pub generator: String,
    /// Nombre del jugador al que se le ponen (los demás jugadores no cambian).
    pub player: String,
    #[serde(default)]
    pub cape: Option<CapeOut>,
    #[serde(default)]
    pub items: Vec<ItemOut>,
    #[serde(default)]
    pub emotes: Vec<EmoteOut>,
}

pub fn dir(game_dir: &Path) -> PathBuf {
    game_dir.join("config").join("brasa").join("cosmetics")
}

/// Escribe (o borra, si no lleva nada) los cosméticos en la instancia.
pub fn write(game_dir: &Path, l: &Loadout, player: &str) -> Result<Option<Export>> {
    let d = dir(game_dir);
    let _ = std::fs::remove_dir_all(&d);
    if l.is_empty() {
        return Ok(None);
    }
    std::fs::create_dir_all(&d).with_context(|| format!("no se pudo crear {}", d.display()))?;
    let mut out = Export {
        format: FORMAT,
        generator: format!("Brasa {}", env!("CARGO_PKG_VERSION")),
        player: player.to_string(),
        cape: None,
        items: vec![],
        emotes: vec![],
    };

    if let Some(c) = l.cape.as_deref().and_then(catalog::get) {
        let mut frames = vec![];
        for f in 0..c.frames.max(1) {
            let name = format!("cape_{f}.png");
            let cv = catalog::cape(c.id, l.tint(c.id), f).context("capa sin textura")?;
            std::fs::write(d.join(&name), cv.to_png())?;
            frames.push(name);
        }
        out.cape = Some(CapeOut { id: c.id.into(), name: crate::i18n::tr(c.name).into(), frames, fps: c.fps });
    }
    for (slot, file) in [(Slot::Hat, "hat.png"), (Slot::Back, "back.png"), (Slot::Pet, "pet.png")] {
        if let Some(c) = l.equipped(slot).and_then(catalog::get) {
            let (model, cv) = catalog::build(c.id, l.tint(c.id)).context("modelo desconocido")?;
            std::fs::write(d.join(file), cv.to_png())?;
            out.items.push(ItemOut { id: c.id.into(), name: crate::i18n::tr(c.name).into(), slot, texture: file.into(), model });
        }
    }
    for id in &l.emotes {
        if let (Some(c), Some(e)) = (catalog::get(id), catalog::emote(id)) {
            out.emotes.push(EmoteOut { id: c.id.into(), name: crate::i18n::tr(c.name).into(), emote: e });
        }
    }
    crate::paths::write_atomic(&d.join("loadout.json"), serde_json::to_vec_pretty(&out)?)?;
    Ok(Some(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_everything_the_mod_needs_and_cleans_up() {
        let game = std::env::temp_dir().join(format!("brasa-exp-{}", uuid::Uuid::new_v4()));
        let mut l = Loadout {
            cape: Some("cape.aurora".into()),
            hat: Some("hat.gorro".into()),
            pet: Some("pet.murcielago".into()),
            emotes: vec!["emote.bailar".into()],
            ..Default::default()
        };
        l.tints.insert("hat.gorro".into(), [10, 200, 10]);
        let e = write(&game, &l, "Steve").unwrap().unwrap();
        let d = dir(&game);
        assert_eq!(e.cape.as_ref().unwrap().frames.len(), 12);
        assert!(d.join("cape_11.png").exists() && d.join("hat.png").exists() && d.join("pet.png").exists());
        assert!(!d.join("back.png").exists());
        // el json se vuelve a leer igual (es lo que lee el mod)
        let back: Export = serde_json::from_slice(&std::fs::read(d.join("loadout.json")).unwrap()).unwrap();
        assert_eq!(back, e);
        assert_eq!(back.format, FORMAT);
        assert_eq!(back.items.iter().map(|i| i.slot).collect::<Vec<_>>(), vec![Slot::Hat, Slot::Pet]);
        assert_eq!(back.emotes[0].id, "emote.bailar");
        // la textura teñida es la que se escribe
        let img = image::open(d.join("hat.png")).unwrap().to_rgba8();
        let (m, _) = catalog::build("hat.gorro", Some([10, 200, 10])).unwrap();
        let uv = m.bones[0].cubes[0].uv;
        let px = img.get_pixel(uv[0] + 9 + 2, uv[1] + 9 + 1);
        assert!(px[1] > px[0] && px[1] > px[2], "{px:?}");
        // sin nada puesto, la carpeta desaparece
        assert!(write(&game, &Loadout::default(), "Steve").unwrap().is_none());
        assert!(!d.exists());
        let _ = std::fs::remove_dir_all(game);
    }
}
