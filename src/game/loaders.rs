//! Instalación de mod loaders: Fabric, Quilt (perfiles JSON) y Forge/NeoForge (instalador oficial headless).

use crate::config::Loader;
use crate::game::java;
use crate::game::mojang;
use crate::net::{self, Download};
use crate::paths::Paths;
use crate::progress::Reporter;
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::io::Read;

fn cache_file(p: &Paths, loader: Loader, mc: &str) -> std::path::PathBuf {
    p.cache().join("loader-versions").join(format!("{}-{}.json", loader.label().to_lowercase(), crate::paths::sanitize(mc)))
}

/// Como [`versions`], pero si el servidor del loader no responde (Fabric se cae de vez en cuando)
/// usa la última lista que se descargó. El `bool` dice si viene de esa copia guardada.
pub async fn versions_cached(client: &reqwest::Client, p: &Paths, loader: Loader, mc: &str) -> Result<(Vec<String>, bool)> {
    let file = cache_file(p, loader, mc);
    match versions(client, loader, mc).await {
        Ok(v) => {
            if !v.is_empty() {
                if let Some(d) = file.parent() {
                    let _ = std::fs::create_dir_all(d);
                }
                let _ = crate::paths::write_atomic(&file, serde_json::to_vec(&v).unwrap_or_default());
            }
            Ok((v, false))
        }
        Err(e) => match std::fs::read(&file).ok().and_then(|b| serde_json::from_slice::<Vec<String>>(&b).ok()).filter(|v| !v.is_empty()) {
            Some(v) => Ok((v, true)),
            None => Err(e),
        },
    }
}

