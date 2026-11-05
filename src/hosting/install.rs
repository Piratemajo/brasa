//! Descarga el software del servidor y deja apuntado cómo arrancarlo.
//!
//! - Vanilla: el `server.jar` oficial (del JSON de la versión de Mojang).
//! - Paper: la última build estable de la API de PaperMC (`fill.papermc.io`).
//! - Fabric: el lanzador de servidor de `meta.fabricmc.net` (un .jar que descarga lo demás al arrancar).
//! - NeoForge/Forge: su instalador oficial con `--installServer`; los modernos dejan un fichero de
//!   argumentos (`libraries/…/unix_args.txt`), los antiguos un `forge-….jar`.

use super::{Kind, Launch, Server};
use crate::net::{self, Download};
use crate::paths::Paths;
use crate::progress::Reporter;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Instala (o reinstala) el software y guarda cómo se arranca.
pub async fn install(client: &reqwest::Client, p: &Paths, s: &mut Server, rep: &Reporter) -> Result<()> {
    let dir = s.dir(p);
    std::fs::create_dir_all(&dir)?;
    rep.stage(format!("Preparando el servidor {} ({})", s.name, s.kind.label()));
    let vanilla = crate::game::mojang::resolve(client, p, &s.mc_version).await.with_context(|| format!("no se encontró Minecraft {}", s.mc_version))?;
    s.java_major = crate::game::mojang::required_java(&vanilla.json).max(8);

    let launch = match s.kind {
        Kind::Vanilla => {
            let a =
                vanilla.json.downloads.as_ref().and_then(|d| d.get("server")).with_context(|| format!("Mojang no publica servidor para {}", s.mc_version))?;
            rep.stage(format!("Descargando el servidor de Minecraft {}", s.mc_version));
            net::download_one(client, &Download { url: a.url.clone(), path: dir.join("server.jar"), sha1: a.sha1.clone(), size: a.size }).await?;
            Launch::Jar("server.jar".into())
        }
        Kind::Paper => {
            let b = paper_build(client, &s.mc_version).await?;
            rep.stage(format!("Descargando Paper {} (build {})", s.mc_version, b.id));
            let path = dir.join("paper.jar");
            net::download_one(client, &Download { url: b.url.clone(), path: path.clone(), sha1: None, size: b.size }).await?;
            if let Some(want) = b.sha256 {
                let got = net::sha256_file(&path)?;
                if !got.eq_ignore_ascii_case(&want) {
                    let _ = std::fs::remove_file(&path);
                    bail!("el paper.jar descargado no coincide con su suma (esperada {want}, recibida {got})");
                }
            }
            s.loader_version = Some(b.id.to_string());
            Launch::Jar("paper.jar".into())
        }
        Kind::Fabric => {
            let loader = match s.loader_version.clone().filter(|v| !v.is_empty()) {
                Some(v) => v,
                None => crate::game::loaders::versions_cached(client, p, crate::config::Loader::Fabric, &s.mc_version)
                    .await?
                    .0
                    .into_iter()
                    .next()
                    .with_context(|| format!("Fabric no tiene versiones para {}", s.mc_version))?,
            };
            let installer = fabric_installer(client).await?;
            rep.stage(format!("Descargando Fabric {loader} para servidor"));
            let url = format!("https://meta.fabricmc.net/v2/versions/loader/{}/{loader}/{installer}/server/jar", s.mc_version);
            net::download_one(client, &Download { url, path: dir.join("fabric-server-launch.jar"), sha1: None, size: None }).await?;
            s.loader_version = Some(loader);
            Launch::Jar("fabric-server-launch.jar".into())
        }
        Kind::NeoForge | Kind::Forge => {
            let loader = s.kind.client_loader();
            let ver = match s.loader_version.clone().filter(|v| !v.is_empty()) {
                Some(v) => v,
                None => crate::game::loaders::versions_cached(client, p, loader, &s.mc_version)
                    .await?
                    .0
                    .into_iter()
                    .next()
                    .with_context(|| format!("{} no tiene versiones para {}", loader.label(), s.mc_version))?,
            };
            let jar = download_installer(client, p, s.kind, &s.mc_version, &ver).await?;
            let settings = crate::config::Settings::load(p);
            let component = crate::game::mojang::java_component(&vanilla.json);
            let java = crate::game::java::pick_for_installer(
                client,
                p,
                Some(settings.java_path.as_str()),
                s.java_major,
                component.as_deref(),
                settings.java_auto_download,
                rep,
            )
            .await?;
            rep.stage(format!("Ejecutando el instalador de {} {ver} (tarda un par de minutos)", loader.label()));
            let mut cmd = tokio::process::Command::new(&java);
            cmd.arg("-jar").arg(&jar).arg("--installServer").arg(&dir).current_dir(&dir);
            #[cfg(windows)]
            cmd.creation_flags(0x0800_0000); // sin ventana de consola
            let out = cmd.output().await.context("no se pudo ejecutar Java para el instalador")?;
            let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
            for l in text.lines().rev().take(12).collect::<Vec<_>>().into_iter().rev() {
                rep.log(format!("[instalador] {l}"));
            }
            s.loader_version = Some(ver.clone());
            match find_forge_launch(&dir, s.kind, &s.mc_version, &ver) {
                Some(l) => l,
                None => bail!("el instalador de {} terminó sin dejar el servidor listo (mira la consola)", loader.label()),
            }
        }
    };
    s.launch = Some(launch);
    s.write_files(p)?;
    let (l, j, v) = (s.launch.clone(), s.java_major, s.loader_version.clone());
    Server::update(p, &s.id, |f| {
        f.launch = l;
        f.java_major = j;
        f.loader_version = v;
        Ok(())
    })?;
    rep.stage(format!("{} listo", s.name));
    Ok(())
}

