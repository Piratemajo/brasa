//! Pone (o quita) el mod de cosméticos en la instancia antes de jugar.
//!
//! El jar de cada familia lo publica el CI en la release de esta misma versión de Brasa
//! (`brasa-cosmetics-<familia>.jar` y su `.sha256`). Se descarga una vez, se verifica y se guarda en
//! la caché. Para desarrollar, `BRASA_COSMETICS_JAR=<ruta>` usa un jar compilado a mano.

use super::support::{self, Status};
use crate::config::Instance;
use crate::paths::Paths;
use crate::updater::{parse_checksum, Release};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// Nombre fijo dentro de `mods/` (así se reconoce y se sustituye sin tocar los demás).
pub const JAR: &str = "brasa-cosmetics.jar";

pub fn asset(family: &str) -> String {
    format!("brasa-cosmetics-{family}.jar")
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    Installed,
    AlreadyThere,
    /// El jugador lo desactivó en la pestaña Mods: se respeta.
    DisabledByUser,
    Removed,
    NotNeeded,
}

/// Copia el jar a `mods/` si hace falta (o lo quita si ya no toca). No descarga: recibe el jar.
pub fn place(mods: &Path, jar: Option<&Path>) -> Result<Outcome> {
    let dst = mods.join(JAR);
    let disabled = mods.join(format!("{JAR}.disabled"));
    let Some(src) = jar else {
        if dst.exists() {
            std::fs::remove_file(&dst)?;
            return Ok(Outcome::Removed);
        }
        return Ok(Outcome::NotNeeded);
    };
    if disabled.exists() {
        return Ok(Outcome::DisabledByUser);
    }
    let new = std::fs::read(src).with_context(|| format!("no se pudo leer {}", src.display()))?;
    if std::fs::read(&dst).ok().as_deref() == Some(new.as_slice()) {
        return Ok(Outcome::AlreadyThere);
    }
    std::fs::create_dir_all(mods)?;
    crate::paths::write_atomic(&dst, &new)?;
    Ok(Outcome::Installed)
}

async fn release(client: &reqwest::Client, repo: &str) -> Result<Release> {
    let get = |url: String| async move {
        client.get(url).header("Accept", "application/vnd.github+json").send().await?.error_for_status()?.json::<Release>().await.map_err(anyhow::Error::from)
    };
    // la release de esta versión; si no la hay (compilación propia), la última
    match get(format!("https://api.github.com/repos/{repo}/releases/tags/v{}", env!("CARGO_PKG_VERSION"))).await {
        Ok(r) => Ok(r),
        Err(_) => get(format!("https://api.github.com/repos/{repo}/releases/latest")).await,
    }
}

/// Jar de la familia en la caché, descargándolo y verificándolo si aún no está.
pub async fn fetch(client: &reqwest::Client, p: &Paths, repo: &str, family: &str) -> Result<PathBuf> {
    if let Ok(dev) = std::env::var("BRASA_COSMETICS_JAR") {
        return Ok(PathBuf::from(dev));
    }
    let dir = p.cache().join("cosmetics-mod").join(env!("CARGO_PKG_VERSION"));
    let path = dir.join(asset(family));
    if path.exists() {
        return Ok(path);
    }
    if !crate::distro::valid_repo(repo) {
        bail!("esta copia de Brasa no sabe de dónde descargar el mod de cosméticos (se compiló sin repositorio)");
    }
    let rel = release(client, repo).await.with_context(|| crate::updater::not_found(repo))?;
    let name = asset(family);
    let a = rel.assets.iter().find(|a| a.name == name).with_context(|| format!("la release {} aún no incluye {name}", rel.tag_name))?;
    let sum =
        rel.assets.iter().find(|a| a.name == format!("{name}.sha256")).with_context(|| format!("la release {} no publica {name}.sha256", rel.tag_name))?;
    let expected = parse_checksum(&client.get(&sum.browser_download_url).send().await?.error_for_status()?.text().await?).context("SHA-256 no válido")?;
    let bytes = client.get(&a.browser_download_url).send().await?.error_for_status()?.bytes().await?;
    use sha2::Digest;
    let actual = hex::encode(sha2::Sha256::digest(&bytes));
    if actual != expected {
        bail!("el SHA-256 de {name} no coincide con el publicado; no se instala");
    }
    std::fs::create_dir_all(&dir)?;
    crate::paths::write_atomic(&path, &bytes)?;
    Ok(path)
}

/// Antes de jugar. Devuelve una frase para la consola.
pub async fn ensure(client: &reqwest::Client, p: &Paths, repo: &str, inst: &Instance, wanted: bool) -> String {
    let mods = inst.game_dir(p).join("mods");
    let st = support::status(inst);
    let jar = match (&st, wanted) {
        (Status::Ready(f), true) => match fetch(client, p, repo, f.id).await {
            Ok(j) => Some(j),
            Err(e) => return format!("Cosméticos sin mod en esta partida: {e:#}"),
        },
        _ => None,
    };
    match place(&mods, jar.as_deref()) {
        Ok(Outcome::Installed) => "Mod de cosméticos instalado en la instancia".into(),
        Ok(Outcome::AlreadyThere) => "Mod de cosméticos al día".into(),
        Ok(Outcome::DisabledByUser) => "Mod de cosméticos desactivado en la pestaña Mods: no se toca".into(),
        Ok(Outcome::Removed) => "Mod de cosméticos retirado (ya no hace falta en esta instancia)".into(),
        Ok(Outcome::NotNeeded) => String::new(),
        Err(e) => format!("No se pudo colocar el mod de cosméticos: {e:#}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_replaces_respects_disabled_and_removes() {
        let root = std::env::temp_dir().join(format!("brasa-cosmod-{}", uuid::Uuid::new_v4()));
        let mods = root.join("mods");
        let jar = root.join("a.jar");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&jar, b"v1").unwrap();
        assert_eq!(place(&mods, Some(&jar)).unwrap(), Outcome::Installed);
        assert_eq!(place(&mods, Some(&jar)).unwrap(), Outcome::AlreadyThere);
        std::fs::write(&jar, b"v2").unwrap();
        assert_eq!(place(&mods, Some(&jar)).unwrap(), Outcome::Installed);
        assert_eq!(std::fs::read(mods.join(JAR)).unwrap(), b"v2");
        assert_eq!(place(&mods, None).unwrap(), Outcome::Removed);
        assert_eq!(place(&mods, None).unwrap(), Outcome::NotNeeded);
        std::fs::write(mods.join(format!("{JAR}.disabled")), b"v1").unwrap();
        assert_eq!(place(&mods, Some(&jar)).unwrap(), Outcome::DisabledByUser);
        assert!(!mods.join(JAR).exists());
        let _ = std::fs::remove_dir_all(root);
    }
}
