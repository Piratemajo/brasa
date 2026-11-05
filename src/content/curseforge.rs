//! CurseForge: búsqueda, instalación con dependencias e importación de modpacks (.zip con manifest.json).
//! Necesita la API key de la aplicación (console.curseforge.com), incrustada al compilar: ver `distro`.

use crate::config::{Instance, Loader};
use crate::net::{self, Download};
use crate::paths::Paths;
use crate::progress::Reporter;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::io::Read;
use std::path::Path;

const API: &str = "https://api.curseforge.com/v1";
const MINECRAFT: u32 = 432;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Mod,
    ResourcePack,
    Shader,
    Modpack,
}

impl Class {
    pub fn id(&self) -> u32 {
        match self {
            Class::Mod => 6,
            Class::ResourcePack => 12,
            Class::Shader => 6552,
            Class::Modpack => 4471,
        }
    }
    pub fn folder(&self) -> &'static str {
        match self {
            Class::Mod | Class::Modpack => "mods",
            Class::ResourcePack => "resourcepacks",
            Class::Shader => "shaderpacks",
        }
    }
    pub fn from_class_id(id: u32) -> Class {
        match id {
            12 => Class::ResourcePack,
            6552 => Class::Shader,
            4471 => Class::Modpack,
            _ => Class::Mod,
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CfMod {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub download_count: f64,
    #[serde(default)]
    pub class_id: Option<u32>,
    #[serde(default)]
    pub logo: Option<Logo>,
    #[serde(default)]
    pub authors: Vec<Author>,
    #[serde(default)]
    pub links: Option<Links>,
}
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Logo {
    #[serde(default)]
    pub thumbnail_url: String,
}
#[derive(Deserialize, Clone, Debug)]
pub struct Author {
    pub name: String,
}
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Links {
    #[serde(default)]
    pub website_url: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CfFile {
    pub id: u64,
    pub mod_id: u64,
    pub file_name: String,
    #[serde(default)]
    pub download_url: Option<String>,
    #[serde(default)]
    pub file_length: Option<u64>,
    #[serde(default)]
    pub hashes: Vec<CfHash>,
    #[serde(default)]
    pub dependencies: Vec<CfDep>,
}
#[derive(Deserialize, Clone, Debug)]
pub struct CfHash {
    pub value: String,
    pub algo: u32,
}
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CfDep {
    pub mod_id: u64,
    pub relation_type: u32,
}

#[derive(Deserialize)]
struct Data<T> {
    data: T,
}

impl CfFile {
    pub fn sha1(&self) -> Option<String> {
        self.hashes.iter().find(|h| h.algo == 1).map(|h| h.value.to_lowercase())
    }
    /// URL de descarga; si el autor desactivó la distribución por terceros se reconstruye la del CDN.
    pub fn url(&self) -> String {
        self.download_url
            .clone()
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| format!("https://edge.forgecdn.net/files/{}/{}/{}", self.id / 1000, self.id % 1000, urlencoding::encode(&self.file_name)))
    }
}

fn key_or_fail(key: &str) -> Result<&str> {
    let k = key.trim();
    if k.is_empty() {
        bail!("esta copia de Brasa no incluye acceso a CurseForge (se compiló sin BRASA_CURSEFORGE_KEY); usa Modrinth");
    }
    Ok(k)
}

async fn cf_get<T: serde::de::DeserializeOwned>(client: &reqwest::Client, key: &str, path: &str) -> Result<T> {
    let r = client.get(format!("{API}{path}")).header("x-api-key", key_or_fail(key)?).header("Accept", "application/json").send().await?;
    if r.status().as_u16() == 403 {
        bail!("CurseForge rechazó la API key (403)");
    }
    let r = r.error_for_status()?;
    Ok(r.json::<Data<T>>().await?.data)
}

async fn cf_post<T: serde::de::DeserializeOwned>(client: &reqwest::Client, key: &str, path: &str, body: Value) -> Result<T> {
    let r = client.post(format!("{API}{path}")).header("x-api-key", key_or_fail(key)?).json(&body).send().await?.error_for_status()?;
    Ok(r.json::<Data<T>>().await?.data)
}

pub async fn search(client: &reqwest::Client, key: &str, query: &str, class: Class, mc: Option<&str>, loader: Option<Loader>) -> Result<Vec<CfMod>> {
    let mut path =
        format!("/mods/search?gameId={MINECRAFT}&classId={}&searchFilter={}&sortField=2&sortOrder=desc&pageSize=30", class.id(), urlencoding::encode(query));
    if let Some(mc) = mc.filter(|s| !s.is_empty()) {
        path.push_str(&format!("&gameVersion={}", urlencoding::encode(mc)));
    }
    if class == Class::Mod {
        if let Some(id) = loader.and_then(|l| l.curseforge_id()) {
            path.push_str(&format!("&modLoaderType={id}"));
        }
    }
    cf_get(client, key, &path).await
}

async fn best_file(client: &reqwest::Client, key: &str, mod_id: u64, class: Class, inst: &Instance) -> Result<CfFile> {
    let mut path = format!("/mods/{mod_id}/files?gameVersion={}&pageSize=50", urlencoding::encode(&inst.mc_version));
    if class == Class::Mod {
        if let Some(id) = inst.loader.curseforge_id() {
            path.push_str(&format!("&modLoaderType={id}"));
        }
    }
    let files: Vec<CfFile> = cf_get(client, key, &path).await?;
    files.into_iter().next().with_context(|| format!("el proyecto {mod_id} de CurseForge no tiene versión para {} {}", inst.loader.label(), inst.mc_version))
}

/// Instala un proyecto con sus dependencias obligatorias (relationType 3).
pub async fn install(client: &reqwest::Client, key: &str, p: &Paths, inst: &Instance, mod_id: u64, class: Class, rep: &Reporter) -> Result<Vec<String>> {
    if class == Class::Modpack {
        bail!("los modpacks se instalan como instancia nueva");
    }
    if class == Class::Mod && inst.loader == Loader::Vanilla {
        bail!("la instancia '{}' es vanilla: usa una con loader para instalar mods", inst.name);
    }
    let mut queue = vec![mod_id];
    let mut seen = HashSet::new();
    let mut downloads = vec![];
    let mut names = vec![];
    while let Some(id) = queue.pop() {
        if !seen.insert(id) {
            continue;
        }
        let f = best_file(client, key, id, class, inst).await?;
        rep.log(format!("[curseforge] {} → {}", id, f.file_name));
        downloads.push(Download { url: f.url(), path: inst.game_dir(p).join(class.folder()).join(&f.file_name), sha1: f.sha1(), size: f.file_length });
        names.push(f.file_name.clone());
        for d in &f.dependencies {
            if d.relation_type == 3 {
                queue.push(d.mod_id);
            }
        }
    }
    net::download_all(client, downloads, 8, false, rep).await?;
    Ok(names)
}

// ---------------------------------------------------------------- modpacks

#[derive(Deserialize)]
pub(crate) struct Manifest {
    name: String,
    minecraft: McInfo,
    files: Vec<MFile>,
    #[serde(default = "default_overrides")]
    overrides: String,
}
fn default_overrides() -> String {
    "overrides".into()
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct McInfo {
    version: String,
    #[serde(default)]
    mod_loaders: Vec<MLoader>,
}
#[derive(Deserialize)]
pub(crate) struct MLoader {
    id: String,
    #[serde(default)]
    primary: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MFile {
    #[serde(rename = "projectID")]
    project_id: u64,
    #[serde(rename = "fileID")]
    file_id: u64,
    #[serde(default = "yes")]
    required: bool,
}
fn yes() -> bool {
    true
}

fn parse_loader(id: &str) -> (Loader, Option<String>) {
    for (prefix, l) in [("neoforge-", Loader::NeoForge), ("forge-", Loader::Forge), ("fabric-", Loader::Fabric), ("quilt-", Loader::Quilt)] {
        if let Some(v) = id.strip_prefix(prefix) {
            return (l, Some(v.to_string()));
        }
    }
    (Loader::Vanilla, None)
}

/// El fichero más reciente de un modpack.
pub async fn latest_file(client: &reqwest::Client, key: &str, mod_id: u64) -> Result<CfFile> {
    let files: Vec<CfFile> = cf_get(client, key, &format!("/mods/{mod_id}/files?pageSize=1")).await?;
    files.into_iter().next().context("el modpack no tiene ficheros")
}

pub async fn install_modpack(client: &reqwest::Client, key: &str, p: &Paths, mod_id: u64, concurrency: usize, rep: &Reporter) -> Result<Instance> {
    let f = latest_file(client, key, mod_id).await?;
    let path = p.cache().join(&f.file_name);
    rep.stage(format!("Descargando modpack {}", f.file_name));
    net::download_one(client, &Download { url: f.url(), path: path.clone(), sha1: f.sha1(), size: f.file_length }).await?;
    let r = import_zip_tracked(client, key, p, &path, concurrency, rep).await;
    let _ = std::fs::remove_file(&path);
    let (inst, files) = r?;
    let info = crate::content::modpacks::PackInfo {
        source: crate::content::modpacks::Source::CurseForge,
        project: mod_id.to_string(),
        version_id: f.id.to_string(),
        version_name: f.file_name.clone(),
        files,
    };
    if let Err(e) = info.save(&inst.dir(p)) {
        rep.log(format!("[brasa] No se pudo guardar pack.json: {e:#}"));
    }
    Ok(inst)
}

pub(crate) fn read_manifest(file: &Path) -> Result<Manifest> {
    let f = std::fs::File::open(file)?;
    let mut z = zip::ZipArchive::new(f).context("no es un modpack de CurseForge válido")?;
    let mut s = String::new();
    z.by_name("manifest.json").context("falta manifest.json (¿es un modpack de CurseForge?)")?.read_to_string(&mut s)?;
    Ok(serde_json::from_str(&s)?)
}

/// Versión de Minecraft y loader que pide el modpack.
pub(crate) fn manifest_target(man: &Manifest) -> (String, Loader, Option<String>) {
    let primary = man.minecraft.mod_loaders.iter().find(|l| l.primary).or(man.minecraft.mod_loaders.first());
    let (loader, lv) = primary.map(|l| parse_loader(&l.id)).unwrap_or((Loader::Vanilla, None));
    (man.minecraft.version.clone(), loader, lv)
}

/// Instala el contenido del .zip en `game` (overrides + ficheros de CurseForge). Devuelve las rutas
/// relativas escritas. `keep_user` y `verify` como en `modrinth::apply_mrpack`.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn apply_cf_zip(
    client: &reqwest::Client,
    key: &str,
    file: &Path,
    man: &Manifest,
    game: &Path,
    concurrency: usize,
    verify: bool,
    keep_user: bool,
    rep: &Reporter,
) -> Result<Vec<String>> {
    let mut written = vec![];
    {
        let f = std::fs::File::open(file)?;
        let mut z = zip::ZipArchive::new(f).context("no es un modpack de CurseForge válido")?;
        let prefix = format!("{}/", man.overrides.trim_end_matches('/'));
        for i in 0..z.len() {
            let mut e = z.by_index(i)?;
            let Some(name) = e.enclosed_name() else { continue };
            let name_s = name.to_string_lossy().replace('\\', "/");
            let Some(rel) = name_s.strip_prefix(&prefix) else { continue };
            if rel.is_empty() {
                continue;
            }
            let dst = game.join(rel);
            if e.is_dir() {
                std::fs::create_dir_all(&dst)?;
                continue;
            }
            if keep_user && crate::content::modpacks::is_protected(rel) && dst.exists() {
                continue;
            }
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out = std::fs::File::create(&dst)?;
            std::io::copy(&mut e, &mut out)?;
            written.push(rel.to_string());
        }
    }

    // ficheros (consulta en bloque) + clase de cada proyecto para saber la carpeta
    let ids: Vec<u64> = man.files.iter().filter(|f| f.required).map(|f| f.file_id).collect();
    let mods: Vec<u64> = man.files.iter().filter(|f| f.required).map(|f| f.project_id).collect();
    rep.stage(format!("Resolviendo {} ficheros en CurseForge", ids.len()));
    let mut items = vec![];
    for (chunk_files, chunk_mods) in ids.chunks(500).zip(mods.chunks(500)) {
        let files: Vec<CfFile> = cf_post(client, key, "/mods/files", json!({"fileIds": chunk_files})).await?;
        let infos: Vec<CfMod> = cf_post(client, key, "/mods", json!({"modIds": chunk_mods})).await?;
        for f in files {
            let class = infos.iter().find(|m| m.id == f.mod_id).and_then(|m| m.class_id).map(Class::from_class_id).unwrap_or(Class::Mod);
            let rel = format!("{}/{}", class.folder(), f.file_name);
            if !crate::content::modpacks::safe_rel(&rel) {
                bail!("ruta peligrosa en el modpack: {rel}");
            }
            written.push(rel.clone());
            items.push(Download { url: f.url(), path: game.join(&rel), sha1: f.sha1(), size: f.file_length });
        }
    }
    rep.stage(format!("Descargando {} ficheros del modpack", items.len()));
    net::download_all(client, items, concurrency, verify, rep).await?;
    written.sort();
    written.dedup();
    Ok(written)
}

async fn import_zip_tracked(
    client: &reqwest::Client,
    key: &str,
    p: &Paths,
    file: &Path,
    concurrency: usize,
    rep: &Reporter,
) -> Result<(Instance, Vec<String>)> {
    rep.stage(format!("Importando {}", file.display()));
    let man = read_manifest(file)?;
    let (mc, loader, lv) = manifest_target(&man);
    let inst = Instance::create(p, &man.name, &mc, loader, lv)?;
    let files = apply_cf_zip(client, key, file, &man, &inst.game_dir(p), concurrency, false, false, rep).await?;
    Ok((inst, files))
}

/// Importa un .zip de CurseForge suelto. Sin el ID del proyecto no se puede actualizar después.
pub async fn import_zip(client: &reqwest::Client, key: &str, p: &Paths, file: &Path, concurrency: usize, rep: &Reporter) -> Result<Instance> {
    Ok(import_zip_tracked(client, key, p, file, concurrency, rep).await?.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loaders() {
        assert_eq!(parse_loader("forge-47.2.0"), (Loader::Forge, Some("47.2.0".into())));
        assert_eq!(parse_loader("neoforge-21.1.77"), (Loader::NeoForge, Some("21.1.77".into())));
        assert_eq!(parse_loader("fabric-0.16.5").0, Loader::Fabric);
    }
    #[test]
    fn cdn_url() {
        let f = CfFile { id: 4567890, mod_id: 1, file_name: "a b.jar".into(), download_url: None, file_length: None, hashes: vec![], dependencies: vec![] };
        assert_eq!(f.url(), "https://edge.forgecdn.net/files/4567/890/a%20b.jar");
    }
}
