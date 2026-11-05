//! El servidor en marcha: proceso de Java, su consola, quién está conectado y cómo pararlo.
//!
//! El estado de cada servidor arrancado vive en un registro global ([`Live`]) que la interfaz lee
//! cada fotograma. Se queda ahí al parar, para que la consola siga mostrando cómo acabó.

use super::{Launch, Server};
use crate::paths::Paths;
use crate::progress::Reporter;
use anyhow::{bail, Context, Result};
use std::collections::{HashMap, VecDeque};
use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

pub const MAX_LINES: usize = 5000;
/// Lo que se espera a que el servidor guarde y se cierre con `stop` antes de forzarlo.
pub const STOP_GRACE: Duration = Duration::from_secs(45);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Descargando o instalando.
    Preparing,
    /// Java arrancado, cargando el mundo.
    Starting,
    /// «Done (…)!»: ya se puede entrar.
    Running,
    Stopping,
    Stopped,
}

/// Estado del túnel de playit para este servidor.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Tunnel {
    #[default]
    Off,
    Connecting(String),
    /// Dirección pública (`algo.joinmc.link`).
    Ready(String),
    /// playit.gg no dejó crear el túnel desde Brasa: hay que crearlo en su web (se sigue esperando).
    Manual {
        port: u16,
        why: String,
    },
    Failed(String),
}

#[derive(Default)]
pub struct Live {
    pub status: Option<Status>,
    pub lines: VecDeque<String>,
    /// Líneas que se han ido descartando por arriba (para numerar sin saltos).
    pub dropped: usize,
    pub players: Vec<String>,
    pub started: Option<Instant>,
    pub exit: Option<String>,
    pub error: Option<String>,
    pub tunnel: Tunnel,
    stdin: Option<tokio::sync::mpsc::UnboundedSender<String>>,
    kill: Option<Arc<tokio::sync::Notify>>,
}

impl Live {
    pub fn push(&mut self, l: String) {
        self.lines.push_back(l);
        while self.lines.len() > MAX_LINES {
            self.lines.pop_front();
            self.dropped += 1;
        }
    }
    pub fn is_up(&self) -> bool {
        matches!(self.status, Some(Status::Preparing | Status::Starting | Status::Running | Status::Stopping))
    }
}

type Registry = Mutex<HashMap<String, Arc<Mutex<Live>>>>;
fn registry() -> &'static Registry {
    static R: OnceLock<Registry> = OnceLock::new();
    R.get_or_init(Default::default)
}

pub fn live(id: &str) -> Arc<Mutex<Live>> {
    registry().lock().unwrap().entry(id.to_string()).or_default().clone()
}

/// Lee el estado sin copiarlo.
pub fn with<R>(id: &str, f: impl FnOnce(&Live) -> R) -> Option<R> {
    let l = registry().lock().unwrap().get(id).cloned()?;
    let g = l.lock().unwrap();
    Some(f(&g))
}

pub fn is_running(id: &str) -> bool {
    with(id, |l| l.is_up()).unwrap_or(false)
}

pub fn running_ids() -> Vec<String> {
    let reg = registry().lock().unwrap();
    reg.iter().filter(|(_, l)| l.lock().unwrap().is_up()).map(|(k, _)| k.clone()).collect()
}

/// Manda una línea a la consola del servidor (un comando, sin la barra).
pub fn send(id: &str, cmd: &str) -> bool {
    let cmd = cmd.trim().trim_start_matches('/').to_string();
    if cmd.is_empty() {
        return false;
    }
    let tx = with(id, |l| l.stdin.clone()).flatten();
    match tx {
        Some(tx) => {
            let ok = tx.send(cmd.clone()).is_ok();
            if ok {
                live(id).lock().unwrap().push(format!("> {cmd}"));
            }
            ok
        }
        None => false,
    }
}

/// Para el servidor guardando el mundo (`stop`). Si no se cierra en un rato, se fuerza.
pub fn stop(id: &str) {
    let l = live(id);
    {
        let mut g = l.lock().unwrap();
        if !g.is_up() {
            return;
        }
        if g.status == Some(Status::Preparing) {
            // aún no hay proceso: se cancela al acabar de instalar
            g.status = Some(Status::Stopping);
            return;
        }
        g.status = Some(Status::Stopping);
    }
    if !send(id, "stop") {
        kill(id);
        return;
    }
    let id = id.to_string();
    std::thread::spawn(move || {
        let t0 = Instant::now();
        while t0.elapsed() < STOP_GRACE {
            if !is_running(&id) {
                return;
            }
            std::thread::sleep(Duration::from_millis(300));
        }
        kill(&id);
    });
}

