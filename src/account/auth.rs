//! Cuentas: Microsoft (device code → Xbox Live → XSTS → Minecraft) y offline.

use crate::config::now_secs;
use crate::paths::Paths;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AccountKind {
    /// Premium: cuenta de Microsoft con Minecraft comprado.
    Microsoft,
    /// No premium: sólo nombre, sin autenticación.
    Offline,
    /// No premium con skin: Ely.by, LittleSkin o servidor Yggdrasil propio (authlib-injector).
    Yggdrasil,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Account {
    pub kind: AccountKind,
    pub username: String,
    /// UUID sin guiones.
    pub uuid: String,
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: String,
    #[serde(default)]
    pub expires_at: u64,
    #[serde(default)]
    pub xuid: String,
    /// Cuenta de Microsoft sin el juego: se lanza en modo demo.
    #[serde(default)]
    pub demo: bool,
    /// URL de la API authlib-injector (sólo Yggdrasil).
    #[serde(default)]
    pub auth_server: Option<String>,
    #[serde(default)]
    pub client_token: String,
    /// Personajes disponibles (cuentas Yggdrasil con varios perfiles).
    #[serde(default)]
    pub profiles: Vec<Profile>,
    /// Skin local (PNG) para cuentas offline: se aplica con un resource pack generado.
    #[serde(default)]
    pub skin_path: Option<String>,
    #[serde(default)]
    pub skin_slim: bool,
    /// Los tokens están en el almacén seguro del sistema y no en el fichero.
    #[serde(default)]
    pub in_keyring: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    pub id: String,
    pub name: String,
}

impl Account {
    pub fn offline(name: &str) -> Result<Account> {
        let valid = (3..=16).contains(&name.len()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !valid {
            bail!("nombre offline inválido (3-16 caracteres, letras/números/_)");
        }
        Ok(Account {
            kind: AccountKind::Offline,
            username: name.into(),
            uuid: offline_uuid(name),
            access_token: "0".into(),
            refresh_token: String::new(),
            expires_at: u64::MAX,
            xuid: String::new(),
            demo: false,
            auth_server: None,
            client_token: String::new(),
            profiles: vec![],
            skin_path: None,
            skin_slim: false,
            in_keyring: false,
        })
    }
    pub fn user_type(&self) -> &'static str {
        match self.kind {
            AccountKind::Microsoft => "msa",
            AccountKind::Offline => "legacy",
            AccountKind::Yggdrasil => "mojang",
        }
    }
    pub fn is_premium(&self) -> bool {
        self.kind == AccountKind::Microsoft && !self.demo
    }
    /// Etiqueta para la interfaz.
    pub fn label(&self) -> String {
        match self.kind {
            AccountKind::Microsoft if self.demo => t!("Microsoft · demo (sin juego)").into(),
            AccountKind::Microsoft => "Premium".into(),
            AccountKind::Offline => t!("No premium").into(),
            AccountKind::Yggdrasil => t!("No premium · {}", crate::account::yggdrasil::display_name(self.auth_server.as_deref().unwrap_or_default())),
        }
    }
}

/// Igual que Java `UUID.nameUUIDFromBytes("OfflinePlayer:" + name)` (compatible con servidores offline).
pub fn offline_uuid(name: &str) -> String {
    let mut b = md5::compute(format!("OfflinePlayer:{name}").as_bytes()).0;
    b[6] = (b[6] & 0x0f) | 0x30;
    b[8] = (b[8] & 0x3f) | 0x80;
    uuid::Uuid::from_bytes(b).simple().to_string()
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct AccountStore {
    pub accounts: Vec<Account>,
    pub active: Option<String>,
}

impl AccountStore {
    pub fn load(p: &Paths) -> AccountStore {
        let mut st: AccountStore = std::fs::read_to_string(p.accounts_file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        for a in &mut st.accounts {
            if a.in_keyring {
                a.access_token = crate::account::secrets::get(&format!("{}:access", a.uuid)).unwrap_or_default();
                a.refresh_token = crate::account::secrets::get(&format!("{}:refresh", a.uuid)).unwrap_or_default();
            }
        }
        st
    }
    pub fn save(&self, p: &Paths) -> Result<()> {
        let path = p.accounts_file();
        // Los tokens van al almacén seguro del sistema si está disponible; el fichero se queda sin secretos.
        let mut on_disk = AccountStore { accounts: self.accounts.clone(), active: self.active.clone() };
        for a in &mut on_disk.accounts {
            a.in_keyring = false;
            if a.kind == AccountKind::Offline || !crate::account::secrets::AVAILABLE {
                continue;
            }
            let ok_a = crate::account::secrets::set(&format!("{}:access", a.uuid), &a.access_token);
            let ok_r = a.refresh_token.is_empty() || crate::account::secrets::set(&format!("{}:refresh", a.uuid), &a.refresh_token);
            if ok_a && ok_r {
                a.access_token.clear();
                a.refresh_token.clear();
                a.in_keyring = true;
            }
        }
        crate::paths::write_atomic_private(&path, serde_json::to_string_pretty(&on_disk)?)?;
        Ok(())
    }
    pub fn upsert(&mut self, a: Account) {
        self.accounts.retain(|x| x.uuid != a.uuid);
        self.active = Some(a.uuid.clone());
        self.accounts.push(a);
    }
    pub fn remove(&mut self, uuid_or_name: &str) {
        for a in self.accounts.iter().filter(|x| x.uuid == uuid_or_name || x.username.eq_ignore_ascii_case(uuid_or_name)) {
            crate::account::secrets::delete(&format!("{}:access", a.uuid));
            crate::account::secrets::delete(&format!("{}:refresh", a.uuid));
        }
        self.accounts.retain(|x| x.uuid != uuid_or_name && !x.username.eq_ignore_ascii_case(uuid_or_name));
        if let Some(a) = &self.active {
            if !self.accounts.iter().any(|x| &x.uuid == a) {
                self.active = self.accounts.first().map(|x| x.uuid.clone());
            }
        }
    }
    pub fn active(&self) -> Option<&Account> {
        let id = self.active.as_ref()?;
        self.accounts.iter().find(|a| &a.uuid == id)
    }
    pub fn set_active(&mut self, uuid_or_name: &str) -> Result<()> {
        let a = self.accounts.iter().find(|x| x.uuid == uuid_or_name || x.username.eq_ignore_ascii_case(uuid_or_name)).context("cuenta no encontrada")?;
        self.active = Some(a.uuid.clone());
        Ok(())
    }
}

// ---------------------------------------------------------------- Microsoft

const MS_DEVICE: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const MS_TOKEN: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const SCOPE: &str = "XboxLive.signin offline_access";

#[derive(Deserialize, Clone, Debug)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub interval: u64,
    pub expires_in: u64,
    pub message: String,
}

fn check_client_id(id: &str) -> Result<()> {
    if id.trim().is_empty() {
        bail!(
            "Esta copia de Brasa no incluye el inicio de sesión con Microsoft (se compiló sin BRASA_MSA_CLIENT_ID). \
             Mientras tanto puedes jugar con una cuenta no premium."
        );
    }
    Ok(())
}

pub async fn start_device_code(client: &reqwest::Client, client_id: &str) -> Result<DeviceCode> {
    check_client_id(client_id)?;
    let r = client.post(MS_DEVICE).form(&[("client_id", client_id), ("scope", SCOPE)]).send().await?;
    let status = r.status();
    let body: Value = r.json().await?;
    if !status.is_success() {
        bail!("Microsoft rechazó la petición: {}", body["error_description"].as_str().unwrap_or("?"));
    }
    Ok(serde_json::from_value(body)?)
}

pub async fn poll_device_code(client: &reqwest::Client, client_id: &str, dc: &DeviceCode) -> Result<Account> {
    let deadline = now_secs() + dc.expires_in;
    let mut interval = dc.interval.max(1);
    loop {
        tokio::time::sleep(Duration::from_secs(interval)).await;
        if now_secs() > deadline {
            bail!("el código ha caducado, vuelve a intentarlo");
        }
        let r = client
            .post(MS_TOKEN)
            .form(&[("grant_type", "urn:ietf:params:oauth:grant-type:device_code"), ("client_id", client_id), ("device_code", dc.device_code.as_str())])
            .send()
            .await?;
        let body: Value = r.json().await?;
        if let Some(err) = body["error"].as_str() {
            match err {
                "authorization_pending" => continue,
                "slow_down" => {
                    interval += 5;
                    continue;
                }
                _ => bail!("login cancelado: {}", body["error_description"].as_str().unwrap_or(err)),
            }
        }
        let at = body["access_token"].as_str().context("sin access_token")?;
        let rt = body["refresh_token"].as_str().unwrap_or_default().to_string();
        return minecraft_login(client, at, rt).await;
    }
}

/// Refresca la sesión si caduca en menos de 5 minutos.
pub async fn ensure_fresh(client: &reqwest::Client, client_id: &str, acc: &Account) -> Result<Account> {
    match acc.kind {
        AccountKind::Offline => return Ok(acc.clone()),
        AccountKind::Yggdrasil => return ensure_fresh_yggdrasil(client, acc).await,
        AccountKind::Microsoft => {}
    }
    if acc.expires_at > now_secs() + 300 {
        return Ok(acc.clone());
    }
    check_client_id(client_id)?;
    let body: Value = client
        .post(MS_TOKEN)
        .form(&[("grant_type", "refresh_token"), ("client_id", client_id), ("refresh_token", acc.refresh_token.as_str()), ("scope", SCOPE)])
        .send()
        .await?
        .json()
        .await?;
    let at = body["access_token"].as_str().context("la sesión de Microsoft ha caducado: vuelve a iniciar sesión")?;
    let rt = body["refresh_token"].as_str().unwrap_or(&acc.refresh_token).to_string();
    minecraft_login(client, at, rt).await
}

/// Inicia sesión en Ely.by / LittleSkin / servidor propio. `server` puede ser un alias o una URL.
pub async fn yggdrasil_login(client: &reqwest::Client, server: &str, user: &str, password: &str) -> Result<Account> {
    let api = crate::account::yggdrasil::resolve_api(client, server).await?;
    let s = crate::account::yggdrasil::authenticate(client, &api, user, password).await?;
    Ok(Account {
        kind: AccountKind::Yggdrasil,
        username: s.name,
        uuid: s.uuid,
        access_token: s.access_token,
        refresh_token: String::new(),
        expires_at: 0,
        xuid: String::new(),
        demo: false,
        auth_server: Some(api),
        client_token: s.client_token,
        profiles: s.profiles.iter().map(|(id, name)| Profile { id: id.clone(), name: name.clone() }).collect(),
        skin_path: None,
        skin_slim: false,
        in_keyring: false,
    })
}

/// Cambia el personaje activo de una cuenta Yggdrasil con varios perfiles.
pub async fn yggdrasil_select(client: &reqwest::Client, acc: &Account, profile_id: &str) -> Result<Account> {
    let api = acc.auth_server.clone().context("cuenta sin servidor de autenticación")?;
    let prof = acc.profiles.iter().find(|p| p.id == profile_id || p.name.eq_ignore_ascii_case(profile_id)).context("ese personaje no existe en la cuenta")?;
    let s = crate::account::yggdrasil::select(client, &api, &acc.access_token, &acc.client_token, &prof.id, &prof.name).await?;
    Ok(Account { access_token: s.access_token, username: s.name, uuid: s.uuid, ..acc.clone() })
}

async fn ensure_fresh_yggdrasil(client: &reqwest::Client, acc: &Account) -> Result<Account> {
    let api = acc.auth_server.clone().context("cuenta sin servidor de autenticación")?;
    if crate::account::yggdrasil::validate(client, &api, &acc.access_token, &acc.client_token).await {
        return Ok(acc.clone());
    }
    let s = crate::account::yggdrasil::refresh(client, &api, &acc.access_token, &acc.client_token).await?;
    Ok(Account { access_token: s.access_token, username: s.name, uuid: s.uuid, ..acc.clone() })
}

async fn minecraft_login(client: &reqwest::Client, ms_token: &str, refresh: String) -> Result<Account> {
    // Xbox Live
    let xbl: Value = client
        .post("https://user.auth.xboxlive.com/user/authenticate")
        .json(&json!({
            "Properties": {"AuthMethod": "RPS", "SiteName": "user.auth.xboxlive.com", "RpsTicket": format!("d={ms_token}")},
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT"
        }))
        .send()
        .await?
        .error_for_status()
        .context("Xbox Live rechazó el token")?
        .json()
        .await?;
    let xbl_token = xbl["Token"].as_str().context("Xbox Live sin token")?;
    let uhs = xbl["DisplayClaims"]["xui"][0]["uhs"].as_str().context("Xbox Live sin uhs")?;

    // XSTS
    let xr = client
        .post("https://xsts.auth.xboxlive.com/xsts/authorize")
        .json(&json!({
            "Properties": {"SandboxId": "RETAIL", "UserTokens": [xbl_token]},
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT"
        }))
        .send()
        .await?;
    let status = xr.status();
    let xsts: Value = xr.json().await?;
    if !status.is_success() {
        let msg = match xsts["XErr"].as_u64() {
            Some(2148916233) => "esta cuenta de Microsoft no tiene perfil de Xbox (créalo en xbox.com)",
            Some(2148916235) => "Xbox Live no está disponible en tu país",
            Some(2148916236) | Some(2148916237) => "la cuenta necesita verificación de edad (Corea)",
            Some(2148916238) => "cuenta de menor: debe añadirla un adulto a una Familia de Microsoft",
            _ => "XSTS rechazó la autenticación",
        };
        bail!("{msg}");
    }
    let xsts_token = xsts["Token"].as_str().context("XSTS sin token")?;
    let xuid = xsts["DisplayClaims"]["xui"][0]["xid"].as_str().unwrap_or_default().to_string();

    // Minecraft
    let mc: Value = client
        .post("https://api.minecraftservices.com/authentication/login_with_xbox")
        .json(&json!({"identityToken": format!("XBL3.0 x={uhs};{xsts_token}")}))
        .send()
        .await?
        .error_for_status()
        .context("Minecraft Services rechazó el login (¿Client ID sin aprobar por Mojang?)")?
        .json()
        .await?;
    let mc_token = mc["access_token"].as_str().context("sin token de Minecraft")?.to_string();
    let expires_in = mc["expires_in"].as_u64().unwrap_or(86400);

    let pr = client.get("https://api.minecraftservices.com/minecraft/profile").bearer_auth(&mc_token).send().await?;
    if pr.status().as_u16() == 404 {
        // Cuenta de Microsoft sin el juego comprado: se permite el modo demo oficial.
        let seed = if xuid.is_empty() { uhs.to_string() } else { xuid.clone() };
        return Ok(Account {
            kind: AccountKind::Microsoft,
            username: "Player".into(),
            uuid: offline_uuid(&format!("demo-{seed}")),
            access_token: mc_token,
            refresh_token: refresh,
            expires_at: now_secs() + expires_in,
            xuid,
            demo: true,
            auth_server: None,
            client_token: String::new(),
            profiles: vec![],
            skin_path: None,
            skin_slim: false,
            in_keyring: false,
        });
    }
    let profile: Value = pr.error_for_status()?.json().await?;
    Ok(Account {
        kind: AccountKind::Microsoft,
        username: profile["name"].as_str().context("perfil sin nombre")?.to_string(),
        uuid: profile["id"].as_str().context("perfil sin id")?.to_string(),
        access_token: mc_token,
        refresh_token: refresh,
        expires_at: now_secs() + expires_in,
        xuid,
        demo: false,
        auth_server: None,
        client_token: String::new(),
        profiles: vec![],
        skin_path: None,
        skin_slim: false,
        in_keyring: false,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn offline_uuid_matches_java() {
        // Valor conocido de UUID.nameUUIDFromBytes("OfflinePlayer:Notch")
        assert_eq!(super::offline_uuid("Notch"), "b50ad385829d3141a2167e7d7539ba7f");
    }
}
