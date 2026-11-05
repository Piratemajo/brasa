//! Organizar instancias: grupos, orden, búsqueda e iconos propios.
//!
//! Lógica pura (sin interfaz) para poder probarla. El orden elegido y los grupos plegados son
//! preferencias de la interfaz: se guardan en `ui.toml`, aparte de los ajustes del launcher.

use crate::config::Instance;
use crate::paths::Paths;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Sort {
    /// Jugadas más recientemente primero.
    #[default]
    Recent,
    Name,
    /// Más horas jugadas primero.
    Playtime,
    /// Agrupadas por loader y, dentro, por nombre.
    Loader,
}

impl Sort {
    pub const ALL: [Sort; 4] = [Sort::Recent, Sort::Name, Sort::Playtime, Sort::Loader];
    pub fn label(self) -> &'static str {
        match self {
            Sort::Recent => t!("Recientes"),
            Sort::Name => t!("Nombre"),
            Sort::Playtime => t!("Tiempo jugado"),
            Sort::Loader => "Loader",
        }
    }
}

/// Preferencias de la lista de instancias.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct UiPrefs {
    pub sort: Sort,
    /// Grupos plegados.
    pub collapsed: Vec<String>,
}

impl UiPrefs {
    fn file(p: &Paths) -> std::path::PathBuf {
        p.root.join("ui.toml")
    }
    pub fn load(p: &Paths) -> UiPrefs {
        std::fs::read_to_string(Self::file(p)).ok().and_then(|t| toml::from_str(&t).ok()).unwrap_or_default()
    }
    pub fn save(&self, p: &Paths) -> Result<()> {
        std::fs::create_dir_all(&p.root)?;
        crate::paths::write_atomic(&Self::file(p), toml::to_string_pretty(self)?)?;
        Ok(())
    }
    pub fn is_collapsed(&self, group: &str) -> bool {
        self.collapsed.iter().any(|g| g == group)
    }
    pub fn toggle(&mut self, group: &str) {
        if self.is_collapsed(group) {
            self.collapsed.retain(|g| g != group);
        } else {
            self.collapsed.push(group.to_string());
            self.collapsed.sort();
        }
    }
}

/// Nombre de grupo normalizado: sin espacios sobrantes; vacío = sin grupo.
pub fn clean_group(g: &str) -> Option<String> {
    let g = g.split_whitespace().collect::<Vec<_>>().join(" ");
    (!g.is_empty()).then_some(g)
}

/// La instancia coincide con la búsqueda si cada palabra aparece en su nombre, grupo, loader o versión.
pub fn matches(i: &Instance, query: &str) -> bool {
    let hay = format!("{} {} {} {}", i.name, i.group.as_deref().unwrap_or(""), i.loader.label(), i.mc_version).to_lowercase();
    query.to_lowercase().split_whitespace().all(|w| hay.contains(w))
}

fn compare(a: &Instance, b: &Instance, sort: Sort) -> std::cmp::Ordering {
    let by_name = || a.name.to_lowercase().cmp(&b.name.to_lowercase());
    match sort {
        Sort::Recent => b.last_played.cmp(&a.last_played).then_with(by_name),
        Sort::Name => by_name(),
        Sort::Playtime => b.playtime_secs.cmp(&a.playtime_secs).then_with(by_name),
        Sort::Loader => a.loader.label().cmp(b.loader.label()).then_with(by_name),
    }
}

/// Un grupo de la lista. `name = None` son las instancias sin grupo.
#[derive(Debug)]
pub struct Group<'a> {
    pub name: Option<String>,
    pub items: Vec<&'a Instance>,
}

/// Instancias filtradas, ordenadas y agrupadas: grupos con nombre (alfabéticos) y al final las sueltas.
pub fn grouped<'a>(instances: &'a [Instance], sort: Sort, query: &str) -> Vec<Group<'a>> {
    let mut items: Vec<&Instance> = instances.iter().filter(|i| matches(i, query)).collect();
    items.sort_by(|a, b| compare(a, b, sort));
    let mut names: Vec<String> = items.iter().filter_map(|i| i.group.clone()).collect();
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup();
    let mut out: Vec<Group> = names
        .into_iter()
        .map(|n| {
            let its = items.iter().copied().filter(|i| i.group.as_deref() == Some(n.as_str())).collect();
            Group { name: Some(n), items: its }
        })
        .collect();
    let loose: Vec<&Instance> = items.into_iter().filter(|i| i.group.is_none()).collect();
    if !loose.is_empty() {
        out.push(Group { name: None, items: loose });
    }
    out
}

