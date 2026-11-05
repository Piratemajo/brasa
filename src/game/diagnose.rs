//! Diagnóstico de cierres inesperados del juego.
//!
//! Recibe las últimas líneas de la consola y (si existe) el texto del crash report, y devuelve
//! las causas más probables con una sugerencia concreta. Es una función pura: no toca disco ni red.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnosis {
    pub title: String,
    pub fix: String,
}

fn d(title: &str, fix: impl Into<String>) -> Diagnosis {
    Diagnosis { title: title.to_string(), fix: fix.into() }
}

/// Versión de Java necesaria a partir del «class file version» que reporta la JVM
/// (52 → Java 8, 61 → 17, 65 → 21, 69 → 25...).
pub fn java_for_class_version(major: u32) -> Option<u32> {
    (major >= 45).then(|| major - 44)
}

/// Busca `class file version NN` o `major version NN` y devuelve NN.
fn class_version(text: &str) -> Option<u32> {
    for key in ["class file version ", "major version "] {
        if let Some(i) = text.find(key) {
            let digits: String = text[i + key.len()..].chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = digits.parse() {
                return Some(n);
            }
        }
    }
    None
}

/// Primer `mod` que menciona un error de Mixin/dependencia, para dar una pista concreta.
fn first_line_with<'a>(text: &'a str, needle: &str) -> Option<&'a str> {
    text.lines().find(|l| l.contains(needle)).map(|l| l.trim())
}

