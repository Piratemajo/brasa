//! Brasa P2P: tus amigos entran a tu servidor sin abrir puertos ni usar servicios de túneles.
//!
//! Cómo funciona (ver también el esquema en el CHANGELOG):
//! - Cada Brasa tiene una **clave propia** (Ed25519) que nunca sale del equipo. La conexión es QUIC
//!   con TLS 1.3 (iroh): cifrada de punta a punta y autenticada con esa clave.
//! - El anfitrión crea **invitaciones personales**: un código de un solo uso que caduca, atado a un
//!   nombre de Minecraft. Del secreto sólo guarda su SHA-256.
//! - Al usarla, el equipo del amigo queda **emparejado** con ese nombre. Las siguientes veces entra
//!   sin código. El anfitrión puede revocarlo y, con el **portero**, aprobar cada emparejamiento.
//! - Cada conexión de Minecraft viaja en su propio stream. Antes de dejarla pasar se lee el
//!   paquete de login: **si el nombre no es el emparejado, se corta** (aunque el servidor sea no
//!   premium, nadie puede entrar con el nombre de otro).
//! - Se intenta siempre la **conexión directa** PC a PC; si la red no la deja, va por un relé de
//!   iroh, que sólo ve paquetes cifrados. «Ocultar mi IP» fuerza el relé.
//! - El servidor escucha sólo en `127.0.0.1`: el túnel es la única puerta, y sólo lleva a su puerto.

use super::{Friend, Invite, Server};
use crate::config::now_secs;
use crate::paths::Paths;
use anyhow::{bail, Context, Result};
use base64::Engine;
use iroh::endpoint::{presets, Connection, RecvStream, SendStream};
use iroh::{Endpoint, EndpointAddr, SecretKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;

pub const ALPN: &[u8] = b"brasa/mc/1";
const CODE_PREFIX: &str = "brasa1.";
const KEY_NAME: &str = "p2p-key";
/// Conexiones de Minecraft a la vez por amigo (de sobra para jugar; corta abusos).
const MAX_STREAMS: usize = 8;
/// Intentos fallidos de un mismo equipo antes de dejar de escucharle un rato.
const MAX_FAILS: u32 = 5;
const FAIL_WINDOW: Duration = Duration::from_secs(600);
/// Lo que se espera a que el anfitrión acepte en el portero.
pub const APPROVAL_WAIT: Duration = Duration::from_secs(180);

// =============================================================================== identidad

fn key_file(p: &Paths) -> std::path::PathBuf {
    p.root.join("p2p.key")
}

/// La clave de este equipo: se crea la primera vez y se guarda en el almacén seguro del sistema
/// (en Linux, en un fichero con permisos 600).
pub fn identity(p: &Paths) -> Result<SecretKey> {
    let stored = crate::account::secrets::get(KEY_NAME).or_else(|| std::fs::read_to_string(key_file(p)).ok());
    if let Some(hex) = stored {
        if let Ok(b) = hex::decode(hex.trim()) {
            if let Ok(arr) = <[u8; 32]>::try_from(b.as_slice()) {
                return Ok(SecretKey::from_bytes(&arr));
            }
        }
    }
    let k = SecretKey::generate();
    let hex = hex::encode(k.to_bytes());
    if !crate::account::secrets::set(KEY_NAME, &hex) {
        crate::paths::write_atomic_private(&key_file(p), &hex).context("no se pudo guardar la clave P2P")?;
    }
    Ok(k)
}

/// «3fd1·77ce»: lo que se enseña de una clave para reconocerla.
pub fn short(id: &str) -> String {
    let c: Vec<char> = id.chars().collect();
    if c.len() < 8 {
        return id.to_string();
    }
    format!("{}·{}", c[..4].iter().collect::<String>(), c[c.len() - 4..].iter().collect::<String>())
}

// =============================================================================== códigos

/// Lo que va dentro de un código de invitación.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Ticket {
    pub v: u8,
    /// Cómo llegar al anfitrión (clave + relé + direcciones).
    pub addr: EndpointAddr,
    pub server: String,
    pub title: String,
    pub invite: String,
    pub secret: String,
    /// Nombre de Minecraft al que va atada.
    pub name: String,
}

pub fn encode_ticket(t: &Ticket) -> String {
    let json = serde_json::to_vec(t).unwrap_or_default();
    format!("{CODE_PREFIX}{}", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json))
}

pub fn decode_ticket(code: &str) -> Result<Ticket> {
    let code: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    let body = code.strip_prefix(CODE_PREFIX).context("no es un código de Brasa (empieza por «brasa1.»)")?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(body).map_err(|_| anyhow::anyhow!("el código está incompleto o mal copiado"))?;
    let t: Ticket = serde_json::from_slice(&bytes).context("el código está incompleto o mal copiado")?;
    if t.v != 1 {
        bail!("este código es de otra versión de Brasa: actualiza");
    }
    Ok(t)
}

fn random_token(bytes: usize) -> String {
    let mut out = String::new();
    while out.len() < bytes * 2 {
        out.push_str(&uuid::Uuid::new_v4().simple().to_string());
    }
    out.truncate(bytes * 2);
    out
}

