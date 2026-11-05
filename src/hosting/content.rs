//! Plugins (servidores Paper) y mods (Fabric/NeoForge/Forge) desde Modrinth, y la instancia que
//! deja a los amigos listos para entrar con la misma versión y los mismos mods.

use super::{Content, Kind, Server};
use crate::config::{Instance, Loader};
use crate::content::modrinth::{self, Version};
use crate::net::{self, Download};
use crate::paths::Paths;
use crate::progress::Reporter;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;

const API: &str = "https://api.modrinth.com/v2";

/// Los loaders de plugins que entiende Paper.
pub const PLUGIN_LOADERS: [&str; 4] = ["paper", "spigot", "bukkit", "purpur"];

#[derive(Deserialize, Clone, Debug, PartialEq)]
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
    pub icon_url: Option<String>,
    #[serde(default)]
    pub client_side: String,
    #[serde(default)]
    pub server_side: String,
}

#[derive(Deserialize)]
struct SearchResp {
    hits: Vec<Hit>,
}

fn enc(v: &Value) -> String {
    urlencoding::encode(&v.to_string()).into_owned()
}

/// Los filtros de búsqueda para este servidor.
pub fn facets(kind: Kind, mc: &str) -> Value {
    let mut f: Vec<Vec<String>> = vec![];
    match kind {
        Kind::Paper => f.push(PLUGIN_LOADERS.iter().map(|l| format!("categories:{l}")).collect()),
        k if k.mods() => {
            f.push(vec!["project_type:mod".into()]);
            f.push(vec![format!("categories:{}", k.client_loader().modrinth_name())]);
            // los que no funcionan en servidor (minimapas, shaders…) no tienen sentido aquí
            f.push(vec!["server_side:required".into(), "server_side:optional".into()]);
        }
        _ => {}
    }
    if !mc.is_empty() {
        f.push(vec![format!("versions:{mc}")]);
    }
    json!(f)
}

pub async fn search(client: &reqwest::Client, s: &Server, query: &str) -> Result<Vec<Hit>> {
    if s.kind.content_dir().is_none() {
        bail!("un servidor Vanilla no lleva plugins ni mods: crea uno Paper (plugins) o Fabric/NeoForge/Forge (mods)");
    }
    let url = format!("{API}/search?query={}&limit=30&index=relevance&facets={}", urlencoding::encode(query.trim()), enc(&facets(s.kind, &s.mc_version)));
    let r: SearchResp = net::get_json(client, &url).await?;
    Ok(r.hits)
}

/// Las versiones válidas para el servidor. Muchos plugins no marcan cada versión nueva de
/// Minecraft aunque funcionen: si no hay ninguna exacta, se acepta la más reciente para Paper.
async fn versions_for(client: &reqwest::Client, s: &Server, project: &str) -> Result<(Vec<Version>, bool)> {
    let loaders: Vec<&str> = if s.kind.plugins() { PLUGIN_LOADERS.to_vec() } else { vec![s.kind.client_loader().modrinth_name()] };
    let base = format!("{API}/project/{project}/version?loaders={}", enc(&json!(loaders)));
    let exact: Vec<Version> = net::get_json(client, &format!("{base}&game_versions={}", enc(&json!([s.mc_version])))).await?;
    if !exact.is_empty() || !s.kind.plugins() {
        return Ok((exact, true));
    }
    Ok((net::get_json(client, &base).await?, false))
}

/// Instala un plugin o mod con sus dependencias obligatorias. Devuelve lo instalado.
pub async fn install(client: &reqwest::Client, p: &Paths, id: &str, project: &str, rep: &Reporter) -> Result<Vec<Content>> {
    let mut s = Server::load(p, id)?;
    let folder = s.kind.content_dir().context("un servidor Vanilla no lleva plugins ni mods")?;
    let dir = s.dir(p).join(folder);
    std::fs::create_dir_all(&dir)?;
    let have: HashSet<String> = s.content.iter().map(|c| c.project_id.clone()).collect();
    let mut queue = vec![project.to_string()];
    let mut seen = HashSet::new();
    let mut added = vec![];
    while let Some(pid) = queue.pop() {
        if !seen.insert(pid.clone()) || (pid != project && have.contains(&pid)) {
            continue;
        }
        let info: Value = net::get_json(client, &format!("{API}/project/{pid}")).await?;
        let title = info["title"].as_str().unwrap_or(&pid).to_string();
        let real_id = info["id"].as_str().unwrap_or(&pid).to_string();
        let (versions, exact) = versions_for(client, &s, &pid).await?;
        let v = versions.into_iter().next().with_context(|| format!("«{title}» no tiene versión para {} {}", s.kind.label(), s.mc_version))?;
        if !exact {
            rep.log(format!("[brasa] «{title}» no indica {} entre sus versiones: se instala la más reciente ({})", s.mc_version, v.version_number));
        }
        let f = v.primary_file().context("versión sin ficheros")?.clone();
        rep.stage(format!("Instalando {title} {}", v.version_number));
        // si había otra versión del mismo proyecto, fuera
        if let Some(old) = s.content.iter().find(|c| c.project_id == real_id) {
            if old.file != f.filename {
                let _ = std::fs::remove_file(dir.join(&old.file));
            }
        }
        net::download_one(client, &Download { url: f.url.clone(), path: dir.join(&f.filename), sha1: f.hashes.get("sha1").cloned(), size: f.size }).await?;
        let c =
            Content { file: f.filename.clone(), title, project_id: real_id.clone(), client_side: info["client_side"].as_str().unwrap_or_default().to_string() };
        s.content.retain(|x| x.project_id != real_id);
        s.content.push(c.clone());
        added.push(c);
        for d in v.dependencies.iter().filter(|d| d.dependency_type == "required") {
            match (&d.project_id, &d.version_id) {
                (Some(p), _) => queue.push(p.clone()),
                (None, Some(vid)) => queue.push(modrinth::version(client, vid).await?.project_id),
                _ => {}
            }
        }
    }
    let new = added.clone();
    Server::update(p, id, |f| {
        for c in &new {
            f.content.retain(|x| x.project_id != c.project_id);
            f.content.push(c.clone());
        }
        f.content.sort_by_key(|c| c.title.to_lowercase());
        Ok(())
    })?;
    Ok(added)
}

