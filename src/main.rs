#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

// primero: la macro t! la usan todos los demás módulos
#[macro_use]
mod i18n;
mod account;
mod actions;
mod config;
mod content;
mod cosmetics;
mod discord;
mod distro;
mod game;
mod gui;
mod hosting;
mod instance;
mod net;
mod paths;
mod progress;
mod report;
mod setup;
mod updater;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use config::{Instance, Loader, Settings};
use content::modrinth::ProjectType;
use paths::Paths;
use progress::Reporter;

#[derive(Parser)]
#[command(name = "brasa", version, about = "Launcher de Minecraft rápido con soporte de mods. Sin argumentos abre la interfaz gráfica.")]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Instala Brasa para este usuario (el asistente de instalación)
    Setup,
    /// Desinstala Brasa (con ventana; --quiet sin ella)
    Uninstall {
        #[arg(long)]
        quiet: bool,
        /// Borrar también instancias, mundos, ajustes y cuentas
        #[arg(long)]
        remove_data: bool,
    },
    /// Lista versiones de Minecraft
    Versions {
        #[arg(long)]
        snapshots: bool,
    },
    /// Lista versiones de un loader (fabric, quilt, forge, neoforge) para una versión de MC
    Loaders { loader: String, mc: String },
    /// Crea una instancia
    Create {
        name: String,
        #[arg(short, long)]
        version: Option<String>,
        #[arg(short, long, default_value = "vanilla")]
        loader: String,
        #[arg(long)]
        loader_version: Option<String>,
        #[arg(short, long)]
        memory: Option<u32>,
    },
    /// Lista instancias
    List,
    /// Borra una instancia
    Delete { id: String },
    /// Duplica una instancia
    Clone { id: String, new_name: String },
    /// Descarga todo lo necesario sin lanzar
    Install { id: String },
    /// Re-verifica todos los ficheros por SHA-1 y repara lo corrupto
    Repair { id: String },
    /// Lanza una instancia
    Launch {
        id: String,
        /// Conectar directamente a un servidor (`host[:puerto]`)
        #[arg(long)]
        server: Option<String>,
    },
    /// Inicia sesión con Microsoft (device code)
    Login,
    /// Añade una cuenta no premium (offline, sólo nombre)
    Offline { name: String },
    /// Cuenta no premium con skin: Ely.by, LittleSkin o servidor authlib-injector propio
    Skin {
        /// Usuario o email de esa web
        username: String,
        /// ely | littleskin | URL de la API
        #[arg(short, long, default_value = "ely")]
        server: String,
    },
    /// Lista cuentas
    Accounts,
    /// Selecciona la cuenta activa
    Use { name: String },
    /// Elimina una cuenta
    Logout { name: String },
    /// Busca en Modrinth
    Search {
        query: String,
        #[arg(short, long, default_value = "mod")]
        r#type: String,
        /// Filtra por la versión/loader de esta instancia
        #[arg(short, long)]
        instance: Option<String>,
    },
    /// Instala un mod/resourcepack/shader de Modrinth (con dependencias)
    Add {
        instance: String,
        project: String,
        #[arg(short, long, default_value = "mod")]
        r#type: String,
    },
    /// Lista mods de una instancia
    Mods { instance: String },
    /// Activa/desactiva un mod
    Toggle { instance: String, file: String },
    /// Busca y aplica actualizaciones de mods
    Update {
        instance: String,
        #[arg(long)]
        dry_run: bool,
    },
    /// Importa un modpack .mrpack como nueva instancia
    Import { file: std::path::PathBuf },
    /// Instala un modpack de Modrinth por slug/ID como nueva instancia
    Modpack { project: String },
    /// Abre la carpeta de una instancia
    Open { id: String },
    /// Muestra/edita ajustes (clave=valor). msa_client_id, discord_client_id, curseforge_api_key y
    /// update_repo sólo sirven para desarrollar: las copias distribuidas los traen incrustados.
    Config { set: Option<String> },
    /// Diagnóstico de conexión con todos los servicios
    NetTest,
    /// Lista los Java instalados que detecta Brasa
    Java,
    /// Cierra a la fuerza el juego de una instancia (si se lanzó desde esta misma sesión)
    Kill { id: String },
    /// Cambia de personaje en una cuenta Ely.by/LittleSkin con varios
    Character { name: String },
    /// Sube una skin a la cuenta premium activa
    SkinUpload {
        file: std::path::PathBuf,
        #[arg(long)]
        slim: bool,
    },
    /// Quita la skin de la cuenta premium activa
    SkinReset,
    /// Skin local para la cuenta offline activa (sólo la ves tú)
    OfflineSkin {
        /// PNG 64x64; vacío para quitarla
        file: Option<std::path::PathBuf>,
        #[arg(long)]
        slim: bool,
    },
    /// Busca en CurseForge
    CfSearch {
        query: String,
        #[arg(short, long, default_value = "mod")]
        r#type: String,
        #[arg(short, long)]
        instance: Option<String>,
    },
    /// Instala un mod de CurseForge (por ID numérico) con dependencias
    CfAdd {
        instance: String,
        id: u64,
        #[arg(short, long, default_value = "mod")]
        r#type: String,
    },
    /// Importa un modpack de CurseForge (.zip) o lo instala por ID
    CfModpack { file_or_id: String },
    /// Lista instancias importables de Prism/MultiMC y del launcher oficial
    Importable,
    /// Importa una instancia de Prism/MultiMC (carpeta) o un perfil del launcher oficial (por nombre)
    ImportFrom { source: String },
    /// Exporta una instancia como .mrpack
    Export {
        id: String,
        file: std::path::PathBuf,
        #[arg(long)]
        saves: bool,
    },
    /// Instala el perfil de rendimiento (Sodium, Lithium, FerriteCore...)
    Perf { id: String },
    /// Actualiza el modpack de una instancia a su última versión (copia los mundos antes)
    UpdatePack { id: String },
    /// Lista los mundos de una instancia y sus copias de seguridad
    Worlds { id: String },
    /// Crea una copia de seguridad de un mundo (o de todos si no se indica)
    Backup { id: String, world: Option<String> },
    /// Restaura una copia (nombre del .zip que muestra `worlds`); el mundo actual se conserva aparte
    Restore { id: String, backup: String },
    /// Busca y aplica actualizaciones del launcher
    SelfUpdate {
        #[arg(long)]
        check: bool,
    },
}

