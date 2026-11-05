//! Java: **primero usa el que ya tienes instalado** y sólo descarga si no hay ninguno compatible.
//!
//! Orden de búsqueda:
//! 1. Java fijado en la instancia (`java_path`).
//! 2. Java fijado globalmente en Ajustes.
//! 3. Java del sistema detectado automáticamente (JAVA_HOME, PATH, carpetas típicas de Adoptium/Oracle/
//!    Zulu/Microsoft/Corretto/Liberica, /usr/lib/jvm, SDKMAN, los runtimes del launcher oficial y de Prism...).
//! 4. Java ya descargado por Brasa.
//! 5. Descarga (si está permitido): runtime oficial de Mojang (con espejo BMCLAPI) y, si falla, Temurin.

use crate::net::{self, Download};
use crate::paths::Paths;
use crate::progress::Reporter;
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JavaInstall {
    /// Ejecutable a usar para el juego (javaw.exe en Windows).
    pub exe: PathBuf,
    pub major: u32,
    pub version: String,
    pub is_64bit: bool,
    /// De dónde sale (sistema, Brasa, launcher oficial...).
    pub source: String,
}

impl JavaInstall {
    pub fn describe(&self) -> String {
        format!("Java {} ({}{}) — {}", self.version, self.source, if self.is_64bit { "" } else { ", 32 bits" }, self.exe.display())
    }
}

fn java_exe() -> &'static str {
    if cfg!(windows) {
        "javaw.exe"
    } else {
        "java"
    }
}

// ------------------------------------------------------------------ versiones

/// "1.8.0_392" → 8, "17.0.9" → 17, "21" → 21, "25-ea" → 25
pub fn parse_major(v: &str) -> Option<u32> {
    let v = v.trim().trim_matches('"');
    let mut it = v.split(|c: char| !c.is_ascii_digit()).filter(|s| !s.is_empty());
    let first: u32 = it.next()?.parse().ok()?;
    if first == 1 {
        it.next()?.parse().ok()
    } else {
        Some(first)
    }
}

/// Lee el fichero `release` de un JDK/JRE (JAVA_VERSION="21.0.2", OS_ARCH="amd64").
fn read_release(home: &Path) -> Option<(String, Option<bool>)> {
    let txt = std::fs::read_to_string(home.join("release")).ok()?;
    let mut version = None;
    let mut is64 = None;
    for line in txt.lines() {
        if let Some(v) = line.strip_prefix("JAVA_VERSION=") {
            version = Some(v.trim().trim_matches('"').to_string());
        } else if let Some(a) = line.strip_prefix("OS_ARCH=") {
            let a = a.trim().trim_matches('"');
            is64 = Some(matches!(a, "amd64" | "x86_64" | "aarch64" | "arm64" | "ppc64le" | "s390x"));
        }
    }
    Some((version?, is64))
}

/// Ejecuta `java -version` (sin abrir ventana en Windows) y devuelve (versión, 64 bits).
fn probe_exe(exe: &Path) -> Option<(String, bool)> {
    let console = console_variant(exe);
    let mut cmd = std::process::Command::new(&console);
    cmd.arg("-version").stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd.output().ok()?;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stderr), String::from_utf8_lossy(&out.stdout));
    let first = text.lines().find(|l| l.contains("version"))?;
    let start = first.find('"')? + 1;
    let end = first[start..].find('"')? + start;
    let version = first[start..end].to_string();
    let is64 = text.contains("64-Bit") || text.contains("64-bit");
    Some((version, is64))
}

/// Analiza un ejecutable de Java: primero el fichero `release` (instantáneo), si no `java -version`.
pub fn inspect(exe: &Path, source: &str) -> Option<JavaInstall> {
    if !exe.is_file() {
        return None;
    }
    let home = exe.parent()?.parent()?;
    let (version, is64) = match read_release(home) {
        Some((v, Some(b))) => (v, b),
        Some((v, None)) => (v, cfg!(target_pointer_width = "64")),
        None => probe_exe(exe)?,
    };
    Some(JavaInstall { major: parse_major(&version)?, exe: exe.to_path_buf(), version, is_64bit: is64, source: source.to_string() })
}

// ------------------------------------------------------------------ detección