/// Lo que hay en `plugins/` o `mods/`: lo instalado desde Brasa, y también los .jar puestos a mano.
pub fn installed(p: &Paths, s: &Server) -> Vec<Content> {
    let Some(folder) = s.kind.content_dir() else { return vec![] };
    let mut out: Vec<Content> = vec![];
    let files: Vec<String> = std::fs::read_dir(s.dir(p).join(folder))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.to_ascii_lowercase().ends_with(".jar"))
        .collect();
    for f in &files {
        match s.content.iter().find(|c| &c.file == f) {
            Some(c) => out.push(c.clone()),
            None => out.push(Content { file: f.clone(), title: f.trim_end_matches(".jar").to_string(), ..Default::default() }),
        }
    }
    out.sort_by_key(|c| c.title.to_lowercase());
    out
}

pub fn remove(p: &Paths, id: &str, file: &str) -> Result<()> {
    if file.contains('/') || file.contains('\\') || file.contains("..") {
        bail!("nombre de fichero no válido");
    }
    Server::update(p, id, |s| {
        let folder = s.kind.content_dir().context("sin plugins ni mods")?;
        let path = s.dir(p).join(folder).join(file);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        s.content.retain(|c| c.file != file);
        Ok(())
    })
}

/// ¿Lo necesitan también los jugadores? Sólo los mods; los plugins son cosa del servidor.
pub fn for_players(s: &Server, c: &Content) -> bool {
    s.kind.mods() && c.client_side != "unsupported"
}

/// Crea una instancia para entrar a este servidor: misma versión, mismo loader y los mods que
/// también van en el cliente. Deja el servidor en su lista de servidores.
pub fn join_instance(p: &Paths, s: &Server, address: &str) -> Result<Instance> {
    let loader: Loader = s.kind.client_loader();
    let lv = if loader == Loader::Vanilla { None } else { s.loader_version.clone() };
    let inst = Instance::create(p, &format!("{} (servidor)", s.name), &s.mc_version, loader, lv)?;
    if let Some(folder) = s.kind.content_dir().filter(|_| s.kind.mods()) {
        let to = inst.mods_dir(p);
        std::fs::create_dir_all(&to)?;
        for c in installed(p, s).iter().filter(|c| for_players(s, c)) {
            let _ = std::fs::copy(s.dir(p).join(folder).join(&c.file), to.join(&c.file));
        }
    }
    let gd = inst.game_dir(p);
    let mut list = crate::instance::servers::load(&gd).unwrap_or_default();
    list.insert(0, crate::instance::servers::Server::new(&s.name, address));
    crate::instance::servers::save(&gd, &list)?;
    Ok(inst)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_filters_match_the_server() {
        let paper = facets(Kind::Paper, "1.21.4").to_string();
        assert!(paper.contains("categories:paper") && paper.contains("categories:spigot") && paper.contains("versions:1.21.4"));
        assert!(!paper.contains("project_type"), "los plugins no siempre se marcan como tipo plugin");
        let fabric = facets(Kind::Fabric, "1.21.4").to_string();
        assert!(fabric.contains("categories:fabric") && fabric.contains("server_side:required") && fabric.contains("project_type:mod"));
        let neo = facets(Kind::NeoForge, "").to_string();
        assert!(neo.contains("categories:neoforge") && !neo.contains("versions:"));
    }

    #[test]
    fn installed_lists_brasa_and_hand_placed_jars_and_remove_is_safe() {
        let p = Paths { root: std::env::temp_dir().join(format!("brasa-hc-{}", uuid::Uuid::new_v4())) };
        let mut s = Server::create(&p, "Mods", Kind::Fabric, "1.21.4", None, true).unwrap();
        let mods = s.dir(&p).join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        for f in ["lithium.jar", "a-mano.jar", "notas.txt"] {
            std::fs::write(mods.join(f), "").unwrap();
        }
        s.content.push(Content { file: "lithium.jar".into(), title: "Lithium".into(), project_id: "gvQqBUqZ".into(), client_side: "optional".into() });
        s.content.push(Content { file: "solo-servidor.jar".into(), title: "Zzz".into(), project_id: "x".into(), client_side: "unsupported".into() });
        s.save(&p).unwrap();
        let list = installed(&p, &s);
        assert_eq!(list.iter().map(|c| c.title.as_str()).collect::<Vec<_>>(), vec!["a-mano", "Lithium"]);
        assert!(for_players(&s, &list[1]));
        assert!(!for_players(&s, &s.content[1]));
        assert!(remove(&p, &s.id, "../brasa-server.toml").is_err());
        remove(&p, &s.id, "lithium.jar").unwrap();
        assert!(!mods.join("lithium.jar").exists());
        assert!(Server::load(&p, &s.id).unwrap().content.iter().all(|c| c.file != "lithium.jar"));
        let _ = std::fs::remove_dir_all(&p.root);
    }
}
