//! Datos de un mod leídos de dentro de su `.jar`: nombre, versión, autores, descripción e icono.
//!
//! Formatos: `fabric.mod.json`, `quilt.mod.json`, `META-INF/neoforge.mods.toml`,
//! `META-INF/mods.toml` (Forge 1.13+) y `mcmod.info` (Forge antiguo). Una versión
//! `${file.jarVersion}` se toma del `MANIFEST.MF`.
//!
//! Leer 200 jars cada vez sería lento: los resultados se guardan en `cache/mod-meta.json`
//! (clave: nombre, tamaño y fecha del fichero) y los iconos, reducidos a 48 px, en
//! `cache/mod-icons/`. Todo funciona sin conexión.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const ICON_PX: u32 = 48;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct ModMeta {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub authors: Vec<String>,
    /// fabric, quilt, forge o neoforge.
    pub loader: String,
    /// Icono ya reducido, en la caché.
    pub icon: Option<PathBuf>,
}

/// Lo que se saca del descriptor, antes de tratar el icono.
#[derive(Debug, Default, PartialEq)]
pub struct Parsed {
    pub meta: ModMeta,
    /// Ruta del icono dentro del jar.
    pub icon_path: Option<String>,
}

fn s(v: &Value) -> String {
    v.as_str().unwrap_or("").trim().to_string()
}

/// Autores: lista de textos o de objetos `{name}` (Fabric) o mapa nombre → papel (Quilt).
fn people(v: &Value) -> Vec<String> {
    let mut out: Vec<String> = match v {
        Value::Array(a) => a.iter().map(|x| if x.is_string() { s(x) } else { s(&x["name"]) }).collect(),
        Value::Object(o) => o.keys().cloned().collect(),
        Value::String(t) => t.split([',', '&']).map(|x| x.trim().to_string()).collect(),
        _ => vec![],
    };
    out.retain(|x| !x.is_empty());
    out
}

/// Icono: texto, o mapa tamaño → ruta (se elige el mayor).
fn icon(v: &Value) -> Option<String> {
    match v {
        Value::String(t) if !t.trim().is_empty() => Some(t.trim().to_string()),
        Value::Object(o) => {
            o.iter().filter_map(|(k, v)| Some((k.parse::<u32>().unwrap_or(0), v.as_str()?.to_string()))).max_by_key(|(k, _)| *k).map(|(_, p)| p)
        }
        _ => None,
    }
}

pub fn parse_fabric(json: &str) -> Option<Parsed> {
    let v: Value = serde_json::from_str(json).ok()?;
    let id = s(&v["id"]);
    if id.is_empty() {
        return None;
    }
    let mut authors = people(&v["authors"]);
    if authors.is_empty() {
        authors = people(&v["contributors"]);
    }
    Some(Parsed {
        meta: ModMeta {
            name: Some(s(&v["name"])).filter(|n| !n.is_empty()).unwrap_or_else(|| id.clone()),
            id,
            version: s(&v["version"]),
            description: s(&v["description"]),
            authors,
            loader: "fabric".into(),
            icon: None,
        },
        icon_path: icon(&v["icon"]),
    })
}

pub fn parse_quilt(json: &str) -> Option<Parsed> {
    let v: Value = serde_json::from_str(json).ok()?;
    let ql = &v["quilt_loader"];
    let id = s(&ql["id"]);
    if id.is_empty() {
        return None;
    }
    let md = &ql["metadata"];
    Some(Parsed {
        meta: ModMeta {
            name: Some(s(&md["name"])).filter(|n| !n.is_empty()).unwrap_or_else(|| id.clone()),
            id,
            version: s(&ql["version"]),
            description: s(&md["description"]),
            authors: people(&md["contributors"]),
            loader: "quilt".into(),
            icon: None,
        },
        icon_path: icon(&md["icon"]),
    })
}