pub fn sha256_hex(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

/// Comparación en tiempo constante (para no dar pistas del secreto por lo que tarda).
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Crea una invitación para `name`, la guarda en el servidor y devuelve el código.
pub async fn invite(p: &Paths, server_id: &str, name: &str, hours: u64) -> Result<String> {
    let name = name.trim();
    if !super::valid_player(name) {
        bail!("el nombre de Minecraft debe tener de 3 a 16 letras, números o _");
    }
    let mut s = Server::load(p, server_id)?;
    if s.friends.iter().any(|f| f.name.eq_ignore_ascii_case(name)) {
        bail!("{name} ya está emparejado: revócalo antes si quieres invitarle desde otro equipo");
    }
    let ep = endpoint(p).await?;
    let secret = random_token(16);
    let id = random_token(4);
    let addr = ep.addr();
    let t = Ticket { v: 1, addr, server: s.id.clone(), title: s.name.clone(), invite: id.clone(), secret: secret.clone(), name: name.to_string() };
    let code = encode_ticket(&t);
    let now = now_secs();
    s.invites.retain(|i| i.pending(now) && !i.name.eq_ignore_ascii_case(name));
    s.invites.push(Invite {
        id,
        secret_sha256: sha256_hex(&secret),
        name: name.to_string(),
        created: now,
        expires: now + hours.clamp(1, 24 * 30) * 3600,
        used: false,
        code: code.clone(),
    });
    let new = s.invites.last().cloned();
    Server::update(p, server_id, |f| {
        f.invites.retain(|i| i.pending(now) && !i.name.eq_ignore_ascii_case(name));
        f.invites.extend(new);
        Ok(())
    })?;
    Ok(code)
}

// =============================================================================== protocolo

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Hello {
    pub v: u8,
    pub server: String,
    /// (id, secreto) la primera vez.
    pub invite: Option<(String, String)>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct ModRef {
    pub title: String,
    pub file: String,
    pub sha1: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Welcome {
    pub ok: bool,
    pub reason: String,
    pub title: String,
    pub name: String,
    pub mc: String,
    pub kind: super::Kind,
    pub loader_version: Option<String>,
    /// Mods que también necesita el jugador.
    pub mods: Vec<ModRef>,
}

async fn write_msg<T: Serialize>(s: &mut SendStream, m: &T) -> Result<()> {
    let b = serde_json::to_vec(m)?;
    s.write_all(&(b.len() as u32).to_be_bytes()).await?;
    s.write_all(&b).await?;
    Ok(())
}

async fn read_msg<T: for<'a> Deserialize<'a>>(r: &mut RecvStream) -> Result<T> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len).await.context("la conexión se cortó")?;
    let n = u32::from_be_bytes(len) as usize;
    if n > 256 * 1024 {
        bail!("mensaje demasiado grande");
    }
    let mut b = vec![0u8; n];
    r.read_exact(&mut b).await.context("la conexión se cortó")?;
    Ok(serde_json::from_slice(&b)?)
}

// ------------------------------------------------------------------- paquetes de Minecraft

fn read_varint(b: &[u8], pos: &mut usize) -> Option<i32> {
    let mut v: i32 = 0;
    for i in 0..5 {
        let byte = *b.get(*pos)?;
        *pos += 1;
        v |= ((byte & 0x7f) as i32) << (7 * i);
        if byte & 0x80 == 0 {
            return Some(v);
        }
    }
    None
}

fn read_string(b: &[u8], pos: &mut usize, max: usize) -> Option<String> {
    let n = read_varint(b, pos)? as usize;
    if n > max * 4 {
        return None;
    }
    let s = std::str::from_utf8(b.get(*pos..*pos + n)?).ok()?.to_string();
    *pos += n;
    Some(s)
}

/// Lo que dicen los primeros bytes de una conexión de Minecraft.
#[derive(Debug, PartialEq)]
pub enum Peek {
    /// Faltan bytes.
    More,
    /// Consulta de estado (la lista de servidores): no entra nadie.
    Status,
    /// Quiere entrar con este nombre.
    Login(String),
    /// No es Minecraft.
    Garbage,
}

/// Lee el handshake y, si es para entrar, el «Login Start» con el nombre. Sin compresión ni
/// cifrado todavía: en este punto del protocolo van en claro (dentro de nuestro túnel cifrado).
pub fn peek(b: &[u8]) -> Peek {
    if b.first() == Some(&0xFE) {
        return Peek::Status; // ping antiguo
    }
    let packet = |pos: &mut usize| -> Option<Option<(i32, usize, usize)>> {
        let start = *pos;
        let Some(len) = read_varint(b, pos) else {
            return if b.len() - start >= 5 { None } else { Some(None) };
        };
        if !(1..=2048).contains(&len) {
            return None;
        }
        let body = *pos;
        // el id del paquete llega enseguida: si no es 0, no es Minecraft (aunque falten bytes)
        let mut q = body;
        if let Some(id) = read_varint(b, &mut q) {
            if id != 0 {
                return None;
            }
        }
        if b.len() < body + len as usize {
            return Some(None);
        }
        let id = read_varint(b, pos)?;
        Some(Some((id, body, body + len as usize)))
    };
    let mut pos = 0;
    let Some(hs) = packet(&mut pos) else { return Peek::Garbage };
    let Some((id, _, end)) = hs else { return Peek::More };
    if id != 0 {
        return Peek::Garbage;
    }
    let Some(_proto) = read_varint(b, &mut pos) else { return Peek::Garbage };
    if read_string(b, &mut pos, 255).is_none() || pos + 2 > end {
        return Peek::Garbage;
    }
    pos += 2;
    let Some(next) = read_varint(b, &mut pos) else { return Peek::Garbage };
    pos = end;
    match next {
        1 => Peek::Status,
        2 | 3 => {
            let Some(login) = packet(&mut pos) else { return Peek::Garbage };
            let Some((id, _, _)) = login else { return Peek::More };
            if id != 0 {
                return Peek::Garbage;
            }
            match read_string(b, &mut pos, 16) {
                Some(n) => Peek::Login(n),
                None => Peek::Garbage,
            }
        }
        _ => Peek::Garbage,
    }
}

// =============================================================================== estado

/// Alguien quiere emparejarse y el portero está activado.
pub struct Pending {
    pub key: u64,
    pub server_name: String,
    pub name: String,
    pub peer: String,
    tx: Option<tokio::sync::oneshot::Sender<bool>>,
}

/// Un amigo conectado a uno de mis servidores.
#[derive(Clone, Debug, PartialEq)]
pub struct PeerView {
    pub server: String,
    pub name: String,
    pub direct: bool,
    pub rtt_ms: u32,
}

/// Mi conexión a un servidor de un amigo.
#[derive(Clone, Debug, PartialEq)]
pub enum GuestState {
    Connecting,
    Ready { port: u16, direct: bool, rtt_ms: u32 },
    Failed(String),
}

struct Peer {
    server: String,
    name: String,
    conn: Connection,
}

struct GuestTunnel {
    conn: Option<Connection>,
    state: GuestState,
    task: Option<tokio::task::JoinHandle<()>>,
}

#[derive(Default)]
struct Net {
    ep: Option<Endpoint>,
    hide_ip: bool,
    pending: Vec<Pending>,
    next_key: u64,
    peers: HashMap<usize, Peer>,
    fails: HashMap<String, (u32, Instant)>,
    guests: HashMap<String, GuestTunnel>,
    /// Cambió algo guardado en disco (amigos, invitaciones): la interfaz recarga.
    dirty: bool,
    notify: Option<Arc<dyn Fn() + Send + Sync>>,
    log: Vec<(String, String)>,
}

fn net() -> &'static Mutex<Net> {
    static N: OnceLock<Mutex<Net>> = OnceLock::new();
    N.get_or_init(Default::default)
}

