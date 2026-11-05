//! Rich Presence de Discord: «Jugando a Brasa Launcher» en el perfil.
//!
//! Habla con la app de Discord del propio equipo por su canal local (socket Unix o tubería con
//! nombre en Windows); no usa internet. Protocolo: cada mensaje es `opcode (u32 LE) + longitud
//! (u32 LE) + JSON`. Primero un saludo (op 0) con el ID de la aplicación de Discord; después
//! órdenes `SET_ACTIVITY` (op 1).
//!
//! El trabajo va en un hilo propio: si Discord no está abierto se reintenta cada 15 s, y si se
//! cierra a mitad, se reconecta solo. Nada de esto bloquea el launcher ni el juego.

use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const RETRY: Duration = Duration::from_secs(15);

// ----------------------------------------------------------------------------- protocolo

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Handshake = 0,
    Frame = 1,
    Close = 2,
    Ping = 3,
    Pong = 4,
}

impl Op {
    fn from_u32(n: u32) -> Option<Op> {
        Some(match n {
            0 => Op::Handshake,
            1 => Op::Frame,
            2 => Op::Close,
            3 => Op::Ping,
            4 => Op::Pong,
            _ => return None,
        })
    }
}

pub fn encode(op: Op, payload: &Value) -> Vec<u8> {
    let body = payload.to_string().into_bytes();
    let mut out = Vec::with_capacity(8 + body.len());
    out.extend_from_slice(&(op as u32).to_le_bytes());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

pub fn read_frame(r: &mut impl Read) -> std::io::Result<(Op, Value)> {
    let mut head = [0u8; 8];
    r.read_exact(&mut head)?;
    let op = u32::from_le_bytes(head[0..4].try_into().unwrap());
    let len = u32::from_le_bytes(head[4..8].try_into().unwrap()) as usize;
    if len > 1 << 20 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "mensaje de Discord demasiado grande"));
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body)?;
    let op = Op::from_u32(op).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("opcode desconocido {op}")))?;
    let v = serde_json::from_slice(&body).unwrap_or(Value::Null);
    Ok((op, v))
}

// ----------------------------------------------------------------------------- lo que se muestra

/// Partida en curso que se enseña en Discord.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Playing {
    pub instance: String,
    pub mc_version: String,
    pub loader: String,
    /// Inicio (segundos Unix): Discord muestra el tiempo transcurrido.
    pub since: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Config {
    pub enabled: bool,
    pub client_id: String,
    /// `usuario/repo` de GitHub: si está, se añade un botón para descargar Brasa.
    pub repo: String,
}

/// Discord exige entre 2 y 128 caracteres en los textos.
fn fit(s: &str) -> String {
    let mut t: String = s.trim().chars().take(128).collect();
    while t.chars().count() < 2 {
        t.push(' ');
    }
    t
}

/// La actividad que se envía a Discord.
pub fn activity(playing: Option<&Playing>, repo: &str) -> Value {
    let mut a = json!({
        "type": 0,
        "assets": {
            "large_image": "brasa",
            "large_text": fit(&format!("Brasa Launcher v{}", env!("CARGO_PKG_VERSION"))),
        },
    });
    match playing {
        Some(p) => {
            a["details"] = json!(fit(&format!("Jugando a Minecraft {}", p.mc_version)));
            a["state"] = json!(fit(&format!("{} · {}", p.instance, p.loader)));
            // Discord mide el inicio en milisegundos
            a["timestamps"] = json!({ "start": p.since.saturating_mul(1000) });
        }
        None => {
            a["details"] = json!("En el launcher");
        }
    }
    let repo = repo.trim().trim_matches('/');
    if repo.split('/').count() == 2 && !repo.contains(' ') {
        a["buttons"] = json!([{ "label": "Descargar Brasa", "url": format!("https://github.com/{repo}/releases/latest") }]);
    }
    a
}

impl Config {
    pub fn from_settings(s: &crate::config::Settings) -> Config {
        Config { enabled: s.discord_presence, client_id: s.discord_client_id.trim().to_string(), repo: s.update_repo.clone() }
    }
}

