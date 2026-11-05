//! Errores en lenguaje de persona: qué ha pasado, qué hacer y un botón que lleva a la solución.
//! El texto técnico original se conserva para «Copiar detalles».

/// A dónde lleva el botón del aviso.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fix {
    /// Abrir Cuentas.
    Accounts,
    /// Probar la conexión (en Ajustes → Red).
    TestNetwork,
    /// Abrir Ajustes (Java).
    JavaSettings,
    /// Ver la consola con el registro completo.
    Console,
}

impl Fix {
    pub fn label(self) -> &'static str {
        match self {
            Fix::Accounts => t!("Añadir cuenta"),
            Fix::TestNetwork => t!("Probar conexión"),
            Fix::JavaSettings => t!("Ajustes de Java"),
            Fix::Console => t!("Ver consola"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Human {
    pub title: String,
    pub hint: Option<String>,
    pub fix: Option<Fix>,
}

fn h(title: &str, hint: impl Into<Option<String>>, fix: Option<Fix>) -> Human {
    Human { title: title.to_string(), hint: hint.into(), fix }
}

/// Primer host que aparece en el texto: de una URL (`https://host/…`) o tras «con» (`no se puede conectar con host`).
pub fn host_in(text: &str) -> Option<String> {
    if let Some(i) = text.find("://") {
        let rest = &text[i + 3..];
        let host: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-').collect();
        if host.contains('.') {
            return Some(host);
        }
    }
    // «… conectar con host …», «error de red con host: …»
    text.match_indices(" con ").find_map(|(i, _)| {
        let host: String = text[i + 5..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-').collect();
        (host.contains('.') && !host.starts_with('.') && !host.ends_with('.')).then_some(host)
    })
}

/// Nombre que entiende un jugador para los servidores habituales.
fn friendly_host(host: &str) -> String {
    let known = [
        ("mojang.com", t!("los servidores de Minecraft")),
        ("minecraft.net", t!("los servidores de Minecraft")),
        ("modrinth.com", t!("Modrinth")),
        ("curseforge.com", t!("CurseForge")),
        ("forgecdn.net", t!("CurseForge")),
        ("fabricmc.net", t!("Fabric")),
        ("quiltmc.org", t!("Quilt")),
        ("minecraftforge.net", t!("Forge")),
        ("neoforged.net", t!("NeoForge")),
        ("adoptium.net", t!("el servidor de Java")),
        ("github.com", t!("GitHub")),
        ("live.com", t!("Microsoft")),
        ("xboxlive.com", t!("Xbox Live")),
    ];
    known.iter().find(|(d, _)| host == *d || host.ends_with(&format!(".{d}"))).map(|(_, n)| n.to_string()).unwrap_or_else(|| host.to_string())
}

fn first_line(raw: &str) -> String {
    let l = raw.lines().next().unwrap_or("").trim();
    let l: String = l.chars().take(140).collect();
    let mut c = l.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => t!("Algo ha fallado").to_string(),
    }
}

pub fn humanize(raw: &str) -> Human {
    let s = raw.to_lowercase();
    let has = |w: &str| s.contains(w);

    if has("no hay cuenta activa") || has("no active account") {
        return h(t!("Añade una cuenta para jugar"), Some(t!("Premium con Microsoft, o no premium sólo con un nombre.").into()), Some(Fix::Accounts));
    }
    if has("no se puede conectar")
        || has("error sending request")
        || has("timed out")
        || has("tiempo de espera")
        || has("dns error")
        || has("failed to lookup")
        || has("connection refused")
        || has("error de red")
        || has("tunnel")
    {
        let what = host_in(raw).map(|h| friendly_host(&h)).unwrap_or_else(|| "internet".to_string());
        return h(
            t!("Sin conexión"),
            Some(t!("No se pudo contactar con {what}. Las instancias ya instaladas se pueden jugar igual.", what = what)),
            Some(Fix::TestNetwork),
        );
    }
    if has("no space left") || has("espacio en el disco") || has("os error 28") || has("os error 112") {
        return h(t!("No queda espacio en el disco"), Some(t!("Libera espacio y vuelve a intentarlo.").into()), None);
    }
    if has("permission denied") || has("acceso denegado") || has("os error 13") || has("os error 5)") {
        return h(t!("Brasa no tiene permiso para escribir ahí"), Some(t!("Cierra el juego si está abierto, o elige otra carpeta.").into()), None);
    }
    if has("java") && (has("no se encontr") || has("no hay ningún") || has("no hay java") || has("compatible")) {
        return h(t!("Falta un Java compatible"), Some(t!("Activa la descarga automática de Java o elige uno instalado.").into()), Some(Fix::JavaSettings));
    }
    if has("se cerró con código") || has("crash") {
        return h(t!("El juego se cerró de forma inesperada"), Some(t!("La consola tiene el diagnóstico con la causa probable.").into()), Some(Fix::Console));
    }
    if has("no incluye") {
        // «esta copia de Brasa no incluye…»: ya es una frase para el usuario
        return h(&first_line(raw), None, None);
    }
    h(&first_line(raw), None, Some(Fix::Console))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_at_startup_reads_like_a_person() {
        // el error real que veíamos en la barra roja al abrir sin internet
        let raw = "GET https://piston-meta.mojang.com/mc/game/version_manifest_v2.json: no se puede conectar con piston-meta.mojang.com (tiempo de espera agotado). Causas típicas: …";
        let x = humanize(raw);
        assert_eq!(x.title, "Sin conexión");
        assert_eq!(x.hint.as_deref(), Some("No se pudo contactar con los servidores de Minecraft. Las instancias ya instaladas se pueden jugar igual."));
        assert_eq!(x.fix, Some(Fix::TestNetwork));
    }

    #[test]
    fn known_hosts_and_unknown_ones() {
        assert_eq!(host_in("GET https://api.modrinth.com/v2/search: x").as_deref(), Some("api.modrinth.com"));
        assert_eq!(host_in("no se puede conectar con maven.fabricmc.net (tiempo…").as_deref(), Some("maven.fabricmc.net"));
        assert_eq!(host_in("sin nada"), None);
        assert!(humanize("error sending request for url (https://api.modrinth.com/v2/x)").hint.unwrap().contains("Modrinth"));
        assert!(humanize("error de red con ejemplo.org: x").hint.unwrap().contains("ejemplo.org"));
        assert!(humanize("error sending request").hint.unwrap().contains("internet"));
    }

    #[test]
    fn common_problems() {
        assert_eq!(humanize("no hay cuenta activa: añade una cuenta premium…").fix, Some(Fix::Accounts));
        assert_eq!(humanize("No space left on device (os error 28)").title, "No queda espacio en el disco");
        assert_eq!(humanize("Permission denied (os error 13)").title, "Brasa no tiene permiso para escribir ahí");
        assert_eq!(humanize("no se encontró un Java compatible (hace falta 21)").fix, Some(Fix::JavaSettings));
        assert_eq!(humanize("El juego se cerró con código 1").fix, Some(Fix::Console));
    }

    #[test]
    fn unknown_errors_keep_their_first_line_and_point_to_the_console() {
        let x = humanize("fallo raro en la línea 3\nstack...");
        assert_eq!(x.title, "Fallo raro en la línea 3");
        assert_eq!(x.fix, Some(Fix::Console));
        assert_eq!(humanize("").title, "Algo ha fallado");
        let own = humanize("esta copia de Brasa no incluye acceso a CurseForge (…); usa Modrinth");
        assert!(own.title.starts_with("Esta copia") && own.fix.is_none());
    }
}
