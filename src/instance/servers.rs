//! Servidores de una instancia: la lista del juego (`servers.dat`) y su estado en vivo (ping).
//!
//! - `servers.dat` es NBT sin comprimir: un compuesto raíz con la lista `servers`. Se lee y escribe
//!   el árbol NBT completo, así que las claves que Brasa no conoce (p. ej. `acceptTextures`) se
//!   conservan, y lo que se añade aquí aparece también en el menú Multijugador del juego.
//! - El ping usa el protocolo de estado de Minecraft (1.7+): saludo, petición de estado (JSON con
//!   MOTD, jugadores, versión e icono) y un ping para medir la latencia.

use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::path::Path;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

// ============================================================================ NBT

#[derive(Debug, Clone, PartialEq)]
pub enum Tag {
    End,
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<i8>),
    String(String),
    /// Tipo de los elementos + elementos.
    List(u8, Vec<Tag>),
    Compound(Vec<(String, Tag)>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

impl Tag {
    fn id(&self) -> u8 {
        match self {
            Tag::End => 0,
            Tag::Byte(_) => 1,
            Tag::Short(_) => 2,
            Tag::Int(_) => 3,
            Tag::Long(_) => 4,
            Tag::Float(_) => 5,
            Tag::Double(_) => 6,
            Tag::ByteArray(_) => 7,
            Tag::String(_) => 8,
            Tag::List(..) => 9,
            Tag::Compound(_) => 10,
            Tag::IntArray(_) => 11,
            Tag::LongArray(_) => 12,
        }
    }

    pub fn get(&self, key: &str) -> Option<&Tag> {
        match self {
            Tag::Compound(v) => v.iter().find(|(k, _)| k == key).map(|(_, t)| t),
            _ => None,
        }
    }

    pub fn str(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Tag::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn set(&mut self, key: &str, value: Tag) {
        if let Tag::Compound(v) = self {
            match v.iter_mut().find(|(k, _)| k == key) {
                Some((_, t)) => *t = value,
                None => v.push((key.to_string(), value)),
            }
        }
    }
}

struct Reader<'a> {
    b: &'a [u8],
    depth: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8]> {
        if self.b.len() < n {
            bail!("NBT truncado");
        }
        let (a, rest) = self.b.split_at(n);
        self.b = rest;
        Ok(a)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn i16(&mut self) -> Result<i16> {
        Ok(i16::from_be_bytes(self.take(2)?.try_into()?))
    }
    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_be_bytes(self.take(4)?.try_into()?))
    }
    fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_be_bytes(self.take(8)?.try_into()?))
    }
    fn len(&mut self) -> Result<usize> {
        let n = self.i32()?;
        if n < 0 || n as usize > self.b.len().max(16) * 8 {
            bail!("longitud NBT no válida: {n}");
        }
        Ok(n as usize)
    }
    fn string(&mut self) -> Result<String> {
        let n = u16::from_be_bytes(self.take(2)?.try_into()?) as usize;
        // NBT usa «UTF-8 modificado»; para nombres e IPs coincide con UTF-8 normal.
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }
    fn payload(&mut self, id: u8) -> Result<Tag> {
        self.depth += 1;
        if self.depth > 512 {
            bail!("NBT demasiado anidado");
        }
        let t = match id {
            1 => Tag::Byte(self.u8()? as i8),
            2 => Tag::Short(self.i16()?),
            3 => Tag::Int(self.i32()?),
            4 => Tag::Long(self.i64()?),
            5 => Tag::Float(f32::from_bits(self.i32()? as u32)),
            6 => Tag::Double(f64::from_bits(self.i64()? as u64)),
            7 => {
                let n = self.len()?;
                Tag::ByteArray(self.take(n)?.iter().map(|b| *b as i8).collect())
            }
            8 => Tag::String(self.string()?),
            9 => {
                let elem = self.u8()?;
                let n = self.len()?;
                let mut v = Vec::with_capacity(n.min(4096));
                for _ in 0..n {
                    v.push(self.payload(elem)?);
                }
                Tag::List(elem, v)
            }
            10 => {
                let mut v = vec![];
                loop {
                    let id = self.u8()?;
                    if id == 0 {
                        break;
                    }
                    let name = self.string()?;
                    v.push((name, self.payload(id)?));
                }
                Tag::Compound(v)
            }
            11 => {
                let n = self.len()?;
                Tag::IntArray((0..n).map(|_| self.i32()).collect::<Result<_>>()?)
            }
            12 => {
                let n = self.len()?;
                Tag::LongArray((0..n).map(|_| self.i64()).collect::<Result<_>>()?)
            }
            0 => Tag::End,
            x => bail!("tipo NBT desconocido: {x}"),
        };
        self.depth -= 1;
        Ok(t)
    }
}

