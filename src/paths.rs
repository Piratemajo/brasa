//! Rutas del launcher. Estructura:
//!
//! ```text
//! <data>/
//!   settings.toml, accounts.json
//!   shared/            (layout compatible con .minecraft: versions/, libraries/, assets/)
//!   runtimes/java-N/   (Java descargado automáticamente)
//!   instances/<id>/    (instance.toml + .minecraft/ con mods, saves, config...)
//!   cache/
//! ```

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Paths {
    pub root: PathBuf,
}

impl Paths {
    pub fn new() -> Result<Self> {
        let root = if let Ok(p) = std::env::var("BRASA_HOME") {
            PathBuf::from(p)
        } else {
            directories::ProjectDirs::from("dev", "brasa", "brasa").context("no se pudo determinar el directorio de datos")?.data_dir().to_path_buf()
        };
        let p = Paths { root };
        for d in [p.shared(), p.libraries(), p.versions(), p.assets(), p.instances(), p.runtimes(), p.cache()] {
            std::fs::create_dir_all(&d).with_context(|| format!("creando {}", d.display()))?;
        }
        Ok(p)
    }
    pub fn shared(&self) -> PathBuf {
        self.root.join("shared")
    }
    pub fn libraries(&self) -> PathBuf {
        self.shared().join("libraries")
    }
    pub fn versions(&self) -> PathBuf {
        self.shared().join("versions")
    }
    pub fn assets(&self) -> PathBuf {
        self.shared().join("assets")
    }
    pub fn instances(&self) -> PathBuf {
        self.root.join("instances")
    }
    pub fn runtimes(&self) -> PathBuf {
        self.root.join("runtimes")
    }
    pub fn cache(&self) -> PathBuf {
        self.root.join("cache")
    }
    pub fn settings_file(&self) -> PathBuf {
        self.root.join("settings.toml")
    }
    pub fn accounts_file(&self) -> PathBuf {
        self.root.join("accounts.json")
    }
    pub fn version_json(&self, id: &str) -> PathBuf {
        self.versions().join(id).join(format!("{id}.json"))
    }
    pub fn version_jar(&self, id: &str) -> PathBuf {
        self.versions().join(id).join(format!("{id}.jar"))
    }
}

/// Convierte una coordenada Maven (`grupo:artefacto:versión[:clasificador][@ext]`) en ruta relativa.
pub fn maven_path(coord: &str) -> Option<String> {
    let (coord, ext) = match coord.split_once('@') {
        Some((c, e)) => (c, e),
        None => (coord, "jar"),
    };
    let parts: Vec<&str> = coord.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let file = if parts.len() >= 4 { format!("{artifact}-{version}-{}.{ext}", parts[3]) } else { format!("{artifact}-{version}.{ext}") };
    Some(format!("{group}/{artifact}/{version}/{file}"))
}

/// Nombre de carpeta seguro a partir de un nombre libre.
pub fn sanitize(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '_' }).collect();
    let s = s.trim_matches('.').to_string();
    if s.is_empty() {
        "instancia".into()
    } else {
        s
    }
}

/// Escribe un fichero de forma atómica: primero a un temporal en la misma carpeta y luego lo renombra,
/// de modo que un corte a mitad de escritura nunca deja el fichero corrupto.
pub fn write_atomic(path: &Path, data: impl AsRef<[u8]>) -> std::io::Result<()> {
    write_atomic_inner(path, data.as_ref(), false)
}

/// Como [`write_atomic`], pero el fichero nace con permisos 600 en Unix (para tokens y secretos).
pub fn write_atomic_private(path: &Path, data: impl AsRef<[u8]>) -> std::io::Result<()> {
    write_atomic_inner(path, data.as_ref(), true)
}

fn write_atomic_inner(path: &Path, data: &[u8], private: bool) -> std::io::Result<()> {
    use std::io::Write;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "tmp".into());
    // nombre único: dos guardados a la vez del mismo fichero no se pisan el temporal
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = path.with_file_name(format!(".{name}.{}-{n}.tmp", std::process::id()));
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    if private {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    #[cfg(not(unix))]
    let _ = private;
    {
        let mut f = opts.open(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

pub fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maven() {
        assert_eq!(maven_path("net.fabricmc:fabric-loader:0.16.5").unwrap(), "net/fabricmc/fabric-loader/0.16.5/fabric-loader-0.16.5.jar");
        assert_eq!(maven_path("org.lwjgl:lwjgl:3.3.3:natives-linux").unwrap(), "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-linux.jar");
        assert_eq!(maven_path("a.b:c:1@zip").unwrap(), "a/b/c/1/c-1.zip");
    }

    #[test]
    fn atomic_write_replaces_and_leaves_no_temp() {
        let dir = std::env::temp_dir().join(format!("brasa-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("x.toml");
        write_atomic(&f, "uno").unwrap();
        write_atomic(&f, "dos").unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "dos");
        let names: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(names, vec!["x.toml"]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            write_atomic_private(&dir.join("t.json"), "{}").unwrap();
            assert_eq!(std::fs::metadata(dir.join("t.json")).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
