//! Tipografías incrustadas (IBM Plex, licencia OFL en `assets/fonts/OFL.txt`).
//!
//! - texto: Plex Sans Regular (familia `Proportional`, la de por defecto)
//! - énfasis: Plex Sans SemiBold → [`Weight::semibold`]
//! - títulos y cifras grandes: Plex Sans Condensed Bold → [`Weight::display`]
//!
//! Detrás de cada una van las fuentes de egui por defecto, que aportan emoji y símbolos.

use eframe::egui::{self, FontData, FontDefinitions, FontFamily, RichText};

pub fn semibold_family() -> FontFamily {
    FontFamily::Name("semibold".into())
}

pub fn display_family() -> FontFamily {
    FontFamily::Name("display".into())
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let fallback: Vec<String> = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();

    let faces: [(&str, &'static [u8]); 3] = [
        ("plex-regular", include_bytes!("../../../assets/fonts/IBMPlexSans-Regular.ttf")),
        ("plex-semibold", include_bytes!("../../../assets/fonts/IBMPlexSans-SemiBold.ttf")),
        ("plex-condensed-bold", include_bytes!("../../../assets/fonts/IBMPlexSansCondensed-Bold.ttf")),
    ];
    for (name, bytes) in faces {
        fonts.font_data.insert(name.to_string(), FontData::from_static(bytes));
    }

    let with_fallback = |first: &str| -> Vec<String> {
        let mut v = vec![first.to_string()];
        v.extend(fallback.iter().cloned());
        v
    };
    fonts.families.insert(FontFamily::Proportional, with_fallback("plex-regular"));
    fonts.families.insert(semibold_family(), with_fallback("plex-semibold"));
    fonts.families.insert(display_family(), with_fallback("plex-condensed-bold"));
    ctx.set_fonts(fonts);
}

/// Pesos de letra sobre `RichText` (egui sólo trae «strong», que únicamente cambia el color).
pub trait Weight {
    fn semibold(self) -> RichText;
    fn display(self) -> RichText;
}

impl Weight for RichText {
    fn semibold(self) -> RichText {
        self.family(semibold_family())
    }
    fn display(self) -> RichText {
        self.family(display_family())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Las fuentes incrustadas deben tener los caracteres que usa la interfaz en español.
    #[test]
    fn embedded_fonts_cover_spanish() {
        let ctx = egui::Context::default();
        install(&ctx);
        let _ = ctx.run(Default::default(), |ctx| {
            for fam in [FontFamily::Proportional, semibold_family(), display_family()] {
                let id = egui::FontId::new(14.0, fam.clone());
                ctx.fonts(|f| {
                    for c in "áéíóúñÁÉÍÓÚÑüÜ¿¡«»·…".chars() {
                        assert!(f.has_glyph(&id, c), "falta «{c}» en {fam:?}");
                    }
                });
            }
        });
    }

    /// Ningún símbolo de la interfaz puede salir como un cuadrado vacío (glifo que no existe en
    /// las fuentes): se revisa cada carácter fuera del latín de los textos de `src/gui`.
    #[test]
    fn every_symbol_in_the_ui_has_a_glyph() {
        let ctx = egui::Context::default();
        install(&ctx);
        let mut files = vec![];
        let mut stack = vec![std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/gui")];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().and_then(|x| x.to_str()) == Some("rs") {
                    files.push(p);
                }
            }
        }
        let mut missing = std::collections::BTreeMap::<char, String>::new();
        let _ = ctx.run(Default::default(), |ctx| {
            ctx.fonts(|f| {
                for path in &files {
                    let src = std::fs::read_to_string(path).unwrap();
                    for (n, line) in src.lines().enumerate() {
                        // sólo lo que va entre comillas (textos), no los comentarios
                        if line.trim_start().starts_with("//") {
                            continue;
                        }
                        let mut inside = false;
                        for c in line.chars() {
                            if c == '"' {
                                inside = !inside;
                            } else if inside && (c as u32) >= 0x2000 && !c.is_whitespace() {
                                for fam in [FontFamily::Proportional, semibold_family()] {
                                    if !f.has_glyph(&egui::FontId::new(14.0, fam), c) {
                                        missing.entry(c).or_insert_with(|| format!("{}:{}", path.file_name().unwrap().to_string_lossy(), n + 1));
                                    }
                                }
                            }
                        }
                    }
                }
            });
        });
        assert!(missing.is_empty(), "símbolos sin glifo (saldrían como un cuadrado vacío): {missing:?}");
    }
}
