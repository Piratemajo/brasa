//! `server.properties` sin perder nada: se cambian las claves que toca Brasa y el resto (comentarios,
//! orden, claves desconocidas) se queda como estaba.

use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Properties {
    lines: Vec<String>,
}

impl Properties {
    pub fn parse(text: &str) -> Properties {
        Properties { lines: text.lines().map(str::to_string).collect() }
    }
    pub fn load(path: &Path) -> Properties {
        Properties::parse(&std::fs::read_to_string(path).unwrap_or_default())
    }
    fn find(&self, key: &str) -> Option<usize> {
        self.lines.iter().position(|l| {
            let t = l.trim_start();
            !t.starts_with('#') && t.split_once('=').map(|(k, _)| k.trim() == key).unwrap_or(false)
        })
    }
    pub fn get(&self, key: &str) -> Option<&str> {
        self.find(key).and_then(|i| self.lines[i].split_once('=')).map(|(_, v)| v.trim())
    }
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.get(key).map(|v| v.eq_ignore_ascii_case("true"))
    }
    pub fn set(&mut self, key: &str, value: &str) {
        // los saltos de línea romperían el fichero
        let value: String = value.chars().filter(|c| *c != '\n' && *c != '\r').collect();
        match self.find(key) {
            Some(i) => self.lines[i] = format!("{key}={value}"),
            None => self.lines.push(format!("{key}={value}")),
        }
    }
    /// Sólo si no existe (para no pisar lo que haya cambiado el usuario).
    pub fn set_default(&mut self, key: &str, value: &str) {
        if self.find(key).is_none() {
            self.set(key, value);
        }
    }
    pub fn to_text(&self) -> String {
        let mut s = self.lines.join("\n");
        s.push('\n');
        s
    }
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        crate::paths::write_atomic(path, self.to_text())
    }
}

/// Lo que se puede tocar desde los ajustes del servidor en Brasa.
#[derive(Debug, Clone, PartialEq)]
pub struct Editable {
    pub motd: String,
    pub max_players: u32,
    pub gamemode: String,
    pub difficulty: String,
    pub pvp: bool,
    pub hardcore: bool,
    pub allow_flight: bool,
    pub view_distance: u32,
    pub spawn_protection: u32,
    pub seed: String,
}

impl Default for Editable {
    fn default() -> Self {
        Editable {
            motd: String::new(),
            max_players: 20,
            gamemode: "survival".into(),
            difficulty: "easy".into(),
            pvp: true,
            hardcore: false,
            allow_flight: false,
            view_distance: 10,
            spawn_protection: 16,
            seed: String::new(),
        }
    }
}

impl Editable {
    pub fn read(p: &Properties) -> Editable {
        let d = Editable::default();
        let num = |k: &str, def: u32| p.get(k).and_then(|v| v.parse().ok()).unwrap_or(def);
        Editable {
            motd: p.get("motd").map(str::to_string).unwrap_or(d.motd),
            max_players: num("max-players", d.max_players),
            gamemode: p.get("gamemode").map(str::to_string).unwrap_or(d.gamemode),
            difficulty: p.get("difficulty").map(str::to_string).unwrap_or(d.difficulty),
            pvp: p.get_bool("pvp").unwrap_or(d.pvp),
            hardcore: p.get_bool("hardcore").unwrap_or(d.hardcore),
            allow_flight: p.get_bool("allow-flight").unwrap_or(d.allow_flight),
            view_distance: num("view-distance", d.view_distance),
            spawn_protection: num("spawn-protection", d.spawn_protection),
            seed: p.get("level-seed").map(str::to_string).unwrap_or_default(),
        }
    }
    pub fn write(&self, p: &mut Properties) {
        p.set("motd", &self.motd);
        p.set("max-players", &self.max_players.clamp(1, 1000).to_string());
        p.set("gamemode", &self.gamemode);
        p.set("difficulty", &self.difficulty);
        p.set("pvp", &self.pvp.to_string());
        p.set("hardcore", &self.hardcore.to_string());
        p.set("allow-flight", &self.allow_flight.to_string());
        p.set("view-distance", &self.view_distance.clamp(2, 32).to_string());
        p.set("spawn-protection", &self.spawn_protection.min(64).to_string());
        p.set("level-seed", self.seed.trim());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_comments_order_and_unknown_keys() {
        let src = "#Minecraft server properties\n#Mon Oct 26\nmotd=Hola\nlevel-seed=\nmi-plugin.clave=1\nserver-port=25565\n";
        let mut p = Properties::parse(src);
        p.set("server-port", "25570");
        p.set("white-list", "true");
        p.set_default("motd", "otro");
        let out = p.to_text();
        assert!(out.starts_with("#Minecraft server properties\n#Mon Oct 26\nmotd=Hola\n"));
        assert!(out.contains("mi-plugin.clave=1\nserver-port=25570\nwhite-list=true\n"));
        assert_eq!(p.get("level-seed"), Some(""));
        p.set("motd", "dos\nlíneas");
        assert_eq!(p.get("motd"), Some("doslíneas"), "sin saltos de línea");
        assert_eq!(p.get("#Mon Oct 26"), None);
    }

    #[test]
    fn editable_round_trip() {
        let mut p = Properties::parse("pvp=false\ndifficulty=hard\nmax-players=8\n");
        let mut e = Editable::read(&p);
        assert!(!e.pvp && e.difficulty == "hard" && e.max_players == 8 && e.view_distance == 10);
        e.max_players = 5000;
        e.seed = " 1234 ".into();
        e.write(&mut p);
        assert_eq!(p.get("max-players"), Some("1000"));
        assert_eq!(p.get("level-seed"), Some("1234"));
        assert_eq!(Editable::read(&p).max_players, 1000);
    }
}
