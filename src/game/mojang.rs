//! Formato de versiones de Mojang: manifiesto, JSON de versión, reglas, librerías y assets.

use crate::net::{self, Download};
use crate::paths::{maven_path, Paths};
use crate::progress::Reporter;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

pub const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
pub const RESOURCES_URL: &str = "https://resources.download.minecraft.net";
pub const LIBRARIES_URL: &str = "https://libraries.minecraft.net/";

// ---------------------------------------------------------------- manifiesto

#[derive(Deserialize, Clone, Debug)]
pub struct Manifest {
    pub latest: Latest,
    pub versions: Vec<ManifestVersion>,
}
#[derive(Deserialize, Clone, Debug)]
pub struct Latest {
    pub release: String,
    pub snapshot: String,
}
#[derive(Deserialize, Clone, Debug)]
pub struct ManifestVersion {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    pub sha1: Option<String>,
}

pub async fn manifest(client: &reqwest::Client, p: &Paths) -> Result<Manifest> {
    let cache = p.cache().join("version_manifest_v2.json");
    match net::get_text(client, MANIFEST_URL).await {
        Ok(txt) => {
            let m: Manifest = serde_json::from_str(&txt)?;
            let _ = std::fs::write(&cache, txt);
            Ok(m)
        }
        // Modo offline: usa la copia en caché.
        Err(e) => {
            let txt = std::fs::read_to_string(&cache).map_err(|_| e)?;
            Ok(serde_json::from_str(&txt)?)
        }
    }
}

// ---------------------------------------------------------------- JSON de versión

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct VersionJson {
    pub id: String,
    pub inherits_from: Option<String>,
    pub main_class: Option<String>,
    pub minecraft_arguments: Option<String>,
    pub arguments: Option<Arguments>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    pub asset_index: Option<AssetIndexRef>,
    pub assets: Option<String>,
    pub downloads: Option<HashMap<String, Artifact>>,
    pub java_version: Option<JavaVersion>,
    pub logging: Option<Logging>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub jar: Option<String>,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<Arg>,
    #[serde(default)]
    pub jvm: Vec<Arg>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum Arg {
    Plain(String),
    Ruled { rules: Vec<Rule>, value: ArgValue },
}

#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum ArgValue {
    One(String),
    Many(Vec<String>),
}

#[derive(Deserialize, Clone, Debug)]
pub struct Rule {
    pub action: String,
    pub os: Option<OsRule>,
    pub features: Option<HashMap<String, bool>>,
}
#[derive(Deserialize, Clone, Debug)]
pub struct OsRule {
    pub name: Option<String>,
    pub arch: Option<String>,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct Library {
    pub name: String,
    pub downloads: Option<LibDownloads>,
    pub url: Option<String>,
    pub rules: Option<Vec<Rule>>,
    pub natives: Option<HashMap<String, String>>,
    pub extract: Option<Extract>,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}
#[derive(Deserialize, Clone, Debug, Default)]
pub struct LibDownloads {
    pub artifact: Option<Artifact>,
    pub classifiers: Option<HashMap<String, Artifact>>,
}
#[derive(Deserialize, Clone, Debug, Default)]
pub struct Artifact {
    pub path: Option<String>,
    pub sha1: Option<String>,
    pub size: Option<u64>,
    #[serde(default)]
    pub url: String,
}
#[derive(Deserialize, Clone, Debug, Default)]
pub struct Extract {
    #[serde(default)]
    pub exclude: Vec<String>,
}
#[derive(Deserialize, Clone, Debug)]
pub struct AssetIndexRef {
    pub id: String,
    pub sha1: Option<String>,
    pub size: Option<u64>,
    pub url: String,
}
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersion {
    pub major_version: u32,
    /// Runtime oficial de Mojang (java-runtime-delta, jre-legacy…).
    #[serde(default)]
    pub component: Option<String>,
}
#[derive(Deserialize, Clone, Debug)]
pub struct Logging {
    pub client: Option<LoggingClient>,
}
#[derive(Deserialize, Clone, Debug)]
pub struct LoggingClient {
    pub argument: String,
    pub file: LogFile,
}
#[derive(Deserialize, Clone, Debug)]
pub struct LogFile {
    pub id: String,
    pub sha1: Option<String>,
    pub size: Option<u64>,
    pub url: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct AssetIndex {
    pub objects: HashMap<String, AssetObject>,
    #[serde(rename = "virtual", default)]
    pub is_virtual: bool,
    #[serde(default)]
    pub map_to_resources: bool,
}
#[derive(Deserialize, Clone, Debug)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}

// ---------------------------------------------------------------- reglas

pub fn os_name() -> &'static str {
    match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "osx",
        _ => "linux",
    }
}

