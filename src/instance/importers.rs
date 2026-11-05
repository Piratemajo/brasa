//! Importar instancias de otros launchers: Prism Launcher / MultiMC, CurseForge App, ATLauncher y
//! el launcher oficial. Siempre se copia: el original no se toca.

use crate::config::{Instance, Loader};
use crate::paths::{copy_dir_all, Paths};
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Found {
    pub name: String,
    pub path: PathBuf,
    pub mc: String,
    pub loader: Loader,
    pub loader_version: Option<String>,
    /// Carpeta de juego a copiar.
    pub game_dir: PathBuf,
    pub source: &'static str,
    /// Memoria fijada en el launcher de origen (MB), si la había.
    pub memory_mb: Option<u32>,
}

pub const SRC_PRISM: &str = "Prism/MultiMC";
pub const SRC_CURSEFORGE: &str = "CurseForge App";
pub const SRC_ATLAUNCHER: &str = "ATLauncher";
pub const SRC_OFFICIAL: &str = "Launcher oficial";

fn home() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf())
}

/// Carpetas típicas donde Prism/MultiMC guardan instancias.
pub fn prism_roots() -> Vec<PathBuf> {
    let mut v = vec![];
    if let Some(b) = directories::BaseDirs::new() {
        for name in ["PrismLauncher", "polymc", "MultiMC"] {
            v.push(b.data_dir().join(name).join("instances"));
        }
    }
    if let Some(h) = home() {
        v.push(h.join(".var/app/org.prismlauncher.PrismLauncher/data/PrismLauncher/instances"));
        v.push(h.join("MultiMC/instances"));
    }
    v.into_iter().filter(|p| p.is_dir()).collect()
}

pub fn official_dir() -> Option<PathBuf> {
    let b = directories::BaseDirs::new()?;
    let d = if cfg!(windows) {
        b.data_dir().join(".minecraft")
    } else if cfg!(target_os = "macos") {
        b.data_dir().join("minecraft")
    } else {
        b.home_dir().join(".minecraft")
    };
    d.is_dir().then_some(d)
}

/// Lee una instancia de Prism/MultiMC (carpeta con instance.cfg + mmc-pack.json).
pub fn read_prism(dir: &Path) -> Result<Found> {
    let cfg = std::fs::read_to_string(dir.join("instance.cfg")).context("no hay instance.cfg (¿es una instancia de Prism/MultiMC?)")?;
    let name = cfg
        .lines()
        .find_map(|l| l.strip_prefix("name="))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| dir.file_name().unwrap_or_default().to_string_lossy().into_owned());
    let pack: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("mmc-pack.json")).context("falta mmc-pack.json")?)?;
    let mut mc = String::new();
    let mut loader = Loader::Vanilla;
    let mut lv = None;
    for c in pack["components"].as_array().cloned().unwrap_or_default() {
        let uid = c["uid"].as_str().unwrap_or_default();
        let ver = c["version"].as_str().map(String::from);
        match uid {
            "net.minecraft" => mc = ver.unwrap_or_default(),
            "net.fabricmc.fabric-loader" => (loader, lv) = (Loader::Fabric, ver),
            "org.quiltmc.quilt-loader" => (loader, lv) = (Loader::Quilt, ver),
            "net.minecraftforge" => (loader, lv) = (Loader::Forge, ver),
            "net.neoforged" => (loader, lv) = (Loader::NeoForge, ver),
            _ => {}
        }
    }
    if mc.is_empty() {
        bail!("no se encontró la versión de Minecraft en mmc-pack.json");
    }
    let game_dir = [".minecraft", "minecraft"].iter().map(|d| dir.join(d)).find(|d| d.is_dir()).unwrap_or_else(|| dir.join(".minecraft"));
    let memory_mb = cfg.lines().find_map(|l| l.strip_prefix("MaxMemAlloc=")).and_then(|s| s.trim().parse::<u32>().ok());
    Ok(Found { name, path: dir.to_path_buf(), mc, loader, loader_version: lv, game_dir, source: SRC_PRISM, memory_mb })
}