impl Playing {
    pub fn of(i: &crate::config::Instance) -> Playing {
        Playing { instance: i.name.clone(), mc_version: i.mc_version.clone(), loader: i.loader.label().to_string(), since: crate::config::now_secs() }
    }
}

// ----------------------------------------------------------------------------- conexión

trait Pipe: Read + Write + Send {}
impl<T: Read + Write + Send> Pipe for T {}

/// Dónde puede estar el canal de Discord (incluidas las versiones Flatpak y Snap en Linux).
pub fn candidate_paths() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        (0..10).map(|i| PathBuf::from(format!(r"\\?\pipe\discord-ipc-{i}"))).collect()
    }
    #[cfg(not(windows))]
    {
        let mut bases: Vec<PathBuf> = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"].iter().filter_map(std::env::var_os).map(PathBuf::from).collect();
        bases.push(PathBuf::from("/tmp"));
        let mut out = vec![];
        for b in bases {
            for sub in ["", "app/com.discordapp.Discord", "snap.discord", ".flatpak/dev.vencord.Vesktop/xdg-run"] {
                for i in 0..10 {
                    out.push(b.join(sub).join(format!("discord-ipc-{i}")));
                }
            }
        }
        out.dedup();
        out
    }
}

fn open(path: &std::path::Path) -> std::io::Result<Box<dyn Pipe>> {
    #[cfg(unix)]
    {
        let s = std::os::unix::net::UnixStream::connect(path)?;
        s.set_read_timeout(Some(Duration::from_secs(3)))?;
        s.set_write_timeout(Some(Duration::from_secs(3)))?;
        Ok(Box::new(s))
    }
    #[cfg(windows)]
    {
        let f = std::fs::OpenOptions::new().read(true).write(true).open(path)?;
        Ok(Box::new(f))
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "sistema no soportado"))
    }
}

struct Conn {
    pipe: Box<dyn Pipe>,
}

#[derive(Debug, PartialEq, Eq)]
enum ConnectError {
    NotRunning,
    Rejected(String),
}

impl Conn {
    fn connect(paths: &[PathBuf], client_id: &str) -> Result<Conn, ConnectError> {
        for path in paths {
            let Ok(pipe) = open(path) else { continue };
            let mut c = Conn { pipe };
            match c.handshake(client_id) {
                Ok(()) => return Ok(c),
                Err(ConnectError::NotRunning) => continue,
                Err(e) => return Err(e),
            }
        }
        Err(ConnectError::NotRunning)
    }

    fn handshake(&mut self, client_id: &str) -> Result<(), ConnectError> {
        let hello = json!({ "v": 1, "client_id": client_id });
        self.pipe.write_all(&encode(Op::Handshake, &hello)).map_err(|_| ConnectError::NotRunning)?;
        loop {
            match read_frame(&mut self.pipe) {
                Ok((Op::Frame, v)) if v["evt"] == "READY" => return Ok(()),
                Ok((Op::Close, v)) => {
                    let msg = v["message"].as_str().unwrap_or("Discord rechazó la conexión").to_string();
                    return Err(ConnectError::Rejected(msg));
                }
                Ok((Op::Ping, v)) => {
                    let _ = self.pipe.write_all(&encode(Op::Pong, &v));
                }
                Ok(_) => continue,
                Err(_) => return Err(ConnectError::NotRunning),
            }
        }
    }

    /// Envía la actividad (o la borra con `None`). Devuelve el error de Discord si lo rechaza.
    fn set_activity(&mut self, activity: Option<&Value>) -> std::io::Result<Result<(), String>> {
        let nonce = uuid::Uuid::new_v4().to_string();
        let cmd = json!({
            "cmd": "SET_ACTIVITY",
            "args": { "pid": std::process::id(), "activity": activity },
            "nonce": nonce,
        });
        self.pipe.write_all(&encode(Op::Frame, &cmd))?;
        loop {
            let (op, v) = read_frame(&mut self.pipe)?;
            match op {
                Op::Ping => self.pipe.write_all(&encode(Op::Pong, &v))?,
                Op::Close => return Err(std::io::Error::new(std::io::ErrorKind::ConnectionAborted, "Discord cerró la conexión")),
                Op::Frame if v["nonce"] == nonce.as_str() => {
                    return Ok(if v["evt"] == "ERROR" { Err(v["data"]["message"].as_str().unwrap_or("error de Discord").to_string()) } else { Ok(()) });
                }
                _ => continue,
            }
        }
    }
}

