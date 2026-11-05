# Mods de cosméticos de Brasa

La Taquilla del launcher decide qué lleva puesto cada cuenta. Antes de jugar, Brasa escribe en la
instancia `config/brasa/cosmetics/` con todo ya resuelto: modelos, texturas PNG y emotes. El mod de
esta carpeta lo lee y lo dibuja **sólo en tu jugador**, porque de momento nadie más los ve.

Como el código de dibujo de Minecraft cambia entre versiones, hay **un adaptador por familia**.
Todos leen el mismo formato, así que añadir una versión consiste en traducir unas 600 líneas de Java.

| Familia          | Carpeta            | Estado        |
|------------------|--------------------|---------------|
| Fabric 1.21–1.21.1 | `fabric-1.21.1/` | escrito       |
| Fabric 1.21.2+   | —                  | pendiente (cambia a `EntityRenderState`) |
| Fabric 1.20.x    | —                  | pendiente     |
| Fabric 1.16.5–1.19.4 | —              | pendiente     |
| NeoForge 1.21.1  | —                  | pendiente     |
| Forge 1.20.1     | —                  | pendiente     |
| Forge 1.12.2     | —                  | pendiente     |
| Forge 1.8.9      | —                  | pendiente     |

La lista que usa el launcher está en `src/cosmetics/support.rs`. Para activar una familia, pon
`ready: true` y añádela a la matriz `cosmetics-mod` de `.github/workflows/build.yml`.

## Qué hace el adaptador

1. **Capa:** sustituye la textura de capa del jugador (sirve también para los élitros). Si la capa
   es animada, cambia de fotograma (`cape_0.png`…) a los `fps` indicados.
2. **Sombreros, espalda y mascotas:** construye cada modelo con `ModelPart` y su «box UV»:
   - `attach: head` va pegado a la cabeza;
   - `body` va pegado al cuerpo;
   - `beside` flota junto al jugador.

   Aplica los movimientos `swing`, `bob` y `spin` de cada hueso. Con casco no dibuja el sombrero, y
   con élitros no dibuja lo de la espalda.
3. **Emotes:** con la tecla **B** se abre la rueda de emotes. El emote sustituye la pose del modelo
   (interpolación *smoothstep*, en bucle si `looping`). Pasa a tercera persona mientras dura y se
   para al moverte.

## Formato (`loadout.json`, `format: 1`)

Coordenadas de Brasa, en píxeles de skin con el origen en los pies:
- `x` va hacia la izquierda del jugador.
- `y` va hacia arriba.
- `z` va hacia delante.

Las rotaciones van en grados, con `R = Rz·Ry·Rx`. Al espacio de modelo de Minecraft se pasa con el
punto `(x, 24 − y, −z)` y las rotaciones `(rx, −ry, −rz)`.

```json
{
  "format": 1,
  "player": "Steve",
  "cape": { "id": "cape.aurora", "frames": ["cape_0.png", "…"], "fps": 8 },
  "items": [
    { "id": "hat.gorro", "slot": "hat", "texture": "hat.png",
      "model": { "texture_size": [64, 32], "attach": "head",
        "bones": [{ "name": "root", "pivot": [0, 32, 0], "rot": [0, 0, 0],
          "cubes": [{ "from": [-4.5, 29, -4.5], "size": [9, 4, 9], "uv": [0, 0], "inflate": 0 }],
          "motion": { "kind": "swing", "axis": "y", "amp": 18, "speed": 0.5, "phase": 0 },
          "children": [] }] } }
  ],
  "emotes": [
    { "id": "emote.saludar", "name": "Saludar",
      "emote": { "duration": 1.2, "looping": true,
        "keys": [{ "t": 0, "bones": { "right_arm": [0, 0, -145] }, "lift": 0, "yaw": 0 }] } }
  ]
}
```

## Compilar y probar a mano

Necesitas JDK 21 y Gradle 8.10:

```sh
cd mods/cosmetics/fabric-1.21.1 && gradle build
BRASA_COSMETICS_JAR=$PWD/build/libs/brasa-cosmetics-fabric-1.21.1-1.0.0.jar brasa gui
```

Con esa variable, el launcher instala ese jar en lugar de descargarlo de la release.
