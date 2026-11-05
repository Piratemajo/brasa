//! Preparación y arranque del juego.

use crate::account::auth::Account;
use crate::config::{now_secs, Instance, Settings};
use crate::game::mojang::{self, Arg, ArgValue, Features, Resolved};
use crate::game::{java, loaders};
use crate::paths::Paths;
use crate::progress::Reporter;
use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};

pub struct Prepared {
    pub resolved: Resolved,
    pub java: PathBuf,
}

/// Instala (si hace falta) loader, versión, librerías, assets y Java.
pub async fn prepare(client: &reqwest::Client, p: &Paths, s: &Settings, inst: &mut Instance, verify: bool, rep: &Reporter) -> Result<Prepared> {
    let vid = match &inst.resolved_version {
        Some(v) if p.version_json(v).exists() => v.clone(),
        _ => {
            let id = loaders::install(client, p, inst.loader, &inst.mc_version, inst.loader_version.as_deref(), rep).await?;
            inst.resolved_version = Some(id.clone());
            inst.save(p)?;
            id
        }
    };
    rep.stage(format!("Resolviendo {vid}"));
    let resolved = mojang::resolve(client, p, &vid).await?;
    std::fs::create_dir_all(inst.game_dir(p))?;
    mojang::install(client, p, &resolved, &inst.game_dir(p), s.concurrency, verify, rep).await?;
    let major = mojang::required_java(&resolved.json);
    let component = mojang::java_component(&resolved.json);
    let java = java::pick(client, p, inst.java_path.as_deref(), Some(s.java_path.as_str()), major, component.as_deref(), s.java_auto_download, rep).await?;
    Ok(Prepared { resolved, java })
}

fn is_native_file(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.ends_with(".so") || n.ends_with(".dll") || n.ends_with(".dylib") || n.ends_with(".jnilib")
}

fn extract_natives(p: &Paths, r: &Resolved, dir: &Path) -> Result<()> {
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir)?;
    for lib in mojang::library_files(p, &r.json).into_iter().filter(|l| l.native) {
        let Ok(f) = std::fs::File::open(&lib.path) else { continue };
        let mut z = zip::ZipArchive::new(f).with_context(|| format!("jar de nativos corrupto: {}", lib.path.display()))?;
        for i in 0..z.len() {
            let mut e = z.by_index(i)?;
            let name = e.name().to_string();
            if e.is_dir() || name.starts_with("META-INF") || lib.exclude.iter().any(|x| name.starts_with(x.as_str())) {
                continue;
            }
            if !is_native_file(&name) {
                continue;
            }
            let file_name = name.rsplit('/').next().unwrap_or(&name).to_string();
            let mut buf = Vec::new();
            e.read_to_end(&mut buf)?;
            std::fs::write(dir.join(file_name), buf)?;
        }
    }
    Ok(())
}