/// Busca bin/java(w) dentro de `dir` hasta `depth` niveles (cubre Contents/Home de macOS y carpetas anidadas).
fn find_exes(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
    if depth == 0 || !dir.is_dir() {
        return;
    }
    let candidate = dir.join("bin").join(java_exe());
    if candidate.is_file() {
        out.push(candidate);
        return;
    }
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                find_exes(&e.path(), depth - 1, out);
            }
        }
    }
}

fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(PathBuf::from).filter(|p| !p.as_os_str().is_empty())
}

/// Carpetas donde suele haber Javas instalados en este sistema.
fn search_roots() -> Vec<(PathBuf, &'static str)> {
    let mut v: Vec<(PathBuf, &'static str)> = vec![];
    let home = directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf());
    if cfg!(windows) {
        for pf in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
            if let Some(base) = env_path(pf) {
                for vendor in [
                    "Java",
                    "Eclipse Adoptium",
                    "Eclipse Foundation",
                    "AdoptOpenJDK",
                    "Zulu",
                    "Microsoft",
                    "Amazon Corretto",
                    "BellSoft",
                    "Semeru",
                    "OpenJDK",
                    "Oracle",
                ] {
                    v.push((base.join(vendor), "sistema"));
                }
            }
        }
        if let Some(local) = env_path("LOCALAPPDATA") {
            v.push((local.join("Programs"), "sistema"));
            // Launcher oficial (Microsoft Store / nuevo instalador)
            if let Ok(rd) = std::fs::read_dir(local.join("Packages")) {
                for e in rd.flatten() {
                    if e.file_name().to_string_lossy().starts_with("Microsoft.4297127D64EC6") {
                        v.push((e.path().join("LocalCache/Local/runtime"), "launcher oficial"));
                    }
                }
            }
        }
        if let Some(app) = env_path("APPDATA") {
            v.push((app.join(".minecraft/runtime"), "launcher oficial"));
            v.push((app.join("PrismLauncher/java"), "Prism"));
        }
        if let Some(pf86) = env_path("ProgramFiles(x86)") {
            v.push((pf86.join("Minecraft Launcher/runtime"), "launcher oficial"));
        }
    } else if cfg!(target_os = "macos") {
        v.push((PathBuf::from("/Library/Java/JavaVirtualMachines"), "sistema"));
        v.push((PathBuf::from("/opt/homebrew/opt"), "Homebrew"));
        v.push((PathBuf::from("/usr/local/opt"), "Homebrew"));
        if let Some(h) = &home {
            v.push((h.join("Library/Java/JavaVirtualMachines"), "sistema"));
            v.push((h.join("Library/Application Support/minecraft/runtime"), "launcher oficial"));
            v.push((h.join("Library/Application Support/PrismLauncher/java"), "Prism"));
        }
    } else {
        for d in ["/usr/lib/jvm", "/usr/lib64/jvm", "/usr/java", "/opt", "/opt/java", "/usr/local/java"] {
            v.push((PathBuf::from(d), "sistema"));
        }
        if let Some(h) = &home {
            v.push((h.join(".minecraft/runtime"), "launcher oficial"));
            v.push((h.join(".local/share/PrismLauncher/java"), "Prism"));
        }
    }
    if let Some(h) = &home {
        v.push((h.join(".sdkman/candidates/java"), "SDKMAN"));
        v.push((h.join(".jdks"), "IntelliJ"));
        v.push((h.join(".gradle/jdks"), "Gradle"));
    }
    v
}

/// Todos los Javas que hay en este PC (sin los de Brasa). Puede tardar un poco la primera vez.
pub fn detect_system() -> Vec<JavaInstall> {
    let mut exes: Vec<(PathBuf, String)> = vec![];
    if let Some(jh) = env_path("JAVA_HOME") {
        exes.push((jh.join("bin").join(java_exe()), "JAVA_HOME".into()));
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let e = dir.join(java_exe());
            if e.is_file() {
                exes.push((e, "PATH".into()));
            }
        }
    }
    for (root, source) in search_roots() {
        let mut found = vec![];
        find_exes(&root, 4, &mut found);
        exes.extend(found.into_iter().map(|e| (e, source.to_string())));
    }
    let mut seen = HashSet::new();
    let mut out = vec![];
    for (exe, source) in exes {
        // /usr/bin/java suele ser un enlace: resolver para no repetir
        let key = std::fs::canonicalize(&exe).unwrap_or_else(|_| exe.clone());
        if !seen.insert(key.clone()) {
            continue;
        }
        let real = if key.is_file() { key } else { exe };
        if let Some(j) = inspect(&real, &source) {
            out.push(j);
        }
    }
    out.sort_by(|a, b| b.major.cmp(&a.major).then(b.is_64bit.cmp(&a.is_64bit)));
    out
}