fn ping_ui() {
    let n = net().lock().unwrap().notify.clone();
    if let Some(n) = n {
        n();
    }
}

/// Para repintar la interfaz cuando cambia algo.
pub fn set_notify(f: Arc<dyn Fn() + Send + Sync>) {
    net().lock().unwrap().notify = Some(f);
}

pub fn take_dirty() -> bool {
    std::mem::take(&mut net().lock().unwrap().dirty)
}

/// Avisos para la consola de cada servidor (id del servidor, línea).
pub fn take_log() -> Vec<(String, String)> {
    std::mem::take(&mut net().lock().unwrap().log)
}

fn log(server: &str, line: String) {
    net().lock().unwrap().log.push((server.to_string(), line));
    ping_ui();
}

pub fn pending() -> Vec<(u64, String, String, String)> {
    net().lock().unwrap().pending.iter().map(|p| (p.key, p.server_name.clone(), p.name.clone(), short(&p.peer))).collect()
}

pub fn decide(key: u64, accept: bool) {
    let mut n = net().lock().unwrap();
    if let Some(i) = n.pending.iter().position(|p| p.key == key) {
        let mut p = n.pending.remove(i);
        if let Some(tx) = p.tx.take() {
            let _ = tx.send(accept);
        }
    }
}

fn path_of(c: &Connection) -> (bool, u32) {
    let paths = c.paths();
    let sel = paths.iter().find(|p| p.is_selected());
    match sel {
        Some(p) => (p.is_ip(), p.rtt().as_millis().min(9999) as u32),
        None => (false, 0),
    }
}

/// Amigos conectados ahora a mis servidores.
pub fn peers() -> Vec<PeerView> {
    let n = net().lock().unwrap();
    n.peers
        .values()
        .filter(|p| p.conn.close_reason().is_none())
        .map(|p| {
            let (direct, rtt_ms) = path_of(&p.conn);
            PeerView { server: p.server.clone(), name: p.name.clone(), direct, rtt_ms }
        })
        .collect()
}

pub fn guest_state(key: &str) -> Option<GuestState> {
    let mut n = net().lock().unwrap();
    let g = n.guests.get_mut(key)?;
    if let (GuestState::Ready { port, .. }, Some(c)) = (&g.state, &g.conn) {
        if c.close_reason().is_some() {
            g.state = GuestState::Failed("se cortó la conexión con tu amigo".into());
        } else {
            let (direct, rtt_ms) = path_of(c);
            g.state = GuestState::Ready { port: *port, direct, rtt_ms };
        }
    }
    Some(g.state.clone())
}

// =============================================================================== endpoint

/// El endpoint de este equipo (se crea la primera vez que hace falta).
pub async fn endpoint(p: &Paths) -> Result<Endpoint> {
    let hide = crate::config::Settings::load(p).p2p_hide_ip;
    {
        let n = net().lock().unwrap();
        if let Some(ep) = &n.ep {
            if n.hide_ip == hide && !ep.is_closed() {
                return Ok(ep.clone());
            }
        }
    }
    shutdown().await;
    let key = identity(p)?;
    let mut b = Endpoint::builder(presets::N0).secret_key(key).alpns(vec![ALPN.to_vec()]);
    if hide {
        // sólo relé: nadie ve tu IP (ni tú la suya)
        b = b.clear_ip_transports();
    }
    let ep = b.bind().await.map_err(|e| anyhow::anyhow!("no se pudo preparar la conexión P2P: {e}"))?;
    // sin relé (sin internet) sigue valiendo para la red de casa
    let _ = tokio::time::timeout(Duration::from_secs(8), ep.online()).await;
    {
        let mut n = net().lock().unwrap();
        n.ep = Some(ep.clone());
        n.hide_ip = hide;
    }
    let (ep2, p2) = (ep.clone(), p.clone());
    tokio::spawn(async move {
        while let Some(incoming) = ep2.accept().await {
            let p3 = p2.clone();
            tokio::spawn(async move {
                if let Ok(conn) = incoming.await {
                    host_conn(p3, conn).await;
                }
            });
        }
    });
    Ok(ep)
}

/// Cierra el endpoint (al cambiar «ocultar mi IP» o al salir).
pub async fn shutdown() {
    let ep = net().lock().unwrap().ep.take();
    if let Some(ep) = ep {
        ep.close().await;
    }
}

