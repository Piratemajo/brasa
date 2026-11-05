# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [1.0.0-beta.1]

First public release.

### Instances and game

- Isolated instances, each with its own:
  - Minecraft version;
  - mod loader;
  - Java;
  - memory;
  - launch options: fullscreen, a wrapper command, environment variables, and pre/post-launch
    commands.
- Fabric, Quilt, Forge and NeoForge. Forge and NeoForge use the official installer, run headless.
- Java detection, with an automatic Temurin download when no installed Java is compatible.
- Repair, which checks every file again by SHA-1 and downloads only the broken ones.
- Automatic memory sizing and an optional low-spec mode.
- Crash diagnosis covering:
  - the wrong Java version;
  - running out of memory;
  - Mixin errors;
  - missing dependencies;
  - OpenGL drivers;
  - corrupt jars;
  - ports already in use.
- World backups before playing, and restore without deleting the current world.
- The server list, shared with the game's `servers.dat`, with a live ping and direct join.
- Groups, sorting, search and custom icons for instances. Play time is tracked.

### Content

- Modrinth and CurseForge search and install, with dependency resolution.
- Mod updates found by file hash.
- Modpacks: `.mrpack` and CurseForge `.zip`.
  - Import and export.
  - Updating an installed modpack keeps the player's own changes.
- Resource packs, shaders (Iris, OptiFine) and screenshots with thumbnails.
- Import from Prism Launcher, MultiMC, the CurseForge app, ATLauncher and the official launcher.

### Accounts

- Microsoft accounts, using the device code flow. Accounts without the game launch in demo mode.
- Offline accounts, with an optional local skin.
- Ely.by, LittleSkin and other Yggdrasil servers through authlib-injector.
- Skin upload and reset for Microsoft accounts.
- Tokens kept in the operating system's secure store.

### Server hosting

- Vanilla, Paper, Fabric, NeoForge and Forge servers, with:
  - a console;
  - player management;
  - `server.properties` editing;
  - plugins and mods from Modrinth.
- **Brasa P2P**, for joining a friend's server without port forwarding:
  - personal invite codes that can be used once and expire;
  - devices paired to a Minecraft name, with an optional approval step;
  - login names checked against the pairing;
  - an encrypted QUIC tunnel, direct when possible and through a relay otherwise.
- playit.gg integration as an alternative.

### Interface

- A native egui interface in English and Spanish, with a first-run setup.
- The Wardrobe: original capes, hats, back items, pets and emotes, with a 3D preview. A companion
  Fabric mod draws them in game (Minecraft 1.21–1.21.1).
- A command palette (Ctrl K) and a filterable console with search.
- Discord Rich Presence.
- Report a problem: a redacted upload to mclo.gs.

### Distribution

- One binary for Windows, macOS and Linux. On Windows it also works as the installer.
- Self-update from GitHub Releases, verified by SHA-256. Stable and pre-release channels are kept
  separate.
- A network diagnostic, IPv4 fallback, download mirrors (BMCLAPI, Maven Central) and proxy support.
- A complete command-line interface.

[Unreleased]: https://github.com/Piratemajo/brasa/compare/v1.0.0-beta.1...HEAD
[1.0.0-beta.1]: https://github.com/Piratemajo/brasa/releases/tag/v1.0.0-beta.1
