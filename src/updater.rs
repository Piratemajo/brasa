//! Auto-actualización del launcher desde las releases de GitHub (las que genera el CI con un tag `v*`).

use crate::net;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize, Clone, Debug)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub assets: Vec<Asset>,
}
#[derive(Deserialize, Clone, Debug)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub size: u64,
}

/// Nombre del binario publicado por el CI para esta plataforma.
pub fn asset_name() -> &'static str {
    if cfg!(windows) {
        "brasa-windows-x64.exe"
    } else if cfg!(target_os = "macos") {
        "brasa-macos-arm64"
    } else {
        "brasa-linux-x64"
    }
}

/// Versión estilo semver: `1.0.0`, `v1.0.0-beta.2`, `1.0.0-rc.1`.
#[derive(Debug, PartialEq, Eq)]
pub struct Version {
    core: Vec<u64>,
    /// Lo que va tras el guion (`beta.2`); vacío en una versión final.
    pre: Vec<String>,
}

impl Version {
    pub fn parse(v: &str) -> Version {
        let v = v.trim().trim_start_matches(['v', 'V']);
        let v = v.split('+').next().unwrap_or(v); // metadatos de compilación: no cuentan
        let (core, pre) = v.split_once('-').unwrap_or((v, ""));
        let mut core: Vec<u64> = core.split('.').map(|x| x.parse().unwrap_or(0)).collect();
        while core.len() < 3 {
            core.push(0);
        }
        Version { core, pre: pre.split('.').filter(|s| !s.is_empty()).map(str::to_string).collect() }
    }

    pub fn is_prerelease(&self) -> bool {
        !self.pre.is_empty()
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    /// Como semver: `1.0.0-alpha < 1.0.0-beta.1 < 1.0.0-beta.2 < 1.0.0-rc.1 < 1.0.0 < 1.0.1`.
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering::*;
        match self.core.cmp(&o.core) {
            Equal => {}
            x => return x,
        }
        match (self.pre.is_empty(), o.pre.is_empty()) {
            (true, true) => return Equal,
            (true, false) => return Greater, // la final va después de sus betas
            (false, true) => return Less,
            _ => {}
        }
        for (a, b) in self.pre.iter().zip(&o.pre) {
            let ord = match (a.parse::<u64>(), b.parse::<u64>()) {
                (Ok(x), Ok(y)) => x.cmp(&y),
                (Ok(_), Err(_)) => Less,
                (Err(_), Ok(_)) => Greater,
                _ => a.cmp(b),
            };
            if ord != Equal {
                return ord;
            }
        }
        self.pre.len().cmp(&o.pre.len())
    }
}

#[cfg(test)]
pub fn is_newer(remote: &str, local: &str) -> bool {
    Version::parse(remote) > Version::parse(local)
}

/// De una lista de releases, la que toca: la más nueva que sea posterior a `local`. Quien usa una
/// versión final sólo recibe finales; quien está en una beta recibe también betas nuevas.
pub fn pick(releases: Vec<Release>, local: &str) -> Option<Release> {
    let me = Version::parse(local);
    releases
        .into_iter()
        .filter(|r| !r.draft)
        .filter(|r| me.is_prerelease() || !(r.prerelease || Version::parse(&r.tag_name).is_prerelease()))
        .filter(|r| Version::parse(&r.tag_name) > me)
        .max_by(|a, b| Version::parse(&a.tag_name).cmp(&Version::parse(&b.tag_name)))
}

/// Devuelve la release si es más nueva que la versión actual.
pub async fn check(client: &reqwest::Client, repo: &str) -> Result<Option<Release>> {
    let repo = repo.trim();
    if !crate::distro::valid_repo(repo) {
        bail!("esta copia de Brasa no se actualiza sola (se compiló sin repositorio de actualizaciones)");
    }
    // la lista (y no /latest): /latest no incluye las betas
    let resp = client.get(format!("https://api.github.com/repos/{repo}/releases?per_page=30")).header("Accept", "application/vnd.github+json").send().await?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        bail!("{}", not_found(repo));
    }
    let list: Vec<Release> = resp.error_for_status().context("no se pudo consultar GitHub")?.json().await?;
    if list.is_empty() {
        bail!("{}", not_found(repo));
    }
    Ok(pick(list, env!("CARGO_PKG_VERSION")))
}

/// GitHub responde 404 tanto si no hay releases como si el repo es privado (sin sesión no lo ve).
pub fn not_found(repo: &str) -> String {
    format!(
        "{repo} no tiene releases públicas. Si el repo es privado, los jugadores no pueden verlas: publica las releases en un repo público aparte (variable BRASA_RELEASES_REPO del CI, ver «Distribuir Brasa» en el README)"
    )
}

/// Extrae el SHA-256 de un fichero `.sha256` (formato de `sha256sum`: `<hash>  <nombre>`, o sólo el hash).
pub fn parse_checksum(text: &str) -> Option<String> {
    let h = text.split_whitespace().next()?.to_ascii_lowercase();
    (h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit())).then_some(h)
}

