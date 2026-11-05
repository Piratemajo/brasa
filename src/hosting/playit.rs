//! Túnel de playit.gg: para que entren amigos de fuera de casa sin abrir puertos en el router.
//!
//! - **Enlazar** (una vez): Brasa genera un código, abre `playit.gg/claim/<código>` en el navegador
//!   y, cuando el usuario lo aprueba con su cuenta de playit, recibe la clave del agente. La clave
//!   se guarda en el almacén seguro del sistema (o en un fichero 600 en Linux), nunca en Ajustes.
//! - **Al arrancar un servidor** con playit: el agente oficial de playit (su librería,
//!   `playit-agent-core`, dentro de Brasa) se conecta, se crea un túnel «Minecraft Java» hacia
//!   `127.0.0.1:<puerto>` si no existe, y se muestra su dirección (`algo.joinmc.link`).
//! - **Al parar** el último servidor que lo usa, el agente se desconecta.

use super::run::{Live, Tunnel};
use super::Server;
use crate::paths::Paths;
use anyhow::{bail, Context, Result};
use playit_api_client::api::{
    AccountTunnelOriginCreate, AgentOrigin, AgentRunDataV1, AgentTunnelAttr, AgentTunnelConfig, AgentTunnelV1, ApiError, ClaimAgentType, ClaimExchangeError,
    ClaimSetupResponse, ReqClaimExchange, ReqClaimSetup, ReqTunnelsCreateV1, TunnelCreateErrorV1, TunnelPortDetails, TunnelType,
};
use playit_api_client::PlayitApi;
use std::collections::HashSet;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

pub const API_BASE: &str = "https://api.playit.gg";
const SECRET_KEY: &str = "playit-agent";

// ------------------------------------------------------------------------------- la clave

fn secret_file(p: &Paths) -> std::path::PathBuf {
    p.root.join("playit.key")
}

pub fn secret(p: &Paths) -> Option<String> {
    crate::account::secrets::get(SECRET_KEY).or_else(|| std::fs::read_to_string(secret_file(p)).ok()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

pub fn linked(p: &Paths) -> bool {
    secret(p).is_some()
}

fn save_secret(p: &Paths, key: &str) -> Result<()> {
    if crate::account::secrets::set(SECRET_KEY, key) {
        let _ = std::fs::remove_file(secret_file(p));
        return Ok(());
    }
    crate::paths::write_atomic_private(&secret_file(p), key).context("no se pudo guardar la clave de playit")
}

/// Olvida la clave (el agente queda en la cuenta de playit hasta que se borre allí).
pub fn unlink(p: &Paths) {
    crate::account::secrets::delete(SECRET_KEY);
    let _ = std::fs::remove_file(secret_file(p));
}

// ------------------------------------------------------------------------------- enlazar

/// Código aleatorio de 10 caracteres hexadecimales (el formato de playit).
pub fn claim_code() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..10].to_string()
}

pub fn claim_url(code: &str) -> String {
    format!("https://playit.gg/claim/{code}")
}