/// Sin servidores encendidos ni túneles a amigos, se desconecta (no hay nada que atender).
pub fn maybe_shutdown() {
    let busy = !super::run::running_ids().is_empty()
        || net().lock().unwrap().guests.values().any(|g| matches!(g.state, GuestState::Ready { .. } | GuestState::Connecting));
    if !busy {
        if let Ok(h) = tokio::runtime::Handle::try_current() {
            h.spawn(shutdown());
        }
    }
}

// =============================================================================== anfitrión

fn too_many_fails(peer: &str) -> bool {
    let n = net().lock().unwrap();
    matches!(n.fails.get(peer), Some((c, t)) if *c >= MAX_FAILS && t.elapsed() < FAIL_WINDOW)
}

fn fail(peer: &str) {
    let mut n = net().lock().unwrap();
    let e = n.fails.entry(peer.to_string()).or_insert((0, Instant::now()));
    if e.1.elapsed() > FAIL_WINDOW {
        *e = (0, Instant::now());
    }
    e.0 += 1;
}

/// ¿Puede entrar? Devuelve el servidor y el nombre atado, o el motivo para el amigo.
pub async fn authorize(p: &Paths, peer: &str, hello: &Hello) -> std::result::Result<(Server, String), String> {
    let Ok(mut s) = Server::load(p, &hello.server) else { return Err("ese servidor ya no existe".into()) };
    if !super::run::is_running(&s.id) {
        return Err(format!("«{}» está apagado: pídele a tu amigo que lo encienda", s.name));
    }
    if let Some(f) = s.friends.iter().find(|f| f.id == peer) {
        return Ok((s.clone(), f.name.clone()));
    }
    let Some((inv_id, secret)) = &hello.invite else {
        return Err("este equipo no está emparejado (o te han quitado el acceso): pide una invitación".into());
    };
    let now = now_secs();
    let Some(inv) = s.invites.iter().find(|i| &i.id == inv_id).cloned() else { return Err("esa invitación no existe o la han anulado".into()) };
    if inv.used {
        return Err("esa invitación ya se usó: pide otra".into());
    }
    if now >= inv.expires {
        return Err("esa invitación ha caducado: pide otra".into());
    }
    if !same(&sha256_hex(secret), &inv.secret_sha256) {
        return Err("el código no es válido".into());
    }
    if s.gatekeeper {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let key = {
            let mut n = net().lock().unwrap();
            n.next_key += 1;
            let key = n.next_key;
            n.pending.push(Pending { key, server_name: s.name.clone(), name: inv.name.clone(), peer: peer.to_string(), tx: Some(tx) });
            key
        };
        ping_ui();
        let ok = matches!(tokio::time::timeout(APPROVAL_WAIT, rx).await, Ok(Ok(true)));
        net().lock().unwrap().pending.retain(|x| x.key != key);
        ping_ui();
        if !ok {
            return Err("tu amigo no ha aceptado la invitación".into());
        }
        // pudo cambiar mientras se esperaba
        s = Server::load(p, &hello.server).map_err(|e| e.to_string())?;
        if s.invites.iter().any(|i| &i.id == inv_id && i.used) {
            return Err("esa invitación ya se usó: pide otra".into());
        }
    }
    // marcar la invitación y emparejar, de una vez (si otro equipo la usó justo antes, no vale)
    let paired = Server::update(p, &hello.server, |f| {
        if f.invites.iter().any(|i| &i.id == inv_id && i.used) {
            return Ok(false);
        }
        for i in f.invites.iter_mut().filter(|i| &i.id == inv_id) {
            i.used = true;
            i.code.clear();
        }
        f.friends.retain(|x| x.id != peer && !x.name.eq_ignore_ascii_case(&inv.name));
        f.friends.push(Friend { id: peer.to_string(), name: inv.name.clone(), paired: now, last_seen: Some(now) });
        Ok(true)
    })
    .map_err(|e| e.to_string())?;
    if !paired {
        return Err("esa invitación ya se usó: pide otra".into());
    }
    net().lock().unwrap().dirty = true;
    log(&s.id, format!("[brasa] {} emparejado con este servidor (equipo {})", inv.name, short(peer)));
    // con lista blanca, que también esté en ella (el nombre ya lo garantiza el túnel)
    if s.whitelist {
        super::run::send(&s.id, &format!("whitelist add {}", inv.name));
    }
    Ok((s, inv.name))
}

fn mods_for_players(p: &Paths, s: &Server) -> Vec<ModRef> {
    let Some(folder) = s.kind.content_dir().filter(|_| s.kind.mods()) else { return vec![] };
    super::content::installed(p, s)
        .into_iter()
        .filter(|c| super::content::for_players(s, c))
        .filter_map(|c| {
            let sha1 = crate::net::sha1_file(&s.dir(p).join(folder).join(&c.file)).ok()?;
            Some(ModRef { title: c.title, file: c.file, sha1 })
        })
        .collect()
}

