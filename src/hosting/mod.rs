//! Servidores de Minecraft que se crean y se ejecutan desde Brasa, para jugar con amigos.
//!
//! Cada servidor vive en `<datos>/hosting/<id>/`: `brasa-server.toml` (lo que guarda Brasa) y el
//! resto, lo de cualquier servidor (`server.properties`, `world/`, `plugins/` o `mods/`…).
//!
//! - [`install`]: descarga el software según el tipo (Vanilla, Paper, Fabric, NeoForge, Forge).
//! - [`run`]: arranca el proceso, lee su consola, sabe quién está conectado y lo detiene bien.
//! - [`content`]: plugins y mods desde Modrinth, y la instancia para que los amigos se unan.
//! - [`playit`]: el túnel de playit.gg para que entren amigos de fuera de casa.

pub mod content;
pub mod install;
pub mod p2p;
pub mod playit;
pub mod props;
pub mod run;

use crate::config::{now_secs, Loader};
use crate::paths::Paths;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const DEFAULT_PORT: u16 = 25565;

/// El software del servidor.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Vanilla,
    /// Plugins (Bukkit/Spigot/Paper).
    Paper,
    Fabric,
    NeoForge,
    Forge,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Vanilla => "Vanilla",
            Kind::Paper => "Paper",
            Kind::Fabric => "Fabric",
            Kind::NeoForge => "NeoForge",
            Kind::Forge => "Forge",
        }
    }
    /// El loader que necesitan los jugadores (Paper y Vanilla: ninguno).
    pub fn client_loader(self) -> Loader {
        match self {
            Kind::Vanilla | Kind::Paper => Loader::Vanilla,
            Kind::Fabric => Loader::Fabric,
            Kind::NeoForge => Loader::NeoForge,
            Kind::Forge => Loader::Forge,
        }
    }
    pub fn plugins(self) -> bool {
        self == Kind::Paper
    }
    pub fn mods(self) -> bool {
        matches!(self, Kind::Fabric | Kind::NeoForge | Kind::Forge)
    }
    /// Carpeta del contenido extra: `plugins/` o `mods/`.
    pub fn content_dir(self) -> Option<&'static str> {
        if self.plugins() {
            Some("plugins")
        } else if self.mods() {
            Some("mods")
        } else {
            None
        }
    }
    pub fn has_loader_version(self) -> bool {
        matches!(self, Kind::Fabric | Kind::NeoForge | Kind::Forge)
    }
}

/// Un plugin o mod instalado desde Brasa (para saber si los jugadores lo necesitan).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Content {
    pub file: String,
    pub title: String,
    pub project_id: String,
    /// `required`, `optional` o `unsupported` (de Modrinth). Vacío si no se sabe.
    pub client_side: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Server {
    /// Nombre de la carpeta (no se guarda en el toml).
    #[serde(skip)]
    pub id: String,
    pub name: String,
    pub kind: Kind,
    pub mc_version: String,
    pub loader_version: Option<String>,
    pub memory_mb: u32,
    pub port: u16,
    /// Modo premium: sólo cuentas de Microsoft. Apagado, entran también cuentas no premium.
    pub online_mode: bool,
    pub whitelist: bool,
    /// Abrir el túnel de playit.gg al arrancar.
    pub playit: bool,
    /// Túnel de playit creado para este servidor.
    pub playit_tunnel: Option<String>,
    /// Lo que hay que ejecutar, una vez instalado (relativo a la carpeta del servidor).
    pub launch: Option<Launch>,
    /// Java que pide la versión.
    pub java_major: u32,
    pub eula_accepted: bool,
    pub content: Vec<Content>,
    pub created: u64,
    pub last_started: Option<u64>,
    /// Aceptar conexiones de la red de casa sin Brasa. Apagado, el servidor escucha sólo en este
    /// equipo y los amigos entran por Brasa P2P (cifrado y con invitación).
    pub lan_open: bool,
    /// Portero: preguntar antes de emparejar a alguien con una invitación.
    #[serde(default = "yes")]
    pub gatekeeper: bool,
    /// Amigos emparejados (Brasa P2P): cada equipo atado a un nombre de Minecraft.
    pub friends: Vec<Friend>,
    /// Invitaciones creadas.
    pub invites: Vec<Invite>,
}

fn yes() -> bool {
    true
}

/// Un equipo emparejado con el servidor.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Friend {
    /// Clave pública de su Brasa (EndpointId de iroh).
    pub id: String,
    /// El único nombre de Minecraft con el que puede entrar.
    pub name: String,
    pub paired: u64,
    pub last_seen: Option<u64>,
}

/// Invitación personal de un solo uso. Del secreto sólo se guarda su SHA-256.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Invite {
    pub id: String,
    pub secret_sha256: String,
    pub name: String,
    pub created: u64,
    pub expires: u64,
    pub used: bool,
    /// El código completo, para poder copiarlo otra vez mientras siga pendiente.
    pub code: String,
}

