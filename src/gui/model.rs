//! Modelo: los datos de la aplicación. No sabe nada de egui ni de cómo se dibujan.
//!
//! - [`Model`]: lo que está en disco (instancias, ajustes, cuentas) y lo derivado de la instancia elegida.
//! - [`Session`]: resultados de las tareas asíncronas (progreso, logs, búsquedas...). Se comparte con
//!   los hilos de fondo mediante `Arc<Mutex<_>>`.

use crate::account::auth::AccountStore;
use crate::config::{Instance, Settings};
use crate::content::curseforge::{self, CfMod};
use crate::content::modrinth::{self, Hit, LocalMod, ProjectType, Update};
use crate::game::mojang::ManifestVersion;
use crate::instance::importers::Found;
use crate::instance::worlds;
use crate::paths::Paths;
use crate::updater::Release;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

/// La primera instancia que prepara la bienvenida.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QuickStart {
    pub perf: bool,
    /// Se conoce cuando ya está creada.
    pub id: Option<String>,
    pub name: String,
    pub mc: String,
    /// Mods de rendimiento instalados.
    pub mods: usize,
    pub done: bool,
    pub error: Option<String>,
}

/// Estado del informe de «Informar de un problema».
#[derive(Debug, Clone, PartialEq)]
pub enum ReportState {
    Uploading,
    Done(String),
    Failed(String),
}

/// Estado que escriben las tareas en segundo plano y lee la interfaz.
#[derive(Default)]
pub struct Session {
    /// Estado de la conexión con Discord (Rich Presence).
    pub discord: crate::discord::Status,
    pub tasks: usize,
    pub stage: String,
    pub progress: Option<(usize, usize)>,
    pub logs: Vec<String>,
    /// Líneas tiradas por el principio (límite o «Limpiar»): la vista lo usa para su caché.
    pub logs_dropped: usize,
    pub error: Option<String>,
    pub info: Option<String>,
    pub manifest: Vec<ManifestVersion>,
    pub latest_release: String,
    pub loader_versions: Vec<String>,
    pub hits: Vec<Hit>,
    pub updates: Vec<Update>,
    pub device_code: Option<(String, String)>,
    /// Instancias lanzadas desde que se pulsa JUGAR hasta que el juego se cierra.
    pub playing: HashSet<String>,
    /// Detalle de cada una: preparándose (fase y progreso) o ya en juego.
    pub launches: std::collections::HashMap<String, Launch>,
    /// Nombre, versión, autores e icono de cada mod, leídos de su jar (por ruta del fichero).
    pub mod_meta: std::collections::HashMap<std::path::PathBuf, crate::content::modmeta::ModMeta>,
    /// Estado de los servidores, por dirección.
    pub pings: std::collections::HashMap<String, PingState>,
    /// Comprobación de versión nueva del modpack, por instancia.
    pub pack_checks: std::collections::HashMap<String, PackCheck>,
    pub refresh: bool,
    /// Servidores: resultados de búsqueda de plugins/mods.
    pub host_hits: Vec<crate::hosting::content::Hit>,
    pub host_searching: bool,
    /// Enlace de playit.gg mientras se espera la aprobación.
    pub playit_claim: Option<String>,
    pub playit_cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
    /// Brasa P2P: código recién creado para la invitación abierta.
    pub p2p_code: Option<String>,
    pub p2p_inviting: bool,
    /// «Unirse a un amigo»: paso actual y error.
    pub p2p_join: Option<(crate::hosting::p2p::JoinStep, Option<String>)>,
    /// Lanzar esta instancia contra esta dirección (cuando el túnel está listo).
    pub p2p_launch: Option<(String, String)>,
    /// Avisos de mods del servidor del amigo que no se pudieron poner.
    pub p2p_missing: Vec<String>,
    pub cf_hits: Vec<CfMod>,
    pub net_report: Option<String>,
    /// «Informar de un problema»: subiendo, enlace o error.
    pub report: Option<ReportState>,
    pub quickstart: Option<QuickStart>,
    pub update: Option<Release>,
    pub manifest_error: bool,
    pub javas: Option<Vec<crate::game::java::JavaInstall>>,
}

/// Estado de un lanzamiento.
#[derive(Debug, Clone, PartialEq)]
pub enum Launch {
    /// Descargando, instalando o preparando Java.
    Preparing { stage: String, progress: Option<(usize, usize)> },
    /// El proceso del juego está en marcha.
    Running { since: std::time::Instant },
}

impl Session {
    pub fn launch(&self, id: &str) -> Option<&Launch> {
        self.launches.get(id)
    }
    pub fn is_running(&self, id: &str) -> bool {
        matches!(self.launches.get(id), Some(Launch::Running { .. }))
    }
    pub fn any_preparing(&self) -> bool {
        self.launches.values().any(|l| matches!(l, Launch::Preparing { .. }))
    }
    pub fn any_running(&self) -> bool {
        self.launches.values().any(|l| matches!(l, Launch::Running { .. }))
    }
    /// Tareas en segundo plano que no son lanzamientos (instalar, importar, buscar…).
    pub fn background_tasks(&self) -> usize {
        self.tasks.saturating_sub(self.launches.len())
    }
}