// ----------------------------------------------------------------------------- hilo de trabajo

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Status {
    /// Desactivado en Ajustes.
    #[default]
    Off,
    /// Falta el ID de la aplicación de Discord.
    NoClientId,
    /// Discord no está abierto (se reintenta solo).
    NotRunning,
    /// Mostrando el estado en Discord.
    Connected,
    /// Discord rechazó el ID o la actividad.
    Error(String),
}

impl Status {
    pub fn describe(&self) -> String {
        match self {
            Status::Off => "Desactivado".into(),
            Status::NoClientId => "Esta copia de Brasa no incluye la integración con Discord".into(),
            Status::NotRunning => "Discord no está abierto: se conectará solo cuando lo abras".into(),
            Status::Connected => "Conectado: tu perfil muestra que usas Brasa".into(),
            Status::Error(e) => format!("Discord lo rechazó: {e}"),
        }
    }
}

enum Msg {
    Config(Config),
    Start(String, Playing),
    End(String),
}

/// Asa del hilo de Discord. Se puede clonar y mover a otras tareas.
#[derive(Clone)]
pub struct Presence {
    tx: Sender<Msg>,
    /// La interfaz recibe el estado por `on_change`; esta copia la consultan los tests.
    #[cfg_attr(not(test), allow(dead_code))]
    status: Arc<Mutex<Status>>,
}

impl Presence {
    /// Arranca el hilo. `on_change` recibe cada estado nuevo (para mostrarlo en la interfaz).
    pub fn start(config: Config, on_change: impl Fn(&Status) + Send + 'static) -> Presence {
        Self::start_with_paths(config, candidate_paths, on_change)
    }

    fn start_with_paths(config: Config, paths: fn() -> Vec<PathBuf>, on_change: impl Fn(&Status) + Send + 'static) -> Presence {
        let (tx, rx) = mpsc::channel::<Msg>();
        let status = Arc::new(Mutex::new(Status::Off));
        let st = status.clone();
        std::thread::Builder::new()
            .name("discord".into())
            .spawn(move || {
                let mut cfg = config;
                let mut games: BTreeMap<String, Playing> = BTreeMap::new();
                let mut conn: Option<Conn> = None;
                let mut sent: Option<Value> = None;
                let mut last_try: Option<Instant> = None;
                let set_status = |s: Status| {
                    let mut cur = st.lock().unwrap();
                    if *cur != s {
                        on_change(&s);
                        *cur = s;
                    }
                };
                loop {
                    match rx.recv_timeout(Duration::from_secs(5)) {
                        Ok(Msg::Config(c)) => {
                            if c.client_id != cfg.client_id {
                                conn = None; // otro ID: hay que volver a saludar
                                sent = None;
                                last_try = None;
                            }
                            cfg = c;
                        }
                        Ok(Msg::Start(id, p)) => {
                            games.insert(id, p);
                        }
                        Ok(Msg::End(id)) => {
                            games.remove(&id);
                        }
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => {
                            // el launcher se cierra: se borra el estado antes de salir
                            if let Some(c) = conn.as_mut() {
                                let _ = c.set_activity(None);
                            }
                            return;
                        }
                    }
                    if !cfg.enabled || cfg.client_id.trim().is_empty() {
                        if let Some(mut c) = conn.take() {
                            let _ = c.set_activity(None);
                        }
                        sent = None;
                        set_status(if cfg.enabled { Status::NoClientId } else { Status::Off });
                        continue;
                    }
                    if conn.is_none() {
                        if last_try.is_some_and(|t| t.elapsed() < RETRY) {
                            continue;
                        }
                        last_try = Some(Instant::now());
                        match Conn::connect(&paths(), cfg.client_id.trim()) {
                            Ok(c) => {
                                conn = Some(c);
                                sent = None;
                            }
                            Err(ConnectError::NotRunning) => {
                                set_status(Status::NotRunning);
                                continue;
                            }
                            Err(ConnectError::Rejected(m)) => {
                                set_status(Status::Error(m));
                                continue;
                            }
                        }
                    }
                    // la partida empezada más recientemente es la que se muestra
                    let current = games.values().max_by_key(|p| p.since);
                    let want = activity(current, &cfg.repo);
                    if sent.as_ref() == Some(&want) {
                        set_status(Status::Connected);
                        continue;
                    }
                    let Some(c) = conn.as_mut() else { continue };
                    match c.set_activity(Some(&want)) {
                        Ok(Ok(())) => {
                            sent = Some(want);
                            set_status(Status::Connected);
                        }
                        Ok(Err(m)) => {
                            sent = Some(want); // no reintentar en bucle lo que Discord rechaza
                            set_status(Status::Error(m));
                        }
                        Err(_) => {
                            conn = None;
                            sent = None;
                            last_try = None; // Discord se cerró: reintentar ya
                            set_status(Status::NotRunning);
                        }
                    }
                }
            })
            .expect("no se pudo crear el hilo de Discord");
        Presence { tx, status }
    }

