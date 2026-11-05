# Security policy

## Supported versions

Security fixes go into the latest release (including pre-releases). Brasa updates itself, so please
make sure you are on the newest version before you report.

## Reporting a vulnerability

**Please do not open a public issue.** Report it privately through GitHub:
**Security → Report a vulnerability** on this repository.

Please include:

- the affected version and platform;
- what an attacker could do, and under which conditions;
- steps to reproduce, or a proof of concept.

You'll get an acknowledgement within a few days. Once a fix has been released, we'll credit you in
the release notes, unless you'd rather stay anonymous.

## Scope

These areas are especially relevant:

- **Account tokens:** storage (OS secure store, `accounts.json`) and redaction in logs and problem
  reports.
- **Self-updates:** checksum verification and the release source.
- **Archive extraction:** modpacks, world backups and importers, including path traversal.
- **Brasa P2P:**
  - invite codes;
  - device pairing;
  - name binding;
  - the gatekeeper;
  - the local tunnel.
- **Downloaded executables:** Java runtimes, loader installers and authlib-injector, including how
  they are verified.

Vulnerabilities in Minecraft itself, in mods, or in third-party services (Microsoft, Modrinth,
CurseForge, playit.gg) should be reported to their respective owners.
