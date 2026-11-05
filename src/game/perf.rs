//! Consumo del juego: memoria automática y «modo ahorro».
//!
//! Minecraft es Java y el launcher no puede cambiar su código, pero sí decide las dos cosas que más
//! pesan en su consumo: cuánta memoria reserva la JVM y los ajustes gráficos con los que arranca.
//!
//! - **Memoria automática:** en lugar de 4 GB para todo, se calcula a partir de la RAM del equipo,
//!   la versión de Minecraft y el número de mods activos. Reservar de más no da FPS: deja sin RAM al
//!   sistema y a la gráfica, y alarga las pausas del recolector.
//! - **Modo ahorro:** antes de lanzar, limita en `options.txt` lo que más CPU/GPU consume (distancia
//!   de render, FPS, sombras, partículas). Sólo baja valores; nunca sube los que ya son menores.

use crate::config::{Instance, Settings};
use std::path::Path;

/// Mínimo absoluto que se asigna (por debajo, el juego moderno no arranca).
pub const MIN_MB: u32 = 1024;
/// Máximo en modo automático: por encima, la JVM tarda más en recolectar sin ganar nada.
pub const MAX_AUTO_MB: u32 = 12288;

// ----------------------------------------------------------------------------- versión de Minecraft

/// Versión «menor» de Minecraft: 1.21.1 → 21, 1.8.9 → 8. Las snapshots (`24w14a`) cuentan como
/// modernas; las alfas/betas antiguas, como 0.
pub fn mc_minor(v: &str) -> u32 {
    let v = v.trim();
    if let Some(rest) = v.strip_prefix("1.") {
        return rest.split(|c: char| !c.is_ascii_digit()).next().and_then(|n| n.parse().ok()).unwrap_or(0);
    }
    // snapshot «AAwSSx»: año 2 cifras → desde 2022 (22w..) son 1.19+
    if let Some((year, _)) = v.split_once('w') {
        if let Ok(y) = year.parse::<u32>() {
            return if y >= 22 { 99 } else { 13 };
        }
    }
    // alfas/betas (a1.2, b1.7.3, rd-132211) y desconocidas
    if v.starts_with(['a', 'b', 'r', 'c', 'i']) {
        return 0;
    }
    99
}

// ----------------------------------------------------------------------------- memoria

/// Lo que necesita la versión sin mods.
pub fn base_mb(mc_version: &str) -> u32 {
    match mc_minor(mc_version) {
        0..=12 => 1536,
        13..=17 => 2048,
        _ => 2560,
    }
}

/// Memoria que se puede dar al juego sin ahogar al sistema: se deja ~30 % de la RAM libre
/// (entre 1,5 y 4 GB) y nunca más del 60 %.
pub fn cap_mb(total_ram_mb: u64) -> u32 {
    let t = total_ram_mb as f64;
    let leave = (t * 0.30).clamp(1536.0, 4096.0);
    ((t - leave).min(t * 0.60).max(0.0)) as u32
}