// --------------------------------------------------------------------------------- Paper

#[derive(Debug, Clone, PartialEq)]
pub struct PaperBuild {
    pub id: u64,
    pub url: String,
    pub sha256: Option<String>,
    pub size: Option<u64>,
}

/// La build estable más reciente en la respuesta de `…/versions/{v}/builds` (la API v3 de PaperMC).
pub fn pick_paper_build(builds: &Value) -> Option<PaperBuild> {
    let list = builds.as_array()?;
    let stable = |b: &&Value| b["channel"].as_str().map(|c| c.eq_ignore_ascii_case("STABLE")).unwrap_or(false);
    let best = list.iter().filter(stable).max_by_key(|b| b["id"].as_u64().unwrap_or(0))?;
    let d = &best["downloads"]["server:default"];
    Some(PaperBuild {
        id: best["id"].as_u64()?,
        url: d["url"].as_str()?.to_string(),
        sha256: d["checksums"]["sha256"].as_str().map(str::to_string),
        size: d["size"].as_u64(),
    })
}

async fn paper_build(client: &reqwest::Client, mc: &str) -> Result<PaperBuild> {
    let url = format!("https://fill.papermc.io/v3/projects/paper/versions/{mc}/builds");
    let v: Value = match net::get_json(client, &url).await {
        Ok(v) => v,
        Err(e) => {
            let msg = format!("{e:#}");
            if msg.contains("404") {
                bail!("Paper todavía no tiene versión para Minecraft {mc}: elige otra o usa Vanilla");
            }
            return Err(e).context("no se pudo consultar PaperMC");
        }
    };
    pick_paper_build(&v).with_context(|| format!("Paper no tiene builds estables para {mc} todavía (sólo de prueba)"))
}

// --------------------------------------------------------------------------------- Fabric

#[derive(Deserialize)]
struct FabricInstaller {
    version: String,
    #[serde(default)]
    stable: bool,
}

async fn fabric_installer(client: &reqwest::Client) -> Result<String> {
    let list: Vec<FabricInstaller> = net::get_json(client, "https://meta.fabricmc.net/v2/versions/installer").await?;
    list.iter().find(|i| i.stable).or(list.first()).map(|i| i.version.clone()).context("Fabric no publica instalador")
}

// ------------------------------------------------------------------------- NeoForge / Forge

/// Las URLs posibles del instalador. Los Forge antiguos llevan la versión de MC repetida al final.
pub fn installer_urls(kind: Kind, mc: &str, ver: &str) -> Vec<(String, String)> {
    match kind {
        Kind::NeoForge => vec![(
            format!("https://maven.neoforged.net/releases/net/neoforged/neoforge/{ver}/neoforge-{ver}-installer.jar"),
            format!("neoforge-{ver}-installer.jar"),
        )],
        Kind::Forge => [format!("{mc}-{ver}"), format!("{mc}-{ver}-{mc}")]
            .into_iter()
            .map(|full| {
                (format!("https://maven.minecraftforge.net/net/minecraftforge/forge/{full}/forge-{full}-installer.jar"), format!("forge-{full}-installer.jar"))
            })
            .collect(),
        _ => vec![],
    }
}