/// Cierra el proceso sin guardar (último recurso).
pub fn kill(id: &str) {
    if let Some(n) = with(id, |l| l.kill.clone()).flatten() {
        n.notify_one();
    }
}

/// Lo que dice una línea de la consola.
#[derive(Debug, PartialEq)]
pub enum Event {
    Joined(String),
    Left(String),
    Ready,
    /// Respuesta de `list`: los conectados ahora.
    Players(Vec<String>),
    PortInUse,
    Eula,
}

/// El mensaje sin la cabecera de hora e hilo: `[12:00:00] [Server thread/INFO]: hola` → `hola`.
pub fn message(line: &str) -> &str {
    match line.rfind("]: ") {
        Some(i) => &line[i + 3..],
        None => line,
    }
    .trim()
}

pub fn parse(line: &str) -> Option<Event> {
    let low = line.to_ascii_lowercase();
    if low.contains("failed to bind to port") || low.contains("address already in use") {
        return Some(Event::PortInUse);
    }
    if low.contains("you need to agree to the eula") {
        return Some(Event::Eula);
    }
    let m = message(line);
    if m.starts_with("Done (") && m.contains(")!") {
        return Some(Event::Ready);
    }
    if let Some(name) = m.strip_suffix(" joined the game") {
        return super::valid_player(name).then(|| Event::Joined(name.to_string()));
    }
    if let Some(name) = m.strip_suffix(" left the game") {
        return super::valid_player(name).then(|| Event::Left(name.to_string()));
    }
    if m.starts_with("There are ") && m.contains(" players online") {
        let names = m.split_once(':').map(|(_, r)| r).unwrap_or("");
        let v: Vec<String> = names.split(',').map(|n| n.trim().to_string()).filter(|n| super::valid_player(n)).collect();
        return Some(Event::Players(v));
    }
    None
}

fn apply(l: &mut Live, ev: Event) {
    match ev {
        Event::Ready => {
            if l.status == Some(Status::Starting) {
                l.status = Some(Status::Running);
            }
        }
        Event::Joined(n) => {
            if !l.players.contains(&n) {
                l.players.push(n);
            }
        }
        Event::Left(n) => l.players.retain(|p| p != &n),
        Event::Players(v) => l.players = v,
        Event::PortInUse => l.error = Some("ese puerto ya lo usa otro programa (¿otro servidor abierto?): cámbialo en Ajustes del servidor".into()),
        Event::Eula => l.error = Some("falta aceptar el EULA de Minecraft".into()),
    }
}

/// La línea de comandos de Java para arrancar.
pub fn command(java: &std::path::Path, s: &Server) -> Result<Vec<String>> {
    let mem = s.memory_mb.max(512);
    let mut c = vec![java.display().to_string(), format!("-Xms{}M", mem.min(1024)), format!("-Xmx{mem}M")];
    match s.launch.as_ref().context("el servidor no está instalado")? {
        Launch::Jar(f) => {
            c.push("-jar".into());
            c.push(f.clone());
        }
        Launch::Args(f) => c.push(format!("@{f}")),
    }
    c.push("nogui".into());
    Ok(c)
}

/// Instala si hace falta, arranca y se queda vigilando hasta que el proceso termina.
/// `notify` se llama cada vez que cambia algo (para repintar la interfaz).
pub async fn start(client: reqwest::Client, p: Paths, id: String, notify: Arc<dyn Fn() + Send + Sync>) -> Result<()> {
    let l = live(&id);
    {
        let mut g = l.lock().unwrap();
        if g.is_up() {
            bail!("el servidor ya está en marcha");
        }
        *g = Live { lines: std::mem::take(&mut g.lines), dropped: g.dropped, status: Some(Status::Preparing), ..Default::default() };
        g.push(String::new());
        g.push(format!("==> Arrancando ({})", chrono_now()));
    }
    notify();
    let r = run_inner(&client, &p, &id, &l, notify.clone()).await;
    {
        let mut g = l.lock().unwrap();
        g.status = Some(Status::Stopped);
        g.players.clear();
        g.stdin = None;
        g.kill = None;
        g.tunnel = Tunnel::Off;
        if let Err(e) = &r {
            let msg = format!("{e:#}");
            g.push(format!("[error] {msg}"));
            g.error = Some(msg);
        }
    }
    super::playit::release(&id);
    super::p2p::maybe_shutdown();
    notify();
    r
}