/// Javas descargados por Brasa.
pub fn detect_managed(p: &Paths) -> Vec<JavaInstall> {
    let mut found = vec![];
    find_exes(&p.runtimes(), 5, &mut found);
    found.iter().filter_map(|e| inspect(e, "Brasa")).collect()
}

/// ¿Sirve este Java para una versión que pide `required`?
/// - Minecraft hasta 1.16 (Java 8) es delicado: con Java 9+ fallan LWJGL 2 y Forge antiguo → exige 8.
/// - Desde 1.17 cualquier Java igual o más nuevo funciona.
pub fn compatible(j: &JavaInstall, required: u32) -> bool {
    if required <= 8 {
        j.major == 8
    } else {
        j.major >= required
    }
}

/// El mejor Java instalado para `required`: la versión exacta primero, luego la más cercana por encima; 64 bits antes.
pub fn best(list: &[JavaInstall], required: u32) -> Option<JavaInstall> {
    list.iter().filter(|j| compatible(j, required)).min_by_key(|j| (j.major != required, j.major, !j.is_64bit)).cloned()
}

// ------------------------------------------------------------------ selección

#[allow(clippy::too_many_arguments)]
/// Elige el Java para jugar. `component` es el runtime de Mojang que pide la versión (java-runtime-delta…).
pub async fn pick(
    client: &reqwest::Client,
    p: &Paths,
    instance_override: Option<&str>,
    global_override: Option<&str>,
    required: u32,
    component: Option<&str>,
    allow_download: bool,
    rep: &Reporter,
) -> Result<PathBuf> {
    for (o, from) in [(instance_override, "la instancia"), (global_override, "Ajustes")] {
        if let Some(o) = o.map(str::trim).filter(|s| !s.is_empty()) {
            let mut pb = PathBuf::from(o);
            // Se admite tanto la carpeta del JDK como el ejecutable
            if pb.is_dir() {
                pb = pb.join("bin").join(java_exe());
            }
            if !pb.exists() {
                bail!("el Java configurado en {from} no existe: {o}");
            }
            if let Some(j) = inspect(&pb, from) {
                if !compatible(&j, required) {
                    rep.log(format!("[brasa] Aviso: el Java de {from} es {} y esta versión pide Java {required}", j.major));
                }
            }
            return Ok(pb);
        }
    }

    rep.stage(format!("Buscando Java {required} en el sistema"));
    let system = tokio::task::spawn_blocking(detect_system).await.unwrap_or_default();
    if let Some(j) = best(&system, required) {
        rep.log(format!("[brasa] Usando {}", j.describe()));
        return Ok(j.exe);
    }
    let managed = detect_managed(p);
    if let Some(j) = best(&managed, required) {
        rep.log(format!("[brasa] Usando {}", j.describe()));
        return Ok(j.exe);
    }

    let found: Vec<String> = system.iter().chain(managed.iter()).map(|j| format!("  · {}", j.describe())).collect();
    let found_txt = if found.is_empty() { "  (ninguno)".to_string() } else { found.join("\n") };
    if !allow_download {
        bail!(
            "esta versión necesita Java {required}{} y no hay ninguno compatible instalado.\nJavas encontrados:\n{found_txt}\n\
             Instala Java {required} (p. ej. desde adoptium.net) o activa 'Descargar Java automáticamente' en Ajustes.",
            if required <= 8 { " exactamente" } else { " o superior" }
        );
    }

    rep.log(format!("[brasa] No hay Java {required} compatible instalado. Javas encontrados:\n{found_txt}"));
    match download(client, p, required, component, rep).await {
        Ok(exe) => Ok(exe),
        Err(e) => Err(e).context(format!(
            "no hay Java {required} instalado y no se pudo descargar. Instala Java {required} a mano (adoptium.net) \
             y Brasa lo detectará solo, o pon su ruta en la instancia → Java"
        )),
    }
}

