//! Red: descargas paralelas con SHA-1, reintentos, escritura atómica y **resistencia a redes problemáticas**:
//! - proxy configurable (además del proxy del sistema / variables HTTPS_PROXY),
//! - reintento forzando IPv4 si la conexión falla (IPv6 roto es la causa nº1 de "operation timed out"),
//! - espejos automáticos (BMCLAPI, Maven Central) cuando el servidor oficial no responde.

use crate::progress::Reporter;
use anyhow::{anyhow, bail, Context, Result};
use futures::StreamExt;
use sha1::{Digest, Sha1};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;

pub const USER_AGENT: &str = concat!("brasa-launcher/", env!("CARGO_PKG_VERSION"), " (github.com/brasa-launcher)");
const BMCL: &str = "https://bmclapi2.bangbang93.com";

/// De dónde se descargan los ficheros del juego.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Oficial y, si falla, espejo.
    Auto,
    /// Sólo servidores oficiales.
    Official,
    /// Espejo primero (útil si Mojang va lento o está bloqueado en tu red).
    Mirror,
}

impl Source {
    pub fn parse(s: &str) -> Source {
        match s.to_ascii_lowercase().as_str() {
            "official" | "oficial" => Source::Official,
            "mirror" | "bmclapi" | "espejo" => Source::Mirror,
            _ => Source::Auto,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Source::Auto => "auto",
            Source::Official => "official",
            Source::Mirror => "mirror",
        }
    }
}

#[derive(Clone, Debug)]
struct NetCfg {
    proxy: Option<String>,
    source: Source,
}

fn cfg_lock() -> &'static RwLock<NetCfg> {
    static C: OnceLock<RwLock<NetCfg>> = OnceLock::new();
    C.get_or_init(|| RwLock::new(NetCfg { proxy: None, source: Source::Auto }))
}

fn ipv4_cell() -> &'static RwLock<Option<reqwest::Client>> {
    static C: OnceLock<RwLock<Option<reqwest::Client>>> = OnceLock::new();
    C.get_or_init(|| RwLock::new(None))
}

/// Aplica la configuración de red (llamar al arrancar y al guardar ajustes).
pub fn configure(proxy: &str, source: &str) {
    let proxy = Some(proxy.trim().to_string()).filter(|s| !s.is_empty());
    *cfg_lock().write().unwrap() = NetCfg { proxy, source: Source::parse(source) };
    *ipv4_cell().write().unwrap() = None;
}

