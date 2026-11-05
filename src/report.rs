//! «Informar de un problema»: arma un informe (qué pasó, el equipo, la instancia y el registro),
//! le quita lo privado y lo sube a mclo.gs, el servicio de registros que usa la comunidad de
//! Minecraft. Devuelve un enlace para pegarlo donde se pida ayuda, o para abrir un *issue* en GitHub.
//!
//! Nada se envía sin que el usuario pulse el botón, y antes se borran tokens, el nombre de usuario
//! del sistema y la carpeta personal.

use anyhow::{bail, Context, Result};
use serde::Deserialize;

/// mclo.gs admite hasta 25 000 líneas: se mandan las últimas.
pub const MAX_LINES: usize = 20_000;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Context_ {
    pub description: String,
    pub instance: Option<String>,
    pub loader: Option<String>,
    pub mods: Vec<String>,
    pub memory_mb: Option<u32>,
    pub java: Option<String>,
    pub diagnosis: Vec<String>,
}

/// Quita lo privado de un texto: tokens de sesión, JWT, el usuario del sistema y la carpeta personal.
pub fn redact(text: &str, home: Option<&str>, user: Option<&str>, players: &[String]) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let mut l = line.to_string();
        if let Some(h) = home.filter(|h| h.len() > 3) {
            l = l.replace(h, "~");
            // también con las barras al revés (rutas de Windows que salen en logs de Java)
            l = l.replace(&h.replace('\\', "/"), "~");
        }
        if let Some(u) = user.filter(|u| u.len() > 2) {
            l = replace_word(&l, u, "<usuario>");
        }
        for p in players.iter().filter(|p| p.len() > 2) {
            l = replace_word(&l, p, "<jugador>");
        }
        l = redact_tokens(&l);
        out.push_str(&l);
        out.push('\n');
    }
    out
}

/// Sustituye `word` sólo cuando no forma parte de otra palabra.
fn replace_word(s: &str, word: &str, with: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find(word) {
        let before = rest[..i].chars().last();
        let after = rest[i + word.len()..].chars().next();
        let edge = |c: Option<char>| c.is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
        out.push_str(&rest[..i]);
        out.push_str(if edge(before) && edge(after) { with } else { word });
        rest = &rest[i + word.len()..];
    }
    out.push_str(rest);
    out
}

/// Tokens: `token:<acceso>:<uuid>` (lo imprimen versiones antiguas), JWT (`eyJ…`), y lo que siga a
/// `--accessToken`, `--xuid`, `--clientId`, `accessToken=`.
fn redact_tokens(line: &str) -> String {
    let mut words: Vec<String> = vec![];
    let mut hide_next = false;
    for w in line.split(' ') {
        if hide_next && !w.is_empty() {
            words.push("<oculto>".into());
            hide_next = false;
            continue;
        }
        let lw = w.to_ascii_lowercase();
        if ["--accesstoken", "--xuid", "--clientid", "--session"].contains(&lw.as_str()) {
            hide_next = true;
            words.push(w.into());
            continue;
        }
        let w2 = if let Some(i) = w.find("token:") {
            format!("{}token:<oculto>", &w[..i])
        } else if let Some(i) = w.find("eyJ").filter(|_| w.len() > 40) {
            format!("{}<oculto>", &w[..i])
        } else if let Some(i) = lw.find("accesstoken=") {
            format!("{}<oculto>", &w[..i + "accesstoken=".len()])
        } else {
            w.to_string()
        };
        words.push(w2);
    }
    words.join(" ")
}

/// El informe en texto: resumen arriba y el registro debajo.
pub fn build(c: &Context_, logs: &[String]) -> String {
    let mut s = String::new();
    s.push_str(&format!("Brasa Launcher {} · {} {}\n", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH));
    if let Some(i) = &c.instance {
        s.push_str(&format!("Instancia: {i}"));
        if let Some(l) = &c.loader {
            s.push_str(&format!(" · {l}"));
        }
        if !c.mods.is_empty() {
            s.push_str(&format!(" · {} mods", c.mods.len()));
        }
        s.push('\n');
    }
    if let Some(m) = c.memory_mb {
        s.push_str(&format!("Memoria: {m} MB\n"));
    }
    if let Some(j) = &c.java {
        s.push_str(&format!("Java: {j}\n"));
    }
    if !c.description.trim().is_empty() {
        s.push_str(&format!("\nQué pasó:\n{}\n", c.description.trim()));
    }
    for d in &c.diagnosis {
        s.push_str(&format!("Diagnóstico: {d}\n"));
    }
    if !c.mods.is_empty() {
        s.push_str(&format!("\nMods ({}):\n", c.mods.len()));
        for m in &c.mods {
            s.push_str(&format!("  {m}\n"));
        }
    }
    s.push_str("\n--- registro ---\n");
    let from = logs.len().saturating_sub(MAX_LINES);
    for l in &logs[from..] {
        s.push_str(l);
        s.push('\n');
    }
    s
}