/// Lee un fichero NBT sin comprimir (compuesto raíz con nombre).
pub fn read_nbt(bytes: &[u8]) -> Result<Tag> {
    let mut r = Reader { b: bytes, depth: 0 };
    let id = r.u8()?;
    if id != 10 {
        bail!("el NBT no empieza por un compuesto");
    }
    let _root_name = r.string()?;
    r.payload(10)
}

fn write_string(out: &mut Vec<u8>, s: &str) {
    let b = s.as_bytes();
    let n = b.len().min(u16::MAX as usize);
    out.extend_from_slice(&(n as u16).to_be_bytes());
    out.extend_from_slice(&b[..n]);
}

fn write_payload(out: &mut Vec<u8>, t: &Tag) {
    match t {
        Tag::End => {}
        Tag::Byte(v) => out.push(*v as u8),
        Tag::Short(v) => out.extend_from_slice(&v.to_be_bytes()),
        Tag::Int(v) => out.extend_from_slice(&v.to_be_bytes()),
        Tag::Long(v) => out.extend_from_slice(&v.to_be_bytes()),
        Tag::Float(v) => out.extend_from_slice(&v.to_bits().to_be_bytes()),
        Tag::Double(v) => out.extend_from_slice(&v.to_bits().to_be_bytes()),
        Tag::ByteArray(v) => {
            out.extend_from_slice(&(v.len() as i32).to_be_bytes());
            out.extend(v.iter().map(|b| *b as u8));
        }
        Tag::String(s) => write_string(out, s),
        Tag::List(elem, v) => {
            let elem = v.first().map(|t| t.id()).unwrap_or(*elem);
            out.push(elem);
            out.extend_from_slice(&(v.len() as i32).to_be_bytes());
            for t in v {
                write_payload(out, t);
            }
        }
        Tag::Compound(v) => {
            for (k, t) in v {
                out.push(t.id());
                write_string(out, k);
                write_payload(out, t);
            }
            out.push(0);
        }
        Tag::IntArray(v) => {
            out.extend_from_slice(&(v.len() as i32).to_be_bytes());
            for x in v {
                out.extend_from_slice(&x.to_be_bytes());
            }
        }
        Tag::LongArray(v) => {
            out.extend_from_slice(&(v.len() as i32).to_be_bytes());
            for x in v {
                out.extend_from_slice(&x.to_be_bytes());
            }
        }
    }
}

pub fn write_nbt(root: &Tag) -> Vec<u8> {
    let mut out = vec![10u8];
    write_string(&mut out, "");
    write_payload(&mut out, root);
    out
}

// ============================================================================ servers.dat

/// Un servidor de la lista. Guarda el compuesto NBT completo para no perder campos desconocidos.
#[derive(Debug, Clone, PartialEq)]
pub struct Server {
    raw: Tag,
}

impl Server {
    pub fn new(name: &str, ip: &str) -> Server {
        Server { raw: Tag::Compound(vec![("name".into(), Tag::String(name.trim().to_string())), ("ip".into(), Tag::String(ip.trim().to_string()))]) }
    }
    pub fn name(&self) -> &str {
        self.raw.str("name").filter(|s| !s.is_empty()).unwrap_or("Servidor de Minecraft")
    }
    pub fn ip(&self) -> &str {
        self.raw.str("ip").unwrap_or("")
    }
    /// Icono guardado por el juego (PNG en base64), si lo hay.
    pub fn icon_b64(&self) -> Option<&str> {
        self.raw.str("icon")
    }
}

/// Servidores de la instancia (vacío si el juego aún no ha creado `servers.dat`).
pub fn load(game_dir: &Path) -> Result<Vec<Server>> {
    let path = game_dir.join("servers.dat");
    let Ok(bytes) = std::fs::read(&path) else { return Ok(vec![]) };
    let root = read_nbt(&bytes).with_context(|| format!("{} está dañado", path.display()))?;
    Ok(match root.get("servers") {
        Some(Tag::List(_, v)) => v.iter().filter(|t| matches!(t, Tag::Compound(_))).map(|t| Server { raw: t.clone() }).collect(),
        _ => vec![],
    })
}