async fn download_installer(client: &reqwest::Client, p: &Paths, kind: Kind, mc: &str, ver: &str) -> Result<PathBuf> {
    let mut last = None;
    for (url, name) in installer_urls(kind, mc, ver) {
        let path = p.cache().join(&name);
        match net::download_one(client, &Download { url, path: path.clone(), sha1: None, size: None }).await {
            Ok(()) => return Ok(path),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| anyhow::anyhow!("sin instalador"))).context("no se encontró el instalador para esa versión")
}

/// Cómo arrancar lo que dejó el instalador: el fichero de argumentos de este sistema, o el .jar.
pub fn find_forge_launch(dir: &Path, kind: Kind, mc: &str, ver: &str) -> Option<Launch> {
    let args_name = if cfg!(windows) { "win_args.txt" } else { "unix_args.txt" };
    let candidates = match kind {
        Kind::NeoForge => vec![format!("libraries/net/neoforged/neoforge/{ver}"), format!("libraries/net/neoforged/forge/{mc}-{ver}")],
        _ => vec![format!("libraries/net/minecraftforge/forge/{mc}-{ver}")],
    };
    for c in candidates {
        let f = format!("{c}/{args_name}");
        if dir.join(&f).is_file() {
            return Some(Launch::Args(f));
        }
    }
    // Forge antiguo (hasta 1.16): forge-1.16.5-36.2.39.jar en la carpeta
    let mut jars: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| {
            let l = n.to_ascii_lowercase();
            l.ends_with(".jar") && (l.starts_with("forge-") || l.starts_with("neoforge-")) && !l.contains("installer")
        })
        .collect();
    jars.sort();
    jars.into_iter().next().map(Launch::Jar)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn paper_takes_the_newest_stable_build() {
        let v = json!([
            {"id": 12, "channel": "STABLE", "downloads": {"server:default": {"url": "https://x/12.jar", "checksums": {"sha256": "aa"}, "size": 10}}},
            {"id": 14, "channel": "BETA", "downloads": {"server:default": {"url": "https://x/14.jar"}}},
            {"id": 13, "channel": "STABLE", "downloads": {"server:default": {"url": "https://x/13.jar"}}}
        ]);
        let b = pick_paper_build(&v).unwrap();
        assert_eq!((b.id, b.url.as_str(), b.sha256.as_deref()), (13, "https://x/13.jar", None));
        assert_eq!(pick_paper_build(&json!([{"id": 1, "channel": "ALPHA"}])), None, "sin estables, nada");
        assert_eq!(pick_paper_build(&json!({"ok": false, "message": "not found"})), None);
    }

    #[test]
    fn forge_launch_is_found_for_new_and_old_installers() {
        let d = std::env::temp_dir().join(format!("brasa-forge-{}", uuid::Uuid::new_v4()));
        let args = if cfg!(windows) { "win_args.txt" } else { "unix_args.txt" };
        let lib = d.join("libraries/net/neoforged/neoforge/21.1.77");
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::write(lib.join(args), "-cp x").unwrap();
        assert_eq!(find_forge_launch(&d, Kind::NeoForge, "1.21.1", "21.1.77"), Some(Launch::Args(format!("libraries/net/neoforged/neoforge/21.1.77/{args}"))));
        let old = d.join("old");
        std::fs::create_dir_all(&old).unwrap();
        for f in ["forge-1.16.5-36.2.39-installer.jar", "forge-1.16.5-36.2.39.jar", "minecraft_server.1.16.5.jar"] {
            std::fs::write(old.join(f), "").unwrap();
        }
        assert_eq!(find_forge_launch(&old, Kind::Forge, "1.16.5", "36.2.39"), Some(Launch::Jar("forge-1.16.5-36.2.39.jar".into())));
        assert_eq!(find_forge_launch(&d.join("nada"), Kind::Forge, "1.20.1", "47.3.0"), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn installer_urls_cover_old_forge_names() {
        let u = installer_urls(Kind::Forge, "1.7.10", "10.13.4.1614");
        assert_eq!(u.len(), 2);
        assert!(u[1].0.ends_with("forge-1.7.10-10.13.4.1614-1.7.10-installer.jar"));
        assert!(installer_urls(Kind::NeoForge, "1.21.1", "21.1.77")[0].0.contains("/neoforge/21.1.77/"));
    }
}
