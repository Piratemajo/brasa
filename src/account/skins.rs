//! Skins: subir/quitar skin en cuentas premium y skin local para cuentas offline
//! (resource pack generado que sustituye las skins por defecto; sólo la ves tú).

use crate::account::auth::Account;
use crate::config::Instance;
use crate::paths::Paths;
use anyhow::{bail, Context, Result};
use std::io::Write;
use std::path::Path;

const PACK_NAME: &str = "brasa-skin.zip";
const DEFAULT_SKINS: [&str; 9] = ["alex", "ari", "efe", "kai", "makena", "noor", "steve", "sunny", "zuri"];

/// Comprueba que es un PNG de skin válido (64×64 o 64×32) y devuelve sus bytes.
pub fn read_skin(path: &Path) -> Result<Vec<u8>> {
    let b = std::fs::read(path).with_context(|| format!("no se puede leer {}", path.display()))?;
    if b.len() < 24 || &b[..8] != b"\x89PNG\r\n\x1a\n" || &b[12..16] != b"IHDR" {
        bail!("el fichero no es un PNG válido");
    }
    let w = u32::from_be_bytes([b[16], b[17], b[18], b[19]]);
    let h = u32::from_be_bytes([b[20], b[21], b[22], b[23]]);
    if !(w == 64 && (h == 64 || h == 32)) {
        bail!("una skin debe medir 64×64 (o 64×32 las antiguas); esta mide {w}×{h}");
    }
    Ok(b)
}

// ------------------------------------------------------------------ premium

pub async fn upload_premium(client: &reqwest::Client, acc: &Account, file: &Path, slim: bool) -> Result<()> {
    if !acc.is_premium() {
        bail!("subir skins oficiales sólo funciona con cuentas premium");
    }
    let bytes = read_skin(file)?;
    let part = reqwest::multipart::Part::bytes(bytes).file_name("skin.png").mime_str("image/png")?;
    let form = reqwest::multipart::Form::new().text("variant", if slim { "slim" } else { "classic" }).part("file", part);
    let r = client.post("https://api.minecraftservices.com/minecraft/profile/skins").bearer_auth(&acc.access_token).multipart(form).send().await?;
    if !r.status().is_success() {
        bail!("Mojang rechazó la skin (HTTP {}): {}", r.status(), r.text().await.unwrap_or_default());
    }
    Ok(())
}

pub async fn reset_premium(client: &reqwest::Client, acc: &Account) -> Result<()> {
    if !acc.is_premium() {
        bail!("sólo para cuentas premium");
    }
    let r = client.delete("https://api.minecraftservices.com/minecraft/profile/skins/active").bearer_auth(&acc.access_token).send().await?;
    if !r.status().is_success() {
        bail!("no se pudo quitar la skin (HTTP {})", r.status());
    }
    Ok(())
}

/// Página web donde cambiar la skin de una cuenta Yggdrasil.
pub fn yggdrasil_skin_page(api: &str) -> String {
    if api.contains("ely.by") {
        "https://ely.by/skins".into()
    } else if api.contains("littleskin") {
        "https://littleskin.cn/user/closet".into()
    } else {
        reqwest::Url::parse(api).ok().map(|u| format!("{}://{}/", u.scheme(), u.host_str().unwrap_or_default())).unwrap_or_else(|| api.to_string())
    }
}

// ------------------------------------------------------------------ offline (resource pack)

fn parse_mc(mc: &str) -> (u32, u32, u32) {
    let mut it = mc.split(|c: char| !c.is_ascii_digit()).filter(|s| !s.is_empty()).map(|s| s.parse::<u32>().unwrap_or(0));
    (it.next().unwrap_or(0), it.next().unwrap_or(0), it.next().unwrap_or(0))
}

/// pack_format aproximado por versión (las versiones nuevas usan rango amplio igualmente).
pub fn pack_format(mc: &str) -> u32 {
    let (a, b, c) = parse_mc(mc);
    if a != 1 {
        return 80; // versiones con numeración por año (26.x...)
    }
    match (b, c) {
        (0..=8, _) => 1,
        (9..=10, _) => 2,
        (11..=12, _) => 3,
        (13..=14, _) => 4,
        (15, _) | (16, 0..=1) => 5,
        (16, _) => 6,
        (17, _) => 7,
        (18, _) => 8,
        (19, 0..=2) => 9,
        (19, 3) => 12,
        (19, _) => 13,
        (20, 0..=1) => 15,
        (20, 2) => 18,
        (20, 3..=4) => 22,
        (20, _) => 32,
        (21, 0..=1) => 34,
        (21, 2..=3) => 42,
        (21, 4) => 46,
        (21, 5) => 55,
        (21, 6) => 63,
        (21, 7..=8) => 64,
        _ => 80,
    }
}