impl Invite {
    pub fn pending(&self, now: u64) -> bool {
        !self.used && now < self.expires
    }
}

/// Cómo se arranca, según lo que dejó el instalador.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", content = "path", rename_all = "lowercase")]
pub enum Launch {
    /// `java -jar <fichero>`
    Jar(String),
    /// `java @<fichero>` (Forge/NeoForge modernos: `libraries/…/unix_args.txt`)
    Args(String),
}

impl Server {
    pub fn dir(&self, p: &Paths) -> PathBuf {
        root(p).join(&self.id)
    }
    fn file(&self, p: &Paths) -> PathBuf {
        self.dir(p).join("brasa-server.toml")
    }
    pub fn save(&self, p: &Paths) -> Result<()> {
        std::fs::create_dir_all(self.dir(p))?;
        crate::paths::write_atomic(&self.file(p), toml::to_string_pretty(self)?)?;
        Ok(())
    }
    pub fn load(p: &Paths, id: &str) -> Result<Server> {
        let f = root(p).join(id).join("brasa-server.toml");
        let txt = std::fs::read_to_string(&f).with_context(|| format!("no existe el servidor {id}"))?;
        let mut s: Server = toml::from_str(&txt).with_context(|| format!("{} está dañado", f.display()))?;
        s.id = id.to_string();
        Ok(s)
    }
    /// Cambia el servidor guardado sin pisar otros cambios a la vez (el panel, Brasa P2P y las
    /// tareas en segundo plano escriben el mismo fichero): leer, cambiar y guardar bajo un candado.
    pub fn update<R>(p: &Paths, id: &str, f: impl FnOnce(&mut Server) -> Result<R>) -> Result<R> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut s = Server::load(p, id)?;
        let r = f(&mut s)?;
        s.save(p)?;
        Ok(r)
    }
    pub fn list(p: &Paths) -> Vec<Server> {
        let mut v: Vec<Server> =
            std::fs::read_dir(root(p)).into_iter().flatten().flatten().filter_map(|e| Server::load(p, &e.file_name().to_string_lossy()).ok()).collect();
        v.sort_by_key(|s| std::cmp::Reverse(s.last_started.unwrap_or(s.created)));
        v
    }
    /// Crea la carpeta y la configuración (sin descargar nada todavía).
    pub fn create(p: &Paths, name: &str, kind: Kind, mc: &str, loader_version: Option<String>, online_mode: bool) -> Result<Server> {
        let name = name.trim();
        if name.is_empty() {
            bail!("ponle un nombre al servidor");
        }
        if mc.trim().is_empty() {
            bail!("elige la versión de Minecraft");
        }
        let base = crate::paths::sanitize(name);
        let base = if base.is_empty() { "servidor".to_string() } else { base };
        std::fs::create_dir_all(root(p))?;
        let mut id = base.clone();
        let mut n = 2;
        while root(p).join(&id).exists() {
            id = format!("{base}_{n}");
            n += 1;
        }
        let used: Vec<u16> = Server::list(p).iter().map(|s| s.port).collect();
        let s = Server {
            id,
            name: name.to_string(),
            kind,
            mc_version: mc.trim().to_string(),
            loader_version: loader_version.filter(|v| !v.trim().is_empty()),
            memory_mb: 2048,
            port: free_port(&used),
            online_mode,
            // sin premium cualquiera puede entrar con cualquier nombre: la lista blanca, de serie
            whitelist: !online_mode,
            playit: false,
            gatekeeper: true,
            created: now_secs(),
            ..Default::default()
        };
        s.save(p)?;
        Ok(s)
    }
    pub fn delete(p: &Paths, id: &str) -> Result<()> {
        if run::is_running(id) {
            bail!("detén el servidor antes de borrarlo");
        }
        let d = root(p).join(id);
        if !d.join("brasa-server.toml").exists() {
            bail!("no es una carpeta de servidor de Brasa");
        }
        std::fs::remove_dir_all(&d).with_context(|| format!("no se pudo borrar {}", d.display()))
    }
    pub fn installed(&self, p: &Paths) -> bool {
        match &self.launch {
            Some(Launch::Jar(f)) | Some(Launch::Args(f)) => self.dir(p).join(f).exists(),
            None => false,
        }
    }
    /// «Paper 1.21.4», «Fabric 0.16.9 · 1.21.4».
    pub fn describe(&self) -> String {
        match &self.loader_version {
            Some(v) if self.kind.has_loader_version() => format!("{} {v} · {}", self.kind.label(), self.mc_version),
            _ => format!("{} {}", self.kind.label(), self.mc_version),
        }
    }
    /// Escribe `eula.txt` (sólo cuando el usuario la ha aceptado) y `server.properties` con lo de Brasa.
    pub fn write_files(&self, p: &Paths) -> Result<()> {
        let d = self.dir(p);
        std::fs::create_dir_all(&d)?;
        if self.eula_accepted {
            std::fs::write(d.join("eula.txt"), "# Aceptada en Brasa: https://aka.ms/MinecraftEULA\neula=true\n")?;
        }
        let mut pr = props::Properties::load(&d.join("server.properties"));
        pr.set_default("motd", &format!("{} · Brasa", self.name));
        pr.set("server-port", &self.port.to_string());
        pr.set("online-mode", if self.online_mode { "true" } else { "false" });
        pr.set("white-list", if self.whitelist { "true" } else { "false" });
        pr.set("enforce-whitelist", if self.whitelist { "true" } else { "false" });
        // sin red de casa: sólo este equipo (Brasa P2P y playit llegan por 127.0.0.1)
        pr.set("server-ip", if self.lan_open { "" } else { "127.0.0.1" });
        // sin premium, los nombres no se comprueban: que tampoco haga falta un perfil firmado
        if !self.online_mode {
            pr.set("enforce-secure-profile", "false");
        }
        pr.save(&d.join("server.properties"))?;
        Ok(())
    }
}

