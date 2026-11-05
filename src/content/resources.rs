//! Contenido de una instancia aparte de los mods: texturas (resource packs), shaders y capturas.
//!
//! - Texturas: se activan como lo hace el juego, en la lista `resourcePacks` de `options.txt`
//!   (el último de la lista tiene prioridad).
//! - Shaders: el activo es el de Iris (`config/iris.properties`) u OptiFine (`optionsshaders.txt`).
//! - Capturas: miniaturas pequeñas en caché, para no cargar en memoria imágenes de varios MB.

use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    ResourcePack,
    Shader,
    Screenshot,
}

impl Kind {
    pub fn dir(self) -> &'static str {
        match self {
            Kind::ResourcePack => "resourcepacks",
            Kind::Shader => "shaderpacks",
            Kind::Screenshot => "screenshots",
        }
    }
    /// Extensiones de fichero aceptadas al añadir (las carpetas también valen para packs y shaders).
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Kind::ResourcePack | Kind::Shader => &["zip"],
            Kind::Screenshot => &["png", "jpg", "jpeg"],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// Nombre del fichero o carpeta.
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub modified: u64,
}

fn mtime(m: &std::fs::Metadata) -> u64 {
    m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0)
}

fn dir_size(p: &Path) -> u64 {
    std::fs::read_dir(p)
        .map(|rd| rd.flatten().map(|e| if e.path().is_dir() { dir_size(&e.path()) } else { e.metadata().map(|m| m.len()).unwrap_or(0) }).sum())
        .unwrap_or(0)
}

/// Lista el contenido de ese tipo. Texturas y shaders por nombre; capturas, las más nuevas primero.
pub fn list(game_dir: &Path, kind: Kind) -> Vec<Item> {
    let mut v = vec![];
    let Ok(rd) = std::fs::read_dir(game_dir.join(kind.dir())) else { return v };
    for e in rd.flatten() {
        let path = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        let Ok(meta) = e.metadata() else { continue };
        let ok = if meta.is_dir() {
            kind != Kind::Screenshot && !name.starts_with('.')
        } else {
            let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
            kind.extensions().contains(&ext.as_str())
        };
        if ok {
            let size = if meta.is_dir() { dir_size(&path) } else { meta.len() };
            v.push(Item { name, path, size, modified: mtime(&meta) });
        }
    }
    if kind == Kind::Screenshot {
        v.sort_by_key(|i| std::cmp::Reverse((i.modified, i.name.clone())));
    } else {
        v.sort_by_key(|i| i.name.to_lowercase());
    }
    v
}

/// Copia ficheros a la carpeta de ese tipo. Si ya existe uno con el mismo nombre, se añade « (2)».
pub fn add(game_dir: &Path, kind: Kind, files: &[PathBuf]) -> Result<usize> {
    let dir = game_dir.join(kind.dir());
    std::fs::create_dir_all(&dir)?;
    let mut n = 0;
    for f in files {
        let ext = f.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
        if !kind.extensions().contains(&ext.as_str()) {
            bail!("{} no es un fichero válido aquí (se esperaba .{})", f.display(), kind.extensions().join(" / ."));
        }
        let name = f.file_name().context("fichero sin nombre")?.to_string_lossy().into_owned();
        let mut dst = dir.join(&name);
        let stem = f.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let mut i = 2;
        while dst.exists() {
            dst = dir.join(format!("{stem} ({i}).{ext}"));
            i += 1;
        }
        std::fs::copy(f, &dst).with_context(|| format!("copiando {}", f.display()))?;
        n += 1;
    }
    Ok(n)
}

/// Borra un elemento, sólo si está dentro de la carpeta de ese tipo de la instancia.
pub fn delete(game_dir: &Path, kind: Kind, path: &Path) -> Result<()> {
    let dir = game_dir.join(kind.dir());
    if path.parent() != Some(dir.as_path()) {
        bail!("{} no pertenece a esta instancia", path.display());
    }
    if path.is_dir() {
        std::fs::remove_dir_all(path)?;
    } else {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

// ----------------------------------------------------------------------------- texturas activas

fn options_value<'a>(options: &'a str, key: &str) -> Option<&'a str> {
    options.lines().find_map(|l| l.strip_prefix(key).and_then(|r| r.strip_prefix(':')))
}