/// Espera a que el usuario apruebe Brasa en la web de playit y guarda la clave.
/// `on_url` recibe el enlace en cuanto está (para abrirlo y enseñarlo).
pub async fn link(p: &Paths, code: &str, cancel: Arc<std::sync::atomic::AtomicBool>) -> Result<()> {
    let api = PlayitApi::create(API_BASE.to_string(), None);
    let deadline = std::time::Instant::now() + Duration::from_secs(15 * 60);
    loop {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            bail!("cancelado");
        }
        if std::time::Instant::now() > deadline {
            bail!("se acabó el tiempo para aprobarlo en playit.gg: vuelve a intentarlo");
        }
        let r = api
            .claim_setup(ReqClaimSetup {
                code: code.to_string(),
                agent_type: ClaimAgentType::SelfManaged,
                version: format!("Brasa {}", env!("CARGO_PKG_VERSION")),
            })
            .await;
        match r {
            Ok(ClaimSetupResponse::UserAccepted) => break,
            Ok(ClaimSetupResponse::UserRejected) => bail!("lo has rechazado en playit.gg"),
            Ok(_) => {}
            Err(ApiError::Fail(e)) => bail!("playit.gg no acepta el código ({e:?}): vuelve a intentarlo"),
            Err(_) => {} // sin red un momento: se reintenta
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    loop {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            bail!("cancelado");
        }
        match api.claim_exchange(ReqClaimExchange { code: code.to_string() }).await {
            Ok(k) => {
                save_secret(p, &k.secret_key)?;
                return Ok(());
            }
            Err(ApiError::Fail(ClaimExchangeError::NotAccepted)) | Err(ApiError::ClientError(_)) => {}
            Err(e) => bail!("playit.gg no dio la clave: {e}"),
        }
        if std::time::Instant::now() > deadline {
            bail!("se acabó el tiempo para aprobarlo en playit.gg");
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

// ------------------------------------------------------------------------------- túneles

/// El túnel de esta cuenta que lleva a `127.0.0.1:<port>`.
pub fn find_tunnel(data: &AgentRunDataV1, port: u16, known: Option<&str>) -> Option<AgentTunnelV1> {
    let field = |t: &AgentTunnelV1, k: &str| t.agent_config.fields.iter().find(|f| f.name == k).map(|f| f.value.clone());
    if let Some(k) = known {
        if let Some(t) = data.tunnels.iter().find(|t| t.id.to_string() == k) {
            return Some(t.clone());
        }
    }
    data.tunnels
        .iter()
        // sin puerto local explícito, el de Minecraft por defecto
        .find(|t| {
            t.tunnel_type.as_deref() == Some("minecraft-java")
                && field(t, "local_port").and_then(|v| v.parse::<u16>().ok()).unwrap_or(super::DEFAULT_PORT) == port
        })
        .cloned()
}

fn create_request(agent_id: uuid::Uuid, name: &str, port: u16) -> ReqTunnelsCreateV1 {
    ReqTunnelsCreateV1 {
        ports: TunnelPortDetails::TunnelType(TunnelType::MinecraftJava),
        origin: AccountTunnelOriginCreate::Agent(AgentOrigin {
            agent_id: Some(agent_id),
            config: AgentTunnelConfig {
                fields: vec![
                    AgentTunnelAttr { name: "local_ip".into(), value: "127.0.0.1".into() },
                    AgentTunnelAttr { name: "local_port".into(), value: port.to_string() },
                ],
            },
        }),
        enabled: true,
        alloc: None,
        name: Some(name.chars().filter(|c| c.is_ascii_graphic() || *c == ' ').take(40).collect::<String>().trim().to_string()).filter(|n| !n.is_empty()),
        firewall_id: None,
    }
}

fn explain_create(e: &ApiError<TunnelCreateErrorV1, impl std::fmt::Debug>) -> String {
    match e {
        ApiError::Fail(TunnelCreateErrorV1::RequiresVerifiedAccount) => {
            "playit.gg pide verificar tu correo para crear el túnel: hazlo en playit.gg/account y vuelve a arrancar".into()
        }
        ApiError::Fail(TunnelCreateErrorV1::AgentVersionTooOld) => "playit.gg dice que el agente está desactualizado: actualiza Brasa".into(),
        ApiError::Fail(f) => format!("playit.gg no dejó crear el túnel ({f:?}): créalo en playit.gg (Minecraft Java → 127.0.0.1) y Brasa lo usará"),
        other => format!("no se pudo hablar con playit.gg: {other}"),
    }
}

// ------------------------------------------------------------------------------- el agente

struct Agent {
    users: HashSet<String>,
    stop: Option<Box<dyn FnOnce() + Send>>,
    lookup: Arc<playit_agent_core::network::origin_lookup::OriginLookup>,
}

fn agent() -> &'static Mutex<Option<Agent>> {
    static A: OnceLock<Mutex<Option<Agent>>> = OnceLock::new();
    A.get_or_init(Default::default)
}

/// El servidor deja de necesitar el túnel; si era el último, se desconecta el agente.
pub fn release(id: &str) {
    let mut a = agent().lock().unwrap();
    if let Some(ag) = a.as_mut() {
        ag.users.remove(id);
        if ag.users.is_empty() {
            if let Some(stop) = ag.stop.take() {
                stop();
            }
            *a = None;
        }
    }
}

/// Arranca (o reutiliza) el agente y deja el túnel listo para `s`. Informa en `live.tunnel`.
pub fn serve(client: reqwest::Client, p: Paths, s: Server, live: Arc<Mutex<Live>>, notify: Arc<dyn Fn() + Send + Sync>) {
    let _ = client;
    let set = move |t: Tunnel| {
        live.lock().unwrap().tunnel = t;
        notify();
    };
    tokio::spawn(async move {
        set(Tunnel::Connecting("Conectando con playit.gg…".into()));
        match serve_inner(&p, &s, &set).await {
            Ok(addr) => set(Tunnel::Ready(addr)),
            Err(e) => set(Tunnel::Failed(format!("{e:#}"))),
        }
    });
}

async fn serve_inner(p: &Paths, s: &Server, set: &(impl Fn(Tunnel) + Send + Sync)) -> Result<String> {
    let key = secret(p).context("enlaza tu cuenta de playit.gg (botón «Enlazar» en el servidor)")?;
    let api = PlayitApi::create(API_BASE.to_string(), Some(key.clone()));
    let lookup = ensure_agent(&s.id, &key).await?;
    let mut data = api.v1_agents_rundata().await.map_err(|e| anyhow::anyhow!("playit.gg no responde: {e:?}"))?;
    lookup.update_from_run_data(&data).await;

    let tunnel = match find_tunnel(&data, s.port, s.playit_tunnel.as_deref()) {
        Some(t) => t,
        None => {
            set(Tunnel::Connecting("Creando el túnel en playit.gg…".into()));
            let want = match api.v1_tunnels_create(create_request(data.agent_id, &format!("Brasa {}", s.name), s.port)).await {
                Ok(id) => {
                    Server::update(p, &s.id, |f| {
                        f.playit_tunnel = Some(id.id.to_string());
                        Ok(())
                    })?;
                    Some(id.id)
                }
                Err(e) => {
                    // algunas cuentas o agentes no pueden crear túneles por API: se crea en la web
                    // y Brasa lo encuentra por el puerto
                    set(Tunnel::Manual { port: s.port, why: explain_create(&e) });
                    None
                }
            };
            // el túnel tarda unos segundos en estar listo (o lo que tarde el usuario en la web)
            let tries = if want.is_some() { 40 } else { 180 };
            let mut found = None;
            for _ in 0..tries {
                tokio::time::sleep(Duration::from_secs(if want.is_some() { 2 } else { 5 })).await;
                let Ok(d) = api.v1_agents_rundata().await else { continue };
                data = d;
                lookup.update_from_run_data(&data).await;
                let hit = match want {
                    Some(id) => data.tunnels.iter().find(|t| t.id == id).cloned(),
                    None => find_tunnel(&data, s.port, None),
                };
                if let Some(t) = hit {
                    found = Some(t);
                    break;
                }
                if let Some(pend) = want.and_then(|id| data.pending.iter().find(|t| t.id == id)) {
                    set(Tunnel::Connecting(format!("playit.gg: {}", pend.status_msg)));
                }
            }
            found.context("no apareció el túnel en playit.gg: créalo allí (Minecraft Java → 127.0.0.1) y vuelve a arrancar el servidor")?
        }
    };
    if let Some(r) = &tunnel.disabled_reason {
        bail!("el túnel está desactivado en playit.gg: {r}");
    }
    Ok(tunnel.display_address.clone())
}

/// Arranca el agente si no lo está y apunta a `id` como usuario. Devuelve su tabla de destinos
/// para actualizarla con los túneles nuevos.
async fn ensure_agent(id: &str, key: &str) -> Result<Arc<playit_agent_core::network::origin_lookup::OriginLookup>> {
    use playit_agent_core::network::origin_lookup::OriginLookup;
    use playit_agent_core::network::tcp::tcp_settings::TcpSettings;
    use playit_agent_core::network::udp::udp_settings::UdpSettings;
    use playit_agent_core::playit_agent::{PlayitAgent, PlayitAgentSettings};

    if let Some(a) = agent().lock().unwrap().as_mut() {
        a.users.insert(id.to_string());
        return Ok(a.lookup.clone());
    }
    static PLATFORM: std::sync::Once = std::sync::Once::new();
    PLATFORM.call_once(|| playit_agent_core::agent_control::version::register_platform(playit_agent_core::agent_control::platform::current_platform()));

    let lookup = Arc::new(OriginLookup::default());
    let api = PlayitApi::create(API_BASE.to_string(), Some(key.to_string()));
    if let Ok(d) = api.v1_agents_rundata().await {
        lookup.update_from_run_data(&d).await;
    }
    let settings = PlayitAgentSettings {
        udp_settings: UdpSettings::default(),
        tcp_settings: TcpSettings::default(),
        api_url: API_BASE.to_string(),
        secret_key: key.to_string(),
    };
    let runner = tokio::time::timeout(Duration::from_secs(60), PlayitAgent::new(settings, lookup.clone()))
        .await
        .context("playit.gg tardó demasiado en responder")?
        .map_err(|e| anyhow::anyhow!("no se pudo conectar el agente de playit: {e:?}"))?;
    let cancel = runner.cancellation_token();
    let handle = tokio::spawn(runner.run());
    // los túneles que se creen o cambien después también tienen que llegar al agente
    let refresher = {
        let lookup = lookup.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(10)).await;
                if let Ok(d) = api.v1_agents_rundata().await {
                    lookup.update_from_run_data(&d).await;
                }
            }
        })
    };
    let mut users = HashSet::new();
    users.insert(id.to_string());
    *agent().lock().unwrap() = Some(Agent {
        users,
        stop: Some(Box::new(move || {
            cancel.cancel();
            refresher.abort();
            // si no se cierra solo en unos segundos, se corta
            let abort = handle.abort_handle();
            tokio::spawn(async move {
                if tokio::time::timeout(Duration::from_secs(5), handle).await.is_err() {
                    abort.abort();
                }
            });
        })),
        lookup: lookup.clone(),
    });
    Ok(lookup)
}