/// Escribe la lista conservando el resto del fichero. El original se copia a `servers.dat_old`
/// (el mismo nombre que usa el juego para su copia de seguridad).
pub fn save(game_dir: &Path, servers: &[Server]) -> Result<()> {
    let path = game_dir.join("servers.dat");
    let mut root = match std::fs::read(&path) {
        Ok(b) => {
            let _ = std::fs::copy(&path, game_dir.join("servers.dat_old"));
            read_nbt(&b).unwrap_or(Tag::Compound(vec![]))
        }
        Err(_) => Tag::Compound(vec![]),
    };
    root.set("servers", Tag::List(10, servers.iter().map(|s| s.raw.clone()).collect()));
    std::fs::create_dir_all(game_dir)?;
    crate::paths::write_atomic(&path, write_nbt(&root))?;
    Ok(())
}

/// `host`, `host:puerto`, `[::1]:25565`. Devuelve host y puerto (25565 por defecto).
pub fn parse_address(addr: &str) -> Result<(String, u16)> {
    let a = addr.trim();
    if a.is_empty() {
        bail!("dirección vacía");
    }
    if let Some(rest) = a.strip_prefix('[') {
        let (host, tail) = rest.split_once(']').context("falta «]» en la dirección IPv6")?;
        let port = match tail.strip_prefix(':') {
            Some(p) => p.parse().context("puerto no válido")?,
            None => 25565,
        };
        return Ok((host.to_string(), port));
    }
    match a.rsplit_once(':') {
        Some((h, p)) if !h.contains(':') => Ok((h.to_string(), p.parse().context("puerto no válido")?)),
        _ => Ok((a.to_string(), 25565)),
    }
}

// ============================================================================ ping

#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    pub motd: String,
    pub version: String,
    pub online: i64,
    pub max: i64,
    pub latency_ms: u32,
    /// PNG en base64 (sin el prefijo `data:image/png;base64,`).
    pub favicon_b64: Option<String>,
    /// Algunos nombres de jugadores conectados (si el servidor los comparte).
    pub sample: Vec<String>,
}

fn write_varint(out: &mut Vec<u8>, v: i32) {
    let mut u = v as u32;
    loop {
        if u & !0x7F == 0 {
            out.push(u as u8);
            return;
        }
        out.push((u as u8 & 0x7F) | 0x80);
        u >>= 7;
    }
}

async fn read_varint<R: AsyncReadExt + Unpin>(r: &mut R) -> Result<i32> {
    let mut v: u32 = 0;
    for i in 0..5 {
        let b = r.read_u8().await?;
        v |= ((b & 0x7F) as u32) << (7 * i);
        if b & 0x80 == 0 {
            return Ok(v as i32);
        }
    }
    bail!("VarInt demasiado largo")
}

fn packet(id: i32, body: &[u8]) -> Vec<u8> {
    let mut inner = vec![];
    write_varint(&mut inner, id);
    inner.extend_from_slice(body);
    let mut out = vec![];
    write_varint(&mut out, inner.len() as i32);
    out.extend(inner);
    out
}

/// Texto plano de un MOTD: cadena o componente de chat (`text` + `extra`), sin códigos de color «§».
pub fn motd_text(v: &Value) -> String {
    fn walk(v: &Value, out: &mut String) {
        match v {
            Value::String(s) => out.push_str(s),
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            Value::Object(o) => {
                if let Some(t) = o.get("text") {
                    walk(t, out);
                }
                if let Some(e) = o.get("extra") {
                    walk(e, out);
                }
            }
            _ => {}
        }
    }
    let mut raw = String::new();
    walk(v, &mut raw);
    let mut out = String::new();
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c == '§' {
            chars.next();
        } else {
            out.push(c);
        }
    }
    out.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n")
}

/// Interpreta la respuesta JSON de estado.
pub fn parse_status(json: &Value, latency_ms: u32) -> Status {
    Status {
        motd: motd_text(&json["description"]),
        version: json["version"]["name"].as_str().unwrap_or("").to_string(),
        online: json["players"]["online"].as_i64().unwrap_or(0),
        max: json["players"]["max"].as_i64().unwrap_or(0),
        latency_ms,
        favicon_b64: json["favicon"].as_str().map(|f| f.trim_start_matches("data:image/png;base64,").replace(['\n', '\r'], "")),
        sample: json["players"]["sample"]
            .as_array()
            .map(|a| a.iter().filter_map(|p| p["name"].as_str()).map(String::from).take(12).collect())
            .unwrap_or_default(),
    }
}