pub fn root(p: &Paths) -> PathBuf {
    p.root.join("hosting")
}

/// El primer puerto libre desde 25565 entre los que usan otros servidores de Brasa.
pub fn free_port(used: &[u16]) -> u16 {
    (DEFAULT_PORT..DEFAULT_PORT + 100).find(|p| !used.contains(p)).unwrap_or(DEFAULT_PORT)
}

/// La IP de este equipo en la red de casa (para los amigos de la misma red). No envía nada: sólo
/// pregunta al sistema qué interfaz usaría para salir.
pub fn lan_ip() -> Option<String> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("192.0.2.1:9").ok()?;
    let ip = s.local_addr().ok()?.ip();
    (!ip.is_loopback() && !ip.is_unspecified()).then(|| ip.to_string())
}

/// Dirección para pegar en Minecraft: sin «:25565», que es el puerto por defecto.
pub fn address(host: &str, port: u16) -> String {
    if port == DEFAULT_PORT {
        host.to_string()
    } else {
        format!("{host}:{port}")
    }
}

/// Nombres válidos de Minecraft (para la lista blanca y los operadores).
pub fn valid_player(n: &str) -> bool {
    (3..=16).contains(&n.len()) && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> Paths {
        Paths { root: std::env::temp_dir().join(format!("brasa-host-{}", uuid::Uuid::new_v4())) }
    }

    #[test]
    fn create_list_and_delete() {
        let p = tmp();
        let a = Server::create(&p, "Con amigos", Kind::Paper, "1.21.4", None, false).unwrap();
        let b = Server::create(&p, "Con amigos", Kind::Fabric, "1.21.4", Some("0.16.9".into()), true).unwrap();
        assert_ne!(a.id, b.id, "mismo nombre, otra carpeta");
        assert_eq!((a.port, b.port), (25565, 25566), "cada uno en su puerto");
        assert!(a.whitelist && !b.whitelist, "sin premium, lista blanca de serie");
        assert_eq!(b.describe(), "Fabric 0.16.9 · 1.21.4");
        assert_eq!(Server::list(&p).len(), 2);
        assert!(!a.installed(&p));
        Server::delete(&p, &a.id).unwrap();
        assert_eq!(Server::list(&p).len(), 1);
        assert!(Server::create(&p, "  ", Kind::Vanilla, "1.21.4", None, true).is_err());
        let _ = std::fs::remove_dir_all(&p.root);
    }

    #[test]
    fn files_follow_the_settings_and_eula_needs_consent() {
        let p = tmp();
        let mut s = Server::create(&p, "Casa", Kind::Vanilla, "1.21.4", None, false).unwrap();
        std::fs::write(s.dir(&p).join("server.properties"), "motd=Mi texto\nmax-players=5\nserver-port=1\n").unwrap();
        s.write_files(&p).unwrap();
        assert!(!s.dir(&p).join("eula.txt").exists(), "sin aceptar, no hay eula.txt");
        let pr = props::Properties::load(&s.dir(&p).join("server.properties"));
        assert_eq!(pr.get("motd"), Some("Mi texto"), "lo que puso el usuario se respeta");
        assert_eq!(pr.get("max-players"), Some("5"));
        assert_eq!(pr.get("server-port"), Some("25565"));
        assert_eq!(pr.get("online-mode"), Some("false"));
        assert_eq!(pr.get("white-list"), Some("true"));
        s.eula_accepted = true;
        s.write_files(&p).unwrap();
        assert!(std::fs::read_to_string(s.dir(&p).join("eula.txt")).unwrap().contains("eula=true"));
        let _ = std::fs::remove_dir_all(&p.root);
    }

    #[test]
    fn addresses_and_names() {
        assert_eq!(address("abc.joinmc.link", 25565), "abc.joinmc.link");
        assert_eq!(address("192.168.1.20", 25566), "192.168.1.20:25566");
        assert_eq!(free_port(&[25565, 25567]), 25566);
        assert!(valid_player("Javi_Brasa") && !valid_player("a b") && !valid_player("xy"));
    }
}