pub fn scan_prism() -> Vec<Found> {
    let mut v = vec![];
    for root in prism_roots() {
        if let Ok(rd) = std::fs::read_dir(root) {
            for e in rd.flatten() {
                if let Ok(f) = read_prism(&e.path()) {
                    v.push(f);
                }
            }
        }
    }
    v
}

// ------------------------------------------------------------------------- CurseForge App

/// Carpetas típicas de instancias de la app de CurseForge.
pub fn curseforge_roots() -> Vec<PathBuf> {
    let mut v = vec![];
    if let Some(h) = home() {
        v.push(h.join("curseforge/minecraft/Instances"));
        v.push(h.join("Documents/curseforge/minecraft/Instances"));
    }
    if let Some(d) = directories::UserDirs::new().and_then(|u| u.document_dir().map(|d| d.to_path_buf())) {
        v.push(d.join("curseforge/minecraft/Instances"));
    }
    v.sort();
    v.dedup();
    v.into_iter().filter(|p| p.is_dir()).collect()
}

/// `forge-47.2.0`, `neoforge-21.1.209`, `fabric-0.15.11-1.20.1`, `quilt-0.26.0-1.20.1`.
/// `kind` es el número que usa CurseForge (1 Forge, 4 Fabric, 5 Quilt, 6 NeoForge) por si el nombre falta.
pub fn parse_cf_loader(name: &str, kind: i64, forge_version: Option<&str>) -> Option<(Loader, String)> {
    let lower = name.to_ascii_lowercase();
    for (prefix, l) in [("neoforge-", Loader::NeoForge), ("forge-", Loader::Forge), ("fabric-", Loader::Fabric), ("quilt-", Loader::Quilt)] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let v = match l {
                Loader::Fabric | Loader::Quilt => rest.split_once('-').map(|(v, _)| v).unwrap_or(rest),
                _ => rest,
            };
            return Some((l, v.to_string()));
        }
    }
    let l = match kind {
        1 => Loader::Forge,
        4 => Loader::Fabric,
        5 => Loader::Quilt,
        6 => Loader::NeoForge,
        _ => return None,
    };
    Some((l, forge_version?.to_string()))
}