fn build(ipv4: bool) -> reqwest::Client {
    let cfg = cfg_lock().read().unwrap().clone();
    let mut b =
        reqwest::Client::builder().user_agent(USER_AGENT).connect_timeout(Duration::from_secs(8)).timeout(Duration::from_secs(600)).pool_max_idle_per_host(64);
    if let Some(p) = &cfg.proxy {
        let url = if p.contains("://") { p.clone() } else { format!("http://{p}") };
        match reqwest::Proxy::all(&url) {
            Ok(px) => b = b.proxy(px),
            Err(e) => eprintln!("[brasa] proxy inválido '{p}': {e}"),
        }
    }
    if ipv4 {
        b = b.local_address(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
    }
    b.build().expect("cliente HTTP")
}

pub fn client() -> reqwest::Client {
    build(false)
}

fn ipv4_client() -> reqwest::Client {
    if let Some(c) = ipv4_cell().read().unwrap().as_ref() {
        return c.clone();
    }
    let c = build(true);
    *ipv4_cell().write().unwrap() = Some(c.clone());
    c
}

/// URLs alternativas equivalentes para una URL (espejos).
pub fn mirrors(url: &str) -> Vec<String> {
    let url = if let Some(rest) = url.strip_prefix("http://") { format!("https://{rest}") } else { url.to_string() };
    let rules: &[(&str, &[&str])] = &[
        ("https://piston-meta.mojang.com", &[BMCL]),
        ("https://piston-data.mojang.com", &[BMCL]),
        ("https://launchermeta.mojang.com", &[BMCL]),
        ("https://launcher.mojang.com", &[BMCL]),
        ("https://resources.download.minecraft.net", &["https://bmclapi2.bangbang93.com/assets"]),
        ("https://libraries.minecraft.net", &["https://bmclapi2.bangbang93.com/maven", "https://repo1.maven.org/maven2"]),
        ("https://files.minecraftforge.net/maven", &["https://maven.minecraftforge.net", "https://bmclapi2.bangbang93.com/maven"]),
        ("https://maven.minecraftforge.net", &["https://bmclapi2.bangbang93.com/maven", "https://repo1.maven.org/maven2"]),
        // meta2 es el respaldo oficial de Fabric (otra infraestructura); después, el espejo BMCLAPI
        ("https://meta.fabricmc.net", &["https://meta2.fabricmc.net", "https://bmclapi2.bangbang93.com/fabric-meta"]),
        // muchas librerías de Fabric (ASM, Mixin Extras…) también están en Maven Central
        ("https://maven.fabricmc.net", &["https://repo1.maven.org/maven2", "https://bmclapi2.bangbang93.com/maven"]),
        ("https://maven.neoforged.net/releases", &["https://bmclapi2.bangbang93.com/maven"]),
        ("https://authlib-injector.yushi.moe", &["https://bmclapi2.bangbang93.com/mirrors/authlib-injector"]),
    ];
    for (prefix, alts) in rules {
        if let Some(rest) = url.strip_prefix(prefix) {
            return alts.iter().map(|a| format!("{a}{rest}")).collect();
        }
    }
    vec![]
}

/// Orden de URLs a probar según la fuente configurada.
pub fn candidates(url: &str) -> Vec<String> {
    let source = cfg_lock().read().unwrap().source;
    let alts = mirrors(url);
    match source {
        Source::Official => vec![url.to_string()],
        Source::Auto => std::iter::once(url.to_string()).chain(alts).collect(),
        Source::Mirror => alts.into_iter().chain(std::iter::once(url.to_string())).collect(),
    }
}

fn is_network(e: &reqwest::Error) -> bool {
    e.is_connect() || e.is_timeout() || e.is_request()
}

fn host_of(url: &str) -> String {
    reqwest::Url::parse(url).ok().and_then(|u| u.host_str().map(String::from)).unwrap_or_else(|| url.to_string())
}

/// Mensaje claro para errores de conexión.
pub fn explain(url: &str, e: &reqwest::Error) -> String {
    let host = host_of(url);
    if e.is_timeout() || e.is_connect() {
        format!(
            "no se puede conectar con {host} (tiempo de espera agotado). Causas típicas: red de empresa/instituto con proxy o \
             filtro, firewall/antivirus bloqueando a Brasa, o IPv6 roto. Prueba en Ajustes → Red: poner el proxy, \
             cambiar 'Fuente de descargas' a Espejo, o pulsa 'Probar conexión'"
        )
    } else {
        format!("error de red con {host}: {e}")
    }
}

/// Hosts que no responden en esta sesión (se saltan directamente al espejo) y preferencia IPv4 aprendida.
fn bad_hosts() -> &'static std::sync::Mutex<std::collections::HashSet<String>> {
    static B: OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    B.get_or_init(Default::default)
}
static PREFER_IPV4: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// GET con todas las defensas: candidato oficial/espejos × (cliente normal, cliente IPv4).
pub async fn get(client: &reqwest::Client, url: &str) -> Result<reqwest::Response> {
    let mut first_err: Option<anyhow::Error> = None;
    // qué se probó y qué pasó, para un mensaje que diga la verdad si falla todo
    let mut tried: Vec<(String, bool)> = vec![];
    let cands = candidates(url);
    let n = cands.len();
    for (i, u) in cands.iter().enumerate() {
        let host = host_of(u);
        if i + 1 < n && bad_hosts().lock().unwrap().contains(&host) {
            tried.push((host, true));
            continue;
        }
        let order: [bool; 2] = if PREFER_IPV4.load(Ordering::Relaxed) { [true, false] } else { [false, true] };
        let mut net_fails = 0;
        let mut is_net = false;
        for ipv4 in order {
            let c = if ipv4 { ipv4_client() } else { client.clone() };
            match c.get(u).send().await {
                Ok(r) if r.status().is_success() => {
                    if ipv4 && net_fails > 0 {
                        PREFER_IPV4.store(true, Ordering::Relaxed);
                    }
                    if i > 0 {
                        eprintln!("[brasa] {} no responde: usando {}", host_of(url), host);
                    }
                    return Ok(r);
                }
                Ok(r) => {
                    let st = r.status();
                    if first_err.is_none() {
                        first_err = Some(anyhow!("{} respondió HTTP {}", host_of(u), st));
                    }
                    break; // error HTTP: probar IPv4 no sirve, pasar al siguiente espejo
                }
                Err(e) => {
                    let net = is_network(&e);
                    is_net |= net;
                    if first_err.is_none() {
                        first_err = Some(anyhow!(explain(u, &e)));
                    }
                    if !net {
                        break;
                    }
                    net_fails += 1;
                }
            }
        }
        tried.push((host.clone(), is_net));
        if net_fails == 2 {
            bad_hosts().lock().unwrap().insert(host);
        }
    }
    let all_net = !tried.is_empty() && tried.iter().all(|(_, net)| *net);
    let err = match (tried.len(), all_net) {
        (n, true) if n > 1 => anyhow!(all_unreachable(&tried.iter().map(|(h, _)| h.as_str()).collect::<Vec<_>>())),
        (1, true) if !mirrors(url).is_empty() => anyhow!(
            "{} {}",
            first_err.map(|e| e.to_string()).unwrap_or_default(),
            t!("(los espejos están desactivados: «Fuente de descargas» en Ajustes → Red)")
        ),
        _ => first_err.unwrap_or_else(|| anyhow!("sin URL")),
    };
    Err(err).with_context(|| format!("GET {url}"))
}