fn chrono_now() -> String {
    let secs = crate::config::now_secs();
    let (h, m) = ((secs / 3600) % 24, (secs / 60) % 60);
    format!("{h:02}:{m:02} UTC")
}

async fn run_inner(client: &reqwest::Client, p: &Paths, id: &str, l: &Arc<Mutex<Live>>, notify: Arc<dyn Fn() + Send + Sync>) -> Result<()> {
    let mut s = Server::load(p, id)?;
    if !s.eula_accepted {
        bail!("hay que aceptar el EULA de Minecraft antes de arrancar (Ajustes del servidor)");
    }
    // lo que pasa mientras se prepara va a la consola del servidor
    let rep = {
        let l = l.clone();
        let n = notify.clone();
        Reporter::new(move |e| {
            let line = match e {
                crate::progress::Event::Stage(t) => Some(format!("==> {t}")),
                crate::progress::Event::Log(t) => Some(t),
                _ => None,
            };
            if let Some(t) = line {
                l.lock().unwrap().push(t);
                n();
            }
        })
    };
    if !s.installed(p) {
        super::install::install(client, p, &mut s, &rep).await?;
    }
    s.write_files(p)?;
    if l.lock().unwrap().status == Some(Status::Stopping) {
        bail!("cancelado");
    }
    let settings = crate::config::Settings::load(p);
    // el runtime de Java que pide la versión, si su JSON ya está (para arrancar también sin red)
    let component = match crate::game::mojang::read_json(p, &s.mc_version) {
        Ok(j) => crate::game::mojang::java_component(&j),
        Err(_) => crate::game::mojang::resolve(client, p, &s.mc_version).await.ok().and_then(|v| crate::game::mojang::java_component(&v.json)),
    };
    let java = crate::game::java::pick(
        client,
        p,
        None,
        Some(settings.java_path.as_str()),
        s.java_major.max(8),
        component.as_deref(),
        settings.java_auto_download,
        &rep,
    )
    .await?;
    let java = crate::game::java::console_variant(&java);
    let cmd = command(&java, &s)?;
    rep.log(format!("[brasa] {}", cmd.join(" ")));

    let mut command = tokio::process::Command::new(&cmd[0]);
    command.args(&cmd[1..]).current_dir(s.dir(p)).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    // en Windows, sin ventana de consola aparte: la consola es la de Brasa
    #[cfg(windows)]
    command.creation_flags(0x0800_0000);
    let mut child = command.spawn().with_context(|| format!("no se pudo ejecutar {}", cmd[0]))?;
    s.last_started = Some(crate::config::now_secs());
    Server::update(p, &s.id, |f| {
        f.last_started = s.last_started;
        Ok(())
    })?;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let killer = Arc::new(tokio::sync::Notify::new());
    {
        let mut g = l.lock().unwrap();
        g.status = Some(Status::Starting);
        g.started = Some(Instant::now());
        g.stdin = Some(tx);
        g.kill = Some(killer.clone());
    }
    notify();

    let mut stdin = child.stdin.take().context("stdin")?;
    tokio::spawn(async move {
        while let Some(cmd) = rx.recv().await {
            if stdin.write_all(format!("{cmd}\n").as_bytes()).await.is_err() || stdin.flush().await.is_err() {
                break;
            }
        }
    });
    let pump = |r: Box<dyn tokio::io::AsyncRead + Unpin + Send>| {
        let l = l.clone();
        let n = notify.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(r).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let mut g = l.lock().unwrap();
                if let Some(ev) = parse(&line) {
                    apply(&mut g, ev);
                }
                g.push(line);
                drop(g);
                n();
            }
        })
    };
    let t1 = pump(Box::new(child.stdout.take().context("stdout")?));
    let t2 = pump(Box::new(child.stderr.take().context("stderr")?));

    // Brasa P2P: que los amigos emparejados puedan entrar
    {
        let (p2, l2, n2) = (p.clone(), l.clone(), notify.clone());
        tokio::spawn(async move {
            if let Err(e) = super::p2p::endpoint(&p2).await {
                l2.lock().unwrap().push(format!("[brasa] Brasa P2P no disponible: {e:#}"));
                n2();
            }
        });
    }
    if s.playit {
        super::playit::serve(client.clone(), p.clone(), s.clone(), l.clone(), notify.clone());
    }

    let status = tokio::select! {
        st = child.wait() => st?,
        _ = killer.notified() => {
            let _ = child.start_kill();
            l.lock().unwrap().push("[brasa] Servidor cerrado a la fuerza".into());
            child.wait().await?
        }
    };
    let _ = tokio::time::timeout(Duration::from_secs(2), async {
        let _ = t1.await;
        let _ = t2.await;
    })
    .await;
    let code = status.code().unwrap_or(-1);
    let mut g = l.lock().unwrap();
    let exit = if code == 0 { "Servidor detenido".to_string() } else { format!("El servidor se cerró con código {code}") };
    g.push(format!("==> {exit}"));
    g.exit = Some(exit);
    if code != 0 && g.error.is_none() && g.status != Some(Status::Stopping) {
        g.error = Some(format!("el servidor se cerró con código {code}: mira la consola"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_console_of_each_server_type() {
        assert_eq!(parse("[12:00:01] [Server thread/INFO]: Steve joined the game"), Some(Event::Joined("Steve".into())));
        assert_eq!(parse("[12:00:01 INFO]: Javi_Brasa left the game"), Some(Event::Left("Javi_Brasa".into())));
        assert_eq!(parse("[12:00:01] [Server thread/INFO] [minecraft/MinecraftServer]: Alex joined the game"), Some(Event::Joined("Alex".into())));
        assert_eq!(parse("[12:00:01] [Server thread/INFO]: Done (3.452s)! For help, type \"help\""), Some(Event::Ready));
        assert_eq!(parse("[12:00:01 INFO]: Done (12.0s)! For help, type \"help\""), Some(Event::Ready));
        assert_eq!(
            parse("[12:00:01] [Server thread/INFO]: There are 2 of a max of 20 players online: Steve, Alex"),
            Some(Event::Players(vec!["Steve".into(), "Alex".into()]))
        );
        assert_eq!(parse("[12:00:01] [Server thread/INFO]: There are 0 of a max of 20 players online: "), Some(Event::Players(vec![])));
        assert_eq!(parse("[12:00:01] [Server thread/WARN]: **** FAILED TO BIND TO PORT!"), Some(Event::PortInUse));
        assert_eq!(parse("[12:00:01] [main/INFO]: You need to agree to the EULA in order to run the server."), Some(Event::Eula));
        // un chat que imita el mensaje no cuela: el nombre tendría espacios o símbolos
        assert_eq!(parse("[12:00:01] [Server thread/INFO]: <Steve> hola joined the game"), None);
        assert_eq!(parse("[12:00:01] [Server thread/INFO]: Preparing spawn area: 83%"), None);
    }

    #[test]
    fn players_follow_the_events() {
        let mut l = Live { status: Some(Status::Starting), ..Default::default() };
        for line in
            ["x]: Done (1.0s)! For help", "x]: Steve joined the game", "x]: Alex joined the game", "x]: Steve joined the game", "x]: Steve left the game"]
        {
            if let Some(e) = parse(line) {
                apply(&mut l, e);
            }
        }
        assert_eq!(l.status, Some(Status::Running));
        assert_eq!(l.players, vec!["Alex".to_string()]);
    }

    #[test]
    fn command_line() {
        let mut s = Server { memory_mb: 4096, launch: Some(Launch::Jar("paper.jar".into())), ..Default::default() };
        let java = std::path::Path::new("/usr/bin/java");
        assert_eq!(command(java, &s).unwrap(), vec!["/usr/bin/java", "-Xms1024M", "-Xmx4096M", "-jar", "paper.jar", "nogui"]);
        s.launch = Some(Launch::Args("libraries/x/unix_args.txt".into()));
        s.memory_mb = 100;
        assert_eq!(command(java, &s).unwrap(), vec!["/usr/bin/java", "-Xms512M", "-Xmx512M", "@libraries/x/unix_args.txt", "nogui"]);
        s.launch = None;
        assert!(command(java, &s).is_err());
    }

    #[test]
    fn console_keeps_the_last_lines() {
        let mut l = Live::default();
        for i in 0..MAX_LINES + 10 {
            l.push(i.to_string());
        }
        assert_eq!(l.lines.len(), MAX_LINES);
        assert_eq!(l.dropped, 10);
        assert_eq!(l.lines.front().map(String::as_str), Some("10"));
    }
}
