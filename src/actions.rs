//! Operaciones de alto nivel compartidas por la CLI y la GUI.

use crate::account::auth::{self, AccountKind, AccountStore};
use crate::config::{Instance, Settings};
use crate::game::launch;
use crate::paths::Paths;
use crate::progress::Reporter;
use anyhow::{Context, Result};

/// Prepara y lanza una instancia con la cuenta activa (refrescando el token si caducó).
pub async fn play(client: &reqwest::Client, p: &Paths, id: &str, verify: bool, rep: &Reporter) -> Result<i32> {
    play_on(client, p, id, verify, None, rep).await
}

/// Como [`play`], pero entrando directamente a `server` (sólo en esta partida: no se guarda).
pub async fn play_on(client: &reqwest::Client, p: &Paths, id: &str, verify: bool, server: Option<String>, rep: &Reporter) -> Result<i32> {
    let s = Settings::load(p);
    let mut inst = Instance::load(p, id)?;

    let mut store = AccountStore::load(p);
    let acc = store.active().cloned().context("no hay cuenta activa: añade una cuenta premium (Microsoft) o no premium (offline / Ely.by / LittleSkin)")?;
    rep.stage(format!("Comprobando sesión de {}", acc.username));
    let acc = auth::ensure_fresh(client, &s.msa_client_id, &acc).await?;
    store.upsert(acc.clone());
    store.save(p)?;

    let agent_args = match (&acc.kind, &acc.auth_server) {
        (AccountKind::Yggdrasil, Some(api)) => {
            rep.stage(format!("Preparando skins de {}", crate::account::yggdrasil::display_name(api)));
            crate::account::yggdrasil::jvm_args(client, p, api).await?
        }
        _ => vec![],
    };
    if acc.demo {
        rep.log("[brasa] Esta cuenta de Microsoft no tiene Minecraft: se abre en modo demo".to_string());
    }
    rep.log(format!("[brasa] Cuenta: {} ({})", acc.username, acc.label()));

    let prep = launch::prepare(client, p, &s, &mut inst, verify, rep).await?;
    match crate::account::skins::apply_offline_skin(p, &inst, &acc) {
        Ok(true) => rep.log("[brasa] Skin local aplicada (resource pack brasa-skin)".to_string()),
        Ok(false) => {}
        Err(e) => rep.log(format!("[brasa] No se pudo aplicar la skin local: {e:#}")),
    }
    // cosméticos de la Taquilla: lo puesto, y el mod que los dibuja si la versión tiene adaptador
    let look = crate::cosmetics::loadout::Wardrobe::load(p).of(&acc.uuid);
    let wearing = match crate::cosmetics::export::write(&inst.game_dir(p), &look, &acc.username) {
        Ok(Some(e)) => {
            let mut names: Vec<String> = e.cape.iter().map(|c| c.name.clone()).collect();
            names.extend(e.items.iter().map(|i| i.name.clone()));
            let emotes = if e.emotes.is_empty() { String::new() } else { format!(" y {} emotes", e.emotes.len()) };
            rep.log(format!("[brasa] Cosméticos: {}{emotes}", if names.is_empty() { "ninguno a la vista".to_string() } else { names.join(", ") }));
            rep.log(format!("[brasa] {}", crate::cosmetics::support::status(&inst).describe()));
            true
        }
        Ok(None) => false,
        Err(e) => {
            rep.log(format!("[brasa] No se pudieron preparar los cosméticos: {e:#}"));
            false
        }
    };
    let msg = crate::cosmetics::install::ensure(client, p, &s.update_repo, &inst, wearing).await;
    if !msg.is_empty() {
        rep.log(format!("[brasa] {msg}"));
    }
    if s.auto_backup_worlds {
        rep.stage("Copia de seguridad de mundos");
        let (pp, ii, keep) = (p.clone(), inst.clone(), s.backups_to_keep);
        match tokio::task::spawn_blocking(move || crate::instance::worlds::auto_backup(&pp, &ii, keep)).await {
            Ok((done, errors)) => {
                for b in done {
                    rep.log(format!("[brasa] Copia de «{}» guardada ({})", b.world, crate::instance::worlds::fmt_size(b.size)));
                }
                for e in errors {
                    rep.log(format!("[brasa] No se pudo copiar el mundo {e}"));
                }
            }
            Err(e) => rep.log(format!("[brasa] Copia de mundos interrumpida: {e}")),
        }
    }
    if inst.saving_mode {
        match crate::game::perf::apply_saving_mode(&inst.game_dir(p), &inst.mc_version) {
            Ok(changes) if changes.is_empty() => rep.log("[brasa] Modo ahorro: ajustes gráficos ya limitados".to_string()),
            Ok(changes) => rep.log(format!("[brasa] Modo ahorro: {}", changes.join(", "))),
            Err(e) => rep.log(format!("[brasa] No se pudo aplicar el modo ahorro: {e:#}")),
        }
    }
    let saved_server = inst.join_server.clone();
    if let Some(sv) = server.filter(|s| !s.trim().is_empty()) {
        rep.log(format!("[brasa] Entrando a {sv}"));
        inst.join_server = Some(sv);
    }
    let cmd = launch::build_command(p, &s, &inst, &prep, &acc, &agent_args)?;
    inst.join_server = saved_server;
    rep.log(format!("[brasa] Java: {}", prep.java.display()));
    let total_ram = crate::game::perf::total_ram_mb();
    let mods = crate::game::perf::count_active_mods(&inst.mods_dir(p));
    let (mem, src) = crate::game::perf::effective_memory(&s, &inst, total_ram, mods);
    let origin = match src {
        crate::game::perf::MemorySource::Fixed => "fijada en la instancia".to_string(),
        crate::game::perf::MemorySource::Default => "memoria por defecto".to_string(),
        crate::game::perf::MemorySource::Auto => match total_ram {
            Some(t) => format!("automática: {mods} mods, equipo de {}", crate::game::perf::fmt_gb(t)),
            None => format!("automática: {mods} mods"),
        },
    };
    rep.log(format!("[brasa] Versión: {}  |  Memoria: {mem} MB ({origin})", prep.resolved.json.id));
    if let Some(w) = crate::game::perf::over_allocation_warning(mem, total_ram) {
        rep.log(format!("[brasa] Aviso: {w}"));
    }
    launch::run(p, &mut inst, cmd, rep).await
}

