use crate::paths::{sanitize, Paths};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Memoria por defecto (MB) para instancias sin valor propio.
    pub default_memory_mb: u32,
    /// Calcular la memoria de cada instancia según la RAM del equipo, la versión y los mods
    /// (si la instancia no tiene una fijada). Si está desactivado se usa `default_memory_mb`.
    pub auto_memory: bool,
    /// Descargas simultáneas.
    pub concurrency: usize,
    /// Client ID de la app de Azure con permiso para la API de Minecraft.
    /// Dato de distribución (ver [`crate::distro`]): no se muestra en la interfaz ni se guarda si
    /// coincide con el incrustado en el binario.
    #[serde(skip_serializing_if = "is_baked_msa")]
    pub msa_client_id: String,
    /// Mostrar en Discord que se usa Brasa (Rich Presence).
    pub discord_presence: bool,
    /// ID de la aplicación de Discord «Brasa Launcher». Dato de distribución.
    #[serde(skip_serializing_if = "is_baked_discord")]
    pub discord_client_id: String,
    /// Argumentos JVM extra globales.
    pub extra_jvm_args: Vec<String>,
    /// Usar flags de GC optimizados (G1 tuning estilo Aikar adaptado a cliente).
    pub optimized_gc: bool,
    /// Proxy HTTP(S) manual, p.ej. `http://proxy.empresa:8080` (vacío = proxy del sistema / ninguno).
    pub proxy: String,
    /// Fuente de descargas: `auto` (oficial y espejo si falla), `official` o `mirror`.
    pub download_source: String,
    /// API key de CurseForge. Dato de distribución; vacío = CurseForge desactivado.
    #[serde(skip_serializing_if = "is_baked_curseforge")]
    pub curseforge_api_key: String,
    /// Repositorio de GitHub (`usuario/repo`) de las actualizaciones. Dato de distribución.
    #[serde(skip_serializing_if = "is_baked_repo")]
    pub update_repo: String,
    /// Buscar actualizaciones del launcher al arrancar.
    pub check_updates: bool,
    /// Java fijo para todas las instancias (vacío = detectar el del sistema).
    pub java_path: String,
    /// Descargar Java sólo si no hay ninguno compatible instalado.
    pub java_auto_download: bool,
    /// Copia de seguridad automática de los mundos modificados antes de jugar.
    pub auto_backup_worlds: bool,
    /// Cuántas copias automáticas se conservan por mundo.
    pub backups_to_keep: usize,
    /// Idioma de la interfaz: `auto` (el del sistema), `es` o `en`.
    pub language: String,
    /// Ya se vio (o se saltó) la bienvenida del primer arranque.
    pub onboarded: bool,
    /// Brasa P2P: ir siempre por el relé para que los amigos no vean tu IP.
    pub p2p_hide_ip: bool,
}

// No se escriben en settings.toml cuando valen lo incrustado (o nada): así una versión nueva de
// Brasa con otro ID no queda tapada por el valor que guardó la anterior.
fn is_baked_msa(v: &String) -> bool {
    v.trim().is_empty() || v == crate::distro::MSA_CLIENT_ID
}
fn is_baked_discord(v: &String) -> bool {
    v.trim().is_empty() || v == crate::distro::DISCORD_CLIENT_ID
}
fn is_baked_curseforge(v: &String) -> bool {
    v.trim().is_empty() || v == crate::distro::CURSEFORGE_KEY
}
fn is_baked_repo(v: &String) -> bool {
    v.trim().is_empty() || v == crate::distro::UPDATE_REPO
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            default_memory_mb: 4096,
            auto_memory: true,
            concurrency: 48,
            msa_client_id: crate::distro::MSA_CLIENT_ID.to_string(),
            discord_presence: true,
            discord_client_id: crate::distro::DISCORD_CLIENT_ID.to_string(),
            extra_jvm_args: vec![],
            optimized_gc: true,
            proxy: String::new(),
            download_source: "auto".into(),
            curseforge_api_key: crate::distro::CURSEFORGE_KEY.to_string(),
            update_repo: crate::distro::UPDATE_REPO.to_string(),
            check_updates: true,
            java_path: String::new(),
            java_auto_download: true,
            auto_backup_worlds: true,
            backups_to_keep: 5,
            language: "auto".into(),
            onboarded: false,
            p2p_hide_ip: false,
        }
    }
}

