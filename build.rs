//! - En Windows, mete el icono de Brasa en el .exe (explorador, accesos directos, barra de tareas).
//! - Convierte `i18n/en.tsv` en la macro `t!` (ver `src/i18n.rs`).
use std::fmt::Write as _;

fn main() {
    println!("cargo:rerun-if-changed=assets/icon/brasa.ico");
    println!("cargo:rerun-if-changed=i18n/en.tsv");
    i18n();
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon/brasa.ico");
        res.set("ProductName", "Brasa Launcher");
        res.set("FileDescription", "Brasa Launcher");
        if let Err(e) = res.compile() {
            // sin icono, pero que compile igual
            println!("cargo:warning=no se pudo incrustar el icono: {e}");
        }
    }
}

/// Cada línea `español<TAB>inglés` se vuelve una rama de `t!` que casa con ese literal exacto.
/// Los textos con `{…}` dan una rama con `format!` en los dos idiomas (los argumentos se pasan con
/// nombre: `t!("{n} líneas", n = n)`), así el compilador sigue comprobando los marcadores.
fn i18n() {
    let src = std::fs::read_to_string("i18n/en.tsv").expect("i18n/en.tsv");
    let mut arms = String::new();
    let mut pairs = String::new();
    for (n, line) in src.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let (es, en) = line.split_once('\t').unwrap_or_else(|| panic!("i18n/en.tsv:{}: falta el tabulador", n + 1));
        if es == en {
            continue; // igual en los dos idiomas: basta la rama por defecto
        }
        for (what, s) in [("español", es), ("inglés", en)] {
            assert!(!s.contains('"') || s.contains("\\\""), "i18n/en.tsv:{}: comillas sin escapar en {what}", n + 1);
        }
        if es.contains('{') {
            writeln!(
                arms,
                "    (\"{es}\" $(, $($a:tt)*)?) => {{ if $crate::i18n::en() {{ format!(\"{en}\" $(, $($a)*)?) }} else {{ format!(\"{es}\" $(, $($a)*)?) }} }};"
            )
            .unwrap();
        } else {
            writeln!(arms, "    (\"{es}\") => {{ $crate::i18n::pick(\"{es}\", \"{en}\") }};").unwrap();
            writeln!(pairs, "    (\"{es}\", \"{en}\"),").unwrap();
        }
    }
    let out = format!(
        "/// Texto de la interfaz en el idioma elegido. Generado desde `i18n/en.tsv` por `build.rs`.\n\
         macro_rules! t {{\n{arms}    ($s:literal) => {{ $s }};\n    ($s:literal, $($a:tt)*) => {{ format!($s, $($a)*) }};\n}}\n\n\
         /// Pares sin marcadores, para traducir textos que llegan en tiempo de ejecución ([`tr`]).\n\
         pub static PAIRS: &[(&str, &str)] = &[\n{pairs}];\n"
    );
    let dst = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("i18n.rs");
    std::fs::write(dst, out).unwrap();
}