/// Recomendación para una instancia. `total_ram_mb = None` si no se pudo detectar la RAM.
pub fn recommend_memory_mb(total_ram_mb: Option<u64>, active_mods: usize, mc_version: &str) -> u32 {
    let want = base_mb(mc_version) + active_mods.min(400) as u32 * 24;
    let cap = total_ram_mb.map(cap_mb).unwrap_or(MAX_AUTO_MB);
    let mb = want.min(cap).min(MAX_AUTO_MB);
    (mb / 256 * 256).max(MIN_MB)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemorySource {
    /// Fijada a mano en la instancia.
    Fixed,
    /// Calculada para esta instancia.
    Auto,
    /// La «memoria por defecto» de los ajustes (automática desactivada).
    Default,
}

/// La memoria con la que se lanzará realmente la instancia, y de dónde sale.
pub fn effective_memory(s: &Settings, inst: &Instance, total_ram_mb: Option<u64>, active_mods: usize) -> (u32, MemorySource) {
    match inst.memory_mb {
        Some(m) => (m.max(512), MemorySource::Fixed),
        None if s.auto_memory => (recommend_memory_mb(total_ram_mb, active_mods, &inst.mc_version), MemorySource::Auto),
        None => (s.default_memory_mb.max(512), MemorySource::Default),
    }
}

/// Aviso si la memoria elegida deja demasiado poco al sistema.
pub fn over_allocation_warning(mem_mb: u32, total_ram_mb: Option<u64>) -> Option<String> {
    let total = total_ram_mb?;
    if mem_mb as u64 * 4 > total * 3 {
        Some(format!("{} de {} de RAM: al sistema y a la gráfica les quedará muy poco y el juego irá peor, no mejor.", fmt_gb(mem_mb as u64), fmt_gb(total)))
    } else {
        None
    }
}

/// 3072 → «3 GB», 3584 → «3,5 GB».
pub fn fmt_gb(mb: u64) -> String {
    let gb = mb as f64 / 1024.0;
    if (gb - gb.round()).abs() < 0.05 {
        format!("{} GB", gb.round() as u64)
    } else {
        format!("{:.1} GB", gb).replace('.', ",")
    }
}

/// Mods activos en una carpeta (`.jar`; los desactivados acaban en `.jar.disabled`).
pub fn count_active_mods(dir: &Path) -> usize {
    std::fs::read_dir(dir).map(|rd| rd.flatten().filter(|e| e.file_name().to_string_lossy().to_ascii_lowercase().ends_with(".jar")).count()).unwrap_or(0)
}

// ----------------------------------------------------------------------------- RAM del equipo

/// RAM física total en MB. Se consulta una vez por ejecución.
pub fn total_ram_mb() -> Option<u64> {
    static CACHE: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    *CACHE.get_or_init(detect_total_ram_mb)
}

#[cfg(target_os = "linux")]
fn detect_total_ram_mb() -> Option<u64> {
    parse_meminfo(&std::fs::read_to_string("/proc/meminfo").ok()?)
}

#[cfg(target_os = "macos")]
fn detect_total_ram_mb() -> Option<u64> {
    let out = std::process::Command::new("sysctl").args(["-n", "hw.memsize"]).output().ok()?;
    let bytes: u64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
    Some(bytes / 1024 / 1024)
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn detect_total_ram_mb() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    // SAFETY: estructura de datos simple; la API sólo la rellena y exige dwLength inicializado.
    unsafe {
        let mut st: MEMORYSTATUSEX = std::mem::zeroed();
        st.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if GlobalMemoryStatusEx(&mut st) == 0 {
            return None;
        }
        Some(st.ullTotalPhys / 1024 / 1024)
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn detect_total_ram_mb() -> Option<u64> {
    None
}

/// `MemTotal:  16303892 kB` → 15921.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn parse_meminfo(text: &str) -> Option<u64> {
    let line = text.lines().find(|l| l.starts_with("MemTotal:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb / 1024)
}

// ----------------------------------------------------------------------------- modo ahorro

enum Rule {
    /// Si el valor numérico supera el máximo, se baja. Si falta la clave, se añade con el máximo.
    Cap(u32),
    /// Si el valor numérico es menor, se sube (p. ej. partículas: 0 todas, 1 reducidas, 2 mínimas).
    Floor(u32),
    /// Valor fijo.
    Set(&'static str),
    /// Sólo si la clave ya existe con ese valor, se cambia por el otro (claves cuyo formato varía
    /// entre versiones: no se añaden a ciegas).
    Replace(&'static str, &'static str),
}

/// Claves que toca el modo ahorro, con la primera versión menor en la que existen.
fn saving_rules(minor: u32) -> Vec<(&'static str, Rule)> {
    let mut v = vec![
        ("renderDistance", Rule::Cap(8)),
        // 260 = «sin límite» en el juego; con el tope se queda en 60
        ("maxFps", Rule::Cap(60)),
        ("enableVsync", Rule::Set("true")),
        ("particles", Rule::Floor(1)),
        ("entityShadows", Rule::Set("false")),
    ];
    if minor >= 13 {
        v.push(("biomeBlendRadius", Rule::Cap(1)));
    }
    if minor >= 18 {
        v.push(("simulationDistance", Rule::Cap(6)));
    }
    if minor >= 16 {
        // 2 = «fabulosos», el modo gráfico más caro → 1 «detallados»
        v.push(("graphicsMode", Rule::Replace("2", "1")));
    } else {
        v.push(("fancyGraphics", Rule::Replace("true", "false")));
    }
    v
}

/// Aplica el modo ahorro al contenido de `options.txt` (o a un fichero nuevo si no existe).
/// Devuelve el texto resultante y una descripción de cada cambio.
pub fn apply_saving_options(existing: Option<&str>, mc_version: &str) -> (String, Vec<String>) {
    let rules = saving_rules(mc_minor(mc_version));
    let mut lines: Vec<String> = existing.unwrap_or("").lines().map(str::to_string).collect();
    let mut changes = vec![];

    for (key, rule) in &rules {
        let pos = lines.iter().position(|l| l.split_once(':').map(|(k, _)| k == *key).unwrap_or(false));
        let current = pos.map(|i| lines[i].split_once(':').map(|(_, v)| v.trim().to_string()).unwrap_or_default());
        let new_value: Option<String> = match (rule, current.as_deref()) {
            (Rule::Cap(max), Some(v)) => v.parse::<u32>().ok().filter(|n| n > max).map(|_| max.to_string()),
            (Rule::Cap(max), None) => Some(max.to_string()),
            (Rule::Floor(min), Some(v)) => v.parse::<u32>().ok().filter(|n| n < min).map(|_| min.to_string()),
            (Rule::Floor(min), None) => Some(min.to_string()),
            (Rule::Set(val), Some(v)) => (v != *val).then(|| val.to_string()),
            (Rule::Set(val), None) => Some(val.to_string()),
            (Rule::Replace(from, to), Some(v)) => (v == *from).then(|| to.to_string()),
            (Rule::Replace(..), None) => None,
        };
        if let Some(nv) = new_value {
            match pos {
                Some(i) => {
                    changes.push(format!("{key}: {} → {nv}", current.unwrap_or_default()));
                    lines[i] = format!("{key}:{nv}");
                }
                None => {
                    changes.push(format!("{key}: {nv}"));
                    lines.push(format!("{key}:{nv}"));
                }
            }
        }
    }
    let mut out = lines.join("\n");
    out.push('\n');
    (out, changes)
}

/// Aplica el modo ahorro al `options.txt` de la instancia. Devuelve los cambios hechos.
pub fn apply_saving_mode(game_dir: &Path, mc_version: &str) -> anyhow::Result<Vec<String>> {
    let path = game_dir.join("options.txt");
    let existing = std::fs::read_to_string(&path).ok();
    let (text, changes) = apply_saving_options(existing.as_deref(), mc_version);
    if !changes.is_empty() {
        std::fs::create_dir_all(game_dir)?;
        crate::paths::write_atomic(&path, text)?;
    }
    Ok(changes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(mc_minor("1.21.1"), 21);
        assert_eq!(mc_minor("1.8.9"), 8);
        assert_eq!(mc_minor("1.20"), 20);
        assert_eq!(mc_minor("1.21.4-pre1"), 21);
        assert_eq!(mc_minor("24w14a"), 99);
        assert_eq!(mc_minor("b1.7.3"), 0);
        assert_eq!(base_mb("1.12.2"), 1536);
        assert_eq!(base_mb("1.16.5"), 2048);
        assert_eq!(base_mb("1.21.1"), 2560);
    }

    #[test]
    fn vanilla_gets_much_less_than_the_old_4gb() {
        assert_eq!(recommend_memory_mb(Some(16 * 1024), 0, "1.21.1"), 2560);
        assert_eq!(recommend_memory_mb(Some(16 * 1024), 0, "1.8.9"), 1536);
    }

    #[test]
    fn grows_with_mods_but_respects_the_machine() {
        let small = recommend_memory_mb(Some(16 * 1024), 40, "1.20.1");
        let big = recommend_memory_mb(Some(16 * 1024), 250, "1.20.1");
        assert!(small < big, "{small} < {big}");
        assert_eq!(big, 8448, "2560 + 250 × 24 = 8560 → redondeado a 8448");
        // En un equipo de 8 GB, un modpack enorme no puede pasar de lo que deja libre el sistema.
        let tight = recommend_memory_mb(Some(8 * 1024), 300, "1.20.1");
        assert!(tight <= cap_mb(8 * 1024), "{tight}");
        assert!(tight >= 4096, "pero sigue siendo usable: {tight}");
    }

    #[test]
    fn caps_and_floors() {
        assert_eq!(cap_mb(4 * 1024), 2457);
        assert_eq!(cap_mb(8 * 1024), 4915);
        assert_eq!(cap_mb(16 * 1024), 9830);
        // el modpack más grande en un equipo sobrado: 2560 + 400 × 24 = 12160 → 12032, bajo el techo
        assert_eq!(recommend_memory_mb(Some(64 * 1024), 400, "1.21.1"), 12032);
        assert!(recommend_memory_mb(Some(128 * 1024), 5000, "1.21.1") <= MAX_AUTO_MB);
        assert_eq!(recommend_memory_mb(Some(2 * 1024), 0, "1.21.1"), MIN_MB, "equipos muy justos: el mínimo");
        assert_eq!(recommend_memory_mb(None, 10, "1.21.1") % 256, 0, "siempre múltiplo de 256");
    }

    #[test]
    fn effective_memory_sources() {
        let s = Settings::default();
        let mut i = Instance { mc_version: "1.21.1".into(), ..Default::default() };
        assert_eq!(effective_memory(&s, &i, Some(16384), 0), (2560, MemorySource::Auto));
        i.memory_mb = Some(6144);
        assert_eq!(effective_memory(&s, &i, Some(16384), 0), (6144, MemorySource::Fixed));
        i.memory_mb = None;
        let s = Settings { auto_memory: false, default_memory_mb: 3072, ..Default::default() };
        assert_eq!(effective_memory(&s, &i, Some(16384), 0), (3072, MemorySource::Default));
    }

    #[test]
    fn over_allocation() {
        assert!(over_allocation_warning(4096, Some(16384)).is_none());
        let w = over_allocation_warning(7168, Some(8192)).unwrap();
        assert!(w.contains("7 GB de 8 GB"), "{w}");
        assert!(over_allocation_warning(30000, None).is_none(), "sin RAM conocida no se avisa");
        assert_eq!(fmt_gb(3584), "3,5 GB");
    }

    #[test]
    fn meminfo() {
        let t = "MemTotal:       16303892 kB\nMemFree:         1234 kB\n";
        assert_eq!(parse_meminfo(t), Some(15921));
        assert_eq!(parse_meminfo("nada"), None);
    }

    #[test]
    fn saving_mode_only_lowers_and_keeps_everything_else() {
        let original = "version:3953\nrenderDistance:16\nmaxFps:260\nparticles:0\nentityShadows:true\nsimulationDistance:4\nlang:es_es\ngraphicsMode:2\nkey_key.jump:key.keyboard.space";
        let (text, changes) = apply_saving_options(Some(original), "1.21.1");
        assert!(text.contains("renderDistance:8"));
        assert!(text.contains("maxFps:60"));
        assert!(text.contains("particles:1"));
        assert!(text.contains("entityShadows:false"));
        assert!(text.contains("graphicsMode:1"));
        assert!(text.contains("simulationDistance:4"), "un valor ya menor no se sube");
        assert!(text.contains("lang:es_es") && text.contains("key_key.jump:key.keyboard.space"), "el resto intacto");
        assert!(text.starts_with("version:3953\n"), "el orden se conserva");
        assert!(changes.iter().any(|c| c == "renderDistance: 16 → 8"), "{changes:?}");
        assert!(changes.iter().all(|c| !c.starts_with("simulationDistance")));
    }

    #[test]
    fn saving_mode_is_idempotent() {
        let (once, _) = apply_saving_options(Some("renderDistance:20\n"), "1.21.1");
        let (twice, changes) = apply_saving_options(Some(&once), "1.21.1");
        assert_eq!(once, twice);
        assert!(changes.is_empty(), "la segunda vez no cambia nada: {changes:?}");
    }

    #[test]
    fn saving_mode_on_a_new_or_old_instance() {
        let (new, _) = apply_saving_options(None, "1.21.1");
        assert!(new.contains("renderDistance:8") && new.contains("simulationDistance:6"));
        assert!(!new.contains("graphicsMode"), "no se inventan claves de formato variable");
        let (old, _) = apply_saving_options(Some("fancyGraphics:true\n"), "1.12.2");
        assert!(old.contains("fancyGraphics:false"));
        assert!(!old.contains("simulationDistance") && !old.contains("biomeBlendRadius:"), "claves que no existen en 1.12");
    }

    #[test]
    fn saving_mode_writes_the_file() {
        let dir = std::env::temp_dir().join(format!("brasa-test-{}", uuid::Uuid::new_v4()));
        let changes = apply_saving_mode(&dir, "1.21.1").unwrap();
        assert!(!changes.is_empty());
        assert!(std::fs::read_to_string(dir.join("options.txt")).unwrap().contains("maxFps:60"));
        assert!(apply_saving_mode(&dir, "1.21.1").unwrap().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
