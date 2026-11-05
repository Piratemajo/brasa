//! Actualizar modpacks instalados (Modrinth y CurseForge) sin perder lo del jugador.
//!
//! Al instalar un modpack se guarda `pack.json` en la carpeta de la instancia: de dónde viene,
//! qué versión es y qué ficheros instaló. Al actualizar:
//! 1. se hace copia de seguridad de los mundos;
//! 2. se instala la versión nueva (los ficheros que cambiaron se vuelven a descargar por SHA-1);
//! 3. se borran sólo los ficheros que puso la versión anterior y la nueva ya no trae.
//!
//! Nunca se tocan los mods que añadió el jugador, sus mundos, capturas, `options.txt` ni la lista
//! de servidores.

use crate::config::{Instance, Loader, Settings};
use crate::net::Download;
use crate::paths::Paths;
use crate::progress::Reporter;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

const FILE: &str = "pack.json";

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Modrinth,
    CurseForge,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Modrinth => "Modrinth",
            Source::CurseForge => "CurseForge",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PackInfo {
    pub source: Source,
    /// ID del proyecto (Modrinth) o del mod (CurseForge).
    pub project: String,
    pub version_id: String,
    pub version_name: String,
    /// Rutas (relativas a `.minecraft`, con `/`) que instaló el pack.
    #[serde(default)]
    pub files: Vec<String>,
}

