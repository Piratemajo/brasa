# Privacidad · Privacy

**Resumen: Brasa no tiene servidores propios ni recoge datos. No hay telemetría, ni analíticas, ni
cuentas de Brasa.** Todo lo que guarda se queda en tu ordenador.

## Qué guarda en tu equipo

| Qué | Dónde |
|---|---|
| Cuentas (nombre, UUID) | `accounts.json` en la carpeta de datos de Brasa (permisos sólo para ti) |
| Tokens de sesión de Microsoft / Ely.by / LittleSkin | el almacén seguro del sistema (Llavero de macOS, Administrador de credenciales de Windows, Secret Service en Linux); si no hay, en `accounts.json` |
| Ajustes, instancias, cosméticos, copias de mundos, servidores | la carpeta de datos de Brasa |
| Clave de Brasa P2P (identidad de este equipo) | el almacén seguro del sistema; en Linux, `p2p.key` con permisos sólo para ti |
| Amigos emparejados e invitaciones (del secreto sólo su SHA-256) | `hosting/<servidor>/brasa-server.toml` |
| Clave del agente de playit.gg (si lo enlazas) | el almacén seguro del sistema; en Linux, `playit.key` con permisos sólo para ti |

Las contraseñas de Ely.by o LittleSkin **no se guardan**: se usan una vez para obtener el token.
Puedes borrarlo todo quitando las cuentas en Brasa y borrando la carpeta de datos
(Ajustes → Launcher → Carpeta de datos).

## Con quién se conecta

Sólo cuando hace falta para lo que pides:

| Servicio | Para qué |
|---|---|
| Microsoft, Xbox Live y Mojang | iniciar sesión, descargar el juego y sus recursos |
| Adoptium | descargar Java si no tienes uno compatible |
| Fabric, Quilt, Forge, NeoForge, PaperMC | descargar el loader o el software de servidor elegido |
| Modrinth y CurseForge | buscar e instalar mods, texturas, shaders y modpacks |
| Ely.by / LittleSkin u otro servidor que elijas | cuentas sin premium con skin |
| GitHub | buscar actualizaciones de Brasa y descargar el mod de cosméticos |
| Servidores de Minecraft de tu lista | consultar su estado (ping) |
| mclo.gs | subir un informe sólo cuando pulsas «Informar de un problema» (antes se le quitan tokens, tu usuario y tus carpetas) |
| Relés de iroh (n0.computer) | Brasa P2P: el saludo inicial entre tu Brasa y el de tu amigo y, si la conexión directa no es posible (o activas «Ocultar mi IP»), la partida. Va cifrada de punta a punta: el relé no puede leerla. También se publica en su servicio de nombres qué relé usa tu clave, para que tu amigo te encuentre |
| playit.gg | sólo si lo enlazas: el túnel para que entren amigos de fuera a tus servidores. El tráfico de la partida pasa por sus servidores |
| Discord (en tu propio equipo) | mostrar «Jugando a Brasa Launcher», sólo si lo activas |

Un servidor que creas en Brasa escucha sólo en tu equipo y tus amigos entran por Brasa P2P (con invitación y cifrado). Si activas «Abierto a tu red de casa», también se puede entrar desde tu red local a tu IP local. Con la conexión directa de Brasa P2P, tu amigo y tú veis la IP del otro; «Ocultar mi IP» lo evita.

Cada servicio tiene su propia política de privacidad. Brasa les envía lo mínimo para funcionar (por
ejemplo, el nombre del juego y la versión al buscar mods) y nunca tu contraseña de Microsoft: el
inicio de sesión se hace en la página oficial de Microsoft.

---

**In short:** Brasa has no servers of its own and collects no data: no telemetry, no analytics, no
Brasa accounts. Accounts and settings stay on your computer, session tokens go to the system's
secure store, and Brasa only talks to the services above when you use the feature that needs them.