#[derive(Default, Clone, Debug)]
pub struct Features {
    pub custom_resolution: bool,
    pub quick_play_multiplayer: bool,
    pub demo: bool,
}

pub fn rules_allow(rules: &Option<Vec<Rule>>, feat: &Features) -> bool {
    let Some(rules) = rules else { return true };
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for r in rules {
        let mut matches = true;
        if let Some(os) = &r.os {
            if let Some(n) = &os.name {
                if n != os_name() {
                    matches = false;
                }
            }
            if let Some(a) = &os.arch {
                if a != std::env::consts::ARCH {
                    matches = false;
                }
            }
        }
        if let Some(fs) = &r.features {
            for (k, v) in fs {
                let have = match k.as_str() {
                    "has_custom_resolution" => feat.custom_resolution,
                    "is_quick_play_multiplayer" => feat.quick_play_multiplayer,
                    "is_demo_user" => feat.demo,
                    _ => false, // demo, realms, quick play singleplayer...
                };
                if have != *v {
                    matches = false;
                }
            }
        }
        if matches {
            allowed = r.action == "allow";
        }
    }
    allowed
}

// ---------------------------------------------------------------- resolución de herencia

/// Versión final tras aplicar `inheritsFrom` (Fabric/Quilt/Forge/NeoForge heredan de vanilla).
#[derive(Clone, Debug)]
pub struct Resolved {
    pub json: VersionJson,
    /// ID cuyo .jar es el cliente real (la versión vanilla raíz).
    pub jar_id: String,
}

pub fn lib_key(name: &str) -> String {
    let base = name.split('@').next().unwrap_or(name);
    let p: Vec<&str> = base.split(':').collect();
    match p.len() {
        0..=2 => base.to_string(),
        3 => format!("{}:{}", p[0], p[1]),
        _ => format!("{}:{}:{}", p[0], p[1], p[3]),
    }
}

fn merge(child: VersionJson, parent: VersionJson) -> VersionJson {
    // El hijo (loader) gana sobre el padre en conflictos de grupo:artefacto, pero no se deduplica
    // dentro de un mismo JSON: vanilla 1.14-1.18 lista LWJGL dos veces con reglas por SO.
    let child_keys: HashSet<String> = child.libraries.iter().map(|l| lib_key(&l.name)).collect();
    let mut libraries = child.libraries;
    libraries.extend(parent.libraries.into_iter().filter(|l| !child_keys.contains(&lib_key(&l.name))));
    let arguments = match (parent.arguments, child.arguments) {
        (None, None) => None,
        (Some(a), None) | (None, Some(a)) => Some(a),
        (Some(mut p), Some(c)) => {
            p.game.extend(c.game);
            p.jvm.extend(c.jvm);
            Some(p)
        }
    };
    VersionJson {
        id: child.id,
        inherits_from: None,
        main_class: child.main_class.or(parent.main_class),
        minecraft_arguments: child.minecraft_arguments.or(parent.minecraft_arguments),
        arguments,
        libraries,
        asset_index: child.asset_index.or(parent.asset_index),
        assets: child.assets.or(parent.assets),
        downloads: child.downloads.or(parent.downloads),
        java_version: child.java_version.or(parent.java_version),
        logging: child.logging.or(parent.logging),
        kind: child.kind.or(parent.kind),
        jar: child.jar.or(parent.jar),
    }
}

/// Garantiza que existe el JSON de una versión vanilla (lo descarga del manifiesto si falta).
pub async fn ensure_vanilla_json(client: &reqwest::Client, p: &Paths, id: &str) -> Result<()> {
    let path = p.version_json(id);
    if path.exists() {
        return Ok(());
    }
    let m = manifest(client, p).await?;
    let v = m.versions.iter().find(|v| v.id == id).with_context(|| format!("la versión '{id}' no existe en el manifiesto de Mojang"))?;
    net::download_one(client, &Download { url: v.url.clone(), path, sha1: v.sha1.clone(), size: None }).await
}

pub fn read_json(p: &Paths, id: &str) -> Result<VersionJson> {
    let path = p.version_json(id);
    let txt = std::fs::read_to_string(&path).with_context(|| format!("no se encuentra {}", path.display()))?;
    serde_json::from_str(&txt).with_context(|| format!("JSON de versión inválido: {}", path.display()))
}

