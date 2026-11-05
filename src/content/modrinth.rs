//! Integración con Modrinth: búsqueda, instalación con dependencias, actualizaciones y modpacks .mrpack.

use crate::config::{Instance, Loader};
use crate::net::{self, Download};
use crate::paths::Paths;
use crate::progress::Reporter;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};

const API: &str = "https://api.modrinth.com/v2";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectType {
    Mod,
    ResourcePack,
    ShaderPack,
    Modpack,
}

impl ProjectType {
    pub const ALL: [ProjectType; 4] = [ProjectType::Mod, ProjectType::ResourcePack, ProjectType::ShaderPack, ProjectType::Modpack];
    pub fn api(&self) -> &'static str {
        match self {
            ProjectType::Mod => "mod",
            ProjectType::ResourcePack => "resourcepack",
            ProjectType::ShaderPack => "shader",
            ProjectType::Modpack => "modpack",
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            ProjectType::Mod => "Mods",
            ProjectType::ResourcePack => "Resource packs",
            ProjectType::ShaderPack => "Shaders",
            ProjectType::Modpack => "Modpacks",
        }
    }
    pub fn folder(&self) -> &'static str {
        match self {
            ProjectType::Mod | ProjectType::Modpack => "mods",
            ProjectType::ResourcePack => "resourcepacks",
            ProjectType::ShaderPack => "shaderpacks",
        }
    }
    pub fn parse(s: &str) -> ProjectType {
        match s {
            "resourcepack" => ProjectType::ResourcePack,
            "shader" | "shaderpack" => ProjectType::ShaderPack,
            "modpack" => ProjectType::Modpack,
            _ => ProjectType::Mod,
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct Hit {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub project_type: String,
    #[serde(default)]
    pub icon_url: Option<String>,
}

#[derive(Deserialize)]
struct SearchResp {
    hits: Vec<Hit>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Version {
    pub id: String,
    pub project_id: String,
    pub version_number: String,
    #[serde(default)]
    pub files: Vec<VFile>,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
}
#[derive(Deserialize, Clone, Debug)]
pub struct VFile {
    pub url: String,
    pub filename: String,
    #[serde(default)]
    pub primary: bool,
    pub hashes: HashMap<String, String>,
    #[serde(default)]
    pub size: Option<u64>,
}
#[derive(Deserialize, Clone, Debug)]
pub struct Dependency {
    pub project_id: Option<String>,
    pub version_id: Option<String>,
    pub dependency_type: String,
}

impl Version {
    pub fn primary_file(&self) -> Option<&VFile> {
        self.files.iter().find(|f| f.primary).or_else(|| self.files.first())
    }
}

fn q(v: &Value) -> String {
    urlencoding::encode(&v.to_string()).into_owned()
}

pub async fn search(client: &reqwest::Client, query: &str, kind: ProjectType, mc: Option<&str>, loader: Option<Loader>) -> Result<Vec<Hit>> {
    let mut facets: Vec<Vec<String>> = vec![vec![format!("project_type:{}", kind.api())]];
    if let Some(mc) = mc.filter(|s| !s.is_empty()) {
        facets.push(vec![format!("versions:{mc}")]);
    }
    if let (Some(l), ProjectType::Mod) = (loader, kind) {
        if l != Loader::Vanilla {
            facets.push(vec![format!("categories:{}", l.modrinth_name())]);
        }
    }
    let url = format!("{API}/search?query={}&limit=30&index=relevance&facets={}", urlencoding::encode(query), q(&json!(facets)));
    let r: SearchResp = net::get_json(client, &url).await?;
    Ok(r.hits)
}

/// Versiones compatibles de un proyecto (más reciente primero).
pub async fn compatible_versions(client: &reqwest::Client, project: &str, kind: ProjectType, mc: &str, loader: Loader) -> Result<Vec<Version>> {
    let mut url = format!("{API}/project/{project}/version?game_versions={}", q(&json!([mc])));
    if kind == ProjectType::Mod && loader != Loader::Vanilla {
        let mut loaders = vec![loader.modrinth_name()];
        // Quilt carga mods de Fabric
        if loader == Loader::Quilt {
            loaders.push("fabric");
        }
        url.push_str(&format!("&loaders={}", q(&json!(loaders))));
    }
    net::get_json(client, &url).await
}

pub async fn version(client: &reqwest::Client, id: &str) -> Result<Version> {
    net::get_json(client, &format!("{API}/version/{id}")).await
}

/// Instala un proyecto en la instancia, resolviendo dependencias obligatorias de forma recursiva.
/// Devuelve los nombres de fichero instalados.
pub async fn install_project(client: &reqwest::Client, p: &Paths, inst: &Instance, project: &str, kind: ProjectType, rep: &Reporter) -> Result<Vec<String>> {
    if kind == ProjectType::Modpack {
        bail!("los modpacks se instalan como instancia nueva (usa 'Importar modpack')");
    }
    if kind == ProjectType::Mod && inst.loader == Loader::Vanilla {
        bail!("la instancia '{}' es vanilla: crea una con Fabric/Quilt/Forge/NeoForge para usar mods", inst.name);
    }
    let installed_ids = installed_project_ids(client, p, inst).await.unwrap_or_default();
    let mut queue: Vec<(String, Option<String>)> = vec![(project.to_string(), None)];
    let mut seen: HashSet<String> = HashSet::new();
    let mut downloads = vec![];
    let mut names = vec![];
    while let Some((pid, vid)) = queue.pop() {
        if !seen.insert(pid.clone()) {
            continue;
        }
        if pid != project && installed_ids.contains(&pid) {
            continue; // dependencia ya instalada
        }
        let v = match vid {
            Some(vid) => version(client, &vid).await?,
            None => compatible_versions(client, &pid, kind, &inst.mc_version, inst.loader)
                .await?
                .into_iter()
                .next()
                .with_context(|| format!("'{pid}' no tiene versión para {} {}", inst.loader.label(), inst.mc_version))?,
        };
        let f = v.primary_file().context("versión sin ficheros")?;
        rep.log(format!("[modrinth] {} {} → {}", pid, v.version_number, f.filename));
        downloads.push(Download {
            url: f.url.clone(),
            path: inst.game_dir(p).join(kind.folder()).join(&f.filename),
            sha1: f.hashes.get("sha1").cloned(),
            size: f.size,
        });
        names.push(f.filename.clone());
        for d in &v.dependencies {
            if d.dependency_type != "required" {
                continue;
            }
            let dep_pid = match (&d.project_id, &d.version_id) {
                (Some(p), _) => p.clone(),
                (None, Some(vid)) => version(client, vid).await?.project_id,
                _ => continue,
            };
            queue.push((dep_pid, d.version_id.clone()));
        }
    }
    rep.stage(format!("Descargando {} fichero(s)", downloads.len()));
    net::download_all(client, downloads, 8, false, rep).await?;
    Ok(names)
}

// ---------------------------------------------------------------- gestión de mods locales

#[derive(Clone, Debug)]
pub struct LocalMod {
    pub file_name: String,
    pub path: PathBuf,
    pub enabled: bool,
    pub size: u64,
}

pub fn list_mods(p: &Paths, inst: &Instance) -> Vec<LocalMod> {
    let mut v = vec![];
    if let Ok(rd) = std::fs::read_dir(inst.mods_dir(p)) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let enabled = name.ends_with(".jar");
            if !(enabled || name.ends_with(".jar.disabled")) {
                continue;
            }
            v.push(LocalMod { size: e.metadata().map(|m| m.len()).unwrap_or(0), file_name: name, path: e.path(), enabled });
        }
    }
    v.sort_by_key(|m| m.file_name.to_lowercase());
    v
}

pub fn toggle_mod(m: &LocalMod) -> Result<()> {
    let new = if m.enabled {
        PathBuf::from(format!("{}.disabled", m.path.display()))
    } else {
        PathBuf::from(m.path.to_string_lossy().trim_end_matches(".disabled").to_string())
    };
    std::fs::rename(&m.path, new)?;
    Ok(())
}

async fn hashes_of(paths: Vec<PathBuf>) -> Result<HashMap<String, PathBuf>> {
    tokio::task::spawn_blocking(move || paths.into_iter().filter_map(|p| net::sha1_file(&p).ok().map(|h| (h, p))).collect::<HashMap<_, _>>())
        .await
        .map_err(Into::into)
}

async fn installed_project_ids(client: &reqwest::Client, p: &Paths, inst: &Instance) -> Result<HashSet<String>> {
    let mods = list_mods(p, inst);
    if mods.is_empty() {
        return Ok(HashSet::new());
    }
    let hashes = hashes_of(mods.into_iter().map(|m| m.path).collect()).await?;
    let keys: Vec<&String> = hashes.keys().collect();
    let resp: HashMap<String, Version> =
        client.post(format!("{API}/version_files")).json(&json!({"hashes": keys, "algorithm": "sha1"})).send().await?.error_for_status()?.json().await?;
    Ok(resp.into_values().map(|v| v.project_id).collect())
}

#[derive(Clone, Debug)]
pub struct Update {
    pub old_path: PathBuf,
    pub new_version: Version,
}

/// Comprueba actualizaciones de todos los mods (identificados por hash en Modrinth).
pub async fn check_updates(client: &reqwest::Client, p: &Paths, inst: &Instance) -> Result<Vec<Update>> {
    let mods: Vec<PathBuf> = list_mods(p, inst).into_iter().filter(|m| m.enabled).map(|m| m.path).collect();
    if mods.is_empty() {
        return Ok(vec![]);
    }
    let hashes = hashes_of(mods).await?;
    let mut loaders = vec![inst.loader.modrinth_name()];
    if inst.loader == Loader::Quilt {
        loaders.push("fabric");
    }
    let keys: Vec<&String> = hashes.keys().collect();
    let resp: HashMap<String, Version> = client
        .post(format!("{API}/version_files/update"))
        .json(&json!({"hashes": keys, "algorithm": "sha1", "loaders": loaders, "game_versions": [inst.mc_version]}))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let mut out = vec![];
    for (hash, v) in resp {
        let Some(old) = hashes.get(&hash) else { continue };
        let is_same = v.files.iter().any(|f| f.hashes.get("sha1").map(|h| h == &hash).unwrap_or(false));
        if !is_same {
            out.push(Update { old_path: old.clone(), new_version: v });
        }
    }
    Ok(out)
}

pub async fn apply_updates(client: &reqwest::Client, p: &Paths, inst: &Instance, ups: &[Update], rep: &Reporter) -> Result<usize> {
    let mut n = 0;
    for u in ups {
        let Some(f) = u.new_version.primary_file() else { continue };
        let d = Download { url: f.url.clone(), path: inst.mods_dir(p).join(&f.filename), sha1: f.hashes.get("sha1").cloned(), size: f.size };
        net::download_one(client, &d).await?;
        if u.old_path != d.path {
            let _ = std::fs::remove_file(&u.old_path);
        }
        rep.log(format!("[update] {} → {}", u.old_path.file_name().unwrap_or_default().to_string_lossy(), f.filename));
        n += 1;
    }
    Ok(n)
}

// ---------------------------------------------------------------- modpacks .mrpack

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PackIndex {
    name: String,
    files: Vec<PackFile>,
    dependencies: HashMap<String, String>,
}
#[derive(Deserialize)]
pub(crate) struct PackFile {
    path: String,
    hashes: HashMap<String, String>,
    #[serde(default)]
    env: Option<HashMap<String, String>>,
    downloads: Vec<String>,
    #[serde(rename = "fileSize")]
    file_size: Option<u64>,
}

/// Última versión publicada de un proyecto (Modrinth las devuelve de la más nueva a la más vieja).
pub async fn latest_version(client: &reqwest::Client, project: &str) -> Result<Version> {
    let versions: Vec<Version> = net::get_json(client, &format!("{API}/project/{project}/version")).await?;
    versions.into_iter().next().context("el modpack no tiene versiones")
}

/// Descarga la última versión de un modpack de Modrinth y lo importa como instancia.
pub async fn install_modpack(client: &reqwest::Client, p: &Paths, project: &str, concurrency: usize, rep: &Reporter) -> Result<Instance> {
    let v = latest_version(client, project).await?;
    let f = v.primary_file().context("versión sin fichero")?;
    let path = p.cache().join(&f.filename);
    rep.stage(format!("Descargando modpack {}", f.filename));
    net::download_one(client, &Download { url: f.url.clone(), path: path.clone(), sha1: f.hashes.get("sha1").cloned(), size: f.size }).await?;
    let inst = import_mrpack(client, p, &path, concurrency, rep).await;
    let _ = std::fs::remove_file(&path);
    inst
}

pub(crate) fn read_index(file: &Path) -> Result<PackIndex> {
    let f = std::fs::File::open(file).with_context(|| format!("no se puede abrir {}", file.display()))?;
    let mut z = zip::ZipArchive::new(f).context("no es un .mrpack válido")?;
    let mut s = String::new();
    z.by_name("modrinth.index.json").context("falta modrinth.index.json")?.read_to_string(&mut s)?;
    Ok(serde_json::from_str(&s)?)
}

/// Versión de Minecraft y loader que pide el pack.
pub(crate) fn pack_target(index: &PackIndex) -> Result<(String, Loader, Option<String>)> {
    let mc = index.dependencies.get("minecraft").context("el pack no indica versión de Minecraft")?.clone();
    let (loader, lv) = if let Some(v) = index.dependencies.get("fabric-loader") {
        (Loader::Fabric, Some(v.clone()))
    } else if let Some(v) = index.dependencies.get("quilt-loader") {
        (Loader::Quilt, Some(v.clone()))
    } else if let Some(v) = index.dependencies.get("neoforge") {
        (Loader::NeoForge, Some(v.clone()))
    } else if let Some(v) = index.dependencies.get("forge") {
        (Loader::Forge, Some(v.clone()))
    } else {
        (Loader::Vanilla, None)
    };
    Ok((mc, loader, lv))
}

/// Instala el contenido de un .mrpack en `game`: overrides y ficheros remotos. Devuelve las rutas
/// relativas que escribió. Con `keep_user` no pisa lo protegido del jugador (mundos, opciones...).
/// Con `verify` vuelve a descargar los ficheros existentes cuyo SHA-1 no coincide.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn apply_mrpack(
    client: &reqwest::Client,
    file: &Path,
    index: &PackIndex,
    game: &Path,
    concurrency: usize,
    verify: bool,
    keep_user: bool,
    rep: &Reporter,
) -> Result<Vec<String>> {
    let mut written = vec![];
    {
        let f = std::fs::File::open(file).with_context(|| format!("no se puede abrir {}", file.display()))?;
        let mut z = zip::ZipArchive::new(f).context("no es un .mrpack válido")?;
        for i in 0..z.len() {
            let mut e = z.by_index(i)?;
            let Some(name) = e.enclosed_name() else { continue };
            let rel = if let Ok(r) = name.strip_prefix("overrides") {
                r.to_path_buf()
            } else if let Ok(r) = name.strip_prefix("client-overrides") {
                r.to_path_buf()
            } else {
                continue;
            };
            if rel.as_os_str().is_empty() {
                continue;
            }
            let rel_s = rel.to_string_lossy().replace('\\', "/");
            let dst = game.join(&rel);
            if e.is_dir() {
                std::fs::create_dir_all(&dst)?;
                continue;
            }
            if keep_user && crate::content::modpacks::is_protected(&rel_s) && dst.exists() {
                continue;
            }
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out = std::fs::File::create(&dst)?;
            std::io::copy(&mut e, &mut out)?;
            written.push(rel_s);
        }
    }

    let mut items = vec![];
    for pf in &index.files {
        if let Some(env) = &pf.env {
            if env.get("client").map(|s| s == "unsupported").unwrap_or(false) {
                continue;
            }
        }
        if !crate::content::modpacks::safe_rel(&pf.path) {
            bail!("ruta peligrosa en el modpack: {}", pf.path);
        }
        let Some(url) = pf.downloads.first() else { continue };
        written.push(pf.path.replace('\\', "/"));
        items.push(Download { url: url.clone(), path: game.join(&pf.path), sha1: pf.hashes.get("sha1").cloned(), size: pf.file_size });
    }
    rep.stage(format!("Descargando {} ficheros del modpack", items.len()));
    net::download_all(client, items, concurrency, verify, rep).await?;
    written.sort();
    written.dedup();
    Ok(written)
}

