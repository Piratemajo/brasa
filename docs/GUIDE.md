# User guide

## Accounts

| Type | Use it for | Skin | Servers |
|---|---|---|---|
| **Microsoft** | You own the game | Official | All, including Realms |
| **Microsoft without the game** | Trying the game | — | Official demo mode |
| **Offline** | Singleplayer, LAN | Steve/Alex, or a local skin only you see | `online-mode=false` servers |
| **Ely.by / LittleSkin / custom Yggdrasil** | Playing without a Microsoft account, with a skin | From that service | `online-mode=false` servers and servers that use the same service |

Yggdrasil accounts use [authlib-injector](https://github.com/yushijinhun/authlib-injector). Brasa
downloads it and checks its SHA-256 before using it. Your password is sent once to get a token and
is never stored. Session tokens are kept in the operating system's secure store.

## Connection problems

If you see `operation timed out` or `error sending request`:

1. **Find what fails.** Run **Settings → Test connection** (or `brasa net-test`) to see which
   service doesn't respond.
2. **Behind a corporate or school network?** Set the proxy in **Settings → Proxy**.
3. **Mojang not responding?** Set **Download source** to *Mirror first* (BMCLAPI).

Brasa already retries over IPv4 and through mirrors on its own, so this is rarely needed.

## Performance

These settings live in `src/game/perf.rs`.

- **Automatic memory** (on by default). When an instance has no fixed memory, Brasa sizes it from:
  - your RAM;
  - the Minecraft version;
  - the number of enabled mods.

  It always leaves the system at least 30% of RAM and never assigns more than 60%. If you set
  memory by hand, Brasa warns you when it leaves too little for the system.
- **Low-spec mode** (per instance). Before each launch it lowers the most expensive settings in
  `options.txt`:
  - render distance and simulation distance;
  - frame rate, with vsync on;
  - particles and entity shadows;
  - biome blend;
  - "Fabulous" graphics, which become "Fancy".

  It only ever lowers a value, and leaves the rest of the file alone.

```sh
brasa config auto_memory=false   # use a fixed default instead
```

## Worlds and backups

The **Worlds** tab of each instance lists the instance's worlds and their backups. Backups are
`.zip` files in `<instance>/backups/`, outside the game directory, so they aren't exported or
duplicated with the instance.

- **Automatic backups.** Before you play, Brasa backs up every world that changed since its last
  backup. **Settings → World backup on play** sets how many backups to keep per world.
- **Restore.** The backup is extracted to a temporary folder first. If the world still exists, the
  current copy is moved aside as `<world>.antes-de-restaurar-<timestamp>`. It is never deleted.
- **Safety.** Paths that would escape the world folder are rejected, and `session.lock` is never
  included.

## Crash diagnosis

When the game exits with an error, Brasa reads the end of the log and the crash report from that
session. It then lists the likely causes in the console, each with a suggested fix. Each session's
log is saved to `<instance>/logs/brasa-latest.log`, and the previous one is kept as
`brasa-previous.log`.

**Report a problem** (in the console, on the crash card or in Settings → About) uploads a report to
[mclo.gs](https://mclo.gs) after removing tokens, your username and your folder paths. You can
review it before it is sent.

## Wardrobe

The Wardrobe has:

- capes, some of them animated;
- hats;
- wings and back items;
- pets;
- emotes.

All of them are original designs. You can try them on in a 3D preview with your own skin: drag to
rotate it, and hover over an item to try it on. Your choice is saved per account.

In game, items are drawn by the Brasa mod. The launcher installs it automatically in compatible
instances: currently **Fabric/Quilt 1.21–1.21.1**. Only you see your cosmetics. Press **B** to open
the emote wheel. The data format, and how to add support for other versions, are described in
[`mods/cosmetics/README.md`](../mods/cosmetics/README.md).

## Hosting a server

**Servers → New server** creates a Vanilla, Paper, Fabric, NeoForge or Forge server. Once it exists
you can:

- install plugins (Paper) or mods (Fabric, NeoForge, Forge) from Modrinth;
- edit `server.properties` from the UI;
- manage players and operators.

The server only listens on your own computer (`127.0.0.1`). There are three ways to let friends in:

- **Brasa P2P.** This is the recommended way.
  - **Invite.** **Invite friend** creates a personal code. It can be used once, expires, and is
    bound to a Minecraft name.
  - **Join.** Your friend pastes the code in **Join a friend** and plays.
  - **Connection.** It is encrypted QUIC: direct between the two computers when the network allows,
    and through a relay when it doesn't.
  - **Name check.** Brasa checks the name in the login packet, so nobody can join under another
    name.
  - **Approvals.** With approvals on, you confirm every new device.
- **Home network.** Open the server to your LAN in its settings.
- **playit.gg.** Link your playit account for a public address.

## Shortcuts

- **Ctrl K** (⌘K on macOS) opens the command palette. Use it to launch an instance, jump to a
  section or search for mods.
- **Ctrl F** searches the console.
- **Drag and drop** onto the window:
  - an `.mrpack` or `.zip` imports a modpack;
  - a `.jar` adds a mod.

## Languages

English and Spanish, selectable in **Settings → Launcher → Language**.