/// Consulta el estado de un servidor. Tarda como mucho `timeout` en total.
pub async fn ping(addr: &str, timeout: Duration) -> Result<Status> {
    let (host, port) = parse_address(addr)?;
    tokio::time::timeout(timeout, ping_inner(&host, port)).await.map_err(|_| anyhow::anyhow!("no responde"))?
}

async fn ping_inner(host: &str, port: u16) -> Result<Status> {
    let mut s = tokio::net::TcpStream::connect((host, port)).await.map_err(|e| match e.kind() {
        std::io::ErrorKind::ConnectionRefused => anyhow::anyhow!("conexión rechazada"),
        _ => anyhow::anyhow!("no se pudo conectar: {e}"),
    })?;
    s.set_nodelay(true).ok();
    // Saludo: versión de protocolo -1 («la que sea»), dirección y estado siguiente = 1 (status)
    let mut hs = vec![];
    write_varint(&mut hs, -1);
    write_varint(&mut hs, host.len() as i32);
    hs.extend_from_slice(host.as_bytes());
    hs.extend_from_slice(&port.to_be_bytes());
    write_varint(&mut hs, 1);
    let mut req = packet(0x00, &hs);
    req.extend(packet(0x00, &[]));
    let t0 = Instant::now();
    s.write_all(&req).await?;

    let len = read_varint(&mut s).await?;
    if !(1..=1 << 21).contains(&len) {
        bail!("respuesta no válida (¿es un servidor de Minecraft?)");
    }
    let id = read_varint(&mut s).await?;
    if id != 0 {
        bail!("respuesta inesperada del servidor");
    }
    let n = read_varint(&mut s).await?;
    if !(0..=1 << 21).contains(&n) {
        bail!("respuesta no válida");
    }
    let mut buf = vec![0u8; n as usize];
    s.read_exact(&mut buf).await?;
    let first = t0.elapsed();
    let json: Value = serde_json::from_slice(&buf).context("el servidor devolvió un estado no válido")?;

    // Ping/pong para una latencia más fiel; si el servidor no lo contesta, vale la de la respuesta.
    let t1 = Instant::now();
    let pong = async {
        s.write_all(&packet(0x01, &42i64.to_be_bytes())).await?;
        let _len = read_varint(&mut s).await?;
        let _id = read_varint(&mut s).await?;
        let _payload = s.read_i64().await?;
        anyhow::Ok(t1.elapsed())
    };
    let rtt = tokio::time::timeout(Duration::from_secs(2), pong).await.ok().and_then(|r| r.ok()).unwrap_or(first);
    Ok(parse_status(&json, rtt.as_millis().min(u32::MAX as u128) as u32))
}

// ============================================================================ base64 (iconos)

