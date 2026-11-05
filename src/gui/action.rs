//! Todo lo que el usuario puede pedir. Las vistas las emiten; el controlador las ejecuta.

use crate::config::Settings;
use crate::content::curseforge;
use crate::content::modrinth::{LocalMod, ProjectType};
use std::path::PathBuf;

/// Textos legales incluidos en el binario.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notice {
    License,
    ThirdParty,
    Privacy,
}

impl Notice {
    pub fn file_and_text(self) -> (&'static str, &'static str) {
        match self {
            Notice::License => ("LICENSE.txt", include_str!("../../LICENSE")),
            Notice::ThirdParty => ("THIRD_PARTY_NOTICES.md", include_str!("../../THIRD_PARTY_NOTICES.md")),
            Notice::Privacy => ("PRIVACY.md", include_str!("../../PRIVACY.md")),
        }
    }
}

pub enum Action {
    /// Abrir un texto legal (se escribe en la caché y se abre con el programa del sistema).
    OpenNotice(Notice),
    Select(String),
    ShowCreate,
    Create,
    LoadLoaderVersions,
    Play(String),
    Install(String, bool),
    OpenDir(PathBuf),
    Delete(String),
    Duplicate(String),
    SaveInstance,
    ToggleMod(LocalMod),
    DeleteMod(PathBuf),
    CheckUpdates(String),
    ApplyUpdates(String),
    Search,
    InstallProject(String, ProjectType),
    ImportPack(PathBuf),
    AddJar(PathBuf),
    MsLogin,
    AddOffline,
    SkinLogin,
    SetActive(String),
    RemoveAccount(String),
    SaveSettings,
    Kill(String),
    Perf(String),
    ExportPack(String),
    PickJars,
    PickPack,
    ScanImport,
    ImportFound(usize),
    ImportFolder,
    NetTest,
    CheckUpdate,
    ApplyUpdate,
    SelectCharacter(String, String),
    PickPremiumSkin(bool),
    ResetPremiumSkin,
    PickOfflineSkin,
    ClearOfflineSkin,
    OpenUrl(String),
    RetryManifest,
    CfInstall(u64, curseforge::Class),
    DetectJava,
    PickJava(bool),
    BackupWorld(String, String),
    RestoreBackup(String, PathBuf),
    DeleteBackup(String, PathBuf),
    /// Los ajustes del borrador de la vista pasan al modelo (en memoria, sin guardar).
    ApplySettings(Settings),
    DismissError,
    /// Copiar texto al portapapeles y avisar con ese mensaje.
    Copy(String, String),
    /// Vaciar la consola.
    ClearLogs,
    /// Subir el informe de un problema a mclo.gs (descripción, ¿incluir el registro?).
    ReportProblem(String, bool),
    // ---- servidores propios
    HostCreate,
    HostStart(String),
    HostStop(String),
    HostKill(String),
    HostCommand(String, String),
    HostSave(String, crate::gui::view::hosting::Draft),
    HostDelete(String),
    HostSearch(String, String),
    HostInstall(String, String),
    HostRemove(String, String),
    /// Instancia para jugar en ese servidor desde este equipo.
    HostJoin(String),
    HostSetPlayit(String, bool),
    PlayitLink,
    // ---- Brasa P2P
    /// (servidor, nombre, horas)
    P2pInvite(String, String, u64),
    /// Olvidar el último código creado (al abrir la ventana otra vez).
    P2pInviteReset,
    P2pCancelInvite(String, String),
    /// (servidor, clave del amigo)
    P2pRevoke(String, String),
    P2pDecide(u64, bool),
    HostSetGatekeeper(String, bool),
    SetHideIp(bool),
    P2pJoin(String),
    P2pJoinReset,
    /// Clave del servidor de amigo (`host/servidor`).
    P2pPlay(String),
    P2pForget(String),
    PlayitCancel,
    PlayitUnlink,
    /// Cerrar el informe y olvidar el resultado.
    DismissReport,
    /// Bienvenida: crear la primera instancia (¿con mods de rendimiento?) e instalarla.
    QuickStart(bool),
    /// Bienvenida terminada o saltada: no se vuelve a mostrar.
    FinishOnboarding,
    /// Lo que lleva puesto una cuenta (clave: UUID).
    SetLoadout(String, crate::cosmetics::loadout::Loadout),
    /// Consulta el estado de todos los servidores de la instancia.
    PingServers(String),
    AddServer(String, String, String),
    RemoveServer(String, usize),
    /// Lanza la instancia entrando directamente a ese servidor.
    JoinServer(String, String),
    /// Abre el selector para añadir texturas, shaders o capturas.
    AddContent(String, crate::content::resources::Kind),
    DeleteContent(String, crate::content::resources::Kind, PathBuf),
    TogglePack(String, String, bool),
    SetShader(String, Option<String>),
    MakeThumbs(String),
    /// Orden de la lista de instancias.
    SetSort(crate::instance::organize::Sort),
    /// Plegar o desplegar un grupo de la lista.
    ToggleGroup(String),
    /// Elegir una imagen como icono de la instancia.
    PickIcon(String),
    /// Volver al avatar generado.
    RemoveIcon(String),
    CheckPackUpdate(String),
    UpdatePack(String),
    OpenFile(PathBuf),
    DismissInfo,
    CopyError(String),
}