impl PackInfo {
    pub fn load(inst_dir: &Path) -> Option<PackInfo> {
        serde_json::from_str(&std::fs::read_to_string(inst_dir.join(FILE)).ok()?).ok()
    }
    pub fn save(&self, inst_dir: &Path) -> Result<()> {
        crate::paths::write_atomic(&inst_dir.join(FILE), serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

/// Lo que es del jugador: un modpack nunca lo pisa ni lo borra al actualizar.
pub fn is_protected(rel: &str) -> bool {
    let rel = rel.trim_start_matches("./").replace('\\', "/");
    let first = rel.split('/').next().unwrap_or("");
    matches!(first, "saves" | "screenshots" | "backups" | "logs" | "crash-reports")
        || matches!(rel.as_str(), "options.txt" | "optionsof.txt" | "optionsshaders.txt" | "servers.dat" | "servers.dat_old")
}

/// Ficheros que hay que borrar al pasar de la versión `old` a la `new`.
pub fn files_to_remove(old: &[String], new: &[String]) -> Vec<String> {
    let keep: HashSet<&str> = new.iter().map(|s| s.as_str()).collect();
    let mut v: Vec<String> = old.iter().filter(|f| !keep.contains(f.as_str()) && !is_protected(f)).cloned().collect();
    v.sort();
    v.dedup();
    v
}

/// Una ruta de un modpack es segura si es relativa y no sale de la carpeta del juego.
pub fn safe_rel(rel: &str) -> bool {
    let p = Path::new(rel);
    !rel.is_empty()
        && !p.is_absolute()
        && !p.components().any(|c| matches!(c, std::path::Component::ParentDir | std::path::Component::Prefix(_) | std::path::Component::RootDir))
}

/// La versión más reciente publicada del modpack.
#[derive(Debug, Clone)]
pub struct Latest {
    pub id: String,
    pub name: String,
    pub download: Download,
}

pub async fn check_latest(client: &reqwest::Client, s: &Settings, info: &PackInfo) -> Result<Latest> {
    match info.source {
        Source::Modrinth => {
            let v = crate::content::modrinth::latest_version(client, &info.project).await?;
            let f = v.primary_file().context("la versión nueva no tiene fichero")?;
            Ok(Latest {
                id: v.id.clone(),
                name: v.version_number.clone(),
                download: Download { url: f.url.clone(), path: Default::default(), sha1: f.hashes.get("sha1").cloned(), size: f.size },
            })
        }
        Source::CurseForge => {
            let id: u64 = info.project.parse().context("ID de CurseForge no válido en pack.json")?;
            let f = crate::content::curseforge::latest_file(client, &s.curseforge_api_key, id).await?;
            Ok(Latest {
                id: f.id.to_string(),
                name: f.file_name.clone(),
                download: Download { url: f.url(), path: Default::default(), sha1: f.sha1(), size: f.file_length },
            })
        }
    }
}

/// Actualiza el modpack de la instancia a su última versión. Devuelve el nombre de la versión nueva.
pub async fn update(client: &reqwest::Client, p: &Paths, s: &Settings, id: &str, rep: &Reporter) -> Result<String> {
    let mut inst = Instance::load(p, id)?;
    let info = PackInfo::load(&inst.dir(p)).context("esta instancia no se instaló desde un modpack de Modrinth o CurseForge")?;
    rep.stage(format!("Buscando actualización de {}", inst.name));
    let latest = check_latest(client, s, &info).await?;
    if latest.id == info.version_id {
        bail!("{} ya está en la última versión ({})", inst.name, info.version_name);
    }

    rep.stage("Copia de seguridad de mundos");
    let (pp, ii, keep) = (p.clone(), inst.clone(), s.backups_to_keep.max(1));
    let (done, errors) = tokio::task::spawn_blocking(move || crate::instance::worlds::auto_backup(&pp, &ii, keep)).await?;
    if !errors.is_empty() {
        bail!("no se pudo hacer copia de los mundos ({}); no se actualiza", errors.join("; "));
    }
    for b in done {
        rep.log(format!("[brasa] Copia de «{}» guardada antes de actualizar", b.world));
    }

    let ext = if info.source == Source::Modrinth { "mrpack" } else { "zip" };
    let file = p.cache().join(format!("update-{}-{}.{ext}", crate::paths::sanitize(&inst.id), latest.id));
    rep.stage(format!("Descargando {} {}", inst.name, latest.name));
    crate::net::download_one(client, &Download { path: file.clone(), ..latest.download.clone() }).await?;

    let game = inst.game_dir(p);
    let result = async {
        let (target, new_files) = match info.source {
            Source::Modrinth => {
                let index = crate::content::modrinth::read_index(&file)?;
                let target = crate::content::modrinth::pack_target(&index)?;
                let files = crate::content::modrinth::apply_mrpack(client, &file, &index, &game, s.concurrency, true, true, rep).await?;
                (target, files)
            }
            Source::CurseForge => {
                let man = crate::content::curseforge::read_manifest(&file)?;
                let target = crate::content::curseforge::manifest_target(&man);
                let files = crate::content::curseforge::apply_cf_zip(client, &s.curseforge_api_key, &file, &man, &game, s.concurrency, true, true, rep).await?;
                (target, files)
            }
        };
        anyhow::Ok((target, new_files))
    }
    .await;
    let _ = std::fs::remove_file(&file);
    let ((mc, loader, lv), new_files) = result?;

    let gone = files_to_remove(&info.files, &new_files);
    for rel in &gone {
        if safe_rel(rel) {
            let _ = std::fs::remove_file(game.join(rel));
        }
    }
    if !gone.is_empty() {
        rep.log(format!("[brasa] Quitados {} ficheros que la versión nueva ya no usa", gone.len()));
    }

    apply_target(&mut inst, &mc, loader, lv);
    inst.save(p)?;
    PackInfo { version_id: latest.id, version_name: latest.name.clone(), files: new_files, ..info }.save(&inst.dir(p))?;
    Ok(latest.name)
}

/// Si el pack cambia de versión de Minecraft o de loader, la instancia se reinstala al jugar.
fn apply_target(inst: &mut Instance, mc: &str, loader: Loader, lv: Option<String>) {
    if inst.mc_version != mc || inst.loader != loader || inst.loader_version != lv {
        inst.mc_version = mc.to_string();
        inst.loader = loader;
        inst.loader_version = lv;
        inst.resolved_version = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_paths() {
        for p in ["saves/Mundo/level.dat", "options.txt", "servers.dat", "screenshots/a.png", "./saves/x"] {
            assert!(is_protected(p), "{p}");
        }
        for p in ["mods/sodium.jar", "config/sodium.json", "resourcepacks/x.zip", "kubejs/server.js", "optionsx.txt"] {
            assert!(!is_protected(p), "{p}");
        }
    }

    #[test]
    fn only_old_pack_files_are_removed() {
        let old = vec!["mods/a-1.0.jar".to_string(), "mods/b-1.0.jar".into(), "config/a.toml".into(), "options.txt".into()];
        let new = vec!["mods/a-1.1.jar".to_string(), "mods/b-1.0.jar".into(), "config/a.toml".into()];
        assert_eq!(files_to_remove(&old, &new), vec!["mods/a-1.0.jar"], "options.txt nunca, aunque el pack nuevo no lo traiga");
        assert!(files_to_remove(&[], &new).is_empty(), "los mods del jugador no están en la lista del pack");
    }

    #[test]
    fn unsafe_paths() {
        assert!(safe_rel("mods/a.jar"));
        for bad in ["", "../fuera.jar", "/etc/passwd", "mods/../../x"] {
            assert!(!safe_rel(bad), "{bad}");
        }
    }

    #[test]
    fn pack_info_roundtrip() {
        let dir = std::env::temp_dir().join(format!("brasa-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(PackInfo::load(&dir).is_none());
        let info = PackInfo {
            source: Source::CurseForge,
            project: "123".into(),
            version_id: "9".into(),
            version_name: "Pack 1.2".into(),
            files: vec!["mods/a.jar".into()],
        };
        info.save(&dir).unwrap();
        assert_eq!(PackInfo::load(&dir), Some(info));
        assert!(std::fs::read_to_string(dir.join(FILE)).unwrap().contains("\"curseforge\""));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn target_change_forces_reinstall() {
        let mut i = Instance {
            mc_version: "1.20.1".into(),
            loader: Loader::Fabric,
            loader_version: Some("0.15".into()),
            resolved_version: Some("fabric-x".into()),
            ..Default::default()
        };
        apply_target(&mut i, "1.20.1", Loader::Fabric, Some("0.15".into()));
        assert!(i.resolved_version.is_some(), "sin cambios no se reinstala");
        apply_target(&mut i, "1.20.1", Loader::Fabric, Some("0.16".into()));
        assert_eq!((i.loader_version.as_deref(), i.resolved_version.as_deref()), (Some("0.16"), None));
    }
}
