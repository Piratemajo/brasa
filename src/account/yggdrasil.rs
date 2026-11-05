//! Cuentas no premium con skin: servidores Yggdrasil compatibles con authlib-injector
//! (Ely.by, LittleSkin, Blessing Skin propio...). El juego se lanza con `-javaagent:authlib-injector.jar=<api>`
//! para que skins, capas y servidores online-mode de ese sistema funcionen.

use crate::net::{self, Download};
use crate::paths::Paths;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

pub const PRESETS: &[(&str, &str, &str)] =
    &[("ely", "Ely.by", "https://authserver.ely.by/api/authlib-injector"), ("littleskin", "LittleSkin", "https://littleskin.cn/api/yggdrasil")];

/// Acepta un alias ("ely", "littleskin") o una URL; sigue la cabecera de autodescubrimiento ALI.
pub async fn resolve_api(client: &reqwest::Client, server: &str) -> Result<String> {
    let s = server.trim();
    if let Some((_, _, url)) = PRESETS.iter().find(|(k, n, _)| k.eq_ignore_ascii_case(s) || n.eq_ignore_ascii_case(s)) {
        return Ok(url.to_string());
    }
    let mut url = if s.starts_with("http://") || s.starts_with("https://") { s.to_string() } else { format!("https://{s}") };
    if let Ok(r) = client.get(&url).send().await {
        if let Some(loc) = r.headers().get("x-authlib-injector-api-location").and_then(|v| v.to_str().ok()) {
            url = if loc.starts_with("http") {
                loc.to_string()
            } else {
                let base = reqwest::Url::parse(&url)?;
                base.join(loc)?.to_string()
            };
        }
    }
    Ok(url.trim_end_matches('/').to_string())
}

pub fn display_name(api: &str) -> String {
    PRESETS
        .iter()
        .find(|(_, _, u)| *u == api)
        .map(|(_, n, _)| n.to_string())
        .unwrap_or_else(|| reqwest::Url::parse(api).ok().and_then(|u| u.host_str().map(String::from)).unwrap_or_else(|| api.to_string()))
}

pub struct Session {
    pub access_token: String,
    pub client_token: String,
    pub uuid: String,
    pub name: String,
    /// Todos los personajes de la cuenta (id, nombre).
    pub profiles: Vec<(String, String)>,
}

fn all_profiles(v: &Value) -> Vec<(String, String)> {
    v["availableProfiles"]
        .as_array()
        .map(|a| a.iter().filter_map(|p| Some((p["id"].as_str()?.replace('-', ""), p["name"].as_str()?.to_string()))).collect())
        .unwrap_or_default()
}

fn yerror(v: &Value) -> Option<String> {
    v["error"].as_str().map(|e| {
        let msg = v["errorMessage"].as_str().unwrap_or(e);
        match msg {
            m if m.contains("two factor") => "la cuenta tiene 2FA: escribe la contraseña como contraseña:código".to_string(),
            m if m.to_lowercase().contains("invalid credentials") => "usuario o contraseña incorrectos".to_string(),
            m => m.to_string(),
        }
    })
}

fn pick_profile(v: &Value) -> Result<(String, String)> {
    let prof = if v["selectedProfile"].is_object() {
        &v["selectedProfile"]
    } else {
        // sin perfil seleccionado: usa el primero disponible
        v["availableProfiles"].get(0).context("la cuenta no tiene ningún personaje creado en ese servidor")?
    };
    Ok((prof["id"].as_str().context("perfil sin id")?.replace('-', ""), prof["name"].as_str().context("perfil sin nombre")?.to_string()))
}

pub async fn authenticate(client: &reqwest::Client, api: &str, user: &str, password: &str) -> Result<Session> {
    let client_token = uuid::Uuid::new_v4().simple().to_string();
    let v: Value = client
        .post(format!("{api}/authserver/authenticate"))
        .json(&json!({
            "agent": {"name": "Minecraft", "version": 1},
            "username": user,
            "password": password,
            "clientToken": client_token,
            "requestUser": true
        }))
        .send()
        .await
        .context("no se pudo contactar con el servidor de cuentas")?
        .json()
        .await?;
    if let Some(e) = yerror(&v) {
        bail!("{e}");
    }
    let (uuid, name) = pick_profile(&v)?;
    Ok(Session {
        access_token: v["accessToken"].as_str().context("sin accessToken")?.to_string(),
        client_token: v["clientToken"].as_str().map(String::from).unwrap_or(client_token),
        uuid,
        name,
        profiles: all_profiles(&v),
    })
}