fn subst(s: &str, vars: &HashMap<&'static str, String>) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find('}') {
            Some(end) => {
                let key = &after[..end];
                match vars.get(key) {
                    Some(v) => out.push_str(v),
                    None => {
                        out.push_str("${");
                        out.push_str(key);
                        out.push('}');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

fn expand(args: &[Arg], feat: &Features, vars: &HashMap<&'static str, String>, out: &mut Vec<String>) {
    for a in args {
        match a {
            Arg::Plain(s) => out.push(subst(s, vars)),
            Arg::Ruled { rules, value } => {
                if mojang::rules_allow(&Some(rules.clone()), feat) {
                    match value {
                        ArgValue::One(s) => out.push(subst(s, vars)),
                        ArgValue::Many(v) => out.extend(v.iter().map(|s| subst(s, vars))),
                    }
                }
            }
        }
    }
}

const GC_FLAGS: &[&str] = &[
    "-XX:+UseG1GC",
    "-XX:+ParallelRefProcEnabled",
    "-XX:MaxGCPauseMillis=50",
    "-XX:+UnlockExperimentalVMOptions",
    "-XX:+DisableExplicitGC",
    "-XX:G1NewSizePercent=30",
    "-XX:G1MaxNewSizePercent=40",
    "-XX:G1HeapRegionSize=8M",
    "-XX:G1ReservePercent=20",
    "-XX:InitiatingHeapOccupancyPercent=15",
];

/// Construye la línea de comandos completa (java + args).
/// `agent_args`: argumentos JVM extra según la cuenta (p.ej. authlib-injector para cuentas no premium con skin).
pub fn build_command(p: &Paths, s: &Settings, inst: &Instance, prep: &Prepared, acc: &Account, agent_args: &[String]) -> Result<Vec<String>> {
    let v = &prep.resolved.json;
    let natives = inst.dir(p).join("natives");
    extract_natives(p, &prep.resolved, &natives)?;

    let sep = if cfg!(windows) { ";" } else { ":" };
    let mut cp: Vec<String> = mojang::library_files(p, v).into_iter().filter(|l| l.classpath).map(|l| l.path.to_string_lossy().into_owned()).collect();
    cp.push(p.version_jar(&v.id).to_string_lossy().into_owned());
    let mut seen_cp = HashSet::new();
    cp.retain(|x| seen_cp.insert(x.clone()));

    let game_dir = inst.game_dir(p);
    let assets_id = mojang::assets_id(v);
    let game_assets = if assets_id == "pre-1.6" || assets_id == "legacy" { p.assets().join("virtual").join(&assets_id) } else { p.assets() };
    let (w, h) = (inst.width.unwrap_or(1280), inst.height.unwrap_or(720));
    let supports_quickplay = v
        .arguments
        .as_ref()
        .map(|a| a.game.iter().any(|g| matches!(g, Arg::Ruled { value: ArgValue::Many(x), .. } if x.iter().any(|s| s.contains("quickPlayMultiplayer")))))
        .unwrap_or(false);
    let feat = Features {
        custom_resolution: inst.width.is_some() || inst.height.is_some(),
        quick_play_multiplayer: supports_quickplay && inst.join_server.is_some(),
        demo: acc.demo,
    };

    let path_str = |x: &Path| x.to_string_lossy().into_owned();
    let mut vars: HashMap<&'static str, String> = HashMap::new();
    vars.insert("auth_player_name", acc.username.clone());
    vars.insert("version_name", v.id.clone());
    vars.insert("game_directory", path_str(&game_dir));
    vars.insert("assets_root", path_str(&p.assets()));
    vars.insert("game_assets", path_str(&game_assets));
    vars.insert("assets_index_name", assets_id.clone());
    vars.insert("auth_uuid", acc.uuid.clone());
    vars.insert("auth_access_token", acc.access_token.clone());
    vars.insert("auth_session", format!("token:{}:{}", acc.access_token, acc.uuid));
    vars.insert("clientid", s.msa_client_id.clone());
    vars.insert("auth_xuid", acc.xuid.clone());
    vars.insert("user_type", acc.user_type().to_string());
    vars.insert("version_type", v.kind.clone().unwrap_or_else(|| "release".into()));
    vars.insert("user_properties", "{}".into());
    vars.insert("natives_directory", path_str(&natives));
    vars.insert("launcher_name", "brasa".into());
    vars.insert("launcher_version", env!("CARGO_PKG_VERSION").into());
    vars.insert("classpath", cp.join(sep));
    vars.insert("classpath_separator", sep.into());
    vars.insert("library_directory", path_str(&p.libraries()));
    vars.insert("resolution_width", w.to_string());
    vars.insert("resolution_height", h.to_string());
    vars.insert("quickPlayMultiplayer", inst.join_server.clone().unwrap_or_default());
    vars.insert("quickPlayPath", path_str(&inst.dir(p).join("quickplay.json")));

    let mut cmd: Vec<String> = vec![prep.java.to_string_lossy().into_owned()];
    let (mem, _) = crate::game::perf::effective_memory(s, inst, crate::game::perf::total_ram_mb(), crate::game::perf::count_active_mods(&inst.mods_dir(p)));
    cmd.push(format!("-Xmx{mem}M"));
    cmd.push(format!("-Xms{}M", (mem / 4).max(512)));
    if s.optimized_gc {
        cmd.extend(GC_FLAGS.iter().map(|x| x.to_string()));
    }
    cmd.extend(s.extra_jvm_args.iter().cloned());
    cmd.extend(inst.jvm_args.iter().cloned());
    cmd.extend(agent_args.iter().cloned());

    match &v.arguments {
        Some(a) if !a.jvm.is_empty() => expand(&a.jvm, &feat, &vars, &mut cmd),
        _ => {
            if cfg!(target_os = "macos") {
                cmd.push("-XstartOnFirstThread".into());
            }
            cmd.push(subst("-Djava.library.path=${natives_directory}", &vars));
            cmd.push("-cp".into());
            cmd.push(cp.join(sep));
        }
    }
    if let Some(lc) = v.logging.as_ref().and_then(|l| l.client.as_ref()) {
        let cfg = p.assets().join("log_configs").join(&lc.file.id);
        cmd.push(lc.argument.replace("${path}", &path_str(&cfg)));
    }
    cmd.push(v.main_class.clone().context("la versión no define mainClass")?);

    let mut game = vec![];
    if let Some(a) = &v.arguments {
        expand(&a.game, &feat, &vars, &mut game);
    }
    if let Some(legacy) = &v.minecraft_arguments {
        game.extend(legacy.split_whitespace().map(|x| subst(x, &vars)));
        if feat.custom_resolution {
            game.extend(["--width".into(), w.to_string(), "--height".into(), h.to_string()]);
        }
        if feat.demo {
            game.push("--demo".into());
        }
    }
    if let (Some(server), false) = (&inst.join_server, supports_quickplay) {
        let (host, port) = server.split_once(':').unwrap_or((server.as_str(), "25565"));
        game.extend(["--server".into(), host.to_string(), "--port".into(), port.to_string()]);
    }
    if inst.fullscreen {
        game.push("--fullscreen".into());
    }
    cmd.extend(game);
    // Wrapper (gamemoderun, mangohud...): se antepone a todo el comando.
    if let Some(w) = inst.wrapper.as_deref().map(split_args).filter(|w| !w.is_empty()) {
        let mut full = w;
        full.extend(cmd);
        return Ok(full);
    }
    Ok(cmd)
}

/// Divide una línea de comando respetando comillas simples y dobles (sin expansión de shell).
pub fn split_args(line: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut has = false;
    for c in line.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => cur.push(c),
            (None, '"') | (None, '\'') => {
                quote = Some(c);
                has = true;
            }
            (None, c) if c.is_whitespace() => {
                if has || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            (None, c) => cur.push(c),
        }
    }
    if has || !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// `CLAVE=valor` → pares; ignora líneas vacías o sin `=`.
pub fn parse_env(vars: &[String]) -> Vec<(String, String)> {
    vars.iter().filter_map(|l| l.split_once('=')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).filter(|(k, _)| !k.is_empty()).collect()
}

/// Ejecuta un comando de usuario con el shell del sistema, en la carpeta del juego.
async fn run_hook(line: &str, inst: &Instance, game_dir: &Path, what: &str, rep: &Reporter) -> Result<bool> {
    rep.log(format!("[brasa] {what}: {line}"));
    #[cfg(windows)]
    let mut c = {
        let mut c = tokio::process::Command::new("cmd");
        c.args(["/C", line]);
        c
    };
    #[cfg(not(windows))]
    let mut c = {
        let mut c = tokio::process::Command::new("sh");
        c.args(["-c", line]);
        c
    };
    let status = tokio::time::timeout(
        std::time::Duration::from_secs(120),
        c.current_dir(game_dir).env("BRASA_INSTANCE", &inst.id).env("BRASA_GAME_DIR", game_dir).stdin(Stdio::null()).status(),
    )
    .await
    .with_context(|| format!("{what} tardó más de 120 s"))?
    .with_context(|| format!("no se pudo ejecutar el comando de {what}"))?;
    Ok(status.success())
}

/// Escribe cada línea del juego en disco y guarda las últimas para el diagnóstico.
struct LogSink {
    file: Option<std::io::LineWriter<std::fs::File>>,
    tail: std::collections::VecDeque<String>,
}

const TAIL_LINES: usize = 600;

impl LogSink {
    fn open(inst_dir: &Path) -> LogSink {
        let dir = inst_dir.join("logs");
        let file = std::fs::create_dir_all(&dir).ok().and_then(|_| {
            let latest = dir.join("brasa-latest.log");
            let _ = std::fs::rename(&latest, dir.join("brasa-previous.log"));
            std::fs::File::create(latest).ok()
        });
        LogSink { file: file.map(std::io::LineWriter::new), tail: Default::default() }
    }
    fn push(&mut self, line: &str) {
        use std::io::Write;
        if let Some(f) = &mut self.file {
            let _ = writeln!(f, "{line}");
        }
        if self.tail.len() >= TAIL_LINES {
            self.tail.pop_front();
        }
        self.tail.push_back(line.to_string());
    }
    fn text(&self) -> String {
        self.tail.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n")
    }
}

fn running_map() -> &'static std::sync::Mutex<HashMap<String, std::sync::Arc<tokio::sync::Notify>>> {
    static M: std::sync::OnceLock<std::sync::Mutex<HashMap<String, std::sync::Arc<tokio::sync::Notify>>>> = std::sync::OnceLock::new();
    M.get_or_init(Default::default)
}

/// Mata el juego de una instancia. Devuelve false si no estaba abierta.
pub fn kill(instance_id: &str) -> bool {
    match running_map().lock().unwrap().get(instance_id) {
        Some(n) => {
            n.notify_one();
            true
        }
        None => false,
    }
}

/// Lanza el juego y espera a que termine, reenviando la salida al reporter. Devuelve el código de salida.
pub async fn run(p: &Paths, inst: &mut Instance, cmd: Vec<String>, rep: &Reporter) -> Result<i32> {
    let game_dir = inst.game_dir(p);
    if let Some(pre) = inst.pre_launch.clone().filter(|x| !x.trim().is_empty()) {
        rep.stage("Ejecutando comando previo");
        if !run_hook(&pre, inst, &game_dir, "Comando previo", rep).await? {
            anyhow::bail!("el comando previo falló; no se lanza el juego");
        }
    }
    rep.stage(format!("Jugando: {}", inst.name));
    let mut child = tokio::process::Command::new(&cmd[0])
        .args(&cmd[1..])
        .envs(parse_env(&inst.env_vars))
        .current_dir(&game_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .spawn()
        .with_context(|| format!("no se pudo ejecutar {}", cmd[0]))?;
    rep.started();

    let started = now_secs();
    inst.last_played = Some(started);
    inst.save(p)?;

    let out = child.stdout.take().context("stdout")?;
    let err = child.stderr.take().context("stderr")?;
    let sink = std::sync::Arc::new(std::sync::Mutex::new(LogSink::open(&inst.dir(p))));
    let (r1, r2) = (rep.clone(), rep.clone());
    let (k1, k2) = (sink.clone(), sink.clone());
    let t1 = tokio::spawn(async move {
        let mut l = BufReader::new(out).lines();
        while let Ok(Some(line)) = l.next_line().await {
            k1.lock().unwrap().push(&line);
            r1.log(line);
        }
    });
    let t2 = tokio::spawn(async move {
        let mut l = BufReader::new(err).lines();
        while let Ok(Some(line)) = l.next_line().await {
            k2.lock().unwrap().push(&line);
            r2.log(line);
        }
    });
    let stop = std::sync::Arc::new(tokio::sync::Notify::new());
    running_map().lock().unwrap().insert(inst.id.clone(), stop.clone());
    let status = tokio::select! {
        s = child.wait() => s,
        _ = stop.notified() => {
            rep.log("[brasa] Cerrando el juego a la fuerza".to_string());
            let _ = child.kill().await;
            child.wait().await
        }
    };
    running_map().lock().unwrap().remove(&inst.id);
    let status = status?;
    let _ = tokio::join!(t1, t2);

    inst.playtime_secs += now_secs().saturating_sub(started);
    inst.save(p)?;

    let code = status.code().unwrap_or(-1);
    if code != 0 {
        rep.stage(format!("El juego se cerró con código {code}"));
        let crash = latest_crash_report(&game_dir, started);
        if let Some(cr) = &crash {
            rep.log(format!("[brasa] Crash report: {}", cr.display()));
        }
        let report_text = crash.as_ref().and_then(|c| read_head(c, 256 * 1024));
        let tail = sink.lock().unwrap().text();
        for d in crate::game::diagnose::analyze(&tail, report_text.as_deref()) {
            rep.log(format!("[brasa] Diagnóstico: {} → {}", d.title, d.fix));
        }
    } else {
        rep.stage("Juego cerrado");
    }
    if let Some(post) = inst.post_exit.clone().filter(|x| !x.trim().is_empty()) {
        if let Err(e) = run_hook(&post, inst, &game_dir, "Comando al cerrar", rep).await {
            rep.log(format!("[brasa] {e:#}"));
        }
    }
    Ok(code)
}

/// Crash report más reciente creado desde `since` (segundos Unix); así no se confunde con uno viejo.
fn latest_crash_report(game_dir: &Path, since: u64) -> Option<PathBuf> {
    let since = std::time::UNIX_EPOCH + std::time::Duration::from_secs(since.saturating_sub(2));
    std::fs::read_dir(game_dir.join("crash-reports"))
        .ok()?
        .flatten()
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .filter(|(t, _)| *t >= since)
        .max_by_key(|(t, _)| *t)
        .map(|(_, p)| p)
}

/// Lee como mucho `max` bytes del principio de un fichero de texto.
fn read_head(path: &Path, max: usize) -> Option<String> {
    let mut buf = Vec::new();
    std::fs::File::open(path).ok()?.take(max as u64).read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn substitution() {
        let mut v = HashMap::new();
        v.insert("a", "1".to_string());
        assert_eq!(subst("x${a}y${b}z", &v), "x1y${b}z");
        assert_eq!(subst("${a}", &v), "1");
        assert_eq!(subst("broken${a", &v), "broken${a");
    }

    #[test]
    fn split_args_handles_quotes() {
        assert_eq!(split_args("gamemoderun"), vec!["gamemoderun"]);
        assert_eq!(split_args("  mangohud   --dlsym "), vec!["mangohud", "--dlsym"]);
        assert_eq!(split_args(r#"env "A B=1" 'x y'"#), vec!["env", "A B=1", "x y"]);
        assert_eq!(split_args(r#"a "" b"#), vec!["a", "", "b"]);
        assert!(split_args("   ").is_empty());
    }

    #[test]
    fn parse_env_ignores_junk() {
        let v = vec!["A=1".to_string(), "  B = dos ".into(), "sin_igual".into(), "=x".into(), "C=a=b".into()];
        assert_eq!(parse_env(&v), vec![("A".to_string(), "1".to_string()), ("B".into(), "dos".into()), ("C".into(), "a=b".into())]);
    }

    /// Simula una partida que se cae por falta de memoria: comprueba entorno, hooks, log en disco y diagnóstico.
    #[cfg(unix)]
    #[tokio::test]
    async fn run_pipeline_with_fake_game() {
        use crate::progress::{Event, Reporter};
        use std::sync::{Arc, Mutex};

        let root = std::env::temp_dir().join(format!("brasa-test-{}", uuid::Uuid::new_v4()));
        let p = Paths { root: root.clone() };
        let mut inst = Instance {
            id: "fake".into(),
            name: "fake".into(),
            env_vars: vec!["BRASA_TEST_VAR=hola".into()],
            pre_launch: Some("touch pre.txt".into()),
            post_exit: Some("touch post.txt".into()),
            ..Default::default()
        };
        std::fs::create_dir_all(inst.game_dir(&p)).unwrap();
        inst.save(&p).unwrap();

        let lines = Arc::new(Mutex::new(Vec::<String>::new()));
        let l2 = lines.clone();
        let rep = Reporter::new(move |e| {
            if let Event::Log(l) = e {
                l2.lock().unwrap().push(l);
            }
        });
        let cmd = vec!["sh".to_string(), "-c".into(), "echo valor=$BRASA_TEST_VAR; echo 'java.lang.OutOfMemoryError: Java heap space' >&2; exit 3".into()];
        let code = run(&p, &mut inst, cmd, &rep).await.unwrap();
        assert_eq!(code, 3);

        let out = lines.lock().unwrap().join("\n");
        assert!(out.contains("valor=hola"), "{out}");
        assert!(out.contains("Diagnóstico: El juego se quedó sin memoria"), "{out}");

        let game = inst.game_dir(&p);
        assert!(game.join("pre.txt").exists(), "falta el hook previo");
        assert!(game.join("post.txt").exists(), "falta el hook final");
        let log = std::fs::read_to_string(inst.dir(&p).join("logs/brasa-latest.log")).unwrap();
        assert!(log.contains("valor=hola") && log.contains("OutOfMemoryError"), "{log}");
        assert!(inst.last_played.is_some());

        // Segunda partida: el log anterior pasa a brasa-previous.log.
        let ok = vec!["sh".to_string(), "-c".into(), "echo segunda".into()];
        assert_eq!(run(&p, &mut inst, ok, &rep).await.unwrap(), 0);
        assert!(inst.dir(&p).join("logs/brasa-previous.log").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    /// Si el comando previo falla, el juego no debe lanzarse.
    #[cfg(unix)]
    #[tokio::test]
    async fn failing_pre_launch_blocks_game() {
        let root = std::env::temp_dir().join(format!("brasa-test-{}", uuid::Uuid::new_v4()));
        let p = Paths { root: root.clone() };
        let mut inst = Instance { id: "f".into(), name: "f".into(), pre_launch: Some("exit 1".into()), ..Default::default() };
        std::fs::create_dir_all(inst.game_dir(&p)).unwrap();
        inst.save(&p).unwrap();
        let marker = root.join("lanzado");
        let cmd = vec!["touch".to_string(), marker.to_string_lossy().into_owned()];
        let r = run(&p, &mut inst, cmd, &Reporter::new(|_| {})).await;
        assert!(r.is_err());
        assert!(!marker.exists());
        let _ = std::fs::remove_dir_all(root);
    }
}