    pub fn configure(&self, c: Config) {
        let _ = self.tx.send(Msg::Config(c));
    }

    pub fn start_playing(&self, id: &str, p: Playing) {
        let _ = self.tx.send(Msg::Start(id.to_string(), p));
    }

    pub fn stop_playing(&self, id: &str) {
        let _ = self.tx.send(Msg::End(id.to_string()));
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    #[test]
    fn frames_roundtrip() {
        let bytes = encode(Op::Frame, &json!({ "cmd": "X" }));
        assert_eq!(&bytes[0..4], &1u32.to_le_bytes());
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize, bytes.len() - 8);
        let (op, v) = read_frame(&mut &bytes[..]).unwrap();
        assert_eq!(op, Op::Frame);
        assert_eq!(v["cmd"], "X");
    }

    #[test]
    fn rejects_garbage() {
        let mut bad = 9u32.to_le_bytes().to_vec();
        bad.extend_from_slice(&0u32.to_le_bytes());
        assert!(read_frame(&mut &bad[..]).is_err(), "opcode desconocido");
        let mut huge = 1u32.to_le_bytes().to_vec();
        huge.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(read_frame(&mut &huge[..]).is_err(), "longitud absurda");
    }

    #[test]
    fn activity_texts() {
        let p = Playing { instance: "Create 1.20".into(), mc_version: "1.20.1".into(), loader: "Forge".into(), since: 1_700_000_000 };
        let a = activity(Some(&p), "javier/brasa");
        assert_eq!(a["details"], "Jugando a Minecraft 1.20.1");
        assert_eq!(a["state"], "Create 1.20 · Forge");
        assert_eq!(a["timestamps"]["start"], 1_700_000_000_000u64);
        assert_eq!(a["assets"]["large_image"], "brasa");
        assert_eq!(a["buttons"][0]["url"], "https://github.com/javier/brasa/releases/latest");
        let idle = activity(None, "");
        assert_eq!(idle["details"], "En el launcher");
        assert!(idle.get("buttons").is_none() && idle.get("timestamps").is_none());
        assert!(activity(None, "no es un repo").get("buttons").is_none());
    }

    #[test]
    fn texts_respect_discord_limits() {
        let long = "x".repeat(300);
        let p = Playing { instance: long.clone(), mc_version: "1.21".into(), loader: "Fabric".into(), since: 0 };
        assert_eq!(activity(Some(&p), "")["state"].as_str().unwrap().chars().count(), 128);
        assert_eq!(fit("a"), "a ");
    }