fn cf_class(t: &str) -> content::curseforge::Class {
    match t {
        "resourcepack" => content::curseforge::Class::ResourcePack,
        "shader" | "shaderpack" => content::curseforge::Class::Shader,
        "modpack" => content::curseforge::Class::Modpack,
        _ => content::curseforge::Class::Mod,
    }
}

fn main() -> Result<()> {
    // Brasa-Setup-….exe: el mismo programa hace de instalador
    if setup::launched_as_setup() && std::env::args().len() == 1 {
        return setup::ui::run(setup::ui::Mode::Install);
    }
    let cli = Cli::parse();
    match &cli.cmd {
        Some(Cmd::Setup) => return setup::ui::run(setup::ui::Mode::Install),
        Some(Cmd::Uninstall { quiet: false, .. }) => return setup::ui::run(setup::ui::Mode::Uninstall),
        Some(Cmd::Uninstall { quiet: true, remove_data }) => return setup::ui::quiet_uninstall(*remove_data),
        _ => {}
    }
    let p = Paths::new()?;
    let settings = Settings::load(&p);
    settings.apply_network();
    i18n::apply(&settings.language);
    updater::cleanup();
    match cli.cmd {
        None => gui::run(p),
        Some(cmd) => {
            // Binario sin consola (GUI): engancharse a la terminal padre para que la CLI imprima.
            #[cfg(windows)]
            #[allow(unsafe_code)]
            // SAFETY: llamada sin punteros; si no hay consola padre, simplemente devuelve 0.
            unsafe {
                windows_sys::Win32::System::Console::AttachConsole(windows_sys::Win32::System::Console::ATTACH_PARENT_PROCESS);
            }
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(run_cli(p, cmd))
        }
    }
}

