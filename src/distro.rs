//! Datos de quien distribuye Brasa: se incrustan en el binario al compilar y el usuario no los ve.
//!
//! | Variable al compilar        | Para qué                                            | ¿Secreto?            |
//! |-----------------------------|-----------------------------------------------------|----------------------|
//! | `BRASA_MSA_CLIENT_ID`       | Inicio de sesión con Microsoft (app de Azure)        | No (cliente público) |
//! | `BRASA_CURSEFORGE_KEY`      | Buscar e instalar desde CurseForge                   | Sí, pero va en el binario |
//! | `BRASA_DISCORD_CLIENT_ID`   | «Jugando a Brasa Launcher» en Discord                | No                   |
//! | `BRASA_UPDATE_REPO`         | Repo `usuario/repo` de las actualizaciones           | No                   |
//!
//! `BRASA_UPDATE_REPO` cae en `GITHUB_REPOSITORY`, que GitHub Actions define solo: las builds del
//! CI saben de dónde actualizarse sin configurar nada. Una variable vacía cuenta como ausente.
//!
//! Para desarrollar sin recompilar, las mismas variables en el entorno al ejecutar tienen
//! prioridad (y `brasa config` permite fijarlas en `settings.toml`): ver [`crate::config::Settings::load`].

const fn or_empty(v: Option<&'static str>) -> &'static str {
    match v {
        Some(s) => s,
        None => "",
    }
}

pub const MSA_CLIENT_ID: &str = or_empty(option_env!("BRASA_MSA_CLIENT_ID"));
pub const CURSEFORGE_KEY: &str = or_empty(option_env!("BRASA_CURSEFORGE_KEY"));
pub const DISCORD_CLIENT_ID: &str = or_empty(option_env!("BRASA_DISCORD_CLIENT_ID"));
pub const UPDATE_REPO: &str = match option_env!("BRASA_UPDATE_REPO") {
    Some(r) if !r.is_empty() => r,
    _ => or_empty(option_env!("GITHUB_REPOSITORY")),
};

/// ¿Parece una credencial (token de GitHub, clave privada…)? Esas nunca deben acabar en un campo
/// que no es secreto ni enseñarse en pantalla.
pub fn looks_like_secret(v: &str) -> bool {
    let v = v.trim();
    const PREFIXES: [&str; 7] = ["github_pat_", "ghp_", "gho_", "ghu_", "ghs_", "ghr_", "-----BEGIN"];
    PREFIXES.iter().any(|p| v.starts_with(p)) || (v.len() >= 30 && !v.contains('/') && !v.contains(' ') && !v.chars().all(|c| c.is_ascii_digit()))
}

/// Lleva a la forma `usuario/repo` lo que se suele pegar por error: la URL entera, `github.com/…`,
/// un `.git` o una barra al final. Lo que no se pueda arreglar se devuelve tal cual.
pub fn normalize_repo(r: &str) -> String {
    if looks_like_secret(r) {
        // un token pegado donde iba el repo: se descarta (y no se enseña)
        return String::new();
    }
    let mut s = r.trim();
    for prefix in ["https://", "http://", "www.", "github.com/"] {
        s = s.strip_prefix(prefix).unwrap_or(s);
    }
    let s = s.trim_end_matches('/');
    let s = s.strip_suffix(".git").unwrap_or(s);
    // .../usuario/repo/releases → usuario/repo
    let parts: Vec<&str> = s.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() > 2 && matches!(parts[2], "releases" | "tree" | "blob" | "actions") {
        return format!("{}/{}", parts[0], parts[1]);
    }
    s.to_string()
}

/// `usuario/repo` con forma válida.
pub fn valid_repo(r: &str) -> bool {
    let r = r.trim();
    let mut parts = r.split('/');
    let ok = |s: Option<&str>| s.is_some_and(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)));
    ok(parts.next()) && ok(parts.next()) && parts.next().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_shape() {
        assert!(valid_repo("javier/brasa"));
        assert!(valid_repo(" org-1/brasa.launcher "));
        for bad in ["", "brasa", "a/b/c", "/brasa", "a/", "a b/c", "https://github.com/a/b"] {
            assert!(!valid_repo(bad), "{bad:?}");
        }
    }

    #[test]
    fn pasted_urls_are_fixed() {
        for ok in [
            "https://github.com/Piratemajo/brasa-releases",
            "github.com/Piratemajo/brasa-releases/",
            "Piratemajo/brasa-releases.git",
            " https://github.com/Piratemajo/brasa-releases/releases/latest ",
        ] {
            assert_eq!(normalize_repo(ok), "Piratemajo/brasa-releases", "{ok:?}");
        }
        assert_eq!(normalize_repo("javier/brasa"), "javier/brasa");
        assert!(!valid_repo(&normalize_repo("no es un repo")));
    }

    #[test]
    fn tokens_pasted_as_repo_are_dropped() {
        for t in ["github_pat_11ABCDEFG0123456789_abcdefghijklmnopqrstuvwxyz", "ghp_abcdefghijklmnopqrstuvwxyz0123456789", "zZ9xY8wV7uT6sR5qP4oN3mL2kJ1iH0gF"] {
            assert!(looks_like_secret(t), "{t}");
            assert_eq!(normalize_repo(t), "");
        }
        for ok in ["Piratemajo/brasa-releases", "javier/brasa", "1234567890123456789"] {
            assert!(!looks_like_secret(ok), "{ok}");
        }
    }
}