/// Ni el servidor oficial ni sus espejos responden: casi seguro que es la red de este equipo.
pub fn all_unreachable(hosts: &[&str]) -> String {
    let (first, rest) = hosts.split_first().map(|(f, r)| (*f, r.join(", "))).unwrap_or(("", String::new()));
    t!(
        "no se puede conectar con {first} ni con sus espejos ({rest}). Como no responde ninguno, suele ser la red de este equipo: un proxy o filtro de empresa/instituto, el firewall o antivirus, o un bloqueo de tu proveedor de internet. Prueba con otra red (por ejemplo, los datos del móvil) o con una VPN, y pulsa «Probar conexión» en Ajustes → Red",
        first = first,
        rest = rest
    )
}

#[derive(Clone, Debug)]
pub struct Download {
    pub url: String,
    pub path: PathBuf,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

pub fn sha1_file(path: &Path) -> Result<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha1::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

pub fn sha256_file(path: &Path) -> Result<String> {
    use sha2::Sha256;
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        Digest::update(&mut h, &buf[..n]);
    }
    Ok(hex::encode(Digest::finalize(h)))
}

pub fn sha512_file(path: &Path) -> Result<String> {
    use sha2::Sha512;
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha512::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        Digest::update(&mut h, &buf[..n]);
    }
    Ok(hex::encode(Digest::finalize(h)))
}

/// ¿El fichero ya está bien? `verify_hash=false` sólo compara tamaño (rápido),
/// `true` calcula SHA-1 (modo reparar).
pub fn is_valid(d: &Download, verify_hash: bool) -> bool {
    let Ok(meta) = std::fs::metadata(&d.path) else { return false };
    if let Some(size) = d.size {
        if meta.len() != size {
            return false;
        }
    }
    if verify_hash || d.size.is_none() {
        if let Some(expected) = &d.sha1 {
            return sha1_file(&d.path).map(|h| h.eq_ignore_ascii_case(expected)).unwrap_or(false);
        }
    }
    true
}

pub async fn download_one(client: &reqwest::Client, d: &Download) -> Result<()> {
    if let Some(parent) = d.path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut tmp_os = d.path.clone().into_os_string();
    tmp_os.push(".brasa-part");
    let tmp = PathBuf::from(tmp_os);
    let mut last_err = None;
    for attempt in 0..3u32 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(500 * 2u64.pow(attempt))).await;
        }
        match try_download(client, d, &tmp).await {
            Ok(()) => {
                tokio::fs::rename(&tmp, &d.path).await?;
                return Ok(());
            }
            Err(e) => last_err = Some(e),
        }
    }
    let _ = tokio::fs::remove_file(&tmp).await;
    Err(last_err.unwrap()).with_context(|| format!("descargando {}", d.url))
}