#[derive(Debug, Clone)]
pub enum PingState {
    Pending,
    Ok(crate::instance::servers::Status),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum PackCheck {
    Checking,
    UpToDate,
    Available(String),
    Updating,
    Failed(String),
}

pub struct Model {
    pub p: Paths,
    pub instances: Vec<Instance>,
    pub selected: Option<String>,
    pub mods: Vec<LocalMod>,
    pub settings: Settings,
    pub accounts: AccountStore,
    /// Cosméticos de cada cuenta (`cosmetics.toml`).
    pub wardrobe: crate::cosmetics::loadout::Wardrobe,
    pub worlds: Vec<worlds::World>,
    pub backups: Vec<worlds::Backup>,
    pub servers: Vec<crate::instance::servers::Server>,
    pub packs: Vec<crate::content::resources::Item>,
    pub shaders: Vec<crate::content::resources::Item>,
    pub shots: Vec<crate::content::resources::Item>,
    /// Texturas activas en options.txt (nombres de fichero).
    pub enabled_packs: Vec<String>,
    pub active_shader: Option<String>,
    /// Modpack del que viene la instancia (si se instaló desde Modrinth/CurseForge).
    pub pack: Option<crate::content::modpacks::PackInfo>,
    pub importable: Vec<Found>,
    /// Orden y grupos plegados de la lista de instancias (`ui.toml`).
    pub ui: crate::instance::organize::UiPrefs,
    /// RAM física del equipo (para la memoria automática). `None` si no se pudo detectar.
    pub total_ram_mb: Option<u64>,
    /// Servidores propios (pestaña Servidores).
    pub hosted: Vec<crate::hosting::Server>,
    /// ¿Hay clave de playit.gg guardada?
    pub playit_linked: bool,
    /// Servidores de amigos a los que me he unido (Brasa P2P).
    pub joined: Vec<crate::hosting::p2p::Joined>,
    pub session: Arc<Mutex<Session>>,
}

impl Model {
    pub fn load(p: Paths) -> Model {
        Model {
            instances: Instance::list(&p),
            selected: None,
            mods: vec![],
            settings: Settings::load(&p),
            accounts: AccountStore::load(&p),
            wardrobe: crate::cosmetics::loadout::Wardrobe::load(&p),
            worlds: vec![],
            backups: vec![],
            servers: vec![],
            packs: vec![],
            shaders: vec![],
            shots: vec![],
            enabled_packs: vec![],
            active_shader: None,
            pack: None,
            importable: vec![],
            ui: crate::instance::organize::UiPrefs::load(&p),
            total_ram_mb: crate::game::perf::total_ram_mb(),
            hosted: crate::hosting::Server::list(&p),
            playit_linked: crate::hosting::playit::linked(&p),
            joined: crate::hosting::p2p::joined(&p),
            session: Arc::new(Mutex::new(Session::default())),
            p,
        }
    }

    /// URI del icono propio de una instancia, si tiene (para `egui::Image`).
    pub fn icon_uri(&self, i: &Instance) -> Option<String> {
        let name = i.icon.as_deref().filter(|n| crate::instance::organize::valid_icon_name(n))?;
        let path = i.dir(&self.p).join(name);
        path.is_file().then(|| format!("file://{}", path.display()))
    }

    pub fn selected_instance(&self) -> Option<&Instance> {
        let id = self.selected.as_ref()?;
        self.instances.iter().find(|i| &i.id == id)
    }

    /// Vuelve a leer del disco mods, mundos y copias de la instancia seleccionada.
    pub fn load_selected_lists(&mut self) {
        if let Some(i) = self.selected_instance().cloned() {
            self.mods = modrinth::list_mods(&self.p, &i);
            self.worlds = worlds::list_worlds(&self.p, &i);
            self.backups = worlds::list_backups(&self.p, &i);
            use crate::content::resources::{self, Kind};
            let g = i.game_dir(&self.p);
            self.servers = crate::instance::servers::load(&g).unwrap_or_default();
            self.packs = resources::list(&g, Kind::ResourcePack);
            self.shaders = resources::list(&g, Kind::Shader);
            self.shots = resources::list(&g, Kind::Screenshot);
            self.enabled_packs = resources::enabled_packs(&std::fs::read_to_string(g.join("options.txt")).unwrap_or_default());
            self.active_shader = resources::active_shader(&g);
            self.pack = crate::content::modpacks::PackInfo::load(&i.dir(&self.p));
        }
    }

    /// Relee instancias y cuentas. Devuelve la instancia seleccionada si sigue existiendo;
    /// si ya no existe, deja de haber selección.
    pub fn reload_from_disk(&mut self) -> Option<Instance> {
        self.instances = Instance::list(&self.p);
        self.accounts = AccountStore::load(&self.p);
        self.hosted = crate::hosting::Server::list(&self.p);
        self.playit_linked = crate::hosting::playit::linked(&self.p);
        self.joined = crate::hosting::p2p::joined(&self.p);
        let id = self.selected.clone()?;
        match self.instances.iter().find(|i| i.id == id).cloned() {
            Some(i) => {
                self.load_selected_lists();
                Some(i)
            }
            None => {
                self.selected = None;
                None
            }
        }
    }
}

pub fn cf_class_of(t: ProjectType) -> curseforge::Class {
    match t {
        ProjectType::Mod => curseforge::Class::Mod,
        ProjectType::ResourcePack => curseforge::Class::ResourcePack,
        ProjectType::ShaderPack => curseforge::Class::Shader,
        ProjectType::Modpack => curseforge::Class::Modpack,
    }
}