#[cfg(test)]
mod tests {
    use super::*;
    use playit_api_client::api::{AccountStatus, AgentPermissions, PortType};

    fn tunnel(id: u128, kind: &str, port: &str) -> AgentTunnelV1 {
        AgentTunnelV1 {
            id: uuid::Uuid::from_u128(id),
            internal_id: id as u64,
            name: "x".into(),
            display_address: format!("t{id}.joinmc.link"),
            port_type: PortType::Tcp,
            port_count: 1,
            tunnel_type: Some(kind.into()),
            tunnel_type_display: kind.into(),
            agent_config: AgentTunnelConfig {
                fields: vec![
                    AgentTunnelAttr { name: "local_ip".into(), value: "127.0.0.1".into() },
                    AgentTunnelAttr { name: "local_port".into(), value: port.into() },
                ],
            },
            disabled_reason: None,
        }
    }

    #[test]
    fn finds_the_tunnel_of_each_server() {
        let data = AgentRunDataV1 {
            agent_id: uuid::Uuid::nil(),
            tunnels: vec![tunnel(1, "https", "25565"), tunnel(2, "minecraft-java", "25566"), tunnel(3, "minecraft-java", "25565")],
            pending: vec![],
            notices: vec![],
            permissions: AgentPermissions { is_self_managed: true, has_premium: false, account_status: AccountStatus::Guest },
        };
        assert_eq!(find_tunnel(&data, 25565, None).map(|t| t.display_address), Some("t3.joinmc.link".into()));
        assert_eq!(find_tunnel(&data, 25566, None).map(|t| t.display_address), Some("t2.joinmc.link".into()));
        assert!(find_tunnel(&data, 25570, None).is_none());
        let known = uuid::Uuid::from_u128(2).to_string();
        assert_eq!(find_tunnel(&data, 25565, Some(&known)).map(|t| t.display_address), Some("t2.joinmc.link".into()), "el guardado manda");
    }