async fn try_download(client: &reqwest::Client, d: &Download, tmp: &Path) -> Result<()> {
    let resp = get(client, &d.url).await?;
    let mut stream = resp.bytes_stream();
    let mut file = tokio::fs::File::create(tmp).await?;
    let mut h = Sha1::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        h.update(&chunk);
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    drop(file);
    if let Some(expected) = &d.sha1 {
        let got = hex::encode(h.finalize());
        if !got.eq_ignore_ascii_case(expected) {
            bail!("SHA-1 incorrecto (esperado {expected}, obtenido {got})");
        }
    }
    Ok(())
}

/// Descarga en paralelo todo lo que falte o esté corrupto.
pub async fn download_all(client: &reqwest::Client, items: Vec<Download>, concurrency: usize, verify_hash: bool, rep: &Reporter) -> Result<()> {
    let pending: Vec<Download> = tokio::task::spawn_blocking(move || {
        let mut seen = std::collections::HashSet::new();
        items.into_iter().filter(|d| seen.insert(d.path.clone())).filter(|d| !is_valid(d, verify_hash)).collect::<Vec<_>>()
    })
    .await?;

    let total = pending.len();
    if total == 0 {
        return Ok(());
    }
    let done = Arc::new(AtomicUsize::new(0));
    rep.progress(0, total);
    let results: Vec<(Download, Result<()>)> = futures::stream::iter(pending.into_iter().map(|d| {
        let client = client.clone();
        let done = done.clone();
        let rep = rep.clone();
        async move {
            let r = download_one(&client, &d).await;
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            rep.progress(n, total);
            (d, r)
        }
    }))
    .buffer_unordered(concurrency.max(1))
    .collect()
    .await;

    // Segunda vuelta para lo que falló: con calma (de dos en dos), tras una pausa y volviendo a
    // dar una oportunidad a los servidores marcados como caídos (los cortes suelen ser de segundos,
    // y con muchas descargas a la vez algunas redes las cortan).
    let failed: Vec<Download> = results.into_iter().filter(|(_, r)| r.is_err()).map(|(d, _)| d).collect();
    if failed.is_empty() {
        return Ok(());
    }
    rep.log(format!("[brasa] {} descargas fallaron: se reintentan más despacio…", failed.len()));
    tokio::time::sleep(Duration::from_secs(3)).await;
    bad_hosts().lock().unwrap().clear();
    let retry: Vec<Result<()>> = futures::stream::iter(failed.into_iter().map(|d| {
        let client = client.clone();
        async move { download_one(&client, &d).await }
    }))
    .buffer_unordered(2)
    .collect()
    .await;

    let errors: Vec<String> = retry.into_iter().filter_map(|r| r.err()).map(|e| format!("{e:#}")).collect();
    if !errors.is_empty() {
        bail!("{} descargas fallaron. Primera: {}", errors.len(), errors[0]);
    }
    Ok(())
}

pub async fn get_json<T: serde::de::DeserializeOwned>(client: &reqwest::Client, url: &str) -> Result<T> {
    let r = get(client, url).await?;
    r.json::<T>().await.with_context(|| format!("JSON inválido en {url}"))
}

pub async fn get_text(client: &reqwest::Client, url: &str) -> Result<String> {
    let r = get(client, url).await?;
    Ok(r.text().await?)
}

/// Resultado de probar un servicio.
#[derive(Debug, Clone, PartialEq)]
pub enum Check {
    /// Responde (milisegundos).
    Ok(u128),
    /// No responde, pero hay cómo seguir (un espejo que sí, o sólo IPv4): Brasa lo resuelve solo.
    Warn(String),
    /// No responde y no hay alternativa.
    Fail(String),
}

