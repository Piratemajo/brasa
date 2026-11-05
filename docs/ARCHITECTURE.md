# Architecture

Brasa is a single binary crate. The same code drives the GUI and the CLI: both call the operations
in `actions.rs` and the domain modules below it.

## Source layout

```text
src/
  main.rs          CLI (clap) and entry point; no arguments starts the GUI
  actions.rs       Operations shared by CLI and GUI (prepare, play, install, repair)
  config.rs        Settings and instances (TOML, written atomically)
  paths.rs         Data directory layout and Maven coordinates
  net.rs           Parallel downloads: SHA-1, retries, mirrors, IPv4 fallback, atomic writes
  progress.rs      Progress and log reporting for long-running tasks
  i18n.rs          `t!` macro and runtime translation (table generated from i18n/en.tsv)
  distro.rs        Values embedded at build time (Microsoft, CurseForge, Discord, update repo)
  updater.rs       Self-update from GitHub Releases, verified by SHA-256
  discord.rs       Discord Rich Presence over the local IPC socket
  report.rs        "Report a problem": redacted log upload

  game/            Running the game
    mojang.rs        Version manifest, version JSON, inheritance, rules, libraries, assets
    loaders.rs       Fabric/Quilt profiles; Forge/NeoForge via the official installer (headless)
    java.rs          Detect installed Java, download Temurin when needed
    launch.rs        Natives, classpath, argument substitution, game process
    perf.rs          Automatic memory and low-spec mode
    diagnose.rs      Crash diagnosis from the log and the crash report

  content/         What goes into an instance
    modrinth.rs      Search, dependencies, hash-based updates, .mrpack
    curseforge.rs    Search, dependencies, CurseForge modpacks
    modpacks.rs      Updating installed modpacks without losing user changes
    modmeta.rs       Metadata read from mod jars (name, version, icon), cached
    resources.rs     Resource packs, shaders and screenshots

  account/         Identity
    auth.rs          Microsoft (device code → Xbox Live → XSTS → Minecraft) and offline
    yggdrasil.rs     Ely.by, LittleSkin and other authlib-injector servers
    skins.rs         Skin upload/reset and local offline skins
    secrets.rs       OS secure store (Keychain, Credential Manager, Secret Service)

  instance/        Managing existing instances
    worlds.rs        Worlds, backups, safe restore and pruning
    servers.rs       servers.dat (NBT) and server list ping
    organize.rs      Groups, ordering, search and icons
    importers.rs     Prism/MultiMC, CurseForge app, ATLauncher, official launcher

  hosting/         Dedicated servers run by the launcher
    install.rs       Vanilla, Paper, Fabric and Forge/NeoForge server installs
    run.rs           Process supervision, console, status parsing
    props.rs         server.properties editing that preserves the rest of the file
    content.rs       Plugins and mods from Modrinth
    p2p.rs           Brasa P2P: invites, pairing, gatekeeper, QUIC tunnel (iroh)
    playit.rs        playit.gg agent integration

  cosmetics/       Wardrobe
    catalog.rs       Procedural, original item designs
    paint.rs, pose.rs, loadout.rs, export.rs, install.rs, support.rs

  setup/           Installer and uninstaller (same binary, started as "…Setup…")
  gui/             egui interface (see below)
```

## GUI: model, view, controller

```text
input ──► view::draw(&Model, &mut ViewState) ──► Vec<Action>
                                                     │
Model ◄── Controller::handle(&mut Model, &mut ViewState, Action)
```

```text
src/gui/
  mod.rs          eframe App wiring the three layers together
  model.rs        Model (instances, settings, accounts, mods, worlds…) and Session (task state)
  state.rs        ViewState: tabs, forms, filters, edit drafts
  action.rs       Action: everything the user can ask for
  controller/     handle.rs (runs actions), tasks.rs (async work and progress), mod.rs
  view/           one module per screen, plus theme, fonts, widgets and formatting helpers
```

The compiler enforces these rules:

- **Views only read the model.** They receive `&Model`. The only thing they can write is
  `ViewState`.
- **Only the controller changes the model**, and only in response to an `Action`. It is also the
  only place that spawns background work (tokio) and owns the HTTP client.
- **Settings are edited as a draft** in `ViewState::settings` and applied through actions.
- **The UI is testable without a window.** Tests run actions against a temporary `Model` and draw
  every tab headlessly.

To add a feature:

1. Add a variant to `action.rs`.
2. Emit it from a view.
3. Handle it in `controller/handle.rs`.

Put data that belongs to the domain in `Model`, and purely visual state in `ViewState`.

## Translations

UI strings are written in Spanish in the source and wrapped in `t!("…")`. `build.rs` turns
`i18n/en.tsv` (Spanish, tab, English) into the `t!` macro, so a lookup costs a single branch. A
test fails if any `t!` literal is missing from the table. `scripts/i18n_scan.py` finds new strings
that aren't wrapped yet, and `--apply` wraps them.

## Testing

`cargo test` runs about 200 tests. None of them need network access, a display or a Minecraft
install. Among them:

- the launch pipeline against a fake game process;
- mirror fallback rules;
- modpack import, export and update;
- world backup and restore, including zip-slip protection;
- `servers.dat` NBT round trips and a ping against a fake server;
- Discord IPC against a fake client;
- the installer, run in a sandbox directory;
- a full Brasa P2P session on loopback: invite, pairing, tunnel, and rejecting a player who uses
  someone else's name;
- every GUI tab drawn headlessly.

CI runs `cargo fmt --check`, `cargo clippy -D warnings`, `cargo doc` with warnings denied and the
test suite on every push.