    #[test]
    fn requests_and_codes() {
        let c = claim_code();
        assert_eq!(c.len(), 10);
        assert!(c.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(claim_url("abc123"), "https://playit.gg/claim/abc123");
        let r = create_request(uuid::Uuid::nil(), "Brasa Mi señor servidor", 25566);
        let j = serde_json::to_value(&r).unwrap();
        assert_eq!(j["ports"], serde_json::json!({"type": "tunnel-type", "details": "minecraft-java"}));
        assert_eq!(j["origin"]["type"], "agent");
        assert_eq!(j["origin"]["data"]["config"]["fields"][1]["value"], "25566");
        assert_eq!(j["name"], "Brasa Mi seor servidor", "sólo ASCII, como pide playit");
    }

    #[test]
    fn secret_never_lands_in_settings() {
        let p = Paths { root: std::env::temp_dir().join(format!("brasa-playit-{}", uuid::Uuid::new_v4())) };
        std::fs::create_dir_all(&p.root).unwrap();
        if !crate::account::secrets::AVAILABLE {
            assert!(!linked(&p));
            save_secret(&p, "clave-secreta").unwrap();
            assert_eq!(secret(&p).as_deref(), Some("clave-secreta"));
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(std::fs::metadata(secret_file(&p)).unwrap().permissions().mode() & 0o777, 0o600);
            }
            unlink(&p);
            assert!(!linked(&p));
        }
        assert!(!crate::config::Settings::default().proxy.contains("clave"));
        let _ = std::fs::remove_dir_all(&p.root);
    }
}