/// Diagnóstico de red: prueba cada servicio y, si falla, sus espejos.
pub async fn diagnose(client: &reqwest::Client) -> Vec<(String, Check)> {
    let targets = [
        ("Mojang (versiones)", "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json"),
        ("Mojang (assets)", "https://resources.download.minecraft.net/"),
        ("Mojang (librerías)", "https://libraries.minecraft.net/"),
        ("Microsoft login", "https://login.microsoftonline.com/"),
        ("Modrinth", "https://api.modrinth.com/v2"),
        ("Fabric", "https://meta.fabricmc.net/v2/versions/game"),
        ("Forge", "https://maven.minecraftforge.net/"),
        ("Java (Adoptium)", "https://api.adoptium.net/v3/info/available_releases"),
        ("Ely.by", "https://authserver.ely.by/api/authlib-injector"),
        ("Espejo BMCLAPI", "https://bmclapi2.bangbang93.com/mc/game/version_manifest_v2.json"),
    ];
    let futs = targets.iter().map(|(name, url)| {
        let client = client.clone();
        async move {
            let t = Instant::now();
            let r = match client.get(*url).timeout(Duration::from_secs(10)).send().await {
                Ok(_) => Check::Ok(t.elapsed().as_millis()),
                Err(e) => {
                    // ¿funciona forzando IPv4?
                    let t2 = Instant::now();
                    match ipv4_client().get(*url).timeout(Duration::from_secs(10)).send().await {
                        Ok(_) => Check::Warn(t!("sólo funciona por IPv4 ({} ms): IPv6 roto, Brasa lo esquiva solo", t2.elapsed().as_millis())),
                        Err(_) => {
                            let why = if e.is_timeout() || e.is_connect() { t!("no responde").to_string() } else { e.to_string() };
                            // ¿y algún espejo?
                            let mut alt = None;
                            for m in mirrors(url) {
                                let t3 = Instant::now();
                                if client.get(&m).timeout(Duration::from_secs(8)).send().await.is_ok_and(|r| r.status().is_success()) {
                                    alt = Some((host_of(&m), t3.elapsed().as_millis()));
                                    break;
                                }
                            }
                            match alt {
                                Some((host, ms)) => {
                                    Check::Warn(t!("{why}, pero el espejo {host} funciona ({ms} ms): Brasa lo usa solo", why = why, host = host, ms = ms))
                                }
                                None => Check::Fail(why),
                            }
                        }
                    }
                }
            };
            (name.to_string(), r)
        }
    });
    futures::future::join_all(futs).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn says_when_the_mirrors_failed_too() {
        let m = all_unreachable(&["meta.fabricmc.net", "meta2.fabricmc.net", "bmclapi2.bangbang93.com"]);
        assert!(m.starts_with("no se puede conectar con meta.fabricmc.net ni con sus espejos (meta2.fabricmc.net, bmclapi2.bangbang93.com)"), "{m}");
    }

    #[test]
    fn fabric_libraries_also_come_from_maven_central() {
        let m = mirrors("https://maven.fabricmc.net/org/ow2/asm/asm-tree/9.8/asm-tree-9.8.jar");
        assert_eq!(m[0], "https://repo1.maven.org/maven2/org/ow2/asm/asm-tree/9.8/asm-tree-9.8.jar");
    }

    #[test]
    fn fabric_meta_falls_back_to_its_official_backup_first() {
        let m = mirrors("https://meta.fabricmc.net/v2/versions/loader/1.21.1");
        assert_eq!(m[0], "https://meta2.fabricmc.net/v2/versions/loader/1.21.1");
        assert_eq!(m[1], "https://bmclapi2.bangbang93.com/fabric-meta/v2/versions/loader/1.21.1");
    }

    #[test]
    fn mirror_rules() {
        assert_eq!(
            mirrors("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json")[0],
            "https://bmclapi2.bangbang93.com/mc/game/version_manifest_v2.json"
        );
        assert_eq!(mirrors("https://resources.download.minecraft.net/ab/abcd")[0], "https://bmclapi2.bangbang93.com/assets/ab/abcd");
        assert_eq!(mirrors("http://files.minecraftforge.net/maven/a/b.jar")[0], "https://maven.minecraftforge.net/a/b.jar");
        assert!(mirrors("https://api.modrinth.com/v2").is_empty());
    }
}
