//! Mundos de una instancia y copias de seguridad.
//!
//! Las copias son `.zip` en `<instancia>/backups/` (fuera de `.minecraft`, así no se exportan ni
//! se duplican con la instancia). Nombre: `<mundo>__<unix>.zip`. Dentro del zip todo cuelga de
//! una carpeta raíz con el nombre del mundo.

use crate::config::{now_secs, Instance};
use crate::paths::{sanitize, Paths};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct World {
    pub name: String,
    pub size: u64,
    /// Última modificación (segundos Unix) del `level.dat`.
    pub modified: u64,
}

#[derive(Debug, Clone)]
pub struct Backup {
    pub file: PathBuf,
    /// Nombre (saneado) del mundo según el nombre del fichero.
    pub world: String,
    pub size: u64,
    pub created: u64,
}

pub fn saves_dir(p: &Paths, inst: &Instance) -> PathBuf {
    inst.game_dir(p).join("saves")
}

pub fn backups_dir(p: &Paths, inst: &Instance) -> PathBuf {
    inst.dir(p).join("backups")
}

/// Un nombre de mundo válido es una sola carpeta: sin separadores ni `..`.
fn check_world_name(name: &str) -> Result<()> {
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\\') || name.contains('\0') {
        bail!("nombre de mundo no válido: {name:?}");
    }
    Ok(())
}

/// Tamaño legible (1.5 GB, 320 MB...).
pub fn fmt_size(bytes: u64) -> String {
    const U: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", U[i])
    }
}

fn dir_size(dir: &Path) -> u64 {
    let mut total = 0;
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                total += dir_size(&e.path());
            } else if let Ok(m) = e.metadata() {
                total += m.len();
            }
        }
    }
    total
}

fn mtime_secs(path: &Path) -> u64 {
    std::fs::metadata(path).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0)
}

/// Mundos de la instancia: carpetas de `saves/` que contienen `level.dat`. Más recientes primero.
pub fn list_worlds(p: &Paths, inst: &Instance) -> Vec<World> {
    let mut v = vec![];
    if let Ok(rd) = std::fs::read_dir(saves_dir(p, inst)) {
        for e in rd.flatten() {
            let dir = e.path();
            let level = dir.join("level.dat");
            if dir.is_dir() && level.is_file() {
                v.push(World { name: e.file_name().to_string_lossy().into_owned(), size: dir_size(&dir), modified: mtime_secs(&level) });
            }
        }
    }
    v.sort_by(|a, b| b.modified.cmp(&a.modified).then(a.name.cmp(&b.name)));
    v
}

fn parse_backup_name(file: &Path) -> Option<(String, u64)> {
    let stem = file.file_stem()?.to_str()?;
    let (world, ts) = stem.rsplit_once("__")?;
    Some((world.to_string(), ts.parse().ok()?))
}

/// Copias de la instancia, las más nuevas primero.
pub fn list_backups(p: &Paths, inst: &Instance) -> Vec<Backup> {
    let mut v = vec![];
    if let Ok(rd) = std::fs::read_dir(backups_dir(p, inst)) {
        for e in rd.flatten() {
            let file = e.path();
            if file.extension().and_then(|x| x.to_str()) != Some("zip") {
                continue;
            }
            if let Some((world, created)) = parse_backup_name(&file) {
                let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                v.push(Backup { file, world, size, created });
            }
        }
    }
    v.sort_by_key(|b| std::cmp::Reverse(b.created));
    v
}

fn add_dir(z: &mut zip::ZipWriter<std::fs::File>, opts: zip::write::SimpleFileOptions, dir: &Path, prefix: &str) -> Result<()> {
    use std::io::Write;
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name().to_string_lossy().into_owned();
        let path = e.path();
        let rel = format!("{prefix}/{name}");
        let ft = e.file_type()?;
        if ft.is_dir() {
            z.add_directory(format!("{rel}/"), opts)?;
            add_dir(z, opts, &path, &rel)?;
        } else if ft.is_file() {
            // El juego mantiene session.lock bloqueado mientras corre: no hace falta y en Windows no se puede leer.
            if name == "session.lock" {
                continue;
            }
            z.start_file(&rel, opts)?;
            let mut f = std::fs::File::open(&path).with_context(|| format!("leyendo {}", path.display()))?;
            let mut buf = vec![0u8; 256 * 1024];
            loop {
                use std::io::Read;
                let n = f.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                z.write_all(&buf[..n])?;
            }
        }
    }
    Ok(())
}