pub async fn resolve(client: &reqwest::Client, p: &Paths, id: &str) -> Result<Resolved> {
    let mut chain = vec![];
    let mut cur = id.to_string();
    for _ in 0..8 {
        if !p.version_json(&cur).exists() {
            ensure_vanilla_json(client, p, &cur).await?;
        }
        let j = read_json(p, &cur)?;
        let next = j.inherits_from.clone();
        chain.push(j);
        match next {
            Some(n) => cur = n,
            None => break,
        }
    }
    let root = chain.pop().context("cadena de versiones vacía")?;
    let jar_id = root.jar.clone().unwrap_or_else(|| root.id.clone());
    let mut acc = root;
    while let Some(child) = chain.pop() {
        acc = merge(child, acc);
    }
    Ok(Resolved { json: acc, jar_id })
}

// ---------------------------------------------------------------- librerías

#[derive(Clone, Debug)]
pub struct LibFile {
    pub path: PathBuf,
    pub download: Option<Download>,
    /// Jar de nativos a extraer.
    pub native: bool,
    pub exclude: Vec<String>,
    pub classpath: bool,
}

pub fn library_files(p: &Paths, v: &VersionJson) -> Vec<LibFile> {
    let feat = Features::default();
    let libdir = p.libraries();
    let arch_bits = if cfg!(target_pointer_width = "64") { "64" } else { "32" };
    let mut out = vec![];
    for lib in &v.libraries {
        if !rules_allow(&lib.rules, &feat) {
            continue;
        }
        let exclude = lib.extract.as_ref().map(|e| e.exclude.clone()).unwrap_or_default();

        // Nativos estilo antiguo (<= 1.18): clasificador por SO.
        if let Some(natives) = &lib.natives {
            if let Some(classifier) = natives.get(os_name()) {
                let classifier = classifier.replace("${arch}", arch_bits);
                let art = lib.downloads.as_ref().and_then(|d| d.classifiers.as_ref()).and_then(|c| c.get(&classifier)).cloned();
                if let Some(rel) = art.as_ref().and_then(|a| a.path.clone()).or_else(|| maven_path(&format!("{}:{}", lib.name, classifier))) {
                    let path = libdir.join(&rel);
                    let url = art
                        .as_ref()
                        .map(|a| a.url.clone())
                        .filter(|u| !u.is_empty())
                        .unwrap_or_else(|| format!("{}{}", lib.url.as_deref().unwrap_or(LIBRARIES_URL), rel));
                    out.push(LibFile {
                        download: Some(Download {
                            url,
                            path: path.clone(),
                            sha1: art.as_ref().and_then(|a| a.sha1.clone()),
                            size: art.as_ref().and_then(|a| a.size),
                        }),
                        path,
                        native: true,
                        exclude: exclude.clone(),
                        classpath: false,
                    });
                }
            }
            // Algunas entradas antiguas sólo traen nativos.
            if lib.downloads.as_ref().and_then(|d| d.artifact.as_ref()).is_none() {
                continue;
            }
        }

        let art = lib.downloads.as_ref().and_then(|d| d.artifact.clone());
        let Some(rel) = art.as_ref().and_then(|a| a.path.clone()).or_else(|| maven_path(&lib.name)) else {
            continue;
        };
        let path = libdir.join(&rel);
        let (url, sha1, size) = match &art {
            Some(a) => (a.url.clone(), a.sha1.clone(), a.size),
            None => {
                let base = lib.url.clone().unwrap_or_else(|| LIBRARIES_URL.to_string());
                let base = if base.ends_with('/') { base } else { format!("{base}/") };
                (format!("{base}{rel}"), lib.sha1.clone(), lib.size)
            }
        };
        // url vacía = librería generada por el instalador de Forge/NeoForge (ya existe en disco).
        let download = if url.is_empty() { None } else { Some(Download { url, path: path.clone(), sha1, size }) };
        out.push(LibFile { path, download, native: lib.name.contains(":natives-"), exclude, classpath: true });
    }
    out
}

// ---------------------------------------------------------------- instalación

pub fn assets_id(v: &VersionJson) -> String {
    v.asset_index.as_ref().map(|a| a.id.clone()).or_else(|| v.assets.clone()).unwrap_or_else(|| "legacy".into())
}