async fn host_conn(p: Paths, conn: Connection) {
    let peer = conn.remote_id().to_string();
    if too_many_fails(&peer) {
        conn.close(1u32.into(), b"too many attempts");
        return;
    }
    let Ok(Ok((mut send, mut recv))) = tokio::time::timeout(Duration::from_secs(20), conn.accept_bi()).await else { return };
    let Ok(Ok(hello)) = tokio::time::timeout(Duration::from_secs(20), read_msg::<Hello>(&mut recv)).await else {
        fail(&peer);
        return;
    };
    let (srv, name) = match authorize(&p, &peer, &hello).await {
        Ok(x) => x,
        Err(reason) => {
            fail(&peer);
            let _ = write_msg(&mut send, &Welcome { ok: false, reason, ..Default::default() }).await;
            let _ = send.finish();
            tokio::time::sleep(Duration::from_millis(500)).await;
            return;
        }
    };
    let w = Welcome {
        ok: true,
        reason: String::new(),
        title: srv.name.clone(),
        name: name.clone(),
        mc: srv.mc_version.clone(),
        kind: srv.kind,
        loader_version: srv.loader_version.clone(),
        mods: mods_for_players(&p, &srv),
    };
    if write_msg(&mut send, &w).await.is_err() {
        return;
    }
    let _ = send.finish();
    let key = conn.stable_id();
    net().lock().unwrap().peers.insert(key, Peer { server: srv.id.clone(), name: name.clone(), conn: conn.clone() });
    ping_ui();
    // ¿sigue siendo amigo? Si lo revocan, se corta en cuanto se nota
    let streams = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    loop {
        let next = tokio::select! {
            r = conn.accept_bi() => r,
            _ = tokio::time::sleep(Duration::from_secs(5)) => {
                if !still_friend(&p, &srv.id, &peer) { conn.close(2u32.into(), b"revoked"); break; }
                continue;
            }
        };
        let Ok((send, recv)) = next else { break };
        if streams.load(std::sync::atomic::Ordering::Relaxed) >= MAX_STREAMS || !still_friend(&p, &srv.id, &peer) {
            continue;
        }
        let (port, name, sid, n) = (srv.port, name.clone(), srv.id.clone(), streams.clone());
        n.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        tokio::spawn(async move {
            if let Err(e) = proxy_in(send, recv, port, &name).await {
                log(&sid, format!("[brasa] {e}"));
            }
            n.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        });
    }
    net().lock().unwrap().peers.remove(&key);
    let _ = Server::update(&p, &srv.id, |s| {
        if let Some(f) = s.friends.iter_mut().find(|f| f.id == peer) {
            f.last_seen = Some(now_secs());
        }
        Ok(())
    });
    net().lock().unwrap().dirty = true;
    ping_ui();
}

fn still_friend(p: &Paths, server: &str, peer: &str) -> bool {
    Server::load(p, server).map(|s| s.friends.iter().any(|f| f.id == peer)).unwrap_or(false)
}

/// Un stream del amigo → el servidor de Minecraft, comprobando antes el nombre.
async fn proxy_in(mut send: SendStream, mut recv: RecvStream, port: u16, name: &str) -> Result<()> {
    let mut buf = Vec::with_capacity(512);
    let mut chunk = [0u8; 512];
    let verdict = loop {
        match peek(&buf) {
            Peek::More if buf.len() < 4096 => {}
            Peek::More => break Peek::Garbage,
            v => break v,
        }
        let n = tokio::time::timeout(Duration::from_secs(15), recv.read(&mut chunk)).await.context("sin datos")??;
        match n {
            Some(0) | None => return Ok(()),
            Some(n) => buf.extend_from_slice(&chunk[..n]),
        }
    };
    match verdict {
        Peek::Login(n) if !n.eq_ignore_ascii_case(name) => {
            let _ = send.finish();
            bail!("Intento de entrar como «{n}» desde el equipo de «{name}»: bloqueado");
        }
        Peek::Garbage => {
            let _ = send.finish();
            return Ok(());
        }
        _ => {}
    }
    let mut tcp = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.context("el servidor no responde en su puerto")?;
    let _ = tcp.set_nodelay(true);
    tcp.write_all(&buf).await?;
    let (mut tr, mut tw) = tcp.into_split();
    let up = async {
        let _ = tokio::io::copy(&mut recv, &mut tw).await;
        let _ = tw.shutdown().await;
    };
    let down = async {
        let _ = tokio::io::copy(&mut tr, &mut send).await;
        let _ = send.finish();
    };
    tokio::join!(up, down);
    Ok(())
}

/// Corta ya a los amigos que ya no están en la lista (al revocar).
pub fn kick_revoked(p: &Paths) {
    let mut n = net().lock().unwrap();
    for peer in n.peers.values() {
        let id = peer.conn.remote_id().to_string();
        if !still_friend(p, &peer.server, &id) {
            peer.conn.close(2u32.into(), b"revoked");
        }
    }
    n.dirty = true;
}

// =============================================================================== invitado

/// Un servidor de un amigo al que me he unido.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Joined {
    pub host: Option<EndpointAddr>,
    pub host_id: String,
    pub server: String,
    pub title: String,
    pub name: String,
    pub mc: String,
    pub kind: super::Kind,
    pub loader_version: Option<String>,
    pub instance: Option<String>,
    /// Mods puestos por Brasa desde el servidor (para quitarlos si el servidor los quita).
    pub synced: Vec<String>,
    pub added: u64,
}

impl Joined {
    pub fn key(&self) -> String {
        format!("{}/{}", self.host_id, self.server)
    }
}

fn joined_file(p: &Paths) -> std::path::PathBuf {
    p.root.join("p2p-joined.json")
}