pub fn analyze(log: &str, crash_report: Option<&str>) -> Vec<Diagnosis> {
    let mut all = String::with_capacity(log.len() + crash_report.map_or(0, |c| c.len()) + 1);
    all.push_str(log);
    if let Some(c) = crash_report {
        all.push('\n');
        all.push_str(c);
    }
    let mut out: Vec<Diagnosis> = vec![];

    // Java demasiado antiguo para el juego o para un mod.
    if all.contains("UnsupportedClassVersionError") || all.contains("Unsupported class file major version") {
        let need = class_version(&all).and_then(java_for_class_version);
        let fix = match need {
            Some(n) => {
                t!("Hace falta Java {n} o superior. En Ajustes → Java pulsa «Detectar» y elige uno compatible, o activa la descarga automática de Java.", n = n)
            }
            None => t!("Un Java demasiado antiguo para esta versión o mod. En Ajustes → Java elige uno más nuevo o activa la descarga automática.").to_string(),
        };
        out.push(d(t!("Versión de Java incompatible"), fix));
    }

    // Memoria.
    if all.contains("OutOfMemoryError") {
        out.push(d(
            t!("El juego se quedó sin memoria"),
            t!("Sube la memoria de la instancia (Ajustes de la instancia → Memoria). Con muchos mods suelen hacer falta 6-8 GB; no pases de lo que tengas libre."),
        ));
    }
    if all.contains("Could not reserve enough space") || all.contains("Too small initial heap") || all.contains("Invalid maximum heap size") {
        out.push(d(
            t!("La JVM no pudo reservar la memoria pedida"),
            t!("Baja la memoria de la instancia. Si pusiste más de ~3 GB con un Java de 32 bits, instala un Java de 64 bits."),
        ));
    }

    // Fabric / Quilt: dependencias.
    if all.contains("Incompatible mod set") || all.contains("Mod resolution failed") {
        let hint = first_line_with(&all, "requires").or_else(|| first_line_with(&all, "depends on")).unwrap_or("");
        let mut fix = t!("Falta una dependencia o hay una versión incompatible. Usa «Buscar actualizaciones» en la pestaña Mods; si sigue igual, desactiva el último mod que añadiste.").to_string();
        if !hint.is_empty() {
            fix.push_str(&t!(" Pista del log: {}", hint.chars().take(200).collect::<String>()));
        }
        out.push(d(t!("Mods incompatibles o con dependencias sin cumplir"), fix));
    }

    // Forge / NeoForge: dependencias.
    if all.contains("Missing or unsupported mandatory dependencies") {
        out.push(d(
            t!("Dependencia obligatoria ausente (Forge/NeoForge)"),
            t!("Instala la dependencia que indica el log (busca «Mod ID») con la misma versión de Minecraft."),
        ));
    }

    // Mixins rotos: casi siempre un mod para otra versión.
    if all.contains("Mixin apply failed") || all.contains("MixinTransformerError") || all.contains("InvalidMixinException") {
        out.push(d(
            t!("Un mod falla al modificar el juego (Mixin)"),
            t!("Suele ser un mod hecho para otra versión de Minecraft o de otro mod. Actualiza los mods o quita el que aparece en el log junto a «Mixin apply failed»."),
        ));
    }

    // Mods para otro loader/versión.
    if all.contains("NoSuchMethodError") || all.contains("NoSuchFieldError") || all.contains("NoClassDefFoundError") || all.contains("ClassNotFoundException") {
        out.push(d(
            t!("Falta una clase o método (versión de mod incorrecta)"),
            t!("Casi siempre es un mod o librería de una versión distinta. Comprueba que todos los mods son de la versión de Minecraft y del loader de la instancia."),
        ));
    }

    // Gráficos.
    if all.contains("Pixel format not accelerated")
        || all.contains("GLFW error 65542")
        || all.contains("WGL: The driver does not appear to support OpenGL")
        || all.contains("OpenGL 3.2 not supported")
        || all.contains("GLXBadFBConfig")
    {
        out.push(d(
            t!("Problema con el controlador gráfico (OpenGL)"),
            t!("Actualiza el driver de tu tarjeta gráfica. En portátiles con dos GPU, fuerza la dedicada para el Java que usa Brasa."),
        ));
    }

    // Archivos corruptos.
    if all.contains("ZipException") || all.contains("zip END header not found") || all.contains("Invalid or corrupt jarfile") {
        out.push(d(t!("Un archivo del juego está corrupto"), t!("Pulsa «Reparar» en la instancia: re-verifica todo por SHA-1 y vuelve a bajar solo lo roto.")));
    }

    // Sesión.
    if all.contains("Invalid session") || all.contains("Failed to verify authentication") {
        out.push(d(
            t!("Sesión no válida"),
            t!("Cierra y vuelve a añadir la cuenta en la pestaña Cuentas. Si juegas sin premium, el servidor debe tener online-mode=false."),
        ));
    }

    // Puerto ocupado / servidor integrado.
    if all.contains("Address already in use") {
        out.push(d(t!("Puerto ocupado"), t!("Otro programa (o otra copia del juego) usa el puerto. Ciérralo e inténtalo de nuevo.")));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_versions() {
        assert_eq!(java_for_class_version(52), Some(8));
        assert_eq!(java_for_class_version(61), Some(17));
        assert_eq!(java_for_class_version(65), Some(21));
        assert_eq!(java_for_class_version(69), Some(25));
        assert_eq!(java_for_class_version(10), None);
    }

    #[test]
    fn detects_wrong_java() {
        let log = "Exception in thread \"main\" java.lang.UnsupportedClassVersionError: net/minecraft/client/main/Main has been compiled by a more recent version of the Java Runtime (class file version 65.0), this version of the Java Runtime only recognizes class file versions up to 61.0";
        let r = analyze(log, None);
        assert_eq!(r.len(), 1);
        assert!(r[0].fix.contains("Java 21"), "{:?}", r[0]);
    }

    #[test]
    fn detects_oom_and_mixin_together() {
        let log = "java.lang.OutOfMemoryError: Java heap space\nMixin apply failed sodium.mixins.json";
        let r = analyze(log, None);
        let titles: Vec<_> = r.iter().map(|x| x.title.as_str()).collect();
        assert!(titles.iter().any(|t| t.contains("memoria")));
        assert!(titles.iter().any(|t| t.contains("Mixin")));
    }

    #[test]
    fn uses_crash_report_too() {
        let r = analyze("todo normal", Some("Caused by: java.lang.OutOfMemoryError"));
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn fabric_hint_is_included() {
        let log = "Incompatible mod set!\n - Mod 'Sodium' (sodium) 0.5 requires any version of fabric-api, which is missing!";
        let r = analyze(log, None);
        assert!(r[0].fix.contains("fabric-api"));
    }

    #[test]
    fn clean_log_gives_nothing() {
        assert!(analyze("[Render thread/INFO]: Stopping!", None).is_empty());
    }
}