/// `mods.toml` (Forge) y `neoforge.mods.toml`: el primer `[[mods]]`. `logoFile` puede ir en él o arriba.
pub fn parse_mods_toml(text: &str, loader: &str) -> Option<Parsed> {
    let v: toml::Value = toml::from_str(text).ok()?;
    let m = v.get("mods")?.as_array()?.first()?;
    let get = |t: &toml::Value, k: &str| t.get(k).and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
    let id = get(m, "modId");
    if id.is_empty() {
        return None;
    }
    let logo = Some(get(m, "logoFile")).filter(|x| !x.is_empty()).or_else(|| Some(get(&v, "logoFile")).filter(|x| !x.is_empty()));
    let authors = match m.get("authors") {
        Some(toml::Value::Array(a)) => a.iter().filter_map(|x| x.as_str()).map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect(),
        Some(toml::Value::String(t)) => t.split([',', '&']).map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect(),
        _ => vec![],
    };
    Some(Parsed {
        meta: ModMeta {
            name: Some(get(m, "displayName")).filter(|n| !n.is_empty()).unwrap_or_else(|| id.clone()),
            id,
            version: get(m, "version"),
            description: get(m, "description").lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" "),
            authors,
            loader: loader.into(),
            icon: None,
        },
        icon_path: logo,
    })
}

/// `mcmod.info`: una lista, o `{modList: [...]}`.
pub fn parse_mcmod_info(json: &str) -> Option<Parsed> {
    let v: Value = serde_json::from_str(json).ok()?;
    let m = match &v {
        Value::Array(a) => a.first()?,
        Value::Object(_) => v["modList"].as_array()?.first()?,
        _ => return None,
    };
    let id = s(&m["modid"]);
    if id.is_empty() {
        return None;
    }
    let mut authors = people(&m["authorList"]);
    if authors.is_empty() {
        authors = people(&m["authors"]);
    }
    Some(Parsed {
        meta: ModMeta {
            name: Some(s(&m["name"])).filter(|n| !n.is_empty()).unwrap_or_else(|| id.clone()),
            id,
            version: s(&m["version"]),
            description: s(&m["description"]),
            authors,
            loader: "forge".into(),
            icon: None,
        },
        icon_path: Some(s(&m["logoFile"])).filter(|x| !x.is_empty()),
    })
}

fn manifest_version(mf: &str) -> Option<String> {
    mf.lines().find_map(|l| l.strip_prefix("Implementation-Version:")).map(|x| x.trim().to_string()).filter(|x| !x.is_empty())
}

fn read_entry(z: &mut zip::ZipArchive<std::fs::File>, name: &str, max: u64) -> Option<Vec<u8>> {
    let name = name.trim_start_matches('/');
    let e = z.by_name(name).ok()?;
    if e.size() > max {
        return None;
    }
    let mut buf = Vec::with_capacity(e.size() as usize);
    e.take(max).read_to_end(&mut buf).ok()?;
    Some(buf)
}

/// Lee el descriptor de un jar (sin el icono).
pub fn read_jar(jar: &Path) -> Option<(Parsed, zip::ZipArchive<std::fs::File>)> {
    let f = std::fs::File::open(jar).ok()?;
    let mut z = zip::ZipArchive::new(f).ok()?;
    let txt = |z: &mut zip::ZipArchive<std::fs::File>, n: &str| read_entry(z, n, 512 * 1024).map(|b| String::from_utf8_lossy(&b).into_owned());
    let mut parsed = None;
    for (file, kind) in [
        ("fabric.mod.json", "fabric"),
        ("quilt.mod.json", "quilt"),
        ("META-INF/neoforge.mods.toml", "neoforge"),
        ("META-INF/mods.toml", "forge"),
        ("mcmod.info", "mcmod"),
    ] {
        if let Some(t) = txt(&mut z, file) {
            parsed = match kind {
                "fabric" => parse_fabric(&t),
                "quilt" => parse_quilt(&t),
                "mcmod" => parse_mcmod_info(&t),
                l => parse_mods_toml(&t, l),
            };
            if parsed.is_some() {
                break;
            }
        }
    }
    let mut parsed = parsed?;
    if parsed.meta.version.is_empty() || parsed.meta.version.contains("${") {
        parsed.meta.version = txt(&mut z, "META-INF/MANIFEST.MF").and_then(|m| manifest_version(&m)).unwrap_or_default();
    }
    Some((parsed, z))
}

/// Reduce un icono a `ICON_PX` px (los de pixel-art, sin suavizar) y lo guarda.
fn save_icon(bytes: &[u8], out: &Path) -> Option<()> {
    let img = image::load_from_memory(bytes).ok()?;
    let filter = if img.width() <= 32 { image::imageops::FilterType::Nearest } else { image::imageops::FilterType::Triangle };
    let small = img.resize(ICON_PX, ICON_PX, filter);
    std::fs::create_dir_all(out.parent()?).ok()?;
    small.save_with_format(out, image::ImageFormat::Png).ok()
}