/// Identifica un .mrpack publicado en Modrinth por su SHA-1 (para poder actualizarlo después).
async fn identify(client: &reqwest::Client, file: &Path) -> Option<Version> {
    let path = file.to_path_buf();
    let sha1 = tokio::task::spawn_blocking(move || net::sha1_file(&path)).await.ok()?.ok()?;
    net::get_json(client, &format!("{API}/version_file/{sha1}?algorithm=sha1")).await.ok()
}

pub async fn import_mrpack(client: &reqwest::Client, p: &Paths, file: &Path, concurrency: usize, rep: &Reporter) -> Result<Instance> {
    rep.stage(format!("Importando {}", file.display()));
    let index = read_index(file)?;
    let (mc, loader, lv) = pack_target(&index)?;
    let inst = Instance::create(p, &index.name, &mc, loader, lv)?;
    let files = apply_mrpack(client, file, &index, &inst.game_dir(p), concurrency, false, false, rep).await?;
    match identify(client, file).await {
        Some(v) => {
            let info = crate::content::modpacks::PackInfo {
                source: crate::content::modpacks::Source::Modrinth,
                project: v.project_id,
                version_id: v.id,
                version_name: v.version_number,
                files,
            };
            if let Err(e) = info.save(&inst.dir(p)) {
                rep.log(format!("[brasa] No se pudo guardar pack.json: {e:#}"));
            }
        }
        None => rep.log("[brasa] Este .mrpack no está publicado en Modrinth: se podrá jugar, pero no actualizar desde Brasa".to_string()),
    }
    Ok(inst)
}