/// true si el token sigue siendo válido (204).
pub async fn validate(client: &reqwest::Client, api: &str, access: &str, client_token: &str) -> bool {
    client
        .post(format!("{api}/authserver/validate"))
        .json(&json!({"accessToken": access, "clientToken": client_token}))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

pub async fn refresh(client: &reqwest::Client, api: &str, access: &str, client_token: &str) -> Result<Session> {
    let v: Value = client
        .post(format!("{api}/authserver/refresh"))
        .json(&json!({"accessToken": access, "clientToken": client_token, "requestUser": true}))
        .send()
        .await?
        .json()
        .await?;
    if let Some(e) = yerror(&v) {
        bail!("la sesión ha caducado, vuelve a iniciar sesión ({e})");
    }
    let (uuid, name) = pick_profile(&v)?;
    Ok(Session {
        access_token: v["accessToken"].as_str().context("sin accessToken")?.to_string(),
        client_token: client_token.to_string(),
        uuid,
        name,
        profiles: vec![],
    })
}

/// Selecciona un personaje concreto (refresh con `selectedProfile`).
pub async fn select(client: &reqwest::Client, api: &str, access: &str, client_token: &str, id: &str, name: &str) -> Result<Session> {
    let v: Value = client
        .post(format!("{api}/authserver/refresh"))
        .json(&json!({
            "accessToken": access,
            "clientToken": client_token,
            "requestUser": true,
            "selectedProfile": {"id": id, "name": name}
        }))
        .send()
        .await?
        .json()
        .await?;
    if let Some(e) = yerror(&v) {
        bail!("no se pudo cambiar de personaje: {e}");
    }
    let (uuid, name) = pick_profile(&v)?;
    Ok(Session {
        access_token: v["accessToken"].as_str().context("sin accessToken")?.to_string(),
        client_token: client_token.to_string(),
        uuid,
        name,
        profiles: vec![],
    })
}

/// Descarga (una vez) authlib-injector y devuelve su ruta.
pub async fn ensure_injector(client: &reqwest::Client, p: &Paths) -> Result<PathBuf> {
    let dir = p.root.join("agents");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("authlib-injector.jar");
    let meta: Result<Value> = net::get_json(client, "https://authlib-injector.yushi.moe/artifact/latest.json").await;
    let meta = match meta {
        Ok(m) => m,
        // sin red: usa el que ya haya
        Err(_) if path.exists() => return Ok(path),
        Err(e) => return Err(e).context("no se pudo obtener authlib-injector"),
    };
    let url = meta["download_url"].as_str().context("latest.json sin download_url")?;
    let sha256 = meta["checksums"]["sha256"].as_str().unwrap_or_default().to_lowercase();
    let current = std::fs::read(&path).ok().map(|b| hex::encode(Sha256::digest(&b)));
    if current.as_deref() == Some(sha256.as_str()) {
        return Ok(path);
    }
    let tmp = dir.join("authlib-injector.download");
    net::download_one(client, &Download { url: url.to_string(), path: tmp.clone(), sha1: None, size: None }).await?;
    let got = hex::encode(Sha256::digest(&std::fs::read(&tmp)?));
    if !sha256.is_empty() && got != sha256 {
        let _ = std::fs::remove_file(&tmp);
        bail!("authlib-injector descargado con SHA-256 incorrecto");
    }
    std::fs::rename(&tmp, &path)?;
    Ok(path)
}

/// Argumentos JVM para inyectar el servidor de skins/sesiones.
pub async fn jvm_args(client: &reqwest::Client, p: &Paths, api: &str) -> Result<Vec<String>> {
    let jar = ensure_injector(client, p).await?;
    let mut args = vec![format!("-javaagent:{}={}", jar.display(), api)];
    // Prefetch: evita que el juego tenga que volver a pedir los metadatos al arrancar.
    if let Ok(meta) = net::get_text(client, api).await {
        args.push(format!("-Dauthlibinjector.yggdrasil.prefetched={}", base64(meta.as_bytes())));
    }
    Ok(args)
}

pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn b64() {
        assert_eq!(super::base64(b""), "");
        assert_eq!(super::base64(b"f"), "Zg==");
        assert_eq!(super::base64(b"fo"), "Zm8=");
        assert_eq!(super::base64(b"foo"), "Zm9v");
        assert_eq!(super::base64(b"foobar"), "Zm9vYmFy");
    }
}