pub fn joined(p: &Paths) -> Vec<Joined> {
    std::fs::read(joined_file(p)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save_joined(p: &Paths, v: &[Joined]) -> Result<()> {
    crate::paths::write_atomic(&joined_file(p), serde_json::to_vec_pretty(v)?)?;
    Ok(())
}

pub fn forget(p: &Paths, key: &str) -> Result<()> {
    let mut v = joined(p);
    v.retain(|j| j.key() != key);
    save_joined(p, &v)?;
    if let Some(g) = net().lock().unwrap().guests.remove(key) {
        if let Some(t) = g.task {
            t.abort();
        }
        if let Some(c) = g.conn {
            c.close(0u32.into(), b"bye");
        }
    }
    Ok(())
}

/// Conecta con el anfitrión y se presenta. Devuelve la conexión y su respuesta.
async fn hello(p: &Paths, addr: EndpointAddr, server: &str, invite: Option<(String, String)>) -> Result<(Connection, Welcome)> {
    let ep = endpoint(p).await?;
    let conn = tokio::time::timeout(Duration::from_secs(30), ep.connect(addr, ALPN))
        .await
        .context("tu amigo no responde: ¿tiene Brasa abierto y el servidor encendido?")?
        .map_err(|e| anyhow::anyhow!("no se pudo conectar con tu amigo: {e}"))?;
    let (mut send, mut recv) = conn.open_bi().await.map_err(|e| anyhow::anyhow!("{e}"))?;
    write_msg(&mut send, &Hello { v: 1, server: server.to_string(), invite }).await?;
    let wait = APPROVAL_WAIT + Duration::from_secs(20);
    let w: Welcome = tokio::time::timeout(wait, read_msg(&mut recv)).await.context("tu amigo no ha contestado a tiempo")??;
    if !w.ok {
        conn.close(0u32.into(), b"denied");
        bail!("{}", w.reason);
    }
    Ok((conn, w))
}

/// Paso de «Unirse a un amigo», para enseñar el progreso.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JoinStep {
    Code,
    Found,
    Waiting,
    Paired,
    Preparing,
    Done,
}

/// Usa el código: empareja este equipo y guarda el servidor en «De amigos».
pub async fn join(p: &Paths, code: &str, step: impl Fn(JoinStep)) -> Result<(Joined, Welcome)> {
    let t = decode_ticket(code)?;
    step(JoinStep::Code);
    let me = identity(p)?.public().to_string();
    if t.addr.id.to_string() == me {
        bail!("ese código es de un servidor tuyo: dáselo a tu amigo");
    }
    step(JoinStep::Found);
    step(JoinStep::Waiting);
    let (conn, w) = hello(p, t.addr.clone(), &t.server, Some((t.invite.clone(), t.secret.clone()))).await?;
    conn.close(0u32.into(), b"paired");
    step(JoinStep::Paired);
    let mut list = joined(p);
    let j = Joined {
        host: Some(t.addr.clone()),
        host_id: t.addr.id.to_string(),
        server: t.server.clone(),
        title: w.title.clone(),
        name: w.name.clone(),
        mc: w.mc.clone(),
        kind: w.kind,
        loader_version: w.loader_version.clone(),
        added: now_secs(),
        ..Default::default()
    };
    list.retain(|x| x.key() != j.key());
    list.insert(0, j.clone());
    save_joined(p, &list)?;
    Ok((j, w))
}

pub fn update_joined(p: &Paths, j: &Joined) -> Result<()> {
    let mut list = joined(p);
    for x in list.iter_mut().filter(|x| x.key() == j.key()) {
        *x = j.clone();
    }
    save_joined(p, &list)
}

/// Abre el túnel hacia el servidor del amigo: un puerto en este equipo (127.0.0.1) por el que
/// entra Minecraft. Devuelve ese puerto y la respuesta del anfitrión (con sus mods).
pub async fn open_tunnel(p: &Paths, j: &Joined) -> Result<(u16, Welcome)> {
    let key = j.key();
    if let Some(GuestState::Ready { port, .. }) = guest_state(&key) {
        // ya abierto: se vuelve a saludar para tener la lista de mods al día
        let addr = j.host.clone().context("falta la dirección de tu amigo")?;
        let (c, w) = hello(p, addr, &j.server, None).await?;
        c.close(0u32.into(), b"refresh");
        return Ok((port, w));
    }
    net().lock().unwrap().guests.insert(key.clone(), GuestTunnel { conn: None, state: GuestState::Connecting, task: None });
    ping_ui();
    let addr = j.host.clone().context("falta la dirección de tu amigo")?;
    let r = hello(p, addr, &j.server, None).await;
    let (conn, w) = match r {
        Ok(x) => x,
        Err(e) => {
            if let Some(g) = net().lock().unwrap().guests.get_mut(&key) {
                g.state = GuestState::Failed(format!("{e:#}"));
            }
            ping_ui();
            return Err(e);
        }
    };
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
    let port = listener.local_addr()?.port();
    let c2 = conn.clone();
    let k2 = key.clone();
    let task = tokio::spawn(async move {
        loop {
            tokio::select! {
                a = listener.accept() => {
                    let Ok((tcp, _)) = a else { break };
                    let c3 = c2.clone();
                    tokio::spawn(async move {
                        let Ok((mut send, mut recv)) = c3.open_bi().await else { return };
                        let _ = tcp.set_nodelay(true);
                        let (mut tr, mut tw) = tcp.into_split();
                        let up = async { let _ = tokio::io::copy(&mut tr, &mut send).await; let _ = send.finish(); };
                        let down = async { let _ = tokio::io::copy(&mut recv, &mut tw).await; let _ = tw.shutdown().await; };
                        tokio::join!(up, down);
                    });
                }
                _ = c2.closed() => break,
            }
        }
        if let Some(g) = net().lock().unwrap().guests.get_mut(&k2) {
            g.state = GuestState::Failed("se cortó la conexión con tu amigo".into());
            g.conn = None;
        }
        ping_ui();
    });
    let (direct, rtt_ms) = path_of(&conn);
    net().lock().unwrap().guests.insert(key, GuestTunnel { conn: Some(conn), state: GuestState::Ready { port, direct, rtt_ms }, task: Some(task) });
    ping_ui();
    Ok((port, w))
}

/// Pone en la instancia los mods del servidor (por su SHA-1 en Modrinth: el mismo fichero exacto)
/// y quita los que puso Brasa y el servidor ya no tiene. Devuelve los que no se encontraron.
pub async fn sync_mods(client: &reqwest::Client, p: &Paths, j: &mut Joined, inst: &crate::config::Instance, mods: &[ModRef]) -> Result<Vec<String>> {
    let dir = inst.mods_dir(p);
    std::fs::create_dir_all(&dir)?;
    let want: Vec<&str> = mods.iter().map(|m| m.file.as_str()).collect();
    for old in j.synced.iter().filter(|f| !want.contains(&f.as_str())) {
        let _ = std::fs::remove_file(dir.join(old));
    }
    let mut missing = vec![];
    let mut synced = vec![];
    for m in mods {
        if m.file.contains('/') || m.file.contains('\\') || m.file.contains("..") {
            continue;
        }
        let path = dir.join(&m.file);
        if crate::net::sha1_file(&path).map(|h| h.eq_ignore_ascii_case(&m.sha1)).unwrap_or(false) {
            synced.push(m.file.clone());
            continue;
        }
        let v: Result<crate::content::modrinth::Version> =
            crate::net::get_json(client, &format!("https://api.modrinth.com/v2/version_file/{}?algorithm=sha1", m.sha1)).await;
        let Some(f) = v.ok().and_then(|v| v.files.into_iter().find(|f| f.hashes.get("sha1").map(|h| h.eq_ignore_ascii_case(&m.sha1)).unwrap_or(false))) else {
            missing.push(m.title.clone());
            continue;
        };
        crate::net::download_one(client, &crate::net::Download { url: f.url, path: path.clone(), sha1: Some(m.sha1.clone()), size: f.size }).await?;
        synced.push(m.file.clone());
    }
    j.synced = synced;
    Ok(missing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    fn varint(mut v: i32, out: &mut Vec<u8>) {
        loop {
            let mut b = (v & 0x7f) as u8;
            v = ((v as u32) >> 7) as i32;
            if v != 0 {
                b |= 0x80;
            }
            out.push(b);
            if v == 0 {
                break;
            }
        }
    }
    fn string(s: &str, out: &mut Vec<u8>) {
        varint(s.len() as i32, out);
        out.extend_from_slice(s.as_bytes());
    }
    fn packet(body: Vec<u8>) -> Vec<u8> {
        let mut out = vec![];
        varint(body.len() as i32, &mut out);
        out.extend(body);
        out
    }
    pub fn handshake(next: i32) -> Vec<u8> {
        let mut b = vec![];
        varint(0, &mut b);
        varint(769, &mut b);
        string("localhost", &mut b);
        b.extend_from_slice(&25565u16.to_be_bytes());
        varint(next, &mut b);
        packet(b)
    }
    pub fn login(name: &str) -> Vec<u8> {
        let mut b = vec![];
        varint(0, &mut b);
        string(name, &mut b);
        b.extend_from_slice(&[0u8; 16]); // UUID (1.20.2+)
        packet(b)
    }

    #[test]
    fn reads_the_name_from_the_login() {
        let mut all = handshake(2);
        all.extend(login("Primo_Dani"));
        assert_eq!(peek(&all), Peek::Login("Primo_Dani".into()));
        for cut in 0..all.len() - 1 {
            assert_eq!(peek(&all[..cut]), Peek::More, "a trozos ({cut} bytes) hay que esperar");
        }
        assert_eq!(peek(&handshake(1)), Peek::Status);
        assert_eq!(peek(&[0xFE, 0x01]), Peek::Status);
        assert_eq!(peek(b"GET / HTTP/1.1\r\n\r\n"), Peek::Garbage);
        let mut transfer = handshake(3);
        transfer.extend(login("Alex"));
        assert_eq!(peek(&transfer), Peek::Login("Alex".into()));
    }

    #[test]
    fn codes_round_trip_and_reject_junk() {
        let key = SecretKey::generate();
        let t = Ticket {
            v: 1,
            addr: EndpointAddr::new(key.public()),
            server: "survival".into(),
            title: "Survival con amigos".into(),
            invite: "ab12cd34".into(),
            secret: "s3cr3t".into(),
            name: "Marcos".into(),
        };
        let code = encode_ticket(&t);
        assert!(code.starts_with("brasa1."));
        assert_eq!(decode_ticket(&format!("  {code}\n")).unwrap(), t, "espacios y saltos de línea al pegar no importan");
        assert!(decode_ticket("hola").is_err());
        assert!(decode_ticket(&code[..code.len() - 10]).is_err());
        assert_eq!(short("3fd1aaaabbbb77ce"), "3fd1·77ce");
        assert!(same("abc", "abc") && !same("abc", "abd") && !same("abc", "ab"));
    }

    /// De punta a punta, con conexiones QUIC reales en este equipo: invitación, portero,
    /// emparejado, túnel local, nombre atado y revocación.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn friend_joins_plays_and_cannot_impersonate() {
        use crate::hosting::run::{live, Status};
        use tokio::net::TcpListener;
        let tmp = |n: &str| Paths { root: std::env::temp_dir().join(format!("brasa-p2p-{n}-{}", uuid::Uuid::new_v4())) };
        let (host_p, guest_p) = (tmp("host"), tmp("guest"));
        std::fs::create_dir_all(&host_p.root).unwrap();
        std::fs::create_dir_all(&guest_p.root).unwrap();

        // «Minecraft»: guarda lo que recibe y contesta
        let mc = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mc_port = mc.local_addr().unwrap().port();
        let seen = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
        let seen2 = seen.clone();
        tokio::spawn(async move {
            loop {
                let (mut s, _) = mc.accept().await.unwrap();
                let seen3 = seen2.clone();
                tokio::spawn(async move {
                    let mut b = vec![0u8; 4096];
                    let n = s.read(&mut b).await.unwrap_or(0);
                    seen3.lock().unwrap().push(b[..n].to_vec());
                    let _ = s.write_all(b"hola desde el servidor").await;
                });
            }
        });

        // el servidor del anfitrión, encendido
        let mut srv = Server::create(&host_p, "Survival", super::super::Kind::Paper, "1.21.4", None, false).unwrap();
        srv.port = mc_port;
        srv.gatekeeper = true;
        srv.save(&host_p).unwrap();
        live(&srv.id).lock().unwrap().status = Some(Status::Running);

        // el Brasa del anfitrión: su propio endpoint, sin relé, atendiendo con host_conn
        let host_key = SecretKey::generate();
        let host_ep = Endpoint::builder(presets::Minimal)
            .secret_key(host_key.clone())
            .alpns(vec![ALPN.to_vec()])
            .relay_mode(iroh::RelayMode::Disabled)
            .bind()
            .await
            .unwrap();
        let socks: Vec<iroh::TransportAddr> = host_ep
            .bound_sockets()
            .into_iter()
            .filter(|a| a.is_ipv4())
            .map(|a| iroh::TransportAddr::Ip(std::net::SocketAddr::new(std::net::Ipv4Addr::LOCALHOST.into(), a.port())))
            .collect();
        let host_addr = EndpointAddr::from_parts(host_key.public(), socks);
        let hp = host_p.clone();
        let ep2 = host_ep.clone();
        tokio::spawn(async move {
            while let Some(inc) = ep2.accept().await {
                let hp2 = hp.clone();
                tokio::spawn(async move {
                    if let Ok(c) = inc.await {
                        host_conn(hp2, c).await;
                    }
                });
            }
        });

        // invitación para Marcos (a mano: el código usa la dirección de este endpoint de prueba)
        let secret = "secreto-de-prueba".to_string();
        let mut s = Server::load(&host_p, &srv.id).unwrap();
        s.invites.push(Invite {
            id: "inv1".into(),
            secret_sha256: sha256_hex(&secret),
            name: "Marcos".into(),
            created: 0,
            expires: now_secs() + 3600,
            used: false,
            code: String::new(),
        });
        s.save(&host_p).unwrap();
        let code = encode_ticket(&Ticket {
            v: 1,
            addr: host_addr.clone(),
            server: srv.id.clone(),
            title: "Survival".into(),
            invite: "inv1".into(),
            secret: secret.clone(),
            name: "Marcos".into(),
        });

        // el portero acepta en cuanto aparece
        tokio::spawn(async {
            for _ in 0..600 {
                if let Some((k, _, name, _)) = pending().first().cloned() {
                    assert_eq!(name, "Marcos");
                    decide(k, true);
                    return;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        });

        // Marcos se une con su Brasa (el endpoint global, con su propia clave)
        let (j, w) = join(&guest_p, &code, |_| {}).await.expect("unirse");
        assert_eq!((w.name.as_str(), w.title.as_str(), w.mc.as_str()), ("Marcos", "Survival", "1.21.4"));
        let s = Server::load(&host_p, &srv.id).unwrap();
        assert_eq!(s.friends.len(), 1);
        assert_eq!(s.friends[0].name, "Marcos");
        assert!(s.invites[0].used, "la invitación ya no vale");
        assert_eq!(joined(&guest_p).len(), 1);

        // el mismo código desde otro equipo: ya usado
        let again = authorize(&host_p, "otro-equipo", &Hello { v: 1, server: srv.id.clone(), invite: Some(("inv1".into(), secret.clone())) }).await;
        assert!(again.unwrap_err().contains("ya se usó"));
        // y un secreto inventado tampoco
        Server::update(&host_p, &srv.id, |s| {
            s.invites.push(Invite {
                id: "inv2".into(),
                secret_sha256: sha256_hex("bueno"),
                name: "Otro".into(),
                created: 0,
                expires: now_secs() + 60,
                used: false,
                code: String::new(),
            });
            s.gatekeeper = false;
            Ok(())
        })
        .unwrap();
        let bad = authorize(&host_p, "otro-equipo", &Hello { v: 1, server: srv.id.clone(), invite: Some(("inv2".into(), "malo".into())) }).await;
        assert!(bad.unwrap_err().contains("no es válido"));

        // túnel: Minecraft entra por un puerto local
        let (port, _) = open_tunnel(&guest_p, &j).await.expect("túnel");
        let mut c = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let mut hs = handshake(2);
        hs.extend(login("Marcos"));
        c.write_all(&hs).await.unwrap();
        let mut reply = vec![0u8; 64];
        let n = tokio::time::timeout(Duration::from_secs(10), c.read(&mut reply)).await.unwrap().unwrap();
        assert_eq!(&reply[..n], b"hola desde el servidor");
        assert_eq!(seen.lock().unwrap().len(), 1, "el login llegó al servidor tal cual");
        assert_eq!(seen.lock().unwrap()[0], hs);

        // con el nombre de otro: no llega ni a tocar el servidor
        let mut c2 = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let mut fake = handshake(2);
        fake.extend(login("Javi_Brasa"));
        c2.write_all(&fake).await.unwrap();
        let n = tokio::time::timeout(Duration::from_secs(10), c2.read(&mut reply)).await.unwrap().unwrap_or(0);
        assert_eq!(n, 0, "cortado");
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(seen.lock().unwrap().len(), 1, "el servidor no recibió nada");
        assert!(take_log().iter().any(|(_, l)| l.contains("«Javi_Brasa»") && l.contains("bloqueado")));
        assert!(peers().iter().any(|p| p.name == "Marcos"));

        // revocado: se corta y no puede volver
        Server::update(&host_p, &srv.id, |s| {
            s.friends.clear();
            Ok(())
        })
        .unwrap();
        kick_revoked(&host_p);
        let back = open_tunnel(&guest_p, &Joined { ..j.clone() }).await;
        assert!(back.is_err() || matches!(guest_state(&j.key()), Some(GuestState::Failed(_))));
        let denied = hello(&guest_p, host_addr, &srv.id, None).await;
        assert!(denied.unwrap_err().to_string().contains("no está emparejado"));

        host_ep.close().await;
        let _ = std::fs::remove_dir_all(&host_p.root);
        let _ = std::fs::remove_dir_all(&guest_p.root);
    }
}