/// Grupos que existen, para sugerirlos al editar una instancia.
pub fn group_names(instances: &[Instance]) -> Vec<String> {
    let mut v: Vec<String> = instances.iter().filter_map(|i| i.group.clone()).collect();
    v.sort_by_key(|n| n.to_lowercase());
    v.dedup();
    v
}

// ----------------------------------------------------------------------------- iconos

pub const ICON_SIZE: u32 = 128;

/// Convierte una imagen en el icono de la instancia: recorte cuadrado centrado y 128×128 px.
/// Cada icono nuevo lleva un nombre distinto (la interfaz guarda las imágenes en caché por ruta)
/// y se borran los anteriores. Devuelve el nombre del fichero dentro de la carpeta de la instancia.
pub fn set_icon(inst_dir: &Path, image: &Path) -> Result<String> {
    let img = image::open(image).with_context(|| format!("no se pudo abrir la imagen {}", image.display()))?;
    let side = img.width().min(img.height());
    if side == 0 {
        anyhow::bail!("la imagen está vacía");
    }
    let (x, y) = ((img.width() - side) / 2, (img.height() - side) / 2);
    // las imágenes pequeñas (pixel-art) se escalan sin suavizar para que sigan nítidas
    let filter = if side <= ICON_SIZE / 2 { image::imageops::FilterType::Nearest } else { image::imageops::FilterType::Lanczos3 };
    let icon = img.crop_imm(x, y, side, side).resize_exact(ICON_SIZE, ICON_SIZE, filter);
    let name = format!("icon-{}.png", uuid::Uuid::new_v4().simple());
    std::fs::create_dir_all(inst_dir)?;
    let tmp = inst_dir.join(format!(".{name}.tmp"));
    icon.save_with_format(&tmp, image::ImageFormat::Png).context("no se pudo guardar el icono")?;
    std::fs::rename(&tmp, inst_dir.join(&name))?;
    remove_icons_except(inst_dir, Some(&name));
    Ok(name)
}