/// Crea una copia del mundo. Devuelve la copia creada.
pub fn backup_world(p: &Paths, inst: &Instance, world: &str) -> Result<Backup> {
    check_world_name(world)?;
    let src = saves_dir(p, inst).join(world);
    if !src.join("level.dat").is_file() {
        bail!("'{world}' no es un mundo de esta instancia");
    }
    let out_dir = backups_dir(p, inst);
    std::fs::create_dir_all(&out_dir)?;
    let mut ts = now_secs();
    let mut file = out_dir.join(format!("{}__{ts}.zip", sanitize(world)));
    while file.exists() {
        ts += 1;
        file = out_dir.join(format!("{}__{ts}.zip", sanitize(world)));
    }
    let tmp = file.with_extension("zip.brasa-part");
    let res = (|| -> Result<()> {
        let f = std::fs::File::create(&tmp)?;
        let mut z = zip::ZipWriter::new(f);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        z.add_directory(format!("{world}/"), opts)?;
        add_dir(&mut z, opts, &src, world)?;
        z.finish()?.sync_all()?;
        Ok(())
    })();
    if let Err(e) = res {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    std::fs::rename(&tmp, &file)?;
    let size = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
    Ok(Backup { file, world: sanitize(world), size, created: ts })
}

/// Restaura una copia. Si el mundo ya existe, el actual se aparta como `<mundo>.antes-de-restaurar-<unix>`
/// (nunca se borra). Devuelve el nombre del mundo restaurado.
pub fn restore_backup(p: &Paths, inst: &Instance, backup: &Path) -> Result<String> {
    let f = std::fs::File::open(backup).with_context(|| format!("abriendo {}", backup.display()))?;
    let mut z = zip::ZipArchive::new(f).context("la copia está corrupta")?;

    // El mundo es la carpeta raíz común de todas las entradas.
    let mut world: Option<String> = None;
    for i in 0..z.len() {
        let entry = z.by_index(i)?;
        let Some(path) = entry.enclosed_name() else { bail!("la copia contiene rutas no seguras") };
        let Some(first) = path.components().next().map(|c| c.as_os_str().to_string_lossy().into_owned()) else { continue };
        match &world {
            None => world = Some(first),
            Some(w) if *w == first => {}
            Some(_) => bail!("la copia no tiene un único mundo en la raíz"),
        }
    }
    let world = world.context("la copia está vacía")?;
    check_world_name(&world)?;

    let saves = saves_dir(p, inst);
    std::fs::create_dir_all(&saves)?;
    let target = saves.join(&world);
    // Se extrae primero a una carpeta temporal: si algo falla, el mundo actual queda intacto.
    let staging = saves.join(format!(".{world}.restaurando"));
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    std::fs::create_dir_all(&staging)?;
    let extract = (|| -> Result<()> {
        for i in 0..z.len() {
            let mut entry = z.by_index(i)?;
            let rel = entry.enclosed_name().context("ruta no segura en la copia")?;
            let dest = staging.join(&rel);
            if entry.is_dir() {
                std::fs::create_dir_all(&dest)?;
            } else {
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let mut out = std::fs::File::create(&dest)?;
                std::io::copy(&mut entry, &mut out)?;
            }
        }
        Ok(())
    })();
    if let Err(e) = extract {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }
    let restored = staging.join(&world);
    if !restored.join("level.dat").is_file() {
        let _ = std::fs::remove_dir_all(&staging);
        bail!("la copia no contiene un mundo válido (falta level.dat)");
    }
    if target.exists() {
        let aside = saves.join(format!("{world}.antes-de-restaurar-{}", now_secs()));
        std::fs::rename(&target, &aside).context("no se pudo apartar el mundo actual (¿el juego está abierto?)")?;
    }
    std::fs::rename(&restored, &target)?;
    let _ = std::fs::remove_dir_all(&staging);
    Ok(world)
}

pub fn delete_backup(p: &Paths, inst: &Instance, file: &Path) -> Result<()> {
    // Sólo se borran ficheros que estén dentro de la carpeta de copias de esta instancia.
    let dir = backups_dir(p, inst);
    if file.parent() != Some(dir.as_path()) || file.extension().and_then(|x| x.to_str()) != Some("zip") {
        bail!("ese fichero no es una copia de esta instancia");
    }
    std::fs::remove_file(file)?;
    Ok(())
}

/// Conserva sólo las `keep` copias más nuevas de ese mundo. Devuelve cuántas borró.
pub fn prune(p: &Paths, inst: &Instance, world: &str, keep: usize) -> usize {
    let key = sanitize(world);
    let mut n = 0;
    for b in list_backups(p, inst).into_iter().filter(|b| b.world == key).skip(keep) {
        if std::fs::remove_file(&b.file).is_ok() {
            n += 1;
        }
    }
    n
}

/// Copia de seguridad previa a jugar: sólo de los mundos que cambiaron desde su última copia.
/// Nunca falla el lanzamiento: devuelve las copias hechas y los errores como texto.
pub fn auto_backup(p: &Paths, inst: &Instance, keep: usize) -> (Vec<Backup>, Vec<String>) {
    let mut done = vec![];
    let mut errors = vec![];
    let existing = list_backups(p, inst);
    for w in list_worlds(p, inst) {
        // Los mundos apartados por una restauración ya son una copia en sí mismos.
        if w.name.contains(".antes-de-restaurar-") {
            continue;
        }
        let key = sanitize(&w.name);
        let last = existing.iter().filter(|b| b.world == key).map(|b| b.created).max().unwrap_or(0);
        if w.modified <= last {
            continue;
        }
        match backup_world(p, inst, &w.name) {
            Ok(b) => {
                prune(p, inst, &w.name, keep.max(1));
                done.push(b);
            }
            Err(e) => errors.push(format!("{}: {e:#}", w.name)),
        }
    }
    (done, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_paths() -> (Paths, PathBuf) {
        let root = std::env::temp_dir().join(format!("brasa-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        (Paths { root: root.clone() }, root)
    }

    fn make_instance(p: &Paths) -> Instance {
        Instance { id: "t".into(), name: "t".into(), ..Default::default() }.tap(|i| std::fs::create_dir_all(i.game_dir(p).join("saves")).unwrap())
    }

    trait Tap: Sized {
        fn tap(self, f: impl FnOnce(&Self)) -> Self {
            f(&self);
            self
        }
    }
    impl Tap for Instance {}

    fn make_world(p: &Paths, inst: &Instance, name: &str, data: &str) {
        let w = saves_dir(p, inst).join(name);
        std::fs::create_dir_all(w.join("region")).unwrap();
        std::fs::write(w.join("level.dat"), data).unwrap();
        std::fs::write(w.join("region/r.0.0.mca"), "chunk").unwrap();
        std::fs::write(w.join("session.lock"), "lock").unwrap();
    }

    #[test]
    fn backup_and_restore_roundtrip() {
        let (p, root) = temp_paths();
        let inst = make_instance(&p);
        make_world(&p, &inst, "Mi mundo", "v1");
        assert_eq!(list_worlds(&p, &inst).len(), 1);

        let b = backup_world(&p, &inst, "Mi mundo").unwrap();
        assert!(b.file.exists());
        assert_eq!(list_backups(&p, &inst).len(), 1);

        // Se rompe el mundo y se restaura.
        std::fs::write(saves_dir(&p, &inst).join("Mi mundo/level.dat"), "ROTO").unwrap();
        let name = restore_backup(&p, &inst, &b.file).unwrap();
        assert_eq!(name, "Mi mundo");
        let restored = std::fs::read_to_string(saves_dir(&p, &inst).join("Mi mundo/level.dat")).unwrap();
        assert_eq!(restored, "v1");
        // session.lock no entra en la copia.
        assert!(!saves_dir(&p, &inst).join("Mi mundo/session.lock").exists());
        // El mundo roto se conservó aparte.
        let aside: Vec<_> =
            std::fs::read_dir(saves_dir(&p, &inst)).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().contains("antes-de-restaurar")).collect();
        assert_eq!(aside.len(), 1);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn sizes() {
        assert_eq!(fmt_size(10), "10 B");
        assert_eq!(fmt_size(1536), "1.5 KB");
        assert_eq!(fmt_size(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn rejects_bad_world_names() {
        let (p, root) = temp_paths();
        let inst = make_instance(&p);
        for bad in ["", "..", "../x", "a/b", "a\\b"] {
            assert!(backup_world(&p, &inst, bad).is_err(), "{bad:?}");
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn prune_keeps_newest() {
        let (p, root) = temp_paths();
        let inst = make_instance(&p);
        make_world(&p, &inst, "w", "x");
        for _ in 0..4 {
            backup_world(&p, &inst, "w").unwrap();
        }
        assert_eq!(list_backups(&p, &inst).len(), 4);
        assert_eq!(prune(&p, &inst, "w", 2), 2);
        assert_eq!(list_backups(&p, &inst).len(), 2);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn auto_backup_skips_unchanged() {
        let (p, root) = temp_paths();
        let inst = make_instance(&p);
        make_world(&p, &inst, "w", "x");
        let (done, errs) = auto_backup(&p, &inst, 5);
        assert_eq!((done.len(), errs.len()), (1, 0));
        // Sin cambios: no se repite.
        let (done, _) = auto_backup(&p, &inst, 5);
        assert!(done.is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn zip_slip_is_rejected() {
        use std::io::Write;
        let (p, root) = temp_paths();
        let inst = make_instance(&p);
        let evil = root.join("evil__1.zip");
        {
            let f = std::fs::File::create(&evil).unwrap();
            let mut z = zip::ZipWriter::new(f);
            let opts = zip::write::SimpleFileOptions::default();
            z.start_file("../../escapado.txt", opts).unwrap();
            z.write_all(b"x").unwrap();
            z.finish().unwrap();
        }
        assert!(restore_backup(&p, &inst, &evil).is_err());
        assert!(!root.parent().unwrap().join("escapado.txt").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn delete_only_inside_backups_dir() {
        let (p, root) = temp_paths();
        let inst = make_instance(&p);
        let outside = root.join("otro.zip");
        std::fs::write(&outside, "x").unwrap();
        assert!(delete_backup(&p, &inst, &outside).is_err());
        assert!(outside.exists());
        let _ = std::fs::remove_dir_all(root);
    }
}
