//! Lectura de las líneas de la consola: nivel, hora, origen y mensaje, trazas de error agrupadas
//! y el último cierre inesperado con su diagnóstico. Funciones puras (sin egui), con tests.
//!
//! Formatos que reconoce:
//! - log4j del juego: `[14:32:01] [Render thread/INFO]: mensaje`
//! - Forge/NeoForge: `[25oct.2026 14:32:01.123] [main/WARN] [net.minecraftforge.Foo/CORE]: mensaje`
//! - Brasa: `==> fase`, `[brasa] mensaje`, `[error] mensaje`
//! - trazas de Java: `\tat …`, `Caused by: …`, `\t... 12 more`

use std::ops::Range;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Error,
    Warn,
    Info,
    Debug,
    /// Mensajes del propio launcher (`[brasa] …`).
    Brasa,
    /// Cambio de fase (`==> …`): separa la consola en secciones.
    Stage,
    /// Línea de una traza de Java: se agrupa bajo la línea anterior.
    Trace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    pub level: Level,
    /// Hora `HH:MM:SS` dentro de la línea.
    pub time: Option<Range<usize>>,
    /// Hilo o componente que escribe (nombre corto).
    pub source: Option<String>,
    /// Dónde empieza el mensaje.
    pub msg: usize,
}

fn level_word(w: &str) -> Level {
    match w.trim().to_ascii_uppercase().as_str() {
        "ERROR" | "FATAL" | "SEVERE" => Level::Error,
        "WARN" | "WARNING" => Level::Warn,
        "DEBUG" | "TRACE" => Level::Debug,
        _ => Level::Info,
    }
}

/// `[...]` al principio de `s` (tras espacios): rango del contenido y fin del corchete.
fn bracket(s: &str, from: usize) -> Option<(Range<usize>, usize)> {
    let rest = &s[from..];
    let skip = rest.len() - rest.trim_start_matches(' ').len();
    let start = from + skip;
    if !s[start..].starts_with('[') {
        return None;
    }
    let end = start + s[start..].find(']')?;
    Some((start + 1..end, end + 1))
}

/// Hora `HH:MM:SS` dentro de un texto como `14:32:01` o `25oct.2026 14:32:01.123`.
fn find_time(s: &str, r: Range<usize>) -> Option<Range<usize>> {
    let b = s.as_bytes();
    (r.start..r.end.saturating_sub(7)).find_map(|i| {
        let d = |k: usize| b[i + k].is_ascii_digit();
        (d(0) && d(1) && b[i + 2] == b':' && d(3) && d(4) && b[i + 5] == b':' && d(6) && d(7)).then_some(i..i + 8)
    })
}

/// Nombre corto de un hilo o logger: `net.minecraftforge.fml.Loader/CORE` → `Loader`.
fn short_source(raw: &str) -> String {
    let base = raw.split('/').next().unwrap_or(raw);
    let last = base.rsplit('.').next().unwrap_or(base);
    last.trim().to_string()
}

fn is_trace(line: &str) -> bool {
    let t = line.trim_start();
    let indented = line.starts_with('\t') || line.starts_with("    ");
    (indented && (t.starts_with("at ") || t.starts_with("... "))) || t.starts_with("Caused by:") || t.starts_with("Suppressed:")
}

/// `java.lang.OutOfMemoryError: …`, `net.foo.BarException`: la cabecera de una excepción de Java.
fn is_throwable(line: &str) -> bool {
    let first = line.split_whitespace().next().unwrap_or("").trim_end_matches(':');
    first.contains('.')
        && (first.ends_with("Error") || first.ends_with("Exception") || first.ends_with("Throwable"))
        && first.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '$' || c == '_')
}

pub fn parse(line: &str) -> Parsed {
    let plain = |level, msg| Parsed { level, time: None, source: None, msg };
    if let Some(rest) = line.strip_prefix("==> ") {
        return plain(Level::Stage, line.len() - rest.len());
    }
    if line.starts_with("[brasa] ") {
        return plain(Level::Brasa, 8);
    }
    if line.starts_with("[error] ") {
        return plain(Level::Error, 8);
    }
    if is_trace(line) {
        return plain(Level::Trace, 0);
    }
    // [hora] [hilo/NIVEL] ([logger/MARCA])?: mensaje
    if let Some((t, after_t)) = bracket(line, 0) {
        if let Some(time) = find_time(line, t.clone()) {
            if let Some((th, after_th)) = bracket(line, after_t) {
                let inner = &line[th];
                let (thread, lvl) = match inner.rfind('/') {
                    Some(i) => (&inner[..i], level_word(&inner[i + 1..])),
                    None => (inner, Level::Info),
                };
                let mut source = short_source(thread);
                let mut end = after_th;
                if let Some((lg, after_lg)) = bracket(line, after_th) {
                    source = short_source(&line[lg]);
                    end = after_lg;
                }
                let rest = &line[end..];
                let msg = end + (rest.len() - rest.trim_start_matches([':', ' ']).len());
                return Parsed { level: lvl, time: Some(time), source: Some(source), msg };
            }
            return Parsed { level: Level::Info, time: Some(time), source: None, msg: after_t };
        }
    }
    // sin formato: algunas pistas sueltas
    let lv = if is_throwable(line) || line.contains("Exception") || line.contains("ERROR") || line.starts_with("Error:") {
        Level::Error
    } else if line.contains("WARN") {
        Level::Warn
    } else {
        Level::Info
    };
    plain(lv, 0)
}