    /// Un Discord de mentira: acepta el saludo y registra cada actividad que recibe.
    fn fake_discord(path: PathBuf, seen: Arc<Mutex<Vec<Value>>>, reject_id: Option<&'static str>) {
        let listener = UnixListener::bind(&path).unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { return };
                let seen = seen.clone();
                std::thread::spawn(move || {
                    let Ok((Op::Handshake, hello)) = read_frame(&mut s) else { return };
                    if Some(hello["client_id"].as_str().unwrap_or("")) == reject_id {
                        let _ = s.write_all(&encode(Op::Close, &json!({ "code": 4000, "message": "Invalid Client ID" })));
                        return;
                    }
                    let _ = s.write_all(&encode(Op::Frame, &json!({ "cmd": "DISPATCH", "evt": "READY", "data": {} })));
                    while let Ok((_, cmd)) = read_frame(&mut s) {
                        seen.lock().unwrap().push(cmd["args"]["activity"].clone());
                        let _ = s.write_all(&encode(Op::Frame, &json!({ "cmd": "SET_ACTIVITY", "evt": null, "nonce": cmd["nonce"] })));
                    }
                });
            }
        });
    }

    fn wait_for(cond: impl Fn() -> bool) -> bool {
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(10) {
            if cond() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }

    static SOCKET: Mutex<Option<PathBuf>> = Mutex::new(None);
    fn test_paths() -> Vec<PathBuf> {
        SOCKET.lock().unwrap().clone().into_iter().collect()
    }

    #[test]
    fn end_to_end_with_a_fake_discord() {
        // ruta corta: los sockets Unix admiten ~104 bytes en macOS, y su carpeta temporal
        // (/var/folders/…/T/) ya se come la mitad
        let id = uuid::Uuid::new_v4().simple().to_string();
        let dir = PathBuf::from("/tmp").join(format!("bd-{}", &id[..8]));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("discord-ipc-0");
        let seen = Arc::new(Mutex::new(vec![]));
        fake_discord(path.clone(), seen.clone(), Some("malo"));
        *SOCKET.lock().unwrap() = Some(path);

        let cfg = Config { enabled: true, client_id: "123".into(), repo: String::new() };
        let pr = Presence::start_with_paths(cfg.clone(), test_paths, |_| {});
        pr.configure(cfg.clone()); // despierta al hilo sin esperar 5 s
        assert!(wait_for(|| pr.status() == Status::Connected), "{:?}", pr.status());
        assert!(wait_for(|| seen.lock().unwrap().last().map(|a| a["details"] == "En el launcher").unwrap_or(false)));

        let p = Playing { instance: "Prueba".into(), mc_version: "1.21.1".into(), loader: "Vanilla".into(), since: 42 };
        pr.start_playing("prueba", p);
        assert!(wait_for(|| seen.lock().unwrap().last().map(|a| a["state"] == "Prueba · Vanilla").unwrap_or(false)));

        pr.stop_playing("prueba");
        assert!(wait_for(|| seen.lock().unwrap().last().map(|a| a["details"] == "En el launcher").unwrap_or(false)));

        // Desactivarlo borra el estado (actividad nula).
        pr.configure(Config { enabled: false, ..cfg.clone() });
        assert!(wait_for(|| pr.status() == Status::Off));
        assert!(wait_for(|| seen.lock().unwrap().last().map(|a| a.is_null()).unwrap_or(false)));

        // Un ID que Discord rechaza se muestra como error, no se reintenta en bucle.
        pr.configure(Config { enabled: true, client_id: "malo".into(), repo: String::new() });
        assert!(wait_for(|| matches!(pr.status(), Status::Error(ref m) if m.contains("Invalid Client ID"))), "{:?}", pr.status());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn without_discord_it_waits_quietly() {
        let pr = Presence::start_with_paths(Config { enabled: true, client_id: "1".into(), repo: String::new() }, Vec::new, |_| {});
        pr.configure(Config { enabled: true, client_id: "1".into(), repo: String::new() });
        assert!(wait_for(|| pr.status() == Status::NotRunning));
        let off = Presence::start_with_paths(Config::default(), Vec::new, |_| {});
        off.configure(Config { enabled: true, ..Config::default() });
        assert!(wait_for(|| off.status() == Status::NoClientId));
    }
}