/// Decodifica base64 estándar. Para los iconos de servidor (PNG de 64×64).
pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    }
    let bytes: Vec<u8> = s.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=').collect();
    if bytes.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let mut n = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            n |= val(*c)? << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn nbt_roundtrip_keeps_unknown_fields() {
        let root = Tag::Compound(vec![(
            "servers".into(),
            Tag::List(
                10,
                vec![Tag::Compound(vec![
                    ("name".into(), Tag::String("Mi server".into())),
                    ("ip".into(), Tag::String("play.ejemplo.com".into())),
                    ("acceptTextures".into(), Tag::Byte(1)),
                    ("x".into(), Tag::LongArray(vec![1, -2])),
                    ("d".into(), Tag::Double(1.5)),
                ])],
            ),
        )]);
        let bytes = write_nbt(&root);
        assert_eq!(read_nbt(&bytes).unwrap(), root);
    }

    #[test]
    fn nbt_rejects_garbage() {
        assert!(read_nbt(&[]).is_err());
        assert!(read_nbt(&[10, 0, 0, 9, 0, 1, b'a', 10, 0x7f, 0xff, 0xff, 0xff]).is_err(), "lista con longitud absurda");
        assert!(read_nbt(&[8, 0, 0]).is_err(), "raíz que no es compuesto");
    }

    #[test]
    fn servers_dat_add_remove_and_preserve() {
        let dir = std::env::temp_dir().join(format!("brasa-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(load(&dir).unwrap().is_empty(), "sin fichero: lista vacía");

        // Un servers.dat «del juego» con un campo que Brasa no conoce.
        let mut original = Server::new("Hypixel", "mc.hypixel.net");
        original.raw.set("acceptTextures", Tag::Byte(1));
        save(&dir, &[original]).unwrap();

        let mut list = load(&dir).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!((list[0].name(), list[0].ip()), ("Hypixel", "mc.hypixel.net"));
        list.push(Server::new("  Casa ", " 192.168.1.20:25570 "));
        save(&dir, &list).unwrap();

        let again = load(&dir).unwrap();
        assert_eq!(again.len(), 2);
        assert_eq!(again[1].ip(), "192.168.1.20:25570", "se recortan espacios");
        assert_eq!(again[0].raw.get("acceptTextures"), Some(&Tag::Byte(1)), "campo desconocido conservado");
        assert!(dir.join("servers.dat_old").exists(), "copia del anterior, como hace el juego");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn addresses() {
        assert_eq!(parse_address("mc.hypixel.net").unwrap(), ("mc.hypixel.net".into(), 25565));
        assert_eq!(parse_address("1.2.3.4:25570").unwrap(), ("1.2.3.4".into(), 25570));
        assert_eq!(parse_address("[::1]:25566").unwrap(), ("::1".into(), 25566));
        assert_eq!(parse_address("[::1]").unwrap(), ("::1".into(), 25565));
        assert!(parse_address("host:abc").is_err());
        assert!(parse_address("  ").is_err());
    }

    #[test]
    fn motd_from_text_and_components() {
        assert_eq!(motd_text(&json!("§aBienvenido §lal server")), "Bienvenido al server");
        let comp = json!({"text": "", "extra": [{"text": "Brasa "}, {"text": "SMP", "color": "gold"}, "\n  línea 2 "]});
        assert_eq!(motd_text(&comp), "Brasa SMP\nlínea 2");
    }

    #[test]
    fn status_parsing() {
        let j = json!({"version": {"name": "Paper 1.21.1", "protocol": 767}, "players": {"max": 100, "online": 7, "sample": [{"name": "Alex", "id": "x"}]}, "description": "Hola", "favicon": "data:image/png;base64,iVBO\nRw=="});
        let s = parse_status(&j, 33);
        assert_eq!((s.online, s.max, s.latency_ms), (7, 100, 33));
        assert_eq!(s.version, "Paper 1.21.1");
        assert_eq!(s.sample, vec!["Alex"]);
        assert_eq!(s.favicon_b64.as_deref(), Some("iVBORw=="));
    }

    #[test]
    fn base64() {
        assert_eq!(base64_decode("aG9sYQ==").unwrap(), b"hola");
        assert_eq!(base64_decode("aG9s").unwrap(), b"hol");
        assert!(base64_decode("a").is_none());
        assert!(base64_decode("@@@@").is_none());
    }

    #[test]
    fn varints() {
        let enc = |v| {
            let mut o = vec![];
            write_varint(&mut o, v);
            o
        };
        assert_eq!(enc(0), vec![0]);
        assert_eq!(enc(300), vec![0xac, 0x02]);
        assert_eq!(enc(-1), vec![0xff, 0xff, 0xff, 0xff, 0x0f]);
    }

    /// Servidor de Minecraft de mentira que contesta al estado y al ping.
    #[tokio::test]
    async fn ping_a_fake_server() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut c, _) = listener.accept().await.unwrap();
            // saludo y petición de estado
            for _ in 0..2 {
                let len = read_varint(&mut c).await.unwrap();
                let mut b = vec![0u8; len as usize];
                c.read_exact(&mut b).await.unwrap();
            }
            let body = json!({"version": {"name": "1.21.1"}, "players": {"online": 3, "max": 20}, "description": {"text": "Servidor de prueba"}}).to_string();
            let mut s = vec![];
            write_varint(&mut s, body.len() as i32);
            s.extend_from_slice(body.as_bytes());
            c.write_all(&packet(0, &s)).await.unwrap();
            // ping → pong con el mismo valor
            let len = read_varint(&mut c).await.unwrap();
            let mut b = vec![0u8; len as usize];
            c.read_exact(&mut b).await.unwrap();
            c.write_all(&packet(1, &b[1..])).await.unwrap();
        });
        let st = ping(&format!("127.0.0.1:{port}"), Duration::from_secs(5)).await.unwrap();
        assert_eq!(st.motd, "Servidor de prueba");
        assert_eq!((st.online, st.max), (3, 20));
        assert_eq!(st.version, "1.21.1");
    }

    #[tokio::test]
    async fn ping_errors_are_readable() {
        // Puerto cerrado: «conexión rechazada»
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        drop(l);
        let e = ping(&format!("127.0.0.1:{port}"), Duration::from_secs(3)).await.unwrap_err().to_string();
        assert!(e.contains("rechazada") || e.contains("conectar"), "{e}");
    }
}