/// Lee una instancia de la app de CurseForge (carpeta con `minecraftinstance.json`).
pub fn read_curseforge(dir: &Path) -> Result<Found> {
    let txt = std::fs::read_to_string(dir.join("minecraftinstance.json")).context("no hay minecraftinstance.json (¿es una instancia de CurseForge?)")?;
    let v: Value = serde_json::from_str(&txt).context("minecraftinstance.json no es JSON válido")?;
    let mc = v["gameVersion"].as_str().or_else(|| v["baseModLoader"]["minecraftVersion"].as_str()).unwrap_or_default().to_string();
    if mc.is_empty() {
        bail!("minecraftinstance.json no indica la versión de Minecraft");
    }
    let name = v["name"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .map(String::from)
        .unwrap_or_else(|| dir.file_name().unwrap_or_default().to_string_lossy().into_owned());
    let bl = &v["baseModLoader"];
    let (loader, lv) = if v["isVanilla"].as_bool() == Some(true) || bl.is_null() {
        (Loader::Vanilla, None)
    } else {
        match parse_cf_loader(bl["name"].as_str().unwrap_or(""), bl["type"].as_i64().unwrap_or(0), bl["forgeVersion"].as_str()) {
            Some((l, ver)) => (l, Some(ver)),
            None => (Loader::Vanilla, None),
        }
    };
    let memory_mb = (v["isMemoryOverride"].as_bool() == Some(true)).then(|| v["allocatedMemory"].as_u64()).flatten().map(|m| m as u32);
    Ok(Found { name, path: dir.to_path_buf(), mc, loader, loader_version: lv, game_dir: dir.to_path_buf(), source: SRC_CURSEFORGE, memory_mb })
}

// ------------------------------------------------------------------------- ATLauncher

/// Carpetas típicas de instancias de ATLauncher (es portable: también se puede elegir a mano).
pub fn atlauncher_roots() -> Vec<PathBuf> {
    let mut v = vec![];
    if let Some(b) = directories::BaseDirs::new() {
        v.push(b.data_dir().join("ATLauncher/instances"));
        v.push(b.data_dir().join("atlauncher/instances"));
        v.push(b.config_dir().join("ATLauncher/instances"));
    }
    if let Some(h) = home() {
        v.push(h.join("ATLauncher/instances"));
        v.push(h.join(".var/app/com.atlauncher.ATLauncher/data/instances"));
        v.push(h.join("Library/Application Support/ATLauncher/instances"));
    }
    v.sort();
    v.dedup();
    v.into_iter().filter(|p| p.is_dir()).collect()
}

/// Lee una instancia de ATLauncher (carpeta con `instance.json`).
pub fn read_atlauncher(dir: &Path) -> Result<Found> {
    let txt = std::fs::read_to_string(dir.join("instance.json")).context("no hay instance.json (¿es una instancia de ATLauncher?)")?;
    let v: Value = serde_json::from_str(&txt).context("instance.json no es JSON válido")?;
    let launcher = &v["launcher"];
    if launcher.is_null() {
        bail!("instance.json no es de ATLauncher");
    }
    let mc = v["id"].as_str().unwrap_or_default().to_string();
    if mc.is_empty() {
        bail!("instance.json no indica la versión de Minecraft");
    }
    let name = launcher["name"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .map(String::from)
        .unwrap_or_else(|| dir.file_name().unwrap_or_default().to_string_lossy().into_owned());
    let lvj = &launcher["loaderVersion"];
    let (loader, lv) = if lvj.is_null() {
        (Loader::Vanilla, None)
    } else {
        let raw_kind = lvj["type"].as_str().unwrap_or("");
        let loader = match raw_kind.to_ascii_lowercase().as_str() {
            "fabric" => Loader::Fabric,
            "quilt" => Loader::Quilt,
            "forge" => Loader::Forge,
            "neoforge" => Loader::NeoForge,
            _ => bail!("ATLauncher: el loader «{raw_kind}» no está soportado por Brasa"),
        };
        // ATLauncher a veces guarda «1.20.1-47.2.0»: Brasa quiere sólo la del loader
        let raw = lvj["version"].as_str().unwrap_or("");
        let ver = raw.strip_prefix(&format!("{mc}-")).unwrap_or(raw).to_string();
        (loader, Some(ver).filter(|s| !s.is_empty()))
    };
    let memory_mb = launcher["maximumMemory"].as_u64().map(|m| m as u32);
    Ok(Found { name, path: dir.to_path_buf(), mc, loader, loader_version: lv, game_dir: dir.to_path_buf(), source: SRC_ATLAUNCHER, memory_mb })
}

fn scan_with(roots: Vec<PathBuf>, read: fn(&Path) -> Result<Found>) -> Vec<Found> {
    let mut v = vec![];
    for root in roots {
        if let Ok(rd) = std::fs::read_dir(root) {
            for e in rd.flatten() {
                if let Ok(f) = read(&e.path()) {
                    v.push(f);
                }
            }
        }
    }
    v.sort_by_key(|f| f.name.to_lowercase());
    v
}

pub fn scan_curseforge() -> Vec<Found> {
    scan_with(curseforge_roots(), read_curseforge)
}

pub fn scan_atlauncher() -> Vec<Found> {
    scan_with(atlauncher_roots(), read_atlauncher)
}

/// Reconoce una carpeta de instancia de cualquier launcher soportado.
pub fn read_any(dir: &Path) -> Result<Found> {
    if dir.join("instance.cfg").is_file() {
        return read_prism(dir);
    }
    if dir.join("minecraftinstance.json").is_file() {
        return read_curseforge(dir);
    }
    if dir.join("instance.json").is_file() {
        return read_atlauncher(dir);
    }
    bail!("{} no parece una instancia de Prism/MultiMC, CurseForge o ATLauncher", dir.display())
}

/// Ficheros y carpetas internos de otros launchers que no se copian.
const FOREIGN_JUNK: &[&str] = &[
    "minecraftinstance.json",
    "instance.json",
    "instance.cfg",
    "mmc-pack.json",
    "logs",
    "crash-reports",
    "natives",
    "bin",
    "libraries",
    "versions",
    "assets",
    "jarmods",
    "disabledmods",
    "loaderVersion",
    ".curseclient",
];

fn copy_filtered(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)?.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if FOREIGN_JUNK.contains(&name.as_str()) {
            continue;
        }
        let (from, to) = (e.path(), dst.join(&name));
        if e.file_type()?.is_dir() {
            copy_dir_all(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Interpreta un ID de versión del launcher oficial (fabric-loader-0.16.5-1.21.1, 1.20.1-forge-47.2.0, neoforge-21.1.77...).
pub fn parse_version_id(id: &str, latest_release: &str) -> (String, Loader, Option<String>) {
    if id == "latest-release" || id == "latest-snapshot" || id.is_empty() {
        return (latest_release.to_string(), Loader::Vanilla, None);
    }
    for (prefix, l) in [("fabric-loader-", Loader::Fabric), ("quilt-loader-", Loader::Quilt)] {
        if let Some(rest) = id.strip_prefix(prefix) {
            if let Some((lv, mc)) = rest.split_once('-') {
                return (mc.to_string(), l, Some(lv.to_string()));
            }
        }
    }
    if let Some(v) = id.strip_prefix("neoforge-") {
        // 21.1.77 → 1.21.1
        let mut it = v.split('.');
        let (a, b) = (it.next().unwrap_or("0"), it.next().unwrap_or("0"));
        let mc = if b == "0" { format!("1.{a}") } else { format!("1.{a}.{b}") };
        return (mc, Loader::NeoForge, Some(v.to_string()));
    }
    let lower = id.to_ascii_lowercase();
    if let Some(idx) = lower.find("-forge") {
        let mc = id[..idx].to_string();
        let lv = Loader::Forge.version_from_id(id, &mc);
        return (mc, Loader::Forge, lv);
    }
    (id.to_string(), Loader::Vanilla, None)
}

/// Perfiles del launcher oficial (launcher_profiles.json).
pub fn scan_official(latest_release: &str) -> Vec<Found> {
    let Some(dir) = official_dir() else { return vec![] };
    let Ok(txt) = std::fs::read_to_string(dir.join("launcher_profiles.json")) else { return vec![] };
    let Ok(v) = serde_json::from_str::<Value>(&txt) else { return vec![] };
    let mut out = vec![];
    if let Some(profiles) = v["profiles"].as_object() {
        for (key, prof) in profiles {
            let id = prof["lastVersionId"].as_str().unwrap_or_default();
            let (mc, loader, lv) = parse_version_id(id, latest_release);
            let name = prof["name"].as_str().filter(|s| !s.is_empty()).map(String::from).unwrap_or_else(|| match prof["type"].as_str() {
                Some("latest-release") => "Última versión".into(),
                Some("latest-snapshot") => "Última snapshot".into(),
                _ => key.clone(),
            });
            let game_dir = prof["gameDir"].as_str().map(PathBuf::from).filter(|p| p.is_dir()).unwrap_or_else(|| dir.clone());
            out.push(Found { name, path: dir.clone(), mc, loader, loader_version: lv, game_dir, source: SRC_OFFICIAL, memory_mb: None });
        }
    }
    out
}

/// Copia una instancia encontrada a Brasa. Del .minecraft oficial sólo se copia lo útil (no versiones/librerías).
pub fn import(p: &Paths, f: &Found) -> Result<Instance> {
    let mut inst = Instance::create(p, &f.name, &f.mc, f.loader, f.loader_version.clone())?;
    let dst = inst.game_dir(p);
    match f.source {
        SRC_OFFICIAL => {
            for sub in ["mods", "saves", "resourcepacks", "shaderpacks", "config", "screenshots", "schematics"] {
                let s = f.game_dir.join(sub);
                if s.is_dir() {
                    copy_dir_all(&s, &dst.join(sub))?;
                }
            }
            for file in ["options.txt", "servers.dat", "optionsof.txt", "optionsshaders.txt"] {
                let s = f.game_dir.join(file);
                if s.is_file() {
                    std::fs::copy(&s, dst.join(file))?;
                }
            }
        }
        // en CurseForge y ATLauncher la carpeta de la instancia es la del juego, con sus ficheros internos
        SRC_CURSEFORGE | SRC_ATLAUNCHER => copy_filtered(&f.game_dir, &dst)?,
        _ => {
            if f.game_dir.is_dir() {
                copy_dir_all(&f.game_dir, &dst)?;
            }
        }
    }
    if f.memory_mb.is_some() {
        inst.memory_mb = f.memory_mb;
        inst.save(p)?;
    }
    Ok(inst)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn version_ids() {
        assert_eq!(parse_version_id("fabric-loader-0.16.5-1.21.1", "x"), ("1.21.1".into(), Loader::Fabric, Some("0.16.5".into())));
        assert_eq!(parse_version_id("1.20.1-forge-47.2.0", "x"), ("1.20.1".into(), Loader::Forge, Some("47.2.0".into())));
        assert_eq!(parse_version_id("neoforge-21.1.77", "x").0, "1.21.1");
        assert_eq!(parse_version_id("neoforge-21.0.10", "x").0, "1.21");
        assert_eq!(parse_version_id("latest-release", "1.21.4").0, "1.21.4");
        assert_eq!(parse_version_id("1.8.9", "x"), ("1.8.9".into(), Loader::Vanilla, None));
    }

    fn temp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("brasa-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn curseforge_loader_names() {
        assert_eq!(parse_cf_loader("neoforge-21.1.209", 6, Some("21.1.209")), Some((Loader::NeoForge, "21.1.209".into())));
        assert_eq!(parse_cf_loader("forge-47.2.0", 1, None), Some((Loader::Forge, "47.2.0".into())));
        assert_eq!(parse_cf_loader("fabric-0.15.11-1.20.1", 4, None), Some((Loader::Fabric, "0.15.11".into())));
        assert_eq!(parse_cf_loader("quilt-0.26.0-1.20.1", 5, None), Some((Loader::Quilt, "0.26.0".into())));
        assert_eq!(parse_cf_loader("", 6, Some("21.1.1")), Some((Loader::NeoForge, "21.1.1".into())), "sin nombre, por el tipo");
        assert_eq!(parse_cf_loader("", 0, None), None);
    }

    /// Estructura tomada de un minecraftinstance.json real (AllTheLeaks, CurseForge App).
    #[test]
    fn reads_a_curseforge_instance() {
        let d = temp();
        std::fs::write(
            d.join("minecraftinstance.json"),
            r#"{"name":"AllTheLeaks","gameVersion":"1.21.1","baseModLoader":{"forgeVersion":"21.1.209","name":"neoforge-21.1.209","type":6,"minecraftVersion":"1.21.1"},
               "installPath":"C:\\Users\\x\\curseforge\\minecraft\\Instances\\AllTheLeaks\\","allocatedMemory":8032,"isMemoryOverride":false,"isVanilla":false,"installedAddons":[]}"#,
        )
        .unwrap();
        let f = read_curseforge(&d).unwrap();
        assert_eq!((f.name.as_str(), f.mc.as_str(), f.loader, f.loader_version.as_deref()), ("AllTheLeaks", "1.21.1", Loader::NeoForge, Some("21.1.209")));
        assert_eq!(f.memory_mb, None, "sin isMemoryOverride la memoria no se importa");
        assert_eq!(f.game_dir, d);
        assert_eq!(read_any(&d).unwrap().source, SRC_CURSEFORGE);
        let _ = std::fs::remove_dir_all(d);
    }

    /// Campos según el código de ATLauncher (Instance / InstanceLauncher / LoaderVersion).
    #[test]
    fn reads_an_atlauncher_instance() {
        let d = temp();
        std::fs::write(
            d.join("instance.json"),
            r#"{"id":"1.20.1","type":"release","launcher":{"name":"Create Arcane","pack":"Create Arcane","version":"1.4","loaderVersion":{"version":"1.20.1-47.2.0","rawVersion":"1.20.1-47.2.0","recommended":false,"type":"Forge"},"requiredMemory":4096,"maximumMemory":6144}}"#,
        )
        .unwrap();
        let f = read_atlauncher(&d).unwrap();
        assert_eq!((f.name.as_str(), f.mc.as_str(), f.loader), ("Create Arcane", "1.20.1", Loader::Forge));
        assert_eq!(f.loader_version.as_deref(), Some("47.2.0"), "se quita el prefijo con la versión de Minecraft");
        assert_eq!(f.memory_mb, Some(6144));
        assert_eq!(read_any(&d).unwrap().source, SRC_ATLAUNCHER);

        std::fs::write(d.join("instance.json"), r#"{"id":"1.20.1","launcher":{"name":"Srv","loaderVersion":{"version":"1","type":"Paper"}}}"#).unwrap();
        assert!(read_atlauncher(&d).unwrap_err().to_string().contains("Paper"), "loaders de servidor: no");
        std::fs::write(d.join("instance.json"), r#"{"id":"1.21.1","launcher":{"name":"Vanilla"}}"#).unwrap();
        assert_eq!(read_atlauncher(&d).unwrap().loader, Loader::Vanilla);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn importing_skips_foreign_launcher_files() {
        let src = temp();
        std::fs::write(
            src.join("minecraftinstance.json"),
            r#"{"name":"Pack","gameVersion":"1.20.1","isVanilla":true,"isMemoryOverride":true,"allocatedMemory":5120}"#,
        )
        .unwrap();
        for d in ["mods", "saves/Mundo", "logs", "config"] {
            std::fs::create_dir_all(src.join(d)).unwrap();
        }
        std::fs::write(src.join("mods/a.jar"), "x").unwrap();
        std::fs::write(src.join("saves/Mundo/level.dat"), "x").unwrap();
        std::fs::write(src.join("logs/latest.log"), "x").unwrap();
        std::fs::write(src.join("options.txt"), "renderDistance:12").unwrap();
        let f = read_any(&src).unwrap();
        assert_eq!(f.memory_mb, Some(5120));

        let root = temp();
        let p = Paths { root: root.clone() };
        let inst = import(&p, &f).unwrap();
        let g = inst.game_dir(&p);
        assert!(g.join("mods/a.jar").is_file() && g.join("saves/Mundo/level.dat").is_file() && g.join("options.txt").is_file());
        assert!(!g.join("minecraftinstance.json").exists() && !g.join("logs").exists(), "lo interno del otro launcher no se copia");
        assert_eq!(Instance::load(&p, &inst.id).unwrap().memory_mb, Some(5120));
        assert!(src.join("minecraftinstance.json").exists(), "el original queda intacto");
        let _ = std::fs::remove_dir_all(src);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn unknown_folder_is_rejected() {
        let d = temp();
        assert!(read_any(&d).is_err());
        let _ = std::fs::remove_dir_all(d);
    }
}
