//! Estados de ejemplo para revisar la interfaz sin lanzar Minecraft (sólo en compilaciones de
//! desarrollo). Con `BRASA_UI_DEMO=1`, la primera instancia aparece preparándose, la segunda en
//! juego y la consola con una partida que termina en error.

use super::model::Model;

pub fn apply(model: &Model) {
    use super::model::Launch;
    if std::env::var("BRASA_UI_DEMO").as_deref() != Ok("1") {
        return;
    }
    let mut order: Vec<&crate::config::Instance> = model.instances.iter().collect();
    order.sort_by_key(|i| std::cmp::Reverse(i.last_played));
    let mut s = model.session.lock().unwrap();
    if s.latest_release.is_empty() {
        s.latest_release = "1.21.4".into();
    }
    if let Some(a) = order.first() {
        s.playing.insert(a.id.clone());
        s.launches.insert(a.id.clone(), Launch::Preparing { stage: "Descargando 3412 ficheros de 1.20.1".into(), progress: Some((1432, 3412)) });
    }
    if let Some(b) = order.get(1) {
        s.playing.insert(b.id.clone());
        s.launches.insert(b.id.clone(), Launch::Running { since: std::time::Instant::now() - std::time::Duration::from_secs(12 * 60 + 20) });
    }
    // una partida que acaba mal, para ver la consola
    let demo = [
        "==> Preparando Create 1.20",
        "[brasa] Memoria: 6144 MB (automática: 16 GB de RAM, 41 mods)",
        "==> Jugando: Create 1.20",
        "[25oct.2026 14:32:01.123] [main/INFO] [cpw.mods.modlauncher.Launcher/MODLAUNCHER]: ModLauncher running: args [--version, 1.20.1]",
        "[25oct.2026 14:32:01.410] [main/INFO] [net.minecraftforge.fml.loading.ModDiscoverer/SCAN]: Found 41 mod files",
        "[25oct.2026 14:32:02.002] [main/WARN] [mixin/]: Reference map 'embeddium-refmap.json' could not be read",
        "[14:32:05] [Render thread/INFO]: Setting user: Steve",
        "[14:32:06] [Render thread/INFO]: Backend library: LWJGL version 3.3.1",
        "[14:32:09] [Render thread/WARN]: Missing sound for event: create:fwoomp",
        "[14:32:11] [Worker-Main-3/INFO]: Loaded 7 recipes",
        "[14:33:40] [Render thread/ERROR]: Unreported exception thrown!",
        "java.lang.OutOfMemoryError: Java heap space",
        "\tat java.base/java.util.Arrays.copyOf(Arrays.java:3537)",
        "\tat com.simibubi.create.content.kinetics.KineticNetwork.update(KineticNetwork.java:212)",
        "\tat net.minecraft.client.Minecraft.runTick(Minecraft.java:1205)",
        "\tat net.minecraft.client.main.Main.main(Main.java:218)",
        "==> El juego se cerró con código 1",
        "[brasa] Crash report: /tmp/crash-2026-10-25_14.33.41-client.txt",
        "[brasa] Diagnóstico: El juego se quedó sin memoria → Sube la memoria de la instancia en Ajustes o quita mods pesados",
    ];
    s.logs.extend(demo.iter().map(|l| l.to_string()));
    s.error = Some("GET https://api.modrinth.com/v2/search: error sending request for url (https://api.modrinth.com/v2/search)".into());
}