/// Clave de caché: nombre (sin `.disabled`, para que activar/desactivar no invalide), tamaño y fecha.
pub fn cache_key(jar: &Path) -> Option<String> {
    let md = std::fs::metadata(jar).ok()?;
    let mtime = md.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    let name = jar.file_name()?.to_string_lossy();
    let name = name.trim_end_matches(".disabled");
    Some(format!("{name}|{}|{mtime}", md.len()))
}

fn hash_name(key: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in key.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

/// Datos de un jar, con su icono extraído a `icons_dir`.
pub fn extract(jar: &Path, icons_dir: &Path) -> Option<ModMeta> {
    let key = cache_key(jar)?;
    let (parsed, mut z) = read_jar(jar)?;
    let mut meta = parsed.meta;
    if let Some(ip) = parsed.icon_path {
        let out = icons_dir.join(format!("{}.png", hash_name(&key)));
        if out.is_file() || read_entry(&mut z, &ip, 4 * 1024 * 1024).and_then(|b| save_icon(&b, &out)).is_some() {
            meta.icon = Some(out);
        }
    }
    Some(meta)
}

/// Caché en disco: clave → datos (o `None` si el jar no tiene descriptor, para no reintentarlo).
#[derive(Serialize, Deserialize, Default)]
struct Cache {
    entries: HashMap<String, Option<ModMeta>>,
}

/// Datos de todos los jars, usando y actualizando la caché. Devuelve ruta del jar → datos.
pub fn load_all(cache_dir: &Path, jars: &[PathBuf]) -> HashMap<PathBuf, ModMeta> {
    let file = cache_dir.join("mod-meta.json");
    let icons = cache_dir.join("mod-icons");
    let mut cache: Cache = std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let mut changed = false;
    let mut out = HashMap::new();
    for jar in jars {
        let Some(key) = cache_key(jar) else { continue };
        let hit = cache.entries.get(&key).cloned().filter(|m| m.as_ref().and_then(|m| m.icon.as_ref()).is_none_or(|i| i.is_file()));
        let meta = match hit {
            Some(m) => m,
            None => {
                let m = extract(jar, &icons);
                cache.entries.insert(key, m.clone());
                changed = true;
                m
            }
        };
        if let Some(m) = meta {
            out.insert(jar.clone(), m);
        }
    }
    if changed {
        if let Ok(t) = serde_json::to_string(&cache) {
            let _ = std::fs::create_dir_all(cache_dir);
            let _ = crate::paths::write_atomic(&file, t);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn fabric_with_mixed_authors_and_icon_map() {
        let p = parse_fabric(r#"{"schemaVersion":1,"id":"sodium","version":"0.5.11+mc1.21","name":"Sodium","description":"The fastest rendering optimization mod","authors":["JellySquid",{"name":"IMS"}],"icon":{"16":"a16.png","128":"assets/sodium/icon.png"}}"#).unwrap();
        assert_eq!((p.meta.name.as_str(), p.meta.version.as_str()), ("Sodium", "0.5.11+mc1.21"));
        assert_eq!(p.meta.authors, vec!["JellySquid", "IMS"]);
        assert_eq!(p.icon_path.as_deref(), Some("assets/sodium/icon.png"), "el icono más grande");
        assert!(parse_fabric(r#"{"name":"sin id"}"#).is_none());
        assert_eq!(parse_fabric(r#"{"id":"x"}"#).unwrap().meta.name, "x", "sin nombre, el id");
    }

    #[test]
    fn quilt() {
        let p = parse_quilt(r#"{"schema_version":1,"quilt_loader":{"id":"qsl","version":"7.0.0","metadata":{"name":"Quilt Standard Libraries","description":"Libs","contributors":{"Ennui":"Owner"},"icon":"assets/qsl/icon.png"}}}"#).unwrap();
        assert_eq!((p.meta.name.as_str(), p.meta.loader.as_str()), ("Quilt Standard Libraries", "quilt"));
        assert_eq!(p.meta.authors, vec!["Ennui"]);
    }

    #[test]
    fn forge_and_neoforge_toml() {
        let t = r#"
modLoader="javafml"
loaderVersion="[47,)"
license="MIT"
[[mods]]
modId="create"
version="${file.jarVersion}"
displayName="Create"
authors="simibubi, MRH0"
description='''
Technology that empowers the player.
'''
logoFile="logo.png"
"#;
        let p = parse_mods_toml(t, "forge").unwrap();
        assert_eq!((p.meta.id.as_str(), p.meta.name.as_str()), ("create", "Create"));
        assert_eq!(p.meta.authors, vec!["simibubi", "MRH0"]);
        assert_eq!(p.meta.description, "Technology that empowers the player.");
        assert_eq!(p.icon_path.as_deref(), Some("logo.png"));
        assert_eq!(p.meta.version, "${file.jarVersion}", "se resuelve luego con el MANIFEST");
        let top = parse_mods_toml("logoFile=\"top.png\"\n[[mods]]\nmodId=\"x\"\n", "neoforge").unwrap();
        assert_eq!(top.icon_path.as_deref(), Some("top.png"));
        assert!(parse_mods_toml("esto no es toml [[", "forge").is_none());
    }

    #[test]
    fn old_forge_mcmod_info() {
        let list = parse_mcmod_info(r#"[{"modid":"jei","name":"Just Enough Items","version":"4.16","authorList":["mezz"],"logoFile":"logo.png"}]"#).unwrap();
        assert_eq!((list.meta.name.as_str(), list.meta.authors[0].as_str()), ("Just Enough Items", "mezz"));
        let v2 = parse_mcmod_info(r#"{"modListVersion":2,"modList":[{"modid":"ae2","name":"Applied Energistics 2"}]}"#).unwrap();
        assert_eq!(v2.meta.name, "Applied Energistics 2");
    }

    fn temp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("brasa-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn png(px: u32) -> Vec<u8> {
        let mut b = std::io::Cursor::new(vec![]);
        image::RgbaImage::from_pixel(px, px, image::Rgba([200, 60, 20, 255])).write_to(&mut b, image::ImageFormat::Png).unwrap();
        b.into_inner()
    }

    fn jar(path: &Path, files: &[(&str, &[u8])]) {
        let mut z = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        let o = zip::write::SimpleFileOptions::default();
        for (n, b) in files {
            z.start_file(*n, o).unwrap();
            z.write_all(b).unwrap();
        }
        z.finish().unwrap();
    }

    #[test]
    fn real_jars_end_to_end_with_cache() {
        let d = temp();
        let icon = png(128);
        let a = d.join("create-1.20.1-0.5.1.jar");
        jar(
            &a,
            &[
                ("META-INF/mods.toml", b"[[mods]]\nmodId=\"create\"\nversion=\"${file.jarVersion}\"\ndisplayName=\"Create\"\nlogoFile=\"logo.png\"\n"),
                ("META-INF/MANIFEST.MF", b"Manifest-Version: 1.0\nImplementation-Version: 0.5.1.f\n"),
                ("logo.png", &icon),
            ],
        );
        let b = d.join("sin-datos.jar");
        jar(&b, &[("algo.class", b"x")]);
        let c = d.join("roto.jar");
        std::fs::write(&c, "no es un zip").unwrap();

        let cache = d.join("cache");
        let all = load_all(&cache, &[a.clone(), b.clone(), c.clone()]);
        let m = &all[&a];
        assert_eq!((m.name.as_str(), m.version.as_str()), ("Create", "0.5.1.f"), "versión del MANIFEST");
        let ic = m.icon.as_ref().unwrap();
        let img = image::open(ic).unwrap();
        assert_eq!(img.width(), ICON_PX, "icono reducido");
        assert!(!all.contains_key(&b) && !all.contains_key(&c), "sin descriptor o roto: sin datos, sin fallar");

        // segunda vez: de la caché, aunque el jar desaparezca de su sitio con otro nombre
        let disabled = d.join("create-1.20.1-0.5.1.jar.disabled");
        std::fs::rename(&a, &disabled).unwrap();
        assert!(cache.join("mod-meta.json").is_file());
        let again = load_all(&cache, std::slice::from_ref(&disabled));
        assert_eq!(again[&disabled].name, "Create", "desactivar el mod no invalida la caché");
        let _ = std::fs::remove_dir_all(d);
    }
}
