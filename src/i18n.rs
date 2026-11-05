//! Idiomas de la interfaz: español (el del código) e inglés.
//!
//! - `t!("Texto")` da el texto en el idioma actual (`&'static str`).
//! - `t!("{n} líneas", n = n)` es como `format!` (los marcadores se pasan con nombre o en orden).
//! - [`tr`] traduce un texto que llega en tiempo de ejecución (nombres del catálogo, etiquetas…).
//!
//! La tabla está en `i18n/en.tsv`; `build.rs` la convierte en ramas de la macro, así que un texto
//! nuevo sin traducir sale en español (y `scripts/i18n_scan.py` lo encuentra).

use std::sync::atomic::AtomicU8;
#[cfg(not(test))]
use std::sync::atomic::Ordering;

include!(concat!(env!("OUT_DIR"), "/i18n.rs"));

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Es,
    En,
}

#[cfg_attr(test, allow(dead_code))]
static LANG: AtomicU8 = AtomicU8::new(0);

// En los tests, cada hilo tiene su idioma: así un test en inglés no cambia los demás (que corren a la vez).
#[cfg(test)]
thread_local! {
    static TEST_LANG: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

pub fn set(l: Lang) {
    #[cfg(test)]
    TEST_LANG.with(|c| c.set(l as u8));
    #[cfg(not(test))]
    LANG.store(l as u8, Ordering::Relaxed);
}

pub fn current() -> Lang {
    #[cfg(test)]
    let v = TEST_LANG.with(|c| c.get());
    #[cfg(not(test))]
    let v = LANG.load(Ordering::Relaxed);
    if v == 1 {
        Lang::En
    } else {
        Lang::Es
    }
}

pub fn en() -> bool {
    current() == Lang::En
}

pub fn pick(es: &'static str, en: &'static str) -> &'static str {
    if self::en() {
        en
    } else {
        es
    }
}

/// Idioma según el ajuste: `"es"`, `"en"` o `"auto"` (el del sistema: español si empieza por «es»,
/// inglés en cualquier otro caso).
pub fn resolve(setting: &str, system: Option<&str>) -> Lang {
    match setting.trim() {
        "es" => Lang::Es,
        "en" => Lang::En,
        _ => match system {
            Some(l) if l.to_ascii_lowercase().starts_with("es") => Lang::Es,
            Some(_) => Lang::En,
            None => Lang::Es,
        },
    }
}

/// El idioma del sistema (`es-ES`, `en_US.UTF-8`…).
pub fn system_locale() -> Option<String> {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(v) = std::env::var(var) {
            if !v.is_empty() && v != "C" && v != "POSIX" {
                return Some(v);
            }
        }
    }
    sys_locale::get_locale()
}

pub fn apply(setting: &str) {
    set(resolve(setting, system_locale().as_deref()));
}

/// Traduce un texto sin marcadores que no se conoce al compilar.
pub fn tr(s: &str) -> &str {
    if !en() {
        return s;
    }
    PAIRS.iter().find(|(es, _)| *es == s).map(|(_, en)| *en).unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_follows_the_system() {
        assert_eq!(resolve("auto", Some("es_ES.UTF-8")), Lang::Es);
        assert_eq!(resolve("auto", Some("es-MX")), Lang::Es);
        assert_eq!(resolve("auto", Some("en-US")), Lang::En);
        assert_eq!(resolve("auto", Some("fr_FR")), Lang::En, "fuera del español, inglés");
        assert_eq!(resolve("", None), Lang::Es);
        assert_eq!(resolve("en", Some("es_ES")), Lang::En);
        assert_eq!(resolve("es", Some("en_US")), Lang::Es);
    }

    /// Todos los textos y la tabla en un mismo test (el idioma es global).
    #[test]
    fn both_languages_and_every_placeholder() {
        set(Lang::Es);
        assert_eq!(t!("Inicio"), "Inicio");
        let n = 3;
        assert_eq!(t!("{n} líneas", n = n), "3 líneas");
        set(Lang::En);
        assert_eq!(t!("Inicio"), "Home");
        assert_eq!(t!("{n} líneas", n = n), "3 lines");
        assert_eq!(t!("Instalado: {}", "Sodium"), "Installed: Sodium");
        assert_eq!(t!("texto que no está en la tabla"), "texto que no está en la tabla");
        assert_eq!(tr("Ajustes"), "Settings");
        assert_eq!(tr("no está"), "no está");
        set(Lang::Es);
        assert_eq!(tr("Ajustes"), "Ajustes");
    }

    /// La tabla no tiene claves repetidas y los marcadores coinciden en los dos idiomas.
    #[test]
    fn table_is_consistent() {
        let src = include_str!("../i18n/en.tsv");
        let mut seen = std::collections::HashSet::new();
        let ph = |s: &str| {
            let mut v: Vec<String> = vec![];
            let mut cur = None::<String>;
            for c in s.chars() {
                match (c, cur.as_mut()) {
                    ('{', None) => cur = Some(String::new()),
                    ('}', Some(x)) => {
                        v.push(x.clone());
                        cur = None
                    }
                    (_, Some(x)) => x.push(c),
                    _ => {}
                }
            }
            v.sort();
            v
        };
        for (i, line) in src.lines().enumerate().filter(|(_, l)| !l.is_empty() && !l.starts_with('#')) {
            let (es, en) = line.split_once('\t').unwrap_or_else(|| panic!("línea {}: sin tabulador", i + 1));
            assert!(seen.insert(es), "línea {}: «{es}» repetido", i + 1);
            assert_eq!(ph(es), ph(en), "línea {}: marcadores distintos", i + 1);
            assert!(!en.trim().is_empty(), "línea {}: traducción vacía", i + 1);
        }
    }

    /// Cada `t!("…")` del código tiene su línea en la tabla (si no, saldría en español).
    #[test]
    fn every_text_in_the_code_is_translated() {
        let table: std::collections::HashSet<&str> =
            include_str!("../i18n/en.tsv").lines().filter(|l| !l.starts_with('#')).filter_map(|l| l.split_once('\t')).map(|(es, _)| es).collect();
        // a propósito iguales en los dos idiomas (prefijos que lee la consola)
        let same = ["[error] {msg}"];
        let mut missing = vec![];
        let mut stack = vec![std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().and_then(|x| x.to_str()) != Some("rs") || p.ends_with("i18n.rs") {
                    continue;
                }
                let src = std::fs::read_to_string(&p).unwrap();
                let code = src.split("#[cfg(test)]").next().unwrap_or("");
                for (i, _) in code.match_indices("t!(\"") {
                    if code[..i].ends_with(|c: char| c.is_alphanumeric() || c == '_') {
                        continue; // format!(, print!(…
                    }
                    let rest = &code[i + 4..];
                    let mut end = 0;
                    let b = rest.as_bytes();
                    while end < b.len() && b[end] != b'"' {
                        end += if b[end] == b'\\' { 2 } else { 1 };
                    }
                    let lit = &rest[..end];
                    if !table.contains(lit) && !same.contains(&lit) {
                        missing.push(format!("{}: {lit}", p.file_name().unwrap().to_string_lossy()));
                    }
                }
            }
        }
        assert!(missing.is_empty(), "textos sin traducir en i18n/en.tsv:\n{}", missing.join("\n"));
    }
}