impl Settings {
    pub fn load(p: &Paths) -> Settings {
        let mut s = Settings::read_file(p);
        s.resolve_distro(|k| std::env::var(k).ok());
        s
    }

    /// Datos de distribución: variable de entorno > `settings.toml` > incrustado al compilar.
    /// Los dos primeros sólo sirven para desarrollar; un usuario normal recibe lo incrustado.
    fn resolve_distro(&mut self, env: impl Fn(&str) -> Option<String>) {
        use crate::distro as d;
        for (field, var, baked) in [
            (&mut self.msa_client_id, "BRASA_MSA_CLIENT_ID", d::MSA_CLIENT_ID),
            (&mut self.discord_client_id, "BRASA_DISCORD_CLIENT_ID", d::DISCORD_CLIENT_ID),
            (&mut self.curseforge_api_key, "BRASA_CURSEFORGE_KEY", d::CURSEFORGE_KEY),
            (&mut self.update_repo, "BRASA_UPDATE_REPO", d::UPDATE_REPO),
        ] {
            match env(var).map(|v| v.trim().to_string()).filter(|v| !v.is_empty()) {
                Some(v) => *field = v,
                None if field.trim().is_empty() => *field = baked.to_string(),
                None => *field = field.trim().to_string(),
            }
        }
        // el repo se acepta pegado como URL; si lo de settings.toml no vale, manda lo incrustado
        self.update_repo = d::normalize_repo(&self.update_repo);
        if !d::valid_repo(&self.update_repo) && d::valid_repo(&d::normalize_repo(d::UPDATE_REPO)) && env("BRASA_UPDATE_REPO").is_none() {
            self.update_repo = d::normalize_repo(d::UPDATE_REPO);
        }
    }