// ---------------------------------------------------------------- perfil de rendimiento

/// Mods de optimización probados por loader (slugs de Modrinth). Se instalan los que existan para la versión.
pub fn performance_mods(loader: Loader) -> &'static [&'static str] {
    match loader {
        Loader::Fabric | Loader::Quilt => &["sodium", "lithium", "ferrite-core", "entityculling", "immediatelyfast", "modernfix", "krypton"],
        Loader::NeoForge => &["sodium", "ferrite-core", "entityculling", "immediatelyfast", "modernfix"],
        Loader::Forge => &["embeddium", "ferrite-core", "entityculling", "immediatelyfast", "modernfix"],
        Loader::Vanilla => &[],
    }
}

/// Instala el perfil de rendimiento. Devuelve (instalados, no disponibles para esta versión).
pub async fn install_performance(client: &reqwest::Client, p: &Paths, inst: &Instance, rep: &Reporter) -> Result<(Vec<String>, Vec<String>)> {
    if inst.loader == Loader::Vanilla {
        bail!("el perfil de rendimiento necesita un loader (Fabric, Quilt, Forge o NeoForge)");
    }
    let mut ok = vec![];
    let mut missing = vec![];
    for slug in performance_mods(inst.loader) {
        rep.stage(format!("Rendimiento: {slug}"));
        match install_project(client, p, inst, slug, ProjectType::Mod, rep).await {
            Ok(_) => ok.push(slug.to_string()),
            Err(e) => {
                rep.log(format!("[rendimiento] {slug}: {e:#}"));
                missing.push(slug.to_string());
            }
        }
    }
    Ok((ok, missing))
}