/// Packs activos según `options.txt` (los nombres de fichero, sin el prefijo `file/`).
pub fn enabled_packs(options: &str) -> Vec<String> {
    let Some(raw) = options_value(options, "resourcePacks") else { return vec![] };
    let list: Vec<String> = serde_json::from_str::<Vec<Value>>(raw).unwrap_or_default().iter().filter_map(|v| v.as_str()).map(String::from).collect();
    list.iter().filter_map(|s| s.strip_prefix("file/")).map(String::from).collect()
}

/// Activa o desactiva un pack. Al activarlo se pone el último (máxima prioridad, como en el juego).
/// Conserva el resto de entradas (`vanilla`, packs de mods...) y el resto del fichero.
pub fn set_pack_enabled(options: &str, pack: &str, on: bool) -> String {
    let key = format!("file/{pack}");
    let mut list: Vec<String> = options_value(options, "resourcePacks")
        .and_then(|raw| serde_json::from_str::<Vec<Value>>(raw).ok())
        .map(|v| v.iter().filter_map(|x| x.as_str()).map(String::from).collect())
        .unwrap_or_else(|| vec!["vanilla".to_string()]);
    list.retain(|s| s != &key);
    if on {
        list.push(key);
    }
    let line = format!("resourcePacks:{}", serde_json::to_string(&list).unwrap_or_else(|_| "[]".into()));
    let mut found = false;
    let mut lines: Vec<String> = options
        .lines()
        .map(|l| {
            if l.starts_with("resourcePacks:") {
                found = true;
                line.clone()
            } else {
                l.to_string()
            }
        })
        .collect();
    if !found {
        lines.push(line);
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

pub fn set_pack_enabled_in(game_dir: &Path, pack: &str, on: bool) -> Result<()> {
    let path = game_dir.join("options.txt");
    let cur = std::fs::read_to_string(&path).unwrap_or_default();
    crate::paths::write_atomic(&path, set_pack_enabled(&cur, pack, on))?;
    Ok(())
}

// ----------------------------------------------------------------------------- shader activo

fn prop(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| l.trim().strip_prefix(key).and_then(|r| r.trim_start().strip_prefix('='))).map(|v| v.trim().to_string())
}

fn set_prop(text: &str, key: &str, value: &str) -> String {
    let mut found = false;
    let mut lines: Vec<String> = text
        .lines()
        .map(|l| {
            if l.trim().strip_prefix(key).map(|r| r.trim_start().starts_with('=')).unwrap_or(false) {
                found = true;
                format!("{key}={value}")
            } else {
                l.to_string()
            }
        })
        .collect();
    if !found {
        lines.push(format!("{key}={value}"));
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// Shader activo (Iris primero, luego OptiFine). `None` si no hay o están desactivados.
pub fn active_shader(game_dir: &Path) -> Option<String> {
    if let Ok(t) = std::fs::read_to_string(game_dir.join("config/iris.properties")) {
        let on = prop(&t, "enableShaders").map(|v| v == "true").unwrap_or(true);
        if let Some(p) = prop(&t, "shaderPack").filter(|p| !p.is_empty()) {
            return on.then_some(p);
        }
    }
    let t = std::fs::read_to_string(game_dir.join("optionsshaders.txt")).ok()?;
    prop(&t, "shaderPack").filter(|p| !p.is_empty() && p != "OFF" && p != "(internal)")
}

/// Qué cargador de shaders hay en los mods: Iris/Oculus o OptiFine.
pub fn shader_loader(mod_files: &[String]) -> Option<&'static str> {
    let lower: Vec<String> = mod_files.iter().map(|m| m.to_lowercase()).collect();
    if lower.iter().any(|m| m.contains("iris") || m.contains("oculus")) {
        Some("Iris")
    } else if lower.iter().any(|m| m.contains("optifine") || m.contains("optifabric")) {
        Some("OptiFine")
    } else {
        None
    }
}

/// Activa un shader (o los desactiva con `None`) en la configuración de Iris y/o OptiFine.
pub fn set_active_shader(game_dir: &Path, shader: Option<&str>, loader: Option<&str>) -> Result<()> {
    let iris = game_dir.join("config/iris.properties");
    let of = game_dir.join("optionsshaders.txt");
    let use_iris = iris.exists() || loader == Some("Iris");
    let use_of = of.exists() || loader == Some("OptiFine");
    if !use_iris && !use_of {
        bail!("instala Iris (Fabric/Quilt/NeoForge) u OptiFine para usar shaders");
    }
    if use_iris {
        let cur = std::fs::read_to_string(&iris).unwrap_or_default();
        let t = set_prop(&cur, "shaderPack", shader.unwrap_or(""));
        let t = set_prop(&t, "enableShaders", if shader.is_some() { "true" } else { "false" });
        std::fs::create_dir_all(game_dir.join("config"))?;
        crate::paths::write_atomic(&iris, t)?;
    }
    if use_of {
        let cur = std::fs::read_to_string(&of).unwrap_or_default();
        crate::paths::write_atomic(&of, set_prop(&cur, "shaderPack", shader.unwrap_or("OFF")))?;
    }
    Ok(())
}

// ----------------------------------------------------------------------------- miniaturas

/// Ruta de la miniatura en caché (cambia si la captura cambia).
pub fn thumb_path(cache: &Path, item: &Item) -> PathBuf {
    use sha1::{Digest, Sha1};
    let key = format!("{}|{}|{}", item.path.display(), item.modified, item.size);
    cache.join("thumbs").join(format!("{}.png", hex::encode(&Sha1::digest(key.as_bytes())[..10])))
}

/// Crea las miniaturas que falten (lado mayor 320 px). Devuelve cuántas creó.
pub fn make_thumbs(cache: &Path, items: &[Item]) -> usize {
    let mut n = 0;
    let _ = std::fs::create_dir_all(cache.join("thumbs"));
    for it in items {
        let dst = thumb_path(cache, it);
        if dst.exists() {
            continue;
        }
        let Ok(img) = image::open(&it.path) else { continue };
        let small = img.thumbnail(320, 320);
        if small.save_with_format(&dst, image::ImageFormat::Png).is_ok() {
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("brasa-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn packs_enable_disable_keep_the_rest() {
        let opts = "version:3953\nresourcePacks:[\"vanilla\",\"fabric\",\"file/Viejo.zip\"]\nlang:es_es";
        assert_eq!(enabled_packs(opts), vec!["Viejo.zip"]);
        let on = set_pack_enabled(opts, "Faithful 32x.zip", true);
        assert_eq!(enabled_packs(&on), vec!["Viejo.zip", "Faithful 32x.zip"], "el nuevo, último (más prioridad)");
        assert!(on.contains("\"fabric\"") && on.contains("lang:es_es") && on.starts_with("version:3953"));
        let off = set_pack_enabled(&on, "Viejo.zip", false);
        assert_eq!(enabled_packs(&off), vec!["Faithful 32x.zip"]);
        assert_eq!(set_pack_enabled(&off, "Faithful 32x.zip", true), off, "activar dos veces no duplica");
    }

    #[test]
    fn packs_on_a_fresh_instance() {
        let t = set_pack_enabled("", "A.zip", true);
        assert_eq!(t, "resourcePacks:[\"vanilla\",\"file/A.zip\"]\n");
        assert!(enabled_packs("lang:es_es").is_empty());
    }

    #[test]
    fn list_add_delete() {
        let g = tmp();
        let src = tmp();
        std::fs::write(src.join("Pack.zip"), b"zip").unwrap();
        std::fs::write(src.join("nota.txt"), b"x").unwrap();
        assert_eq!(add(&g, Kind::ResourcePack, &[src.join("Pack.zip")]).unwrap(), 1);
        assert_eq!(add(&g, Kind::ResourcePack, &[src.join("Pack.zip")]).unwrap(), 1);
        assert!(add(&g, Kind::ResourcePack, &[src.join("nota.txt")]).is_err(), "sólo .zip");
        std::fs::create_dir_all(g.join("resourcepacks/Carpeta")).unwrap();
        std::fs::write(g.join("resourcepacks/Carpeta/pack.mcmeta"), b"{}").unwrap();
        let names: Vec<_> = list(&g, Kind::ResourcePack).into_iter().map(|i| i.name).collect();
        assert_eq!(names, vec!["Carpeta", "Pack (2).zip", "Pack.zip"]);
        let it = list(&g, Kind::ResourcePack).remove(2);
        delete(&g, Kind::ResourcePack, &it.path).unwrap();
        assert_eq!(list(&g, Kind::ResourcePack).len(), 2);
        assert!(delete(&g, Kind::ResourcePack, &src.join("nota.txt")).is_err(), "nada fuera de la carpeta");
        assert!(src.join("nota.txt").exists());
        let _ = std::fs::remove_dir_all(g);
        let _ = std::fs::remove_dir_all(src);
    }

    #[test]
    fn shaders_iris_and_optifine() {
        let g = tmp();
        assert_eq!(active_shader(&g), None);
        assert!(set_active_shader(&g, Some("BSL.zip"), None).is_err(), "sin Iris ni OptiFine");
        set_active_shader(&g, Some("BSL.zip"), Some("Iris")).unwrap();
        assert_eq!(active_shader(&g).as_deref(), Some("BSL.zip"));
        set_active_shader(&g, None, Some("Iris")).unwrap();
        assert_eq!(active_shader(&g), None);
        let og = tmp();
        std::fs::write(og.join("optionsshaders.txt"), "antialiasingLevel=0\nshaderPack=OFF\n").unwrap();
        assert_eq!(active_shader(&og), None);
        set_active_shader(&og, Some("Sildurs.zip"), None).unwrap();
        assert_eq!(active_shader(&og).as_deref(), Some("Sildurs.zip"));
        assert!(std::fs::read_to_string(og.join("optionsshaders.txt")).unwrap().contains("antialiasingLevel=0"));
        assert_eq!(shader_loader(&["iris-1.8.jar".into()]), Some("Iris"));
        assert_eq!(shader_loader(&["OptiFine_1.20.1.jar".into()]), Some("OptiFine"));
        assert_eq!(shader_loader(&["sodium.jar".into()]), None);
        let _ = std::fs::remove_dir_all(g);
        let _ = std::fs::remove_dir_all(og);
    }

    #[test]
    fn screenshots_and_thumbnails() {
        let g = tmp();
        let cache = tmp();
        std::fs::create_dir_all(g.join("screenshots")).unwrap();
        let img = image::RgbImage::from_pixel(1920, 1080, image::Rgb([200, 100, 50]));
        img.save(g.join("screenshots/2026-10-25_12.00.00.png")).unwrap();
        std::fs::write(g.join("screenshots/rota.png"), b"no es png").unwrap();
        let items = list(&g, Kind::Screenshot);
        assert_eq!(items.len(), 2);
        assert_eq!(make_thumbs(&cache, &items), 1, "la rota se salta sin fallar");
        let good = items.iter().find(|i| i.name.starts_with("2026")).unwrap();
        let t = image::open(thumb_path(&cache, good)).unwrap();
        assert!(t.width() <= 320 && t.height() <= 320);
        assert_eq!(make_thumbs(&cache, &items), 0, "ya en caché");
        let _ = std::fs::remove_dir_all(g);
        let _ = std::fs::remove_dir_all(cache);
    }
}