/// Borra los iconos de la instancia salvo `keep`.
pub fn remove_icons_except(inst_dir: &Path, keep: Option<&str>) {
    if let Ok(rd) = std::fs::read_dir(inst_dir) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if n.starts_with("icon-") && n.ends_with(".png") && Some(n.as_str()) != keep {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

/// Nombre de icono válido (lo que se guarda en instance.toml no puede apuntar fuera de la carpeta).
pub fn valid_icon_name(n: &str) -> bool {
    n.starts_with("icon-") && n.ends_with(".png") && !n.contains(['/', '\\']) && !n.contains("..")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Loader;

    fn inst(name: &str, group: Option<&str>, loader: Loader, last: Option<u64>, play: u64) -> Instance {
        Instance {
            id: name.to_lowercase(),
            name: name.into(),
            mc_version: "1.21.1".into(),
            loader,
            group: group.map(String::from),
            last_played: last,
            playtime_secs: play,
            ..Default::default()
        }
    }

    fn sample() -> Vec<Instance> {
        vec![
            inst("Skyblock", Some("Con amigos"), Loader::Fabric, Some(10), 500),
            inst("Create", Some("Técnicos"), Loader::Forge, Some(30), 9000),
            inst("Vanilla", None, Loader::Vanilla, Some(50), 100),
            inst("Aventura", Some("Con amigos"), Loader::NeoForge, None, 0),
            inst("Pruebas", None, Loader::Quilt, Some(20), 50),
        ]
    }

    #[test]
    fn groups_first_alphabetically_then_loose() {
        let all = sample();
        let g = grouped(&all, Sort::Name, "");
        let names: Vec<_> = g.iter().map(|g| g.name.as_deref()).collect();
        assert_eq!(names, vec![Some("Con amigos"), Some("Técnicos"), None]);
        let first: Vec<_> = g[0].items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(first, vec!["Aventura", "Skyblock"]);
        let loose: Vec<_> = g[2].items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(loose, vec!["Pruebas", "Vanilla"]);
    }

    #[test]
    fn sort_orders() {
        let all = sample();
        let flat = |s| grouped(&all, s, "").into_iter().flat_map(|g| g.items).map(|i| i.name.clone()).collect::<Vec<_>>();
        // dentro de «Con amigos»: Skyblock (jugada) antes que Aventura (nunca jugada)
        assert_eq!(flat(Sort::Recent), vec!["Skyblock", "Aventura", "Create", "Vanilla", "Pruebas"]);
        assert_eq!(flat(Sort::Playtime)[2], "Create");
        let loose: Vec<_> = grouped(&all, Sort::Loader, "").pop().unwrap().items.iter().map(|i| i.name.clone()).collect();
        assert_eq!(loose, vec!["Pruebas", "Vanilla"], "Quilt antes que Vanilla");
    }

    #[test]
    fn search_by_name_group_loader_or_version() {
        let all = sample();
        let names = |q: &str| grouped(&all, Sort::Name, q).into_iter().flat_map(|g| g.items).map(|i| i.name.clone()).collect::<Vec<_>>();
        assert_eq!(names("sky"), vec!["Skyblock"]);
        assert_eq!(names("amigos"), vec!["Aventura", "Skyblock"]);
        assert_eq!(names("forge"), vec!["Aventura", "Create"], "NeoForge contiene «forge»");
        assert_eq!(names("amigos fabric"), vec!["Skyblock"], "todas las palabras deben aparecer");
        assert_eq!(names("1.21"), names(""));
        assert!(names("nada").is_empty());
        assert!(grouped(&all, Sort::Name, "nada").is_empty(), "sin coincidencias no quedan grupos vacíos");
    }

    #[test]
    fn group_cleaning_and_names() {
        assert_eq!(clean_group("  Con   amigos "), Some("Con amigos".into()));
        assert_eq!(clean_group("   "), None);
        assert_eq!(group_names(&sample()), vec!["Con amigos", "Técnicos"]);
    }

    #[test]
    fn prefs_toggle_and_persist() {
        let root = std::env::temp_dir().join(format!("brasa-test-{}", uuid::Uuid::new_v4()));
        let p = Paths { root: root.clone() };
        assert_eq!(UiPrefs::load(&p), UiPrefs::default());
        let mut u = UiPrefs { sort: Sort::Playtime, ..Default::default() };
        u.toggle("Técnicos");
        assert!(u.is_collapsed("Técnicos"));
        u.save(&p).unwrap();
        assert_eq!(UiPrefs::load(&p), u);
        u.toggle("Técnicos");
        assert!(!u.is_collapsed("Técnicos"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn icons_are_square_small_and_replace_the_old_one() {
        let dir = std::env::temp_dir().join(format!("brasa-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("foto.png");
        image::RgbaImage::from_pixel(640, 360, image::Rgba([200, 80, 20, 255])).save(&src).unwrap();
        let first = set_icon(&dir, &src).unwrap();
        assert!(valid_icon_name(&first));
        let img = image::open(dir.join(&first)).unwrap();
        assert_eq!((img.width(), img.height()), (ICON_SIZE, ICON_SIZE));
        let second = set_icon(&dir, &src).unwrap();
        assert_ne!(first, second, "nombre nuevo para que la interfaz no muestre el viejo en caché");
        assert!(!dir.join(&first).exists() && dir.join(&second).exists());
        assert!(set_icon(&dir, &dir.join("no-existe.png")).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn icon_names_cannot_escape_the_folder() {
        assert!(valid_icon_name("icon-abc.png"));
        assert!(!valid_icon_name("../icon-abc.png") && !valid_icon_name("icon-..png") && !valid_icon_name("icon-a/b.png") && !valid_icon_name("foto.png"));
    }
}