/// Versiones de loader disponibles para una versión de MC (la primera es la recomendada).
pub async fn versions(client: &reqwest::Client, loader: Loader, mc: &str) -> Result<Vec<String>> {
    Ok(match loader {
        Loader::Vanilla => vec![],
        Loader::Fabric => {
            let v: Vec<Value> = net::get_json(client, &format!("https://meta.fabricmc.net/v2/versions/loader/{mc}")).await?;
            // estables primero
            let mut stable = vec![];
            let mut other = vec![];
            for x in v {
                let ver = x["loader"]["version"].as_str().unwrap_or_default().to_string();
                if x["loader"]["stable"].as_bool().unwrap_or(false) {
                    stable.push(ver)
                } else {
                    other.push(ver)
                }
            }
            stable.extend(other);
            stable
        }
        Loader::Quilt => {
            let v: Vec<Value> = net::get_json(client, &format!("https://meta.quiltmc.org/v3/versions/loader/{mc}")).await?;
            let all: Vec<String> = v.iter().filter_map(|x| x["loader"]["version"].as_str().map(String::from)).collect();
            let (stable, beta): (Vec<String>, Vec<String>) = all.into_iter().partition(|s| !s.contains('-'));
            stable.into_iter().chain(beta).collect()
        }
        Loader::Forge => {
            let promos: Value = net::get_json(client, "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json").await?;
            let mut out = vec![];
            for k in ["recommended", "latest"] {
                if let Some(v) = promos["promos"][format!("{mc}-{k}")].as_str() {
                    if !out.iter().any(|x: &String| x == v) {
                        out.push(v.to_string());
                    }
                }
            }
            out
        }
        Loader::NeoForge => {
            let v: Value = net::get_json(client, "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge").await?;
            let prefix = neoforge_prefix(mc);
            let mut list: Vec<String> = v["versions"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
            list.retain(|x| x.starts_with(&prefix));
            list.reverse(); // la API devuelve de más antigua a más nueva
                            // estables antes que betas
            let (stable, beta): (Vec<String>, Vec<String>) = list.into_iter().partition(|s| !s.contains("beta"));
            stable.into_iter().chain(beta).collect()
        }
    })
}

/// 1.21.1 → "21.1."; 1.21 → "21.0."; versiones con año (26.1) → "26.1."
fn neoforge_prefix(mc: &str) -> String {
    if let Some(rest) = mc.strip_prefix("1.") {
        let mut it = rest.split('.');
        let major = it.next().unwrap_or("0");
        let minor = it.next().unwrap_or("0");
        format!("{major}.{minor}.")
    } else {
        format!("{mc}.")
    }
}

/// Instala el loader y devuelve el ID de versión resultante (con inheritsFrom a vanilla).
pub async fn install(client: &reqwest::Client, p: &Paths, loader: Loader, mc: &str, loader_version: Option<&str>, rep: &Reporter) -> Result<String> {
    mojang::ensure_vanilla_json(client, p, mc).await?;
    if loader == Loader::Vanilla {
        return Ok(mc.to_string());
    }
    let ver = match loader_version {
        Some(v) if !v.is_empty() => v.to_string(),
        _ => versions_cached(client, p, loader, mc)
            .await?
            .0
            .into_iter()
            .next()
            .with_context(|| format!("{} no tiene versiones para Minecraft {mc}", loader.label()))?,
    };
    // ¿ya está el perfil? Entonces no hace falta el servidor del loader (por si está caído)
    let known = match loader {
        Loader::Fabric => Some(format!("fabric-loader-{ver}-{mc}")),
        Loader::Quilt => Some(format!("quilt-loader-{ver}-{mc}")),
        _ => None,
    };
    if let Some(id) = known.filter(|id| p.version_json(id).exists()) {
        return Ok(id);
    }
    rep.stage(format!("Instalando {} {ver}", loader.label()));
    match loader {
        Loader::Fabric => profile_json(client, p, &format!("https://meta.fabricmc.net/v2/versions/loader/{mc}/{ver}/profile/json")).await,
        Loader::Quilt => profile_json(client, p, &format!("https://meta.quiltmc.org/v3/versions/loader/{mc}/{ver}/profile/json")).await,
        Loader::Forge => {
            // Las versiones antiguas (1.7.10…) llevan la versión de MC repetida al final.
            let mut last = None;
            for full in [format!("{mc}-{ver}"), format!("{mc}-{ver}-{mc}")] {
                let url = format!("https://maven.minecraftforge.net/net/minecraftforge/forge/{full}/forge-{full}-installer.jar");
                let jar = p.cache().join(format!("forge-{full}-installer.jar"));
                match net::download_one(client, &Download { url: url.clone(), path: jar.clone(), sha1: None, size: None }).await {
                    Ok(()) => return run_installer(client, p, mc, &jar, &format!("forge-{full}"), rep).await,
                    Err(e) => last = Some(e),
                }
            }
            Err(last.unwrap()).context("no se encontró el instalador de Forge para esa versión")
        }
        Loader::NeoForge => {
            let url = format!("https://maven.neoforged.net/releases/net/neoforged/neoforge/{ver}/neoforge-{ver}-installer.jar");
            let jar = p.cache().join(format!("neoforge-{ver}-installer.jar"));
            net::download_one(client, &Download { url: url.clone(), path: jar.clone(), sha1: None, size: None })
                .await
                .with_context(|| format!("no se pudo descargar el instalador ({url})"))?;
            run_installer(client, p, mc, &jar, &format!("neoforge-{ver}"), rep).await
        }
        Loader::Vanilla => unreachable!(),
    }
}

async fn profile_json(client: &reqwest::Client, p: &Paths, url: &str) -> Result<String> {
    let txt = net::get_text(client, url).await?;
    let v: Value = serde_json::from_str(&txt)?;
    let id = v["id"].as_str().context("perfil sin id")?.to_string();
    let path = p.version_json(&id);
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(path, txt)?;
    Ok(id)
}

/// Ejecuta el instalador oficial de Forge/NeoForge en modo cliente sin interfaz.
/// Nuestro `shared/` tiene el mismo layout que `.minecraft`, así que el instalador escribe directamente ahí.
async fn run_installer(client: &reqwest::Client, p: &Paths, mc: &str, jar: &std::path::Path, name: &str, rep: &Reporter) -> Result<String> {
    let jar = jar.to_path_buf();
    // ID que va a generar: está en version.json dentro del instalador.
    // Los instaladores antiguos (Forge < 1.12.2) no lo tienen: se instalan a mano desde install_profile.json.
    let id = {
        let f = std::fs::File::open(&jar)?;
        let mut z = zip::ZipArchive::new(f).context("el instalador descargado está corrupto")?;
        let mut s = String::new();
        let has_version_json = z.by_name("version.json").map(|mut e| e.read_to_string(&mut s)).is_ok();
        if !has_version_json {
            rep.stage(format!("Instalando {name} (formato antiguo)"));
            return install_legacy(p, mc, &mut z);
        }
        let v: Value = serde_json::from_str(&s)?;
        v["id"].as_str().context("version.json sin id")?.to_string()
    };
    if p.version_json(&id).exists() {
        return Ok(id);
    }

    // El instalador exige launcher_profiles.json y el jar vanilla.
    let lp = p.shared().join("launcher_profiles.json");
    if !lp.exists() {
        std::fs::write(&lp, r#"{"profiles":{}}"#)?;
    }
    let vanilla = mojang::resolve(client, p, mc).await?;
    let major = mojang::required_java(&vanilla.json).max(8);
    let settings = crate::config::Settings::load(p);
    let component = mojang::java_component(&vanilla.json);
    let java = java::pick_for_installer(client, p, Some(settings.java_path.as_str()), major, component.as_deref(), settings.java_auto_download, rep).await?;
    if let Some(c) = vanilla.json.downloads.as_ref().and_then(|d| d.get("client")) {
        net::download_one(client, &Download { url: c.url.clone(), path: p.version_jar(mc), sha1: c.sha1.clone(), size: c.size }).await?;
    }

    rep.stage(format!("Ejecutando instalador de {name} (puede tardar un par de minutos)"));
    let shared = p.shared();
    let mut last_out = String::new();
    for flag in ["--installClient", "--install-client"] {
        let out = tokio::process::Command::new(&java)
            .arg("-jar")
            .arg(&jar)
            .arg(flag)
            .arg(&shared)
            .current_dir(p.cache())
            .output()
            .await
            .context("no se pudo ejecutar Java para el instalador")?;
        let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        for l in text.lines().rev().take(15).collect::<Vec<_>>().into_iter().rev() {
            rep.log(l.to_string());
        }
        if p.version_json(&id).exists() {
            return Ok(id);
        }
        last_out = text;
    }
    let mut tail_lines: Vec<&str> = last_out.lines().rev().take(10).collect();
    tail_lines.reverse();
    let tail = tail_lines.join("\n");
    bail!("el instalador terminó sin crear la versión {id}. Últimas líneas:\n{tail}")
}

/// Forge antiguo: `install_profile.json` trae `versionInfo` (el JSON de versión) y el jar universal dentro.
fn install_legacy(p: &Paths, mc: &str, z: &mut zip::ZipArchive<std::fs::File>) -> Result<String> {
    let profile: Value = {
        let mut s = String::new();
        z.by_name("install_profile.json").context("instalador de Forge sin version.json ni install_profile.json")?.read_to_string(&mut s)?;
        serde_json::from_str(&s)?
    };
    let mut vi = profile["versionInfo"].clone();
    if !vi.is_object() {
        bail!("install_profile.json sin versionInfo");
    }
    let id = vi["id"].as_str().context("versionInfo sin id")?.to_string();

    // Jar universal → libraries/<ruta maven>
    let file_path = profile["install"]["filePath"].as_str().context("install sin filePath")?;
    let maven = profile["install"]["path"].as_str().context("install sin path")?;
    let dst = p.libraries().join(crate::paths::maven_path(maven).context("coordenada maven inválida")?);
    std::fs::create_dir_all(dst.parent().unwrap())?;
    {
        let mut e = z.by_name(file_path).with_context(|| format!("falta {file_path} en el instalador"))?;
        let mut out = std::fs::File::create(&dst)?;
        std::io::copy(&mut e, &mut out)?;
    }

    // Normalizar el JSON: heredar de vanilla, quitar librerías sólo-servidor, URLs https.
    if vi.get("inheritsFrom").map(|x| x.is_null()).unwrap_or(true) {
        vi["inheritsFrom"] = Value::String(mc.to_string());
    }
    if vi.get("jar").map(|x| x.is_null()).unwrap_or(true) {
        vi["jar"] = Value::String(mc.to_string());
    }
    if let Some(libs) = vi["libraries"].as_array_mut() {
        libs.retain(|l| l["clientreq"].as_bool() != Some(false));
        for l in libs.iter_mut() {
            if let Some(u) = l["url"].as_str() {
                let fixed = u
                    .replace("http://files.minecraftforge.net/maven/", "https://maven.minecraftforge.net/")
                    .replace("https://files.minecraftforge.net/maven/", "https://maven.minecraftforge.net/")
                    .replace("http://", "https://");
                l["url"] = Value::String(fixed);
            }
            if let Some(o) = l.as_object_mut() {
                o.remove("checksums");
                o.remove("serverreq");
                o.remove("clientreq");
            }
        }
    }
    let path = p.version_json(&id);
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, serde_json::to_string_pretty(&vi)?)?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sin respuesta del servidor (aquí, sin red), se usa la última lista guardada.
    #[tokio::test]
    async fn loader_versions_survive_an_outage() {
        let root = std::env::temp_dir().join(format!("brasa-lv-{}", uuid::Uuid::new_v4()));
        let p = Paths { root: root.clone() };
        let f = cache_file(&p, Loader::Fabric, "1.21.1");
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, br#"["0.16.9","0.16.8"]"#).unwrap();
        let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(5)).build().unwrap();
        let (v, stale) = versions_cached(&client, &p, Loader::Fabric, "1.21.1").await.unwrap();
        assert!(!v.is_empty());
        if stale {
            assert_eq!(v, vec!["0.16.9".to_string(), "0.16.8".to_string()]);
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn neo_prefix() {
        assert_eq!(neoforge_prefix("1.21.1"), "21.1.");
        assert_eq!(neoforge_prefix("1.21"), "21.0.");
        assert_eq!(neoforge_prefix("1.20.4"), "20.4.");
        assert_eq!(neoforge_prefix("26.1"), "26.1.");
    }
}