/// Java para ejecutar el instalador de Forge/NeoForge (versión de consola).
pub async fn pick_for_installer(
    client: &reqwest::Client,
    p: &Paths,
    global_override: Option<&str>,
    required: u32,
    component: Option<&str>,
    allow_download: bool,
    rep: &Reporter,
) -> Result<PathBuf> {
    let exe = pick(client, p, None, global_override, required, component, allow_download, rep).await?;
    Ok(console_variant(&exe))
}

// ------------------------------------------------------------------ descarga

async fn download(client: &reqwest::Client, p: &Paths, major: u32, component: Option<&str>, rep: &Reporter) -> Result<PathBuf> {
    // 1) Runtime oficial de Mojang: el mismo que usa el launcher oficial y tiene espejo (BMCLAPI).
    let component = component.map(String::from).or_else(|| default_component(major).map(String::from));
    if let Some(comp) = &component {
        match mojang_runtime(client, p, comp, rep).await {
            Ok(exe) => return Ok(exe),
            Err(e) => rep.log(format!("[brasa] Runtime de Mojang no disponible ({e:#}); probando Temurin")),
        }
    }
    // 2) Temurin (Adoptium)
    temurin(client, p, major, rep).await
}

fn default_component(major: u32) -> Option<&'static str> {
    match major {
        8 => Some("jre-legacy"),
        16 => Some("java-runtime-alpha"),
        17 => Some("java-runtime-gamma"),
        21 => Some("java-runtime-delta"),
        25 => Some("java-runtime-epsilon"),
        _ => None,
    }
}

const MOJANG_RUNTIMES: &str = "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

fn mojang_platform() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86") => "windows-x86",
        ("windows", "aarch64") => "windows-arm64",
        ("windows", _) => "windows-x64",
        ("macos", "aarch64") => "mac-os-arm64",
        ("macos", _) => "mac-os",
        (_, "x86") => "linux-i386",
        _ => "linux",
    }
}

async fn mojang_runtime(client: &reqwest::Client, p: &Paths, component: &str, rep: &Reporter) -> Result<PathBuf> {
    let dir = p.runtimes().join(component);
    let mut existing = vec![];
    find_exes(&dir, 3, &mut existing);
    if let Some(e) = existing.into_iter().next() {
        return Ok(e);
    }
    rep.stage(format!("Descargando Java ({component}) del servidor de Mojang"));
    let all: Value = net::get_json(client, MOJANG_RUNTIMES).await?;
    let entry = all[mojang_platform()][component].get(0).with_context(|| format!("Mojang no tiene {component} para {}", mojang_platform()))?;
    let manifest_url = entry["manifest"]["url"].as_str().context("runtime sin manifest")?;
    let manifest: Value = net::get_json(client, manifest_url).await?;
    let files = manifest["files"].as_object().context("manifest de runtime vacío")?;

    let tmp = p.runtimes().join(format!("{component}.tmp"));
    let _ = std::fs::remove_dir_all(&tmp);
    let mut items = vec![];
    let mut executables = vec![];
    let mut links = vec![];
    for (rel, f) in files {
        let path = tmp.join(rel);
        match f["type"].as_str() {
            Some("directory") => std::fs::create_dir_all(&path)?,
            Some("file") => {
                let raw = &f["downloads"]["raw"];
                items.push(Download {
                    url: raw["url"].as_str().context("fichero sin url")?.to_string(),
                    path: path.clone(),
                    sha1: raw["sha1"].as_str().map(String::from),
                    size: raw["size"].as_u64(),
                });
                if f["executable"].as_bool().unwrap_or(false) {
                    executables.push(path);
                }
            }
            Some("link") => {
                if let Some(target) = f["target"].as_str() {
                    links.push((path, target.to_string()));
                }
            }
            _ => {}
        }
    }
    net::download_all(client, items, 16, false, rep).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for e in &executables {
            let _ = std::fs::set_permissions(e, std::fs::Permissions::from_mode(0o755));
        }
        for (path, target) in &links {
            let _ = std::os::unix::fs::symlink(target, path);
        }
    }
    #[cfg(not(unix))]
    let _ = (&executables, &links);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::rename(&tmp, &dir)?;
    let mut found = vec![];
    find_exes(&dir, 3, &mut found);
    found.into_iter().next().context("runtime descargado pero sin ejecutable de Java")
}

