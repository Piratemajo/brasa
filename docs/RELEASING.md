# Releasing

## Build-time configuration

Some values belong to whoever distributes Brasa, not to its users. They are embedded into the binary
at compile time (`src/distro.rs`) and never appear in the settings. If one is missing, the feature
that depends on it is hidden and everything else keeps working.

| Value | Build variable | GitHub Actions (Settings → Secrets and variables → Actions) | Without it |
|---|---|---|---|
| Microsoft sign-in | `BRASA_MSA_CLIENT_ID` | variable or secret | Only offline and Yggdrasil accounts |
| CurseForge | `BRASA_CURSEFORGE_KEY` | **secret** | Browse shows Modrinth only |
| Discord Rich Presence | `BRASA_DISCORD_CLIENT_ID` | variable or secret | No Discord option |
| Update repository | `BRASA_UPDATE_REPO` | nothing: CI uses the repository being built (optionally, variable `BRASA_RELEASES_REPO`) | The build does not update itself |

For local development you can set the same variables at runtime, and they take precedence:
`BRASA_MSA_CLIENT_ID=… brasa`. Embedded values are never written to `settings.toml`.

### Microsoft

Each launcher needs its own Azure application, approved by Mojang.

1. **Register the app.** Go to <https://portal.azure.com> → **App registrations** → **New
   registration**.
   - Supported account types: *Personal Microsoft accounts only*.
   - Leave the redirect URI empty. Brasa uses the device code flow.
2. **Allow public client flows.** Under **Authentication**, set *Allow public client flows* to
   **Yes**.
3. **Request access to the Minecraft API.** Use <https://aka.ms/mce-reviewappid>. The form asks for
   the application ID and the tenant ID. Mojang reviews requests weekly.

The application (client) ID is not a secret, since it ships inside every binary.

### CurseForge

Request an API key at <https://console.curseforge.com> and store it as a **secret**. Like every
desktop client key, it can be extracted from the binary. If CurseForge revokes it, generate a new
key and publish a release.

### Discord

1. **Create the application.** At <https://discord.com/developers/applications>, create an
   application named **Brasa Launcher**. Discord shows this name after "Playing".
2. **Add the icon.** Upload `assets/discord/brasa.png` as the application icon.
3. **Add the art asset.** Upload the same file again under **Rich Presence → Art Assets**, with the
   key `brasa`.
4. **Copy the ID.** Copy the **Application ID** into `BRASA_DISCORD_CLIENT_ID`, as a variable or a
   secret. It isn't secret either, since it ships inside every binary.

## Publishing a release

```sh
# bump `version` in Cargo.toml and add an entry to CHANGELOG.md first
git tag v1.0.0-beta.1
git push origin v1.0.0-beta.1
```

CI builds Linux, Windows and macOS and attaches these files to the release:

- the binaries, each with its `.sha256`. The updater refuses an update without one.
- `Brasa-Setup-windows-x64.exe`. This is the same executable: started under a name containing
  "Setup", it runs the installer.
- the cosmetics mod jar for each supported Minecraft family.

Tags with a hyphen (`-beta.1`, `-rc.1`) are published as pre-releases. Only users already on a
pre-release receive them.

Pushes and pull requests build and test on Linux only. To build all three platforms without
tagging, run the workflow by hand (**Actions → build → Run workflow**).

## Code signing (optional)

Without a signature, Windows SmartScreen warns the first time Brasa runs. If these secrets are set,
CI signs `brasa.exe`:

| Platform | Secrets |
|---|---|
| Windows | `WINDOWS_CERTIFICATE` (base64-encoded `.pfx`), `WINDOWS_CERTIFICATE_PASSWORD` |

## Publishing releases to a separate repository (optional)

By default, releases are published in this same repository with the built-in `GITHUB_TOKEN`, so
there is nothing to configure. Players download updates and the cosmetics mod without signing in,
so releases must live in a public repository. Only if you fork Brasa and keep your code private do
you need a second, public repository:

1. **Create the public repository.** For example `user/brasa-releases`, initialised with a README.
2. **Create a fine-grained personal access token.**
   - Repository access: `brasa-releases` only.
   - Permissions: **Contents: Read and write**.
3. **Configure the code repository.**
   - Variable `BRASA_RELEASES_REPO` = `user/brasa-releases`.
   - Secret `BRASA_RELEASES_TOKEN` = the token.

Never embed a token in the binary.