fn uses_file_prefix(mc: &str) -> bool {
    let (a, b, _) = parse_mc(mc);
    a != 1 || b >= 13
}

/// Genera el resource pack con la skin.
pub fn build_pack(skin: &[u8], mc: &str, out: &Path) -> Result<()> {
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let f = std::fs::File::create(out)?;
    let mut z = zip::ZipWriter::new(f);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let fmt = pack_format(mc);
    let meta = serde_json::json!({
        "pack": {
            "pack_format": fmt,
            "supported_formats": [1, 999],
            "min_format": 1,
            "max_format": 999,
            "description": "Skin local de Brasa Launcher"
        }
    });
    z.start_file("pack.mcmeta", opts)?;
    z.write_all(serde_json::to_string_pretty(&meta)?.as_bytes())?;
    z.start_file("pack.png", opts)?;
    z.write_all(skin)?;
    // < 1.19.3
    for name in ["steve", "alex"] {
        z.start_file(format!("assets/minecraft/textures/entity/{name}.png"), opts)?;
        z.write_all(skin)?;
    }
    // >= 1.19.3
    for model in ["wide", "slim"] {
        for name in DEFAULT_SKINS {
            z.start_file(format!("assets/minecraft/textures/entity/player/{model}/{name}.png"), opts)?;
            z.write_all(skin)?;
        }
    }
    z.finish()?;
    Ok(())
}

/// Activa el pack en options.txt (o crea la línea si no existe).
fn enable_in_options(game_dir: &Path, mc: &str) -> Result<()> {
    let entry = if uses_file_prefix(mc) { format!("file/{PACK_NAME}") } else { PACK_NAME.to_string() };
    let path = game_dir.join("options.txt");
    let txt = std::fs::read_to_string(&path).unwrap_or_default();
    let mut found = false;
    let mut lines: Vec<String> = txt
        .lines()
        .map(|l| {
            if let Some(v) = l.strip_prefix("resourcePacks:") {
                found = true;
                let mut list: Vec<String> = serde_json::from_str(v).unwrap_or_default();
                if !list.iter().any(|x| x == &entry) {
                    list.push(entry.clone());
                }
                format!("resourcePacks:{}", serde_json::to_string(&list).unwrap_or_default())
            } else {
                l.to_string()
            }
        })
        .collect();
    if !found {
        let list = if uses_file_prefix(mc) { vec!["vanilla".to_string(), entry] } else { vec![entry] };
        lines.push(format!("resourcePacks:{}", serde_json::to_string(&list)?));
    }
    std::fs::write(path, lines.join("\n") + "\n")?;
    Ok(())
}

/// Antes de lanzar: pone o quita el pack según la cuenta.
pub fn apply_offline_skin(p: &Paths, inst: &Instance, acc: &Account) -> Result<bool> {
    let game = inst.game_dir(p);
    let pack = game.join("resourcepacks").join(PACK_NAME);
    match (&acc.kind, &acc.skin_path) {
        (crate::account::auth::AccountKind::Offline, Some(skin)) => {
            let bytes = read_skin(Path::new(skin))?;
            build_pack(&bytes, &inst.mc_version, &pack)?;
            enable_in_options(&game, &inst.mc_version)?;
            Ok(true)
        }
        _ => {
            let _ = std::fs::remove_file(&pack);
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn formats() {
        assert_eq!(pack_format("1.8.9"), 1);
        assert_eq!(pack_format("1.12.2"), 3);
        assert_eq!(pack_format("1.16.5"), 6);
        assert_eq!(pack_format("1.20.1"), 15);
        assert_eq!(pack_format("1.21.1"), 34);
        assert!(uses_file_prefix("1.21.1"));
        assert!(!uses_file_prefix("1.12.2"));
        assert!(uses_file_prefix("26.1"));
    }
}