#[derive(Deserialize)]
struct Mclogs {
    success: bool,
    #[serde(default)]
    url: String,
    #[serde(default)]
    error: String,
}

/// Sube el texto a mclo.gs y devuelve el enlace.
pub async fn upload(client: &reqwest::Client, text: &str) -> Result<String> {
    let r: Mclogs = client
        .post("https://api.mclo.gs/1/log")
        .form(&[("content", text)])
        .send()
        .await
        .context("no se pudo contactar con mclo.gs")?
        .error_for_status()?
        .json()
        .await
        .context("respuesta inesperada de mclo.gs")?;
    if !r.success || r.url.is_empty() {
        bail!("mclo.gs no aceptó el registro: {}", r.error);
    }
    Ok(r.url)
}

/// Enlace para abrir un *issue* ya rellenado (título y cuerpo cortos: el registro va por el enlace).
pub fn issue_url(repo: &str, title: &str, description: &str, log_url: Option<&str>) -> String {
    let mut body = format!("**Brasa {}** · {} {}\n\n", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH);
    body.push_str("### Qué pasó\n\n");
    body.push_str(if description.trim().is_empty() { "_(cuéntalo aquí)_" } else { description.trim() });
    body.push_str("\n\n### Registro\n\n");
    body.push_str(log_url.unwrap_or("_(sin registro)_"));
    body.push('\n');
    // las URLs muy largas fallan: se recorta la descripción si hace falta
    let body: String = body.chars().take(6000).collect();
    format!("https://github.com/{}/issues/new?title={}&body={}", repo.trim(), urlencoding::encode(title), urlencoding::encode(&body))
}

/// Usuario del sistema y carpeta personal, para quitarlos del informe.
pub fn system_identity() -> (Option<String>, Option<String>) {
    let home = directories::BaseDirs::new().map(|b| b.home_dir().display().to_string());
    let user = std::env::var("USERNAME").or_else(|_| std::env::var("USER")).ok();
    (home, user)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_data_never_leaves() {
        let log = "[12:00:00] [main/INFO]: Setting user: Steve\n\
                   (Session ID is token:eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.abcdefghijklmnop:1234abcd)\n\
                   Loading C:\\Users\\Javier\\AppData\\Roaming\\brasa\\instances\\x\n\
                   /home/javier/.local/share/brasa/instances/y\n\
                   args: --username Steve --accessToken eyJabc.def.ghi --uuid 123 --xuid 2535 --clientId ZZZ\n\
                   Steven y SteveX no son Steve";
        let out = redact(log, Some("/home/javier"), Some("javier"), &["Steve".to_string()]);
        let out = redact(&out, Some("C:\\Users\\Javier"), Some("Javier"), &[]);
        assert!(!out.contains("eyJ"), "{out}");
        assert!(!out.contains("javier") && !out.contains("Javier"), "{out}");
        assert!(out.contains("Setting user: <jugador>"));
        assert!(out.contains("~/.local/share/brasa") && out.contains("~\\AppData"));
        assert!(out.contains("--accessToken <oculto>") && out.contains("--xuid <oculto>") && out.contains("--clientId <oculto>"));
        assert!(out.contains("Steven y SteveX no son <jugador>"), "sólo palabras completas: {out}");
    }

    #[test]
    fn report_has_a_summary_and_the_last_lines() {
        let c = Context_ {
            description: "Se cierra al entrar".into(),
            instance: Some("Create 1.20".into()),
            loader: Some("Forge 1.20.1".into()),
            mods: vec!["create-0.5.1.jar".into()],
            memory_mb: Some(6144),
            java: Some("17".into()),
            diagnosis: vec!["El juego se quedó sin memoria".into()],
        };
        let logs: Vec<String> = (0..MAX_LINES + 50).map(|i| format!("línea {i}")).collect();
        let r = build(&c, &logs);
        assert!(r.starts_with(&format!("Brasa Launcher {}", env!("CARGO_PKG_VERSION"))));
        assert!(r.contains("Instancia: Create 1.20 · Forge 1.20.1 · 1 mods") && r.contains("Qué pasó:\nSe cierra al entrar"));
        assert!(r.contains("Diagnóstico: El juego se quedó sin memoria") && r.contains("  create-0.5.1.jar"));
        assert!(!r.contains("línea 49\n") && r.contains(&format!("línea {}", MAX_LINES + 49)), "sólo las últimas líneas");
    }

    #[test]
    fn issue_link_is_short_and_encoded() {
        let u = issue_url("Piratemajo/brasa-releases", "Se cierra al entrar", "Pasa con & y #", Some("https://mclo.gs/abc"));
        assert!(u.starts_with("https://github.com/Piratemajo/brasa-releases/issues/new?title=Se%20cierra%20al%20entrar&body="));
        assert!(u.contains("https%3A%2F%2Fmclo.gs%2Fabc") && u.contains("%26") && u.contains("%23"));
        let long = issue_url("a/b", "t", &"x".repeat(20_000), None);
        assert!(long.len() < 20_000);
    }
}