/// Sólo instala/descarga (útil para preparar sin jugar o reparar).
pub async fn install_only(client: &reqwest::Client, p: &Paths, id: &str, verify: bool, rep: &Reporter) -> Result<()> {
    let s = Settings::load(p);
    let mut inst = Instance::load(p, id)?;
    if verify {
        // Forzar re-resolución del loader por si el JSON se corrompió.
        inst.resolved_version = inst.resolved_version.take().filter(|v| p.version_json(v).exists());
    }
    launch::prepare(client, p, &s, &mut inst, verify, rep).await?;
    rep.stage(format!("'{}' lista para jugar", inst.name));
    Ok(())
}

pub fn fmt_duration(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    if h > 0 {
        format!("{h} h {m} min")
    } else {
        format!("{m} min")
    }
}

/// Texto del diagnóstico de red.
pub async fn network_report(client: &reqwest::Client) -> String {
    let mut out = String::new();
    for (name, r) in crate::net::diagnose(client).await {
        match r {
            crate::net::Check::Ok(ms) => out.push_str(&format!("✔ {name}: {ms} ms\n")),
            crate::net::Check::Warn(w) => out.push_str(&format!("⚠ {name}: {w}\n")),
            crate::net::Check::Fail(e) => out.push_str(&format!("✖ {name}: {e}\n")),
        }
    }
    out
}
