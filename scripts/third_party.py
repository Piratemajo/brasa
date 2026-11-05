#!/usr/bin/env python3
"""Genera THIRD_PARTY_NOTICES.md con las dependencias de Rust que entran en el binario.
Uso: python3 scripts/third_party.py > THIRD_PARTY_NOTICES.md (lo ejecuta también el CI)."""
import json, subprocess, collections

meta = json.loads(subprocess.check_output(["cargo", "metadata", "--format-version", "1", "--locked"]))
pkgs = {p["id"]: p for p in meta["packages"]}
root = meta["resolve"]["root"]
nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
# sólo lo que se enlaza: dependencias normales (no de desarrollo), en cualquier plataforma
seen, stack = set(), [root]
while stack:
    i = stack.pop()
    if i in seen:
        continue
    seen.add(i)
    for d in nodes[i]["deps"]:
        if any(k["kind"] in (None, "build") for k in d["dep_kinds"]):
            stack.append(d["pkg"])
seen.discard(root)
rows = sorted((pkgs[i]["name"], pkgs[i]["version"], pkgs[i].get("license") or "ver el repositorio", pkgs[i].get("repository") or "") for i in seen)
by = collections.Counter(r[2] for r in rows)

print("""# Avisos de terceros · Third-party notices

Brasa Launcher se publica con licencia MIT (ver `LICENSE`). Incluye o usa el trabajo de otras
personas, con sus propias licencias. Brasa Launcher is MIT-licensed and includes or uses the
work of others under their own licenses:

## Incluido en el programa · Bundled

| Componente | Licencia | Dónde |
|---|---|---|
| IBM Plex Sans y Plex Sans Condensed (IBM Corp.) | SIL Open Font License 1.1 | `assets/fonts/` (texto completo en `assets/fonts/OFL.txt`) |
| Dependencias de Rust (lista abajo) | MIT, Apache-2.0, BSD, ISC, Zlib y similares | dentro del ejecutable |

Unas pocas dependencias (resvg, usvg, option-ext…) son MPL-2.0: se usan sin modificar y su código
fuente está en sus repositorios, enlazados en la tabla. A few dependencies are MPL-2.0; they are
used unmodified and their source code is available at the linked repositories.

## Descargado al usarlo · Downloaded at runtime (no incluido)

| Componente | Licencia | Para qué |
|---|---|---|
| Minecraft (Mojang Studios / Microsoft) | EULA de Minecraft | el juego; se descarga de los servidores oficiales |
| Java (Eclipse Temurin, Adoptium) | GPLv2 con Classpath Exception | si no hay un Java compatible instalado |
| Fabric, Quilt, Forge, NeoForge | Apache-2.0 / LGPL según el proyecto | los loaders que elijas |
| authlib-injector (yushijinhun) | AGPL-3.0 | sólo cuentas con skin de Ely.by / LittleSkin |
| Servidor de Minecraft (Mojang) y Paper (PaperMC) | EULA de Minecraft / GPL-3.0 y MIT | los servidores que crees en Brasa |
| Mods, texturas, shaders y modpacks | los de cada autor (Modrinth, CurseForge) | lo que instales |
| Plugins de servidor | los de cada autor (Modrinth) | lo que instales en tus servidores |

Brasa no está afiliado, respaldado ni aprobado por Mojang Studios ni Microsoft. «Minecraft» es una
marca de Mojang Synergies AB. Not affiliated with, endorsed or approved by Mojang or Microsoft.

## Dependencias de Rust · Rust dependencies
""")
print("Resumen por licencia: " + ", ".join(f"{l} ({n})" for l, n in by.most_common()) + "\n")
print("| Crate | Versión | Licencia | Repositorio |\n|---|---|---|---|")
for n, v, l, r in rows:
    print(f"| {n} | {v} | {l} | {r} |")
print("\nLos textos completos de cada licencia están en el repositorio de cada crate (y en crates.io).")
