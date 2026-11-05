//! Estado de la interfaz que no es del dominio: pestañas, formularios, filtros y borradores.
//!
//! Lo escriben las vistas (cada frame, mientras el usuario teclea) y lo lee el controlador
//! para ejecutar acciones (p. ej. «Crear» lee el formulario de nueva instancia).

use crate::config::{Instance, Loader, Settings};
use crate::content::modrinth::ProjectType;
use std::path::PathBuf;

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum Tab {
    Home,
    Instances,
    /// Cosméticos.
    Wardrobe,
    Browse,
    Accounts,
    Settings,
    Logs,
    Import,
    /// Servidores propios para jugar con amigos.
    Hosting,
}

#[derive(PartialEq, Clone, Copy)]
pub enum InstTab {
    Mods,
    Worlds,
    Servers,
    Packs,
    Shaders,
    Shots,
    Settings,
}

pub struct ViewState {
    pub tab: Tab,
    pub inst_tab: InstTab,
    // crear instancia
    pub show_create: bool,
    pub new_name: String,
    pub new_version: String,
    pub new_loader: Loader,
    pub new_loader_version: String,
    pub show_snapshots: bool,
    // borrador de la instancia seleccionada
    pub edit: Option<Instance>,
    pub edit_jvm: String,
    pub edit_java: String,
    pub edit_server: String,
    pub edit_res: bool,
    pub edit_env: String,
    pub edit_wrapper: String,
    pub edit_pre: String,
    pub edit_post: String,
    pub edit_group: String,
    pub confirm_backup_delete: Option<PathBuf>,
    // servidores
    pub new_server_name: String,
    pub new_server_ip: String,
    pub confirm_server_delete: Option<usize>,
    /// Instancia cuyos servidores ya se consultaron al abrir la pestaña.
    pub pinged_for: Option<String>,
    // texturas / shaders / capturas
    pub confirm_content_delete: Option<PathBuf>,
    /// Instancia para la que ya se pidieron las miniaturas de capturas.
    pub thumbs_for: Option<String>,
    /// Instancia cuyo modpack ya se comprobó en esta sesión.
    pub pack_checked_for: Option<String>,
    pub confirm_delete: bool,
    // explorar
    pub query: String,
    pub ptype: ProjectType,
    pub target: Option<String>,
    pub browse_cf: bool,
    // cuentas
    pub offline_name: String,
    pub skin_server: usize,
    pub skin_custom: String,
    pub skin_user: String,
    pub skin_pass: String,
    // ajustes: borrador editable; el modelo lo recibe con `Action::ApplySettings`
    pub settings: Settings,
    pub settings_jvm: String,
    // varios
    pub log_filter: String,
    /// Consola: qué niveles se ven.
    pub log_show: crate::gui::view::logline::Show,
    /// Trazas abiertas (índice absoluto de su primera línea, contando las ya descartadas).
    pub log_open: std::collections::HashSet<usize>,
    pub log_cache: crate::gui::view::logline::Cache,
    /// Cierre inesperado ya descartado (índice absoluto de su línea).
    pub log_crash_dismissed: Option<usize>,
    /// Bajar al final en el próximo fotograma.
    pub log_jump: bool,
    /// Texto legal abierto en su ventana.
    pub notice: Option<crate::gui::action::Notice>,
    /// Ventana de «Informar de un problema».
    pub report_open: bool,
    pub report_text: String,
    pub report_log: bool,
    pub report_preview: bool,
    /// Poner el cursor en la descripción al abrir.
    pub report_focus: bool,
    /// Bienvenida del primer arranque (abierta si `Some`).
    pub onboard: Option<crate::gui::view::onboarding::Onboard>,
    /// Jugar a la instancia de la bienvenida en cuanto termine de instalarse.
    pub play_after_quickstart: bool,
    /// Taquilla de cosméticos.
    pub wardrobe: crate::gui::view::wardrobe::WardrobeView,
    /// Pestaña Servidores.
    pub hosting: crate::gui::view::hosting::HostView,
    pub export_saves: bool,
    pub inst_filter: String,
    pub mod_filter: String,
    /// Aviso correcto en pantalla y cuándo apareció (para cerrarlo solo).
    pub info_seen: Option<(String, f64)>,
    /// Buscador rápido (Ctrl+K).
    pub palette_open: bool,
    pub palette_query: String,
    pub palette_sel: usize,
}

impl ViewState {
    /// Trazas abiertas como índices dentro de las líneas actuales.
    pub fn log_open_rel(&self, dropped: usize) -> std::collections::HashSet<usize> {
        self.log_open.iter().filter_map(|a| a.checked_sub(dropped)).collect()
    }

    pub fn new(settings: &Settings) -> Self {
        ViewState {
            tab: Tab::Home,
            inst_tab: InstTab::Mods,
            show_create: false,
            new_name: String::new(),
            new_version: String::new(),
            new_loader: Loader::Fabric,
            new_loader_version: String::new(),
            show_snapshots: false,
            edit: None,
            edit_jvm: String::new(),
            edit_java: String::new(),
            edit_server: String::new(),
            edit_res: false,
            edit_env: String::new(),
            edit_wrapper: String::new(),
            edit_pre: String::new(),
            edit_post: String::new(),
            edit_group: String::new(),
            confirm_backup_delete: None,
            new_server_name: String::new(),
            new_server_ip: String::new(),
            confirm_server_delete: None,
            pinged_for: None,
            confirm_content_delete: None,
            thumbs_for: None,
            pack_checked_for: None,
            confirm_delete: false,
            query: String::new(),
            palette_open: false,
            palette_query: String::new(),
            palette_sel: 0,
            ptype: ProjectType::Mod,
            target: None,
            browse_cf: false,
            offline_name: String::new(),
            skin_server: 0,
            skin_custom: String::new(),
            skin_user: String::new(),
            skin_pass: String::new(),
            settings: settings.clone(),
            settings_jvm: settings.extra_jvm_args.join(" "),
            log_filter: String::new(),
            log_show: Default::default(),
            log_open: Default::default(),
            log_cache: Default::default(),
            log_crash_dismissed: None,
            log_jump: false,
            notice: None,
            report_open: false,
            report_text: String::new(),
            report_log: true,
            report_preview: false,
            report_focus: false,
            onboard: None,
            play_after_quickstart: false,
            wardrobe: Default::default(),
            hosting: Default::default(),
            export_saves: false,
            inst_filter: String::new(),
            mod_filter: String::new(),
            info_seen: None,
        }
    }

    /// Rellena los campos de edición con los datos de una instancia recién seleccionada.
    pub fn load_instance_drafts(&mut self, i: &Instance) {
        self.edit_jvm = i.jvm_args.join(" ");
        self.edit_java = i.java_path.clone().unwrap_or_default();
        self.edit_server = i.join_server.clone().unwrap_or_default();
        self.edit_res = i.width.is_some();
        self.edit_env = i.env_vars.join("\n");
        self.edit_wrapper = i.wrapper.clone().unwrap_or_default();
        self.edit_pre = i.pre_launch.clone().unwrap_or_default();
        self.edit_post = i.post_exit.clone().unwrap_or_default();
        self.edit_group = i.group.clone().unwrap_or_default();
        self.confirm_backup_delete = None;
        self.edit = Some(i.clone());
    }
}