// ---------------------------------------------------------------- exportar .mrpack

/// Exporta la instancia como .mrpack. Lo que está en Modrinth se referencia por URL; el resto va en overrides.
pub async fn export_mrpack(client: &reqwest::Client, p: &Paths, inst: &Instance, out: &Path, include_saves: bool, rep: &Reporter) -> Result<(usize, usize)> {
    use std::io::Write;
    let game = inst.game_dir(p);
    let loader_version =
        inst.loader_version.clone().or_else(|| inst.resolved_version.as_deref().and_then(|id| inst.loader.version_from_id(id, &inst.mc_version)));
    if inst.loader != Loader::Vanilla && loader_version.is_none() {
        bail!("instala la instancia al menos una vez antes de exportarla (no se conoce la versión del loader)");
    }

    // Ficheros candidatos a referenciarse en Modrinth
    let mut candidates: Vec<(String, PathBuf)> = vec![];
    for dir in ["mods", "resourcepacks", "shaderpacks"] {
        if let Ok(rd) = std::fs::read_dir(game.join(dir)) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if e.path().is_file() && (name.ends_with(".jar") || name.ends_with(".zip")) && name != "brasa-skin.zip" {
                    candidates.push((format!("{dir}/{name}"), e.path()));
                }
            }
        }
    }
    rep.stage(format!("Calculando hashes de {} ficheros", candidates.len()));
    let hashed: Vec<(String, PathBuf, String, String, u64)> = tokio::task::spawn_blocking(move || {
        candidates
            .into_iter()
            .filter_map(|(rel, path)| {
                let s1 = net::sha1_file(&path).ok()?;
                let s5 = net::sha512_file(&path).ok()?;
                let size = std::fs::metadata(&path).ok()?.len();
                Some((rel, path, s1, s5, size))
            })
            .collect()
    })
    .await?;

    rep.stage("Buscando ficheros en Modrinth");
    let keys: Vec<&String> = hashed.iter().map(|h| &h.2).collect();
    let found: HashMap<String, Version> = if keys.is_empty() {
        HashMap::new()
    } else {
        client.post(format!("{API}/version_files")).json(&json!({"hashes": keys, "algorithm": "sha1"})).send().await?.error_for_status()?.json().await?
    };

    let mut files = vec![];
    let mut overrides_files: Vec<(String, PathBuf)> = vec![];
    for (rel, path, s1, s5, size) in &hashed {
        let url = found.get(s1).and_then(|v| v.files.iter().find(|f| f.hashes.get("sha1") == Some(s1))).map(|f| f.url.clone());
        match url {
            Some(url) => files.push(json!({
                "path": rel,
                "hashes": {"sha1": s1, "sha512": s5},
                "env": {"client": "required", "server": "required"},
                "downloads": [url],
                "fileSize": size
            })),
            None => overrides_files.push((rel.clone(), path.clone())),
        }
    }
    let referenced = files.len();

    let mut deps = serde_json::Map::new();
    deps.insert("minecraft".into(), json!(inst.mc_version));
    if let Some(lv) = &loader_version {
        let key = match inst.loader {
            Loader::Fabric => "fabric-loader",
            Loader::Quilt => "quilt-loader",
            Loader::Forge => "forge",
            Loader::NeoForge => "neoforge",
            Loader::Vanilla => "",
        };
        if !key.is_empty() {
            deps.insert(key.into(), json!(lv));
        }
    }
    let index = json!({
        "formatVersion": 1,
        "game": "minecraft",
        "versionId": "1.0.0",
        "name": inst.name,
        "summary": "Exportado con Brasa Launcher",
        "files": files,
        "dependencies": deps
    });

    rep.stage(format!("Escribiendo {}", out.display()));
    let f = std::fs::File::create(out).with_context(|| format!("no se puede crear {}", out.display()))?;
    let mut z = zip::ZipWriter::new(f);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    z.start_file("modrinth.index.json", opts)?;
    z.write_all(serde_json::to_string_pretty(&index)?.as_bytes())?;

    // overrides: config, options, ficheros no encontrados en Modrinth (y mundos si se pide)
    let mut extra_dirs = vec!["config", "kubejs", "defaultconfigs"];
    if include_saves {
        extra_dirs.push("saves");
    }
    let add = |rel: &str, path: &Path, z: &mut zip::ZipWriter<std::fs::File>| -> Result<()> {
        z.start_file(format!("overrides/{}", rel.replace('\\', "/")), opts)?;
        let mut src = std::fs::File::open(path)?;
        std::io::copy(&mut src, z)?;
        Ok(())
    };
    for (rel, path) in &overrides_files {
        add(rel, path, &mut z)?;
    }
    for name in ["options.txt", "servers.dat"] {
        let path = game.join(name);
        if path.is_file() {
            add(name, &path, &mut z)?;
        }
    }
    for dir in extra_dirs {
        let base = game.join(dir);
        if !base.is_dir() {
            continue;
        }
        let mut stack = vec![base.clone()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d)?.flatten() {
                let path = e.path();
                if path.is_dir() {
                    stack.push(path);
                } else if let Ok(rel) = path.strip_prefix(&game) {
                    add(&rel.to_string_lossy(), &path, &mut z)?;
                }
            }
        }
    }
    z.finish()?;
    Ok((referenced, overrides_files.len()))
}
