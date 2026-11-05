//! Formato de números, tamaños y fechas para mostrar.

/// Recorta un texto largo con "…" (por caracteres, seguro con UTF-8).
pub fn elide(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{t}…")
    }
}

pub fn initial(name: &str) -> String {
    name.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into())
}

pub fn fmt_count(n: u64) -> String {
    match n {
        0..=999 => n.to_string(),
        1_000..=999_999 => format!("{:.1}k", n as f64 / 1e3),
        _ => format!("{:.1}M", n as f64 / 1e6),
    }
}

pub fn fmt_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        t!("{:.1} MB", bytes as f64 / 1048576.0)
    } else {
        t!("{} KB", bytes / 1024)
    }
}

/// «hace 5 min», «hace 3 h», «hace 2 días»… a partir de un instante Unix.
pub fn ago(ts: u64) -> String {
    ago_from(crate::config::now_secs(), ts)
}

pub fn ago_from(now: u64, ts: u64) -> String {
    if ts == 0 {
        return "—".into();
    }
    let d = now.saturating_sub(ts);
    match d {
        0..=59 => t!("hace un momento").into(),
        60..=3599 => t!("hace {} min", d / 60),
        3600..=86_399 => t!("hace {} h", d / 3600),
        _ => {
            let days = d / 86_400;
            if days == 1 {
                t!("hace 1 día").into()
            } else {
                t!("hace {days} días", days = days)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ago_from;

    #[test]
    fn relative_time() {
        assert_eq!(ago_from(1000, 0), "—");
        assert_eq!(ago_from(1000, 990), "hace un momento");
        assert_eq!(ago_from(1000, 700), "hace 5 min");
        assert_eq!(ago_from(100_000, 100_000 - 7200), "hace 2 h");
        assert_eq!(ago_from(1_000_000, 1_000_000 - 86_400), "hace 1 día");
        assert_eq!(ago_from(1_000_000, 1_000_000 - 3 * 86_400), "hace 3 días");
        // Un reloj adelantado no entra en pánico.
        assert_eq!(ago_from(10, 500), "hace un momento");
    }
}