/// Descarga cliente, librerías, assets y config de logging. Devuelve el índice de assets.
pub async fn install(
    client: &reqwest::Client,
    p: &Paths,
    r: &Resolved,
    game_dir: &std::path::Path,
    concurrency: usize,
    verify: bool,
    rep: &Reporter,
) -> Result<()> {
    let v = &r.json;
    let mut items: Vec<Download> = vec![];

    // Jar del cliente
    let client_dl = v.downloads.as_ref().and_then(|d| d.get("client")).context("la versión no tiene descarga de cliente")?;
    items.push(Download { url: client_dl.url.clone(), path: p.version_jar(&r.jar_id), sha1: client_dl.sha1.clone(), size: client_dl.size });

    // Librerías
    for l in library_files(p, v) {
        if let Some(d) = l.download {
            items.push(d);
        } else if !l.path.exists() {
            bail!("falta la librería {} (reinstala el loader)", l.path.display());
        }
    }

    // Logging (parche Log4Shell incluido en las configs oficiales)
    if let Some(lc) = v.logging.as_ref().and_then(|l| l.client.as_ref()) {
        items.push(Download {
            url: lc.file.url.clone(),
            path: p.assets().join("log_configs").join(&lc.file.id),
            sha1: lc.file.sha1.clone(),
            size: lc.file.size,
        });
    }

    // Índice de assets
    let index = if let Some(ai) = &v.asset_index {
        let path = p.assets().join("indexes").join(format!("{}.json", ai.id));
        let d = Download { url: ai.url.clone(), path: path.clone(), sha1: ai.sha1.clone(), size: ai.size };
        if !net::is_valid(&d, true) {
            net::download_one(client, &d).await?;
        }
        let idx: AssetIndex = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
        Some(idx)
    } else {
        None
    };
    if let Some(idx) = &index {
        for obj in idx.objects.values() {
            let pre = &obj.hash[..2];
            items.push(Download {
                url: format!("{RESOURCES_URL}/{pre}/{}", obj.hash),
                path: p.assets().join("objects").join(pre).join(&obj.hash),
                sha1: Some(obj.hash.clone()),
                size: Some(obj.size),
            });
        }
    }

    rep.stage(format!("Descargando {} ficheros de {}", items.len(), v.id));
    net::download_all(client, items, concurrency, verify, rep).await?;

    // Assets legacy (< 1.7.2): copiar a virtual/ o resources/
    if let Some(idx) = &index {
        if idx.is_virtual || idx.map_to_resources {
            rep.stage("Preparando assets legacy");
            let base = if idx.map_to_resources { game_dir.join("resources") } else { p.assets().join("virtual").join(assets_id(v)) };
            for (name, obj) in &idx.objects {
                let dst = base.join(name);
                if !dst.exists() {
                    if let Some(parent) = dst.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    let src = p.assets().join("objects").join(&obj.hash[..2]).join(&obj.hash);
                    std::fs::copy(src, dst)?;
                }
            }
        }
    }

    // El jar de la versión final debe existir con su propio nombre (Forge lo exige en ignoreList).
    if r.jar_id != v.id {
        let dst = p.version_jar(&v.id);
        let src = p.version_jar(&r.jar_id);
        let stale = match (std::fs::metadata(&dst), std::fs::metadata(&src)) {
            (Ok(a), Ok(b)) => a.len() != b.len(),
            _ => true,
        };
        if stale {
            std::fs::create_dir_all(dst.parent().unwrap())?;
            std::fs::copy(src, dst)?;
        }
    }
    Ok(())
}

pub fn required_java(v: &VersionJson) -> u32 {
    v.java_version.as_ref().map(|j| j.major_version).unwrap_or(8)
}

pub fn java_component(v: &VersionJson) -> Option<String> {
    v.java_version.as_ref().and_then(|j| j.component.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keys() {
        assert_eq!(lib_key("org.ow2.asm:asm:9.6"), "org.ow2.asm:asm");
        assert_eq!(lib_key("org.lwjgl:lwjgl:3.3.3:natives-linux"), "org.lwjgl:lwjgl:natives-linux");
    }
    #[test]
    fn parse_args() {
        let j = r#"{"id":"x","arguments":{"game":["--a",{"rules":[{"action":"allow","features":{"has_custom_resolution":true}}],"value":["--width","${resolution_width}"]}],"jvm":[]}}"#;
        let v: VersionJson = serde_json::from_str(j).unwrap();
        assert_eq!(v.arguments.unwrap().game.len(), 2);
    }
    #[test]
    fn rules() {
        let r = Some(vec![
            Rule { action: "allow".into(), os: None, features: None },
            Rule { action: "disallow".into(), os: Some(OsRule { name: Some(os_name().into()), arch: None }), features: None },
        ]);
        assert!(!rules_allow(&r, &Features::default()));
    }
}