fn adoptium_os() -> &'static str {
    match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "mac",
        _ => "linux",
    }
}

fn adoptium_arch() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "aarch64",
        "x86" => "x32",
        _ => "x64",
    }
}

async fn temurin(client: &reqwest::Client, p: &Paths, major: u32, rep: &Reporter) -> Result<PathBuf> {
    let dir = p.runtimes().join(format!("java-{major}"));
    rep.stage(format!("Descargando Java {major} (Temurin)"));
    let ext = if cfg!(windows) { "zip" } else { "tar.gz" };
    let archive = p.cache().join(format!("java-{major}.{ext}"));
    let mut last = None;
    let mut ok = false;
    for image in ["jre", "jdk"] {
        let url = format!("https://api.adoptium.net/v3/binary/latest/{major}/ga/{}/{}/{image}/hotspot/normal/eclipse", adoptium_os(), adoptium_arch());
        match net::download_one(client, &Download { url, path: archive.clone(), sha1: None, size: None }).await {
            Ok(()) => {
                ok = true;
                break;
            }
            Err(e) => last = Some(e),
        }
    }
    if !ok {
        return Err(last.unwrap()).context(format!("Temurin {major} para {}-{} no disponible", adoptium_os(), adoptium_arch()));
    }
    rep.stage(format!("Extrayendo Java {major}"));
    let tmp = p.runtimes().join(format!("java-{major}.tmp"));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp)?;
    let (a, t) = (archive.clone(), tmp.clone());
    tokio::task::spawn_blocking(move || extract_archive(&a, &t)).await??;
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::rename(&tmp, &dir)?;
    let _ = std::fs::remove_file(&archive);
    let mut found = vec![];
    find_exes(&dir, 5, &mut found);
    found.into_iter().next().context("Java descargado pero no se encuentra bin/java")
}

fn extract_archive(archive: &Path, dest: &Path) -> Result<()> {
    let f = std::fs::File::open(archive)?;
    if archive.extension().and_then(|e| e.to_str()) == Some("zip") {
        let mut z = zip::ZipArchive::new(f)?;
        z.extract(dest)?;
    } else {
        let gz = flate2::read::GzDecoder::new(f);
        let mut t = tar::Archive::new(gz);
        t.set_preserve_permissions(true);
        t.unpack(dest)?;
    }
    Ok(())
}

/// Variante de consola (java en vez de javaw) para ejecutar instaladores y `-version`.
pub fn console_variant(exe: &Path) -> PathBuf {
    if cfg!(windows) {
        let c = exe.with_file_name("java.exe");
        if c.exists() {
            return c;
        }
    }
    exe.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn j(major: u32, is64: bool) -> JavaInstall {
        JavaInstall { exe: PathBuf::from(format!("/j{major}{is64}")), major, version: major.to_string(), is_64bit: is64, source: "t".into() }
    }
    #[test]
    fn majors() {
        assert_eq!(parse_major("1.8.0_392"), Some(8));
        assert_eq!(parse_major("\"17.0.9\""), Some(17));
        assert_eq!(parse_major("21"), Some(21));
        assert_eq!(parse_major("25-ea"), Some(25));
        assert_eq!(parse_major("25.0.1+8"), Some(25));
    }
    #[test]
    fn selection() {
        let l = vec![j(8, true), j(17, true), j(21, false), j(21, true), j(25, true)];
        assert_eq!(best(&l, 21).unwrap(), j(21, true));
        assert_eq!(best(&l, 8).unwrap().major, 8);
        assert_eq!(best(&l, 16).unwrap().major, 17);
        assert_eq!(best(&l, 25).unwrap().major, 25);
        assert!(best(&[j(21, true)], 25).is_none());
        assert!(best(&[j(17, true)], 8).is_none());
    }
}