/// Descarga la nueva versión y sustituye el ejecutable actual. Hace falta reiniciar Brasa.
pub async fn apply(client: &reqwest::Client, rel: &Release) -> Result<PathBuf> {
    let asset = rel.assets.iter().find(|a| a.name == asset_name()).with_context(|| format!("la release {} no incluye {}", rel.tag_name, asset_name()))?;
    // Sin suma SHA-256 publicada no se actualiza sola: un binario sin verificar no se ejecuta.
    let sum_name = format!("{}.sha256", asset_name());
    let sum_asset = rel.assets.iter().find(|a| a.name == sum_name).with_context(|| {
        format!("la release {} no publica {sum_name}; por seguridad no se actualiza automáticamente (descarga el binario a mano)", rel.tag_name)
    })?;
    let expected = parse_checksum(&client.get(&sum_asset.browser_download_url).send().await?.error_for_status()?.text().await?)
        .with_context(|| format!("{sum_name} no tiene un SHA-256 válido"))?;
    let exe = std::env::current_exe()?;
    let dir = exe.parent().context("ruta del ejecutable")?.to_path_buf();
    let new = dir.join(format!(".{}.new", asset_name()));
    net::download_one(
        client,
        &net::Download { url: asset.browser_download_url.clone(), path: new.clone(), sha1: None, size: Some(asset.size).filter(|s| *s > 0) },
    )
    .await?;
    let new_for_hash = new.clone();
    let actual = tokio::task::spawn_blocking(move || net::sha256_file(&new_for_hash)).await??;
    if actual != expected {
        let _ = std::fs::remove_file(&new);
        bail!("el SHA-256 de la descarga no coincide con el publicado (esperado {expected}, obtenido {actual}); no se instala");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755))?;
    }
    // Windows no deja sobrescribir un .exe en uso, pero sí renombrarlo.
    let old = exe.with_extension("old");
    let _ = std::fs::remove_file(&old);
    std::fs::rename(&exe, &old).context("no se pudo apartar el ejecutable actual (¿permisos?)")?;
    if let Err(e) = std::fs::rename(&new, &exe) {
        let _ = std::fs::rename(&old, &exe);
        return Err(e).context("no se pudo colocar la nueva versión");
    }
    Ok(exe)
}

/// Borra restos de una actualización anterior.
pub fn cleanup() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(exe.with_extension("old"));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn versions() {
        assert!(super::is_newer("v0.3.1", "0.3.0"));
        assert!(super::is_newer("v1.0.0", "0.9.9"));
        assert!(!super::is_newer("v0.3.0", "0.3.0"));
        assert!(!super::is_newer("v0.2.9", "0.3.0"));
    }

    #[test]
    fn betas_come_before_the_final_release() {
        use super::{is_newer, Version};
        // el fallo de antes: la final no se ofrecía a quien tenía la beta
        assert!(is_newer("v1.0.0", "1.0.0-beta.1"));
        assert!(is_newer("v1.0.0-beta.2", "1.0.0-beta.1"));
        assert!(is_newer("v1.0.0-beta.10", "1.0.0-beta.9"), "numérico, no alfabético");
        assert!(is_newer("v1.0.0-rc.1", "1.0.0-beta.3"));
        assert!(is_newer("v1.0.0-beta.1", "0.4.0"));
        assert!(!is_newer("v1.0.0-beta.1", "1.0.0"));
        assert!(!is_newer("v1.0.0", "1.0.0"));
        assert!(is_newer("v1.0.1", "1.0.0"));
        assert!(is_newer("v1.0", "0.9.9"), "versiones cortas");
        assert!(Version::parse("1.0.0-beta.1+abc") == Version::parse("v1.0.0-beta.1"));
    }

    #[test]
    fn stable_users_only_get_stable_releases() {
        use super::{pick, Release};
        let r = |tag: &str, pre: bool| Release { tag_name: tag.into(), prerelease: pre, draft: false, assets: vec![] };
        let list = || vec![r("v1.0.0-beta.2", true), r("v1.0.0-beta.1", true), r("v0.4.0", false)];
        assert_eq!(pick(list(), "0.4.0").map(|x| x.tag_name), None, "en la final no se ofrecen betas");
        assert_eq!(pick(list(), "1.0.0-beta.1").unwrap().tag_name, "v1.0.0-beta.2");
        let mut with_final = list();
        with_final.push(r("v1.0.0", false));
        assert_eq!(pick(with_final, "1.0.0-beta.2").unwrap().tag_name, "v1.0.0", "desde la beta, a la final");
        let mut drafts = list();
        drafts.push(Release { tag_name: "v9.0.0".into(), prerelease: false, draft: true, assets: vec![] });
        assert_eq!(pick(drafts, "1.0.0-beta.1").unwrap().tag_name, "v1.0.0-beta.2", "los borradores no cuentan");
    }

    #[test]
    fn checksum_parsing() {
        let h = "a".repeat(64);
        assert_eq!(super::parse_checksum(&format!("{h}  brasa-linux-x64\n")), Some(h.clone()));
        assert_eq!(super::parse_checksum(&h.to_uppercase()), Some(h));
        assert_eq!(super::parse_checksum("zzz"), None);
        assert_eq!(super::parse_checksum(&"a".repeat(63)), None);
        assert_eq!(super::parse_checksum(""), None);
    }
}