/// Qué niveles muestra la consola.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Show {
    #[default]
    All,
    /// Errores y avisos.
    Problems,
    /// Sólo lo que dice Brasa (fases y mensajes del launcher).
    Brasa,
}

impl Show {
    pub fn keeps(self, l: Level) -> bool {
        match self {
            Show::All => true,
            Show::Problems => matches!(l, Level::Error | Level::Warn),
            Show::Brasa => matches!(l, Level::Brasa | Level::Stage),
        }
    }
}

/// Una fila de la consola ya dispuesta para dibujar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// Línea normal (índice en `logs`).
    Line(usize),
    /// Resumen plegable de una traza: primera línea de la traza, cuántas son y si está abierta.
    Fold { first: usize, count: usize, open: bool },
}

/// Filas visibles: aplica el filtro de nivel y el texto buscado, y pliega las trazas bajo la
/// línea que las provoca (salvo las que estén abiertas en `open`).
pub fn rows(lines: &[String], parsed: &[Parsed], show: Show, needle: &str, open: &std::collections::HashSet<usize>) -> Vec<Row> {
    let needle = needle.to_ascii_lowercase();
    let mut out = vec![];
    let mut i = 0;
    while i < parsed.len() {
        if parsed[i].level == Level::Trace {
            // traza sin línea delante: cuenta como error suelto
            let start = i;
            while i < parsed.len() && parsed[i].level == Level::Trace {
                i += 1;
            }
            push_group(&mut out, lines, None, start, i, show, &needle, open);
            continue;
        }
        let head = i;
        i += 1;
        let start = i;
        while i < parsed.len() && parsed[i].level == Level::Trace {
            i += 1;
        }
        push_group(&mut out, lines, Some((head, parsed[head].level)), start, i, show, &needle, open);
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn push_group(
    out: &mut Vec<Row>,
    lines: &[String],
    head: Option<(usize, Level)>,
    start: usize,
    end: usize,
    show: Show,
    needle: &str,
    open: &std::collections::HashSet<usize>,
) {
    let has_trace = end > start;
    // una línea con traza es un error aunque su nivel diga otra cosa
    let level = match head {
        Some((_, l)) if has_trace && !matches!(l, Level::Error | Level::Warn) => Level::Error,
        Some((_, l)) => l,
        None => Level::Error,
    };
    if !show.keeps(level) {
        return;
    }
    let hit = |k: usize| needle.is_empty() || lines[k].to_ascii_lowercase().contains(needle);
    let head_hit = head.is_some_and(|(h, _)| hit(h));
    let trace_hits: Vec<usize> = (start..end).filter(|&k| !needle.is_empty() && hit(k)).collect();
    if !needle.is_empty() && !head_hit && trace_hits.is_empty() {
        return;
    }
    if let Some((h, _)) = head {
        out.push(Row::Line(h));
    }
    if has_trace {
        // al buscar, una traza con coincidencias se abre sola
        let is_open = open.contains(&start) || !trace_hits.is_empty() || head.is_none() && end - start <= 3;
        if head.is_some() {
            out.push(Row::Fold { first: start, count: end - start, open: is_open });
        }
        if is_open || head.is_none() {
            out.extend((start..end).map(Row::Line));
        }
    }
}

/// Rangos (en bytes) donde aparece `needle`, sin distinguir mayúsculas.
pub fn matches(hay: &str, needle: &str) -> Vec<Range<usize>> {
    if needle.is_empty() {
        return vec![];
    }
    let h = hay.to_ascii_lowercase();
    let n = needle.to_ascii_lowercase();
    let mut out = vec![];
    let mut from = 0;
    while let Some(i) = h[from..].find(&n) {
        let s = from + i;
        out.push(s..s + n.len());
        from = s + n.len();
    }
    out
}

/// El último cierre inesperado del juego, si nada se ha lanzado después.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crash {
    /// Línea `==> El juego se cerró con código N` (para no volver a enseñarlo si se descarta).
    pub at: usize,
    pub code: String,
    /// (causa, qué hacer)
    pub causes: Vec<(String, String)>,
    pub report: Option<PathBuf>,
    /// Primera línea de esa partida (`==> Jugando: …`), para copiar el informe.
    pub session_start: usize,
}

pub fn last_crash(lines: &[String]) -> Option<Crash> {
    const CLOSED: &str = "==> El juego se cerró con código ";
    let at = lines.iter().rposition(|l| l.starts_with(CLOSED))?;
    // si después se volvió a lanzar algo, ese aviso ya no toca
    if lines[at + 1..].iter().any(|l| l.starts_with("==> Jugando: ") || l.starts_with("==> Preparando")) {
        return None;
    }
    let code = lines[at][CLOSED.len()..].trim().to_string();
    let mut causes = vec![];
    let mut report = None;
    for l in &lines[at + 1..] {
        if let Some(d) = l.strip_prefix("[brasa] Diagnóstico: ") {
            let (t, f) = d.split_once(" → ").unwrap_or((d, ""));
            causes.push((t.to_string(), f.to_string()));
        } else if let Some(p) = l.strip_prefix("[brasa] Crash report: ") {
            report = Some(PathBuf::from(p.trim()));
        }
    }
    let session_start = lines[..at].iter().rposition(|l| l.starts_with("==> Jugando: ")).unwrap_or(0);
    Some(Crash { at, code, causes, report, session_start })
}

/// Texto para pedir ayuda: diagnóstico y el registro de esa partida (las últimas `max` líneas).
pub fn crash_report_text(lines: &[String], c: &Crash, max: usize) -> String {
    let mut s = format!("Brasa {} · el juego se cerró con código {}\n", env!("CARGO_PKG_VERSION"), c.code);
    for (t, f) in &c.causes {
        s.push_str(&format!("Causa probable: {t}. {f}\n"));
    }
    s.push_str("\n--- registro ---\n");
    let end = lines.len();
    let from = c.session_start.max(end.saturating_sub(max));
    for l in &lines[from..end] {
        s.push_str(l);
        s.push('\n');
    }
    s
}

/// Lecturas ya hechas, para no volver a analizar 20.000 líneas en cada fotograma.
#[derive(Default)]
pub struct Cache {
    /// Líneas descartadas por el principio cuando se hizo la última lectura.
    dropped: usize,
    pub parsed: Vec<Parsed>,
}

impl Cache {
    /// Pone la caché al día con `lines`, sabiendo cuántas se han tirado ya por el principio.
    pub fn sync(&mut self, lines: &[String], dropped: usize) {
        let gone = dropped.saturating_sub(self.dropped);
        if gone > 0 {
            self.parsed.drain(..gone.min(self.parsed.len()));
            self.dropped = dropped;
        }
        if self.parsed.len() > lines.len() {
            self.parsed.clear();
        }
        let have = self.parsed.len();
        self.parsed.extend(lines[have..].iter().map(|l| parse(l)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn vanilla_forge_and_brasa_lines() {
        let l = "[14:32:01] [Render thread/WARN]: Missing sound for event";
        let p = parse(l);
        assert_eq!(p.level, Level::Warn);
        assert_eq!(&l[p.time.clone().unwrap()], "14:32:01");
        assert_eq!(p.source.as_deref(), Some("Render thread"));
        assert_eq!(&l[p.msg..], "Missing sound for event");

        let f = "[25oct.2026 14:32:01.123] [main/ERROR] [net.minecraftforge.fml.loading.ModSorter/LOADING]: Mod create needs flywheel";
        let p = parse(f);
        assert_eq!((p.level, p.source.as_deref()), (Level::Error, Some("ModSorter")));
        assert_eq!(&f[p.time.clone().unwrap()], "14:32:01");
        assert_eq!(&f[p.msg..], "Mod create needs flywheel");

        assert_eq!(parse("==> Jugando: Create").level, Level::Stage);
        let b = parse("[brasa] Memoria: 4096 MB (automática)");
        assert_eq!((b.level, b.msg), (Level::Brasa, 8));
        assert_eq!(parse("[error] algo").level, Level::Error);
        assert_eq!(parse("\tat net.minecraft.client.Main.main(Main.java:1)").level, Level::Trace);
        assert_eq!(parse("Caused by: java.lang.NullPointerException").level, Level::Trace);
        assert_eq!(parse("java.lang.IllegalStateException: boom").level, Level::Error);
        assert_eq!(parse("java.lang.OutOfMemoryError: Java heap space").level, Level::Error);
        assert_eq!(parse("Error de usuario normal").level, Level::Info);
        assert_eq!(parse("texto suelto").level, Level::Info);
        // corchetes que no son hora: no se rompe
        assert_eq!(parse("[STDOUT]: hola").level, Level::Info);
        assert_eq!(parse("[").level, Level::Info);
    }

    fn sample() -> Vec<String> {
        [
            "==> Jugando: Create",
            "[14:00:00] [main/INFO]: arranque",
            "[14:00:01] [main/ERROR]: Fallo al cargar",
            "java.lang.RuntimeException: boom",
            "\tat a.B.c(B.java:1)",
            "\tat a.B.d(B.java:2)",
            "Caused by: java.io.IOException: disco",
            "[14:00:02] [main/WARN]: cuidado",
            "[brasa] hecho",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    fn rows_of(show: Show, q: &str, open: &[usize]) -> Vec<Row> {
        let l = sample();
        let p: Vec<Parsed> = l.iter().map(|x| parse(x)).collect();
        rows(&l, &p, show, q, &open.iter().copied().collect::<HashSet<_>>())
    }

    #[test]
    fn traces_fold_under_their_line() {
        let r = rows_of(Show::All, "", &[]);
        // la excepción (3) se lleva las 3 líneas de traza (4, 5, 6)
        assert_eq!(r, vec![Row::Line(0), Row::Line(1), Row::Line(2), Row::Line(3), Row::Fold { first: 4, count: 3, open: false }, Row::Line(7), Row::Line(8)]);
        let opened = rows_of(Show::All, "", &[4]);
        assert!(opened.contains(&Row::Fold { first: 4, count: 3, open: true }) && opened.contains(&Row::Line(6)));
    }

    #[test]
    fn filters_by_level_and_text() {
        let problems = rows_of(Show::Problems, "", &[]);
        assert_eq!(problems, vec![Row::Line(2), Row::Line(3), Row::Fold { first: 4, count: 3, open: false }, Row::Line(7)]);
        assert_eq!(rows_of(Show::Brasa, "", &[]), vec![Row::Line(0), Row::Line(8)]);
        // buscar dentro de la traza la abre
        let r = rows_of(Show::All, "ioexception", &[]);
        assert_eq!(r, vec![Row::Line(3), Row::Fold { first: 4, count: 3, open: true }, Row::Line(4), Row::Line(5), Row::Line(6)]);
        assert_eq!(rows_of(Show::All, "CUIDADO", &[]), vec![Row::Line(7)]);
    }

    #[test]
    fn matches_ignore_case() {
        assert_eq!(matches("Fallo y FALLO", "fallo"), vec![0..5, 8..13]);
        assert!(matches("x", "").is_empty());
    }

    #[test]
    fn finds_the_last_crash_until_the_next_launch() {
        let mut l: Vec<String> = sample();
        l.push("==> El juego se cerró con código 1".into());
        l.push("[brasa] Crash report: /tmp/crash-1.txt".into());
        l.push("[brasa] Diagnóstico: El juego se quedó sin memoria → Sube la memoria en Ajustes".into());
        let c = last_crash(&l).unwrap();
        assert_eq!(c.code, "1");
        assert_eq!(c.causes, vec![("El juego se quedó sin memoria".to_string(), "Sube la memoria en Ajustes".to_string())]);
        assert_eq!(c.report, Some(PathBuf::from("/tmp/crash-1.txt")));
        assert_eq!(c.session_start, 0);
        let text = crash_report_text(&l, &c, 500);
        assert!(text.contains("código 1") && text.contains("sin memoria") && text.contains("Fallo al cargar"));

        l.push("==> Jugando: Create".into());
        assert_eq!(last_crash(&l), None, "ya se ha vuelto a jugar");
        assert_eq!(last_crash(&sample()), None);
    }

    #[test]
    fn cache_follows_new_and_dropped_lines() {
        let mut c = Cache::default();
        let mut l = sample();
        c.sync(&l, 0);
        assert_eq!(c.parsed.len(), l.len());
        l.drain(..2);
        l.push("[brasa] otra".into());
        c.sync(&l, 2);
        assert_eq!(c.parsed.len(), l.len());
        assert_eq!(c.parsed.last().unwrap().level, Level::Brasa);
        assert_eq!(c.parsed[0], parse(&l[0]));
        // consola limpiada
        c.sync(&[], 2 + l.len());
        assert!(c.parsed.is_empty());
    }
}