    /// ¿Esta copia de Brasa puede iniciar sesión con Microsoft?
    pub fn microsoft_available(&self) -> bool {
        !self.msa_client_id.trim().is_empty()
    }
    /// ¿Tiene clave para buscar e instalar desde CurseForge?
    pub fn curseforge_available(&self) -> bool {
        !self.curseforge_api_key.trim().is_empty()
    }
    /// ¿Puede mostrarse en Discord?
    pub fn discord_available(&self) -> bool {
        !self.discord_client_id.trim().is_empty()
    }
    /// ¿Sabe de dónde actualizarse?
    pub fn updates_available(&self) -> bool {
        crate::distro::valid_repo(&self.update_repo)
    }
    /// Lee `settings.toml`. Si existe pero está corrupto, lo aparta como `settings.toml.bak`
    /// (para no perder los ajustes al sobrescribirlo) y avisa por stderr.
    fn read_file(p: &Paths) -> Settings {
        let path = p.settings_file();
        let Ok(txt) = std::fs::read_to_string(&path) else { return Settings::default() };
        match toml::from_str(&txt) {
            Ok(s) => s,
            Err(e) => {
                let bak = path.with_extension("toml.bak");
                let _ = std::fs::copy(&path, &bak);
                eprintln!("[brasa] settings.toml no se pudo leer ({e}); se usan los valores por defecto y se guardó una copia en {}", bak.display());
                Settings::default()
            }
        }
    }
    pub fn save(&self, p: &Paths) -> Result<()> {
        crate::paths::write_atomic(&p.settings_file(), toml::to_string_pretty(self)?)?;
        self.apply_network();
        Ok(())
    }
    pub fn apply_network(&self) {
        crate::net::configure(&self.proxy, &self.download_source);
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Loader {
    #[default]
    Vanilla,
    Fabric,
    Quilt,
    Forge,
    NeoForge,
}

impl Loader {
    pub const ALL: [Loader; 5] = [Loader::Vanilla, Loader::Fabric, Loader::Quilt, Loader::Forge, Loader::NeoForge];
    /// Nombre usado por la API de Modrinth.
    pub fn modrinth_name(&self) -> &'static str {
        match self {
            Loader::Vanilla => "minecraft",
            Loader::Fabric => "fabric",
            Loader::Quilt => "quilt",
            Loader::Forge => "forge",
            Loader::NeoForge => "neoforge",
        }
    }
    /// `modLoaderType` de la API de CurseForge.
    pub fn curseforge_id(&self) -> Option<u32> {
        match self {
            Loader::Vanilla => None,
            Loader::Forge => Some(1),
            Loader::Fabric => Some(4),
            Loader::Quilt => Some(5),
            Loader::NeoForge => Some(6),
        }
    }
    /// Versión del loader a partir del ID instalado (fabric-loader-0.16.5-1.21.1, 1.20.1-forge-47.2.0, neoforge-21.1.77...).
    pub fn version_from_id(&self, id: &str, mc: &str) -> Option<String> {
        let v = match self {
            Loader::Vanilla => return None,
            Loader::Fabric => id.strip_prefix("fabric-loader-")?.strip_suffix(&format!("-{mc}"))?.to_string(),
            Loader::Quilt => id.strip_prefix("quilt-loader-")?.strip_suffix(&format!("-{mc}"))?.to_string(),
            Loader::NeoForge => id.strip_prefix("neoforge-")?.to_string(),
            Loader::Forge => {
                let lower = id.to_ascii_lowercase();
                let idx = lower.find("-forge")?;
                let rest = id[idx + 6..].trim_start_matches('-');
                // 1.7.10-Forge10.13.4.1614-1.7.10 / 1.20.1-forge-47.2.0
                rest.trim_start_matches(mc).trim_start_matches('-').trim_end_matches(&format!("-{mc}")).to_string()
            }
        };
        Some(v).filter(|s| !s.is_empty())
    }
    pub fn label(&self) -> &'static str {
        match self {
            Loader::Vanilla => "Vanilla",
            Loader::Fabric => "Fabric",
            Loader::Quilt => "Quilt",
            Loader::Forge => "Forge",
            Loader::NeoForge => "NeoForge",
        }
    }
    pub fn parse(s: &str) -> Result<Loader> {
        Ok(match s.to_ascii_lowercase().as_str() {
            "vanilla" | "minecraft" => Loader::Vanilla,
            "fabric" => Loader::Fabric,
            "quilt" => Loader::Quilt,
            "forge" => Loader::Forge,
            "neoforge" => Loader::NeoForge,
            o => bail!("loader desconocido: {o}"),
        })
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Instance {
    /// Nombre de carpeta (no se serializa en el toml).
    #[serde(skip)]
    pub id: String,
    pub name: String,
    pub mc_version: String,
    pub loader: Loader,
    pub loader_version: Option<String>,
    /// ID de la versión instalada (p.ej. "fabric-loader-0.16.5-1.21.1"); se rellena al instalar.
    pub resolved_version: Option<String>,
    pub memory_mb: Option<u32>,
    pub java_path: Option<String>,
    pub jvm_args: Vec<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Servidor al que conectarse directamente al arrancar (`host[:puerto]`).
    pub join_server: Option<String>,
    pub playtime_secs: u64,
    pub last_played: Option<u64>,
    /// Arrancar a pantalla completa.
    pub fullscreen: bool,
    /// Variables de entorno extra (`CLAVE=valor`).
    pub env_vars: Vec<String>,
    /// Comando que envuelve al juego (p.ej. `gamemoderun` o `mangohud`).
    pub wrapper: Option<String>,
    /// Comando a ejecutar antes de lanzar el juego (si falla, no se lanza).
    pub pre_launch: Option<String>,
    /// Comando a ejecutar cuando el juego se cierra.
    pub post_exit: Option<String>,
    /// Modo ahorro: antes de lanzar, limita en options.txt lo que más CPU/GPU consume.
    pub saving_mode: bool,
    /// Grupo de la lista de instancias (vacío = sin grupo).
    pub group: Option<String>,
    /// Icono propio: nombre del PNG dentro de la carpeta de la instancia (`icon-….png`).
    pub icon: Option<String>,
}

impl Instance {
    pub fn dir(&self, p: &Paths) -> PathBuf {
        p.instances().join(&self.id)
    }
    pub fn game_dir(&self, p: &Paths) -> PathBuf {
        self.dir(p).join(".minecraft")
    }
    pub fn mods_dir(&self, p: &Paths) -> PathBuf {
        self.game_dir(p).join("mods")
    }

    pub fn create(p: &Paths, name: &str, mc_version: &str, loader: Loader, loader_version: Option<String>) -> Result<Instance> {
        let base = sanitize(name);
        let mut id = base.clone();
        let mut n = 2;
        while p.instances().join(&id).exists() {
            id = format!("{base}-{n}");
            n += 1;
        }
        let inst = Instance { id, name: name.to_string(), mc_version: mc_version.to_string(), loader, loader_version, ..Default::default() };
        for sub in ["mods", "resourcepacks", "shaderpacks", "saves", "config"] {
            std::fs::create_dir_all(inst.game_dir(p).join(sub))?;
        }
        inst.save(p)?;
        Ok(inst)
    }

    pub fn save(&self, p: &Paths) -> Result<()> {
        std::fs::create_dir_all(self.dir(p))?;
        crate::paths::write_atomic(&self.dir(p).join("instance.toml"), toml::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn load(p: &Paths, id: &str) -> Result<Instance> {
        let path = p.instances().join(id).join("instance.toml");
        let txt = std::fs::read_to_string(&path).with_context(|| format!("no existe la instancia '{id}'"))?;
        let mut i: Instance = toml::from_str(&txt).with_context(|| format!("leyendo {}", path.display()))?;
        i.id = id.to_string();
        Ok(i)
    }

    pub fn list(p: &Paths) -> Vec<Instance> {
        let mut v = vec![];
        if let Ok(rd) = std::fs::read_dir(p.instances()) {
            for e in rd.flatten() {
                if let Some(id) = e.file_name().to_str() {
                    if let Ok(i) = Instance::load(p, id) {
                        v.push(i);
                    }
                }
            }
        }
        v.sort_by(|a, b| b.last_played.cmp(&a.last_played).then(a.name.cmp(&b.name)));
        v
    }

    pub fn delete(p: &Paths, id: &str) -> Result<()> {
        let d = p.instances().join(id);
        if !d.join("instance.toml").exists() {
            bail!("no existe la instancia '{id}'");
        }
        std::fs::remove_dir_all(d)?;
        Ok(())
    }

    /// Duplica una instancia completa (mods, mundos, config).
    pub fn clone_to(&self, p: &Paths, new_name: &str) -> Result<Instance> {
        let mut n = Instance::create(p, new_name, &self.mc_version, self.loader, self.loader_version.clone())?;
        crate::paths::copy_dir_all(&self.game_dir(p), &n.game_dir(p))?;
        let id = n.id.clone();
        n = Instance { id, name: new_name.to_string(), playtime_secs: 0, last_played: None, ..self.clone() };
        n.save(p)?;
        Ok(n)
    }
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loader_versions() {
        assert_eq!(Loader::Fabric.version_from_id("fabric-loader-0.16.5-1.21.1", "1.21.1").as_deref(), Some("0.16.5"));
        assert_eq!(Loader::Forge.version_from_id("1.20.1-forge-47.2.0", "1.20.1").as_deref(), Some("47.2.0"));
        assert_eq!(Loader::Forge.version_from_id("1.7.10-Forge10.13.4.1614-1.7.10", "1.7.10").as_deref(), Some("10.13.4.1614"));
        assert_eq!(Loader::NeoForge.version_from_id("neoforge-21.1.77", "1.21.1").as_deref(), Some("21.1.77"));
    }

    fn temp_paths() -> Paths {
        let root = std::env::temp_dir().join(format!("brasa-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        Paths { root }
    }

    #[test]
    fn old_files_get_new_defaults() {
        // Ficheros de la 0.3.x, sin los campos nuevos, deben seguir leyéndose.
        let s: Settings = toml::from_str("default_memory_mb = 2048").unwrap();
        assert_eq!(s.default_memory_mb, 2048);
        assert!(s.auto_backup_worlds);
        assert_eq!(s.backups_to_keep, 5);
        let i: Instance = toml::from_str("name = \"x\"\nmc_version = \"1.21.1\"").unwrap();
        assert!(!i.fullscreen && i.env_vars.is_empty() && i.wrapper.is_none() && i.pre_launch.is_none());
    }

    #[test]
    fn corrupt_settings_are_backed_up() {
        let p = temp_paths();
        std::fs::write(p.settings_file(), "esto no es [toml").unwrap();
        let s = Settings::load(&p);
        assert_eq!(s.default_memory_mb, 4096);
        let bak = p.settings_file().with_extension("toml.bak");
        assert_eq!(std::fs::read_to_string(bak).unwrap(), "esto no es [toml");
        let _ = std::fs::remove_dir_all(&p.root);
    }

    #[test]
    fn settings_roundtrip() {
        let p = temp_paths();
        let s = Settings { backups_to_keep: 9, ..Default::default() };
        s.save(&p).unwrap();
        assert_eq!(Settings::load(&p).backups_to_keep, 9);
        let _ = std::fs::remove_dir_all(&p.root);
    }

    #[test]
    fn distribution_values_never_land_in_settings_toml() {
        let s = Settings::default();
        let txt = toml::to_string_pretty(&s).unwrap();
        for k in ["msa_client_id", "discord_client_id", "curseforge_api_key", "update_repo"] {
            assert!(!txt.contains(k), "{k} no debe guardarse si vale lo incrustado:\n{txt}");
        }
        // un valor puesto a mano para desarrollar sí se conserva
        let dev = Settings { msa_client_id: "id-de-pruebas".into(), ..Default::default() };
        let back: Settings = toml::from_str(&toml::to_string_pretty(&dev).unwrap()).unwrap();
        assert_eq!(back.msa_client_id, "id-de-pruebas");
    }

    #[test]
    fn distribution_precedence() {
        let env = |k: &str| (k == "BRASA_DISCORD_CLIENT_ID").then(|| " 999 ".to_string());
        let mut s =
            Settings { msa_client_id: " de-settings ".into(), discord_client_id: "de-settings".into(), update_repo: String::new(), ..Default::default() };
        s.resolve_distro(env);
        assert_eq!(s.discord_client_id, "999", "el entorno gana");
        assert_eq!(s.msa_client_id, "de-settings", "luego settings.toml (sin espacios)");
        assert_eq!(s.update_repo, crate::distro::UPDATE_REPO, "y si no hay nada, lo incrustado");
        let mut empty_env = Settings { curseforge_api_key: "k".into(), ..Default::default() };
        empty_env.resolve_distro(|_| Some("   ".into()));
        assert_eq!(empty_env.curseforge_api_key, "k", "una variable vacía no cuenta");
    }

    #[test]
    fn availability_flags() {
        let s = Settings {
            msa_client_id: String::new(),
            curseforge_api_key: "x".into(),
            discord_client_id: " ".into(),
            update_repo: "a/b".into(),
            ..Default::default()
        };
        assert!(!s.microsoft_available() && s.curseforge_available() && !s.discord_available() && s.updates_available());
        assert!(!Settings { update_repo: "no-es-un-repo".into(), ..Default::default() }.updates_available());
    }
}