async fn run_cli(p: Paths, cmd: Cmd) -> Result<()> {
    let client = net::client();
    let rep = Reporter::console();
    let s = Settings::load(&p);
    match cmd {
        // ya atendidos en main(), antes de abrir nada
        Cmd::Setup | Cmd::Uninstall { .. } => {}
        Cmd::Versions { snapshots } => {
            let m = game::mojang::manifest(&client, &p).await?;
            println!("Última release: {}   Última snapshot: {}", m.latest.release, m.latest.snapshot);
            for v in m.versions.iter().filter(|v| snapshots || v.kind == "release").take(60) {
                println!("  {:<24} {}", v.id, v.kind);
            }
        }
        Cmd::Loaders { loader, mc } => {
            let l = Loader::parse(&loader)?;
            let (list, stale) = game::loaders::versions_cached(&client, &p, l, &mc).await?;
            if stale {
                eprintln!("(el servidor de {} no responde: lista guardada)", l.label());
            }
            for v in list.iter().take(30) {
                println!("  {v}");
            }
        }
        Cmd::Create { name, version, loader, loader_version, memory } => {
            let mc = match version {
                Some(v) => v,
                None => game::mojang::manifest(&client, &p).await?.latest.release,
            };
            let mut i = Instance::create(&p, &name, &mc, Loader::parse(&loader)?, loader_version)?;
            i.memory_mb = memory;
            i.save(&p)?;
            println!("Instancia creada: {} ({} {})", i.id, i.loader.label(), i.mc_version);
        }
        Cmd::List => {
            for i in Instance::list(&p) {
                println!("  {:<24} {:<10} {:<10} jugado {}", i.id, i.loader.label(), i.mc_version, actions::fmt_duration(i.playtime_secs));
            }
        }
        Cmd::Delete { id } => {
            Instance::delete(&p, &id)?;
            println!("Borrada {id}");
        }
        Cmd::Clone { id, new_name } => {
            let n = Instance::load(&p, &id)?.clone_to(&p, &new_name)?;
            println!("Copiada a {}", n.id);
        }
        Cmd::Install { id } => actions::install_only(&client, &p, &id, false, &rep).await?,
        Cmd::Repair { id } => actions::install_only(&client, &p, &id, true, &rep).await?,
        Cmd::Launch { id, server } => {
            if let Some(sv) = server {
                let mut i = Instance::load(&p, &id)?;
                i.join_server = Some(sv);
                i.save(&p)?;
            }
            // También desde la terminal: Discord muestra la partida mientras dure.
            let presence = discord::Presence::start(discord::Config::from_settings(&s), |_| {});
            if let Ok(i) = Instance::load(&p, &id) {
                presence.start_playing(&id, discord::Playing::of(&i));
            }
            let code = actions::play(&client, &p, &id, false, &rep).await?;
            std::process::exit(code);
        }
        Cmd::Login => {
            let dc = account::auth::start_device_code(&client, &s.msa_client_id).await?;
            println!("{}", dc.message);
            let _ = open::that(&dc.verification_uri);
            let acc = account::auth::poll_device_code(&client, &s.msa_client_id, &dc).await?;
            let mut st = account::auth::AccountStore::load(&p);
            println!("Sesión iniciada como {} ({})", acc.username, acc.label());
            st.upsert(acc);
            st.save(&p)?;
        }
        Cmd::Offline { name } => {
            let acc = account::auth::Account::offline(&name)?;
            let mut st = account::auth::AccountStore::load(&p);
            st.upsert(acc);
            st.save(&p)?;
            println!("Cuenta offline '{name}' activa");
        }
        Cmd::Skin { username, server } => {
            let pass = rpassword::prompt_password(format!("Contraseña de {username}: "))?;
            let acc = account::auth::yggdrasil_login(&client, &server, &username, &pass).await?;
            println!("Sesión iniciada como {} ({})", acc.username, acc.label());
            let mut st = account::auth::AccountStore::load(&p);
            st.upsert(acc);
            st.save(&p)?;
        }
        Cmd::Accounts => {
            let st = account::auth::AccountStore::load(&p);
            for a in &st.accounts {
                let mark = if st.active.as_deref() == Some(a.uuid.as_str()) { "*" } else { " " };
                println!(" {mark} {:<16} {:<28} {}", a.username, a.label(), a.uuid);
            }
        }
        Cmd::Use { name } => {
            let mut st = account::auth::AccountStore::load(&p);
            st.set_active(&name)?;
            st.save(&p)?;
        }
        Cmd::Logout { name } => {
            let mut st = account::auth::AccountStore::load(&p);
            st.remove(&name);
            st.save(&p)?;
        }
        Cmd::Search { query, r#type, instance } => {
            let inst = instance.map(|i| Instance::load(&p, &i)).transpose()?;
            let hits = content::modrinth::search(
                &client,
                &query,
                ProjectType::parse(&r#type),
                inst.as_ref().map(|i| i.mc_version.as_str()),
                inst.as_ref().map(|i| i.loader),
            )
            .await?;
            for h in hits {
                println!("  {:<28} {:>10} desc.  {}", h.slug, h.downloads, h.title);
            }
        }
        Cmd::Add { instance, project, r#type } => {
            let inst = Instance::load(&p, &instance)?;
            let files = content::modrinth::install_project(&client, &p, &inst, &project, ProjectType::parse(&r#type), &rep).await?;
            println!("Instalado: {}", files.join(", "));
        }
        Cmd::Mods { instance } => {
            let inst = Instance::load(&p, &instance)?;
            for m in content::modrinth::list_mods(&p, &inst) {
                println!("  [{}] {}", if m.enabled { "x" } else { " " }, m.file_name);
            }
        }
        Cmd::Toggle { instance, file } => {
            let inst = Instance::load(&p, &instance)?;
            let m = content::modrinth::list_mods(&p, &inst).into_iter().find(|m| m.file_name.contains(&file)).context("mod no encontrado")?;
            content::modrinth::toggle_mod(&m)?;
            println!("{} {}", if m.enabled { "Desactivado" } else { "Activado" }, m.file_name);
        }
        Cmd::Update { instance, dry_run } => {
            let inst = Instance::load(&p, &instance)?;
            let ups = content::modrinth::check_updates(&client, &p, &inst).await?;
            if ups.is_empty() {
                println!("Todo al día");
            }
            for u in &ups {
                println!("  {} → {}", u.old_path.file_name().unwrap_or_default().to_string_lossy(), u.new_version.version_number);
            }
            if !dry_run && !ups.is_empty() {
                let n = content::modrinth::apply_updates(&client, &p, &inst, &ups, &rep).await?;
                println!("{n} mods actualizados");
            }
        }
        Cmd::Import { file } => {
            let i = content::modrinth::import_mrpack(&client, &p, &file, s.concurrency, &rep).await?;
            println!("Modpack importado como {}", i.id);
        }
        Cmd::Modpack { project } => {
            let i = content::modrinth::install_modpack(&client, &p, &project, s.concurrency, &rep).await?;
            println!("Modpack instalado como {}", i.id);
        }
        Cmd::Open { id } => {
            let i = Instance::load(&p, &id)?;
            open::that(i.game_dir(&p))?;
        }
        Cmd::Config { set } => {
            let mut s = s;
            if let Some(kv) = set {
                let (k, v) = kv.split_once('=').context("formato: clave=valor")?;
                match k {
                    "default_memory_mb" => s.default_memory_mb = v.parse()?,
                    "auto_memory" => s.auto_memory = v.parse()?,
                    "concurrency" => s.concurrency = v.parse()?,
                    "msa_client_id" => s.msa_client_id = v.to_string(),
                    "discord_presence" => s.discord_presence = v.parse()?,
                    "discord_client_id" => s.discord_client_id = v.trim().to_string(),
                    "optimized_gc" => s.optimized_gc = v.parse()?,
                    "proxy" => s.proxy = v.to_string(),
                    "download_source" => s.download_source = net::Source::parse(v).as_str().to_string(),
                    "curseforge_api_key" => s.curseforge_api_key = v.to_string(),
                    "update_repo" => s.update_repo = v.to_string(),
                    "check_updates" => s.check_updates = v.parse()?,
                    "java_path" => s.java_path = v.to_string(),
                    "java_auto_download" => s.java_auto_download = v.parse()?,
                    _ => anyhow::bail!("clave desconocida: {k}"),
                }
                s.save(&p)?;
            }
            let mut shown = s.clone();
            if !shown.curseforge_api_key.is_empty() {
                shown.curseforge_api_key = "********".into();
            }
            println!("{}", toml::to_string_pretty(&shown)?);
            println!("Datos en: {}", p.root.display());
        }
        Cmd::Java => {
            let mut all = game::java::detect_system();
            all.extend(game::java::detect_managed(&p));
            if all.is_empty() {
                println!("No se ha encontrado ningún Java instalado");
            }
            for j in all {
                println!("  {}", j.describe());
            }
            if !s.java_path.is_empty() {
                println!("Java fijado en Ajustes: {}", s.java_path);
            }
        }
        Cmd::NetTest => {
            println!("Probando conexión…");
            print!("{}", actions::network_report(&client).await);
        }
        Cmd::Kill { id } => {
            if !game::launch::kill(&id) {
                println!("'{id}' no se lanzó desde esta sesión de Brasa");
            }
        }
        Cmd::Character { name } => {
            let mut st = account::auth::AccountStore::load(&p);
            let acc = st.active().cloned().context("no hay cuenta activa")?;
            let acc = account::auth::ensure_fresh(&client, &s.msa_client_id, &acc).await?;
            let new = account::auth::yggdrasil_select(&client, &acc, &name).await?;
            st.accounts.retain(|a| a.uuid != acc.uuid);
            println!("Ahora juegas como {}", new.username);
            st.upsert(new);
            st.save(&p)?;
        }
        Cmd::SkinUpload { file, slim } => {
            let mut st = account::auth::AccountStore::load(&p);
            let acc = st.active().cloned().context("no hay cuenta activa")?;
            let acc = account::auth::ensure_fresh(&client, &s.msa_client_id, &acc).await?;
            account::skins::upload_premium(&client, &acc, &file, slim).await?;
            st.upsert(acc);
            st.save(&p)?;
            println!("Skin subida");
        }
        Cmd::SkinReset => {
            let st = account::auth::AccountStore::load(&p);
            let acc = st.active().cloned().context("no hay cuenta activa")?;
            let acc = account::auth::ensure_fresh(&client, &s.msa_client_id, &acc).await?;
            account::skins::reset_premium(&client, &acc).await?;
            println!("Skin quitada");
        }
        Cmd::OfflineSkin { file, slim } => {
            let mut st = account::auth::AccountStore::load(&p);
            let mut acc = st.active().cloned().context("no hay cuenta activa")?;
            if acc.kind != account::auth::AccountKind::Offline {
                anyhow::bail!("la skin local es para cuentas offline");
            }
            if let Some(f) = &file {
                account::skins::read_skin(f)?;
            }
            acc.skin_path = file.map(|f| f.canonicalize().unwrap_or(f).to_string_lossy().into_owned());
            acc.skin_slim = slim;
            st.upsert(acc);
            st.save(&p)?;
            println!("Hecho: se aplicará al lanzar el juego");
        }
        Cmd::CfSearch { query, r#type, instance } => {
            let inst = instance.map(|i| Instance::load(&p, &i)).transpose()?;
            let hits = content::curseforge::search(
                &client,
                &s.curseforge_api_key,
                &query,
                cf_class(&r#type),
                inst.as_ref().map(|i| i.mc_version.as_str()),
                inst.as_ref().map(|i| i.loader),
            )
            .await?;
            for h in hits {
                println!("  {:<10} {:>12.0} desc.  {}", h.id, h.download_count, h.name);
            }
        }
        Cmd::CfAdd { instance, id, r#type } => {
            let inst = Instance::load(&p, &instance)?;
            let files = content::curseforge::install(&client, &s.curseforge_api_key, &p, &inst, id, cf_class(&r#type), &rep).await?;
            println!("Instalado: {}", files.join(", "));
        }
        Cmd::CfModpack { file_or_id } => {
            let i = match file_or_id.parse::<u64>() {
                Ok(id) => content::curseforge::install_modpack(&client, &s.curseforge_api_key, &p, id, s.concurrency, &rep).await?,
                Err(_) => content::curseforge::import_zip(&client, &s.curseforge_api_key, &p, std::path::Path::new(&file_or_id), s.concurrency, &rep).await?,
            };
            println!("Modpack instalado como {}", i.id);
        }
        Cmd::Importable => {
            let latest = game::mojang::manifest(&client, &p).await.map(|m| m.latest.release).unwrap_or_default();
            for f in instance::importers::scan_prism().into_iter().chain(instance::importers::scan_official(&latest)) {
                println!("  [{}] {:<30} {} {}  ({})", f.source, f.name, f.loader.label(), f.mc, f.path.display());
            }
        }
        Cmd::ImportFrom { source } => {
            let path = std::path::Path::new(&source);
            let found = if path.is_dir() {
                instance::importers::read_prism(path)?
            } else {
                let latest = game::mojang::manifest(&client, &p).await.map(|m| m.latest.release).unwrap_or_default();
                instance::importers::scan_official(&latest)
                    .into_iter()
                    .find(|f| f.name.eq_ignore_ascii_case(&source))
                    .context("no es una carpeta de Prism/MultiMC ni un perfil del launcher oficial")?
            };
            let i = instance::importers::import(&p, &found)?;
            println!("Importada como {} ({} {})", i.id, i.loader.label(), i.mc_version);
        }
        Cmd::Export { id, file, saves } => {
            let inst = Instance::load(&p, &id)?;
            let (refd, embedded) = content::modrinth::export_mrpack(&client, &p, &inst, &file, saves, &rep).await?;
            println!("Exportado: {refd} ficheros enlazados a Modrinth, {embedded} incluidos dentro del pack");
        }
        Cmd::Worlds { id } => {
            let inst = Instance::load(&p, &id)?;
            let worlds = instance::worlds::list_worlds(&p, &inst);
            println!("Mundos ({}):", worlds.len());
            for w in &worlds {
                println!("  {:<30} {}", w.name, instance::worlds::fmt_size(w.size));
            }
            let backups = instance::worlds::list_backups(&p, &inst);
            println!("Copias ({}):", backups.len());
            for b in &backups {
                let name = b.file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                println!("  {name:<40} {}", instance::worlds::fmt_size(b.size));
            }
        }
        Cmd::Backup { id, world } => {
            let inst = Instance::load(&p, &id)?;
            let names: Vec<String> = match world {
                Some(w) => vec![w],
                None => instance::worlds::list_worlds(&p, &inst).into_iter().map(|w| w.name).collect(),
            };
            if names.is_empty() {
                println!("Esta instancia no tiene mundos");
            }
            for n in names {
                let b = instance::worlds::backup_world(&p, &inst, &n)?;
                println!("✔ {} → {} ({})", n, b.file.display(), instance::worlds::fmt_size(b.size));
            }
        }
        Cmd::Restore { id, backup } => {
            let inst = Instance::load(&p, &id)?;
            if backup.contains('/') || backup.contains('\\') || backup.contains("..") {
                anyhow::bail!("indica sólo el nombre de la copia (el que muestra `brasa worlds`), no una ruta");
            }
            let file = instance::worlds::backups_dir(&p, &inst).join(&backup);
            let name = instance::worlds::restore_backup(&p, &inst, &file)?;
            println!("✔ Mundo «{name}» restaurado (el anterior se conservó como «{name}.antes-de-restaurar-…»)");
        }
        Cmd::UpdatePack { id } => {
            let v = content::modpacks::update(&client, &p, &s, &id, &rep).await?;
            println!("✔ Modpack actualizado a {v}");
        }
        Cmd::Perf { id } => {
            let inst = Instance::load(&p, &id)?;
            let (ok, missing) = content::modrinth::install_performance(&client, &p, &inst, &rep).await?;
            println!("Instalados: {}", ok.join(", "));
            if !missing.is_empty() {
                println!("No disponibles para esta versión: {}", missing.join(", "));
            }
        }
        Cmd::SelfUpdate { check } => match updater::check(&client, &s.update_repo).await? {
            None => println!("Brasa {} está al día", env!("CARGO_PKG_VERSION")),
            Some(rel) => {
                println!("Nueva versión disponible: {}", rel.tag_name);
                if !check {
                    let exe = updater::apply(&client, &rel).await?;
                    println!("Actualizado en {}. Vuelve a abrir Brasa.", exe.display());
                }
            }
        },
    }
    Ok(())
}
