# rer-worldgen — Plugin de generación procedural de mundos 3D

Crate independiente del motor RER-ENGINE que genera **terreno natural con
superficies suaves** (Marching Cubes), por chunks, con cuotas configurables por
feature y distribución automática de blueprints.

La lógica vive **solo en Rust**. El editor nativo Rust envía la config y muestra
progreso; el engine 3D sube mallas y splats a GPU. No se duplica generación
en TypeScript.

## Arquitectura

```
WorldGenConfig (JSON camelCase, validado + fit_config_to_world)
   → FeaturePlan   (planner: sitios con cuotas, lagos lejos de montañas,
                    cuevas, ríos)
   → Terrain       (heightfield + CarveIndex; lecho de río con suelo mínimo)
   → DensityGrid   (grid con padding por chunk, Y-layer pruning, LOD)
   → mesh_chunk    (Marching Cubes, normales hacia fuera, UV de columna)
   → chunk_splat   (biomas → pesos RGBA; nieve solo en cumbres)
   → scatter       (reglas por blueprint → colocaciones deterministas)
```

Contrato: **todo es función pura de `(seed, config)`**. Sin estado global, sin
relojes, sin entropía externa. Chunks vecinos comparten puntos de grid ⇒ las
costuras coinciden por construcción.

Densidad **positiva = sólido**. El mundo es un **rectángulo** (cuadrado por
defecto); `fit_config_to_world` escala features al área generada.

## Configuración (modal del editor)

`src/main/Engine/engine/src/editor/worldgen.rs` arma el JSON (`PanelState::config_json`).

- **Tamaño** del mundo visible (m) y **voxel size** (m): el voxel es la celda de
  Marching Cubes. Más pequeño = más detalle y **muchas más columnas** (el chunk
  sigue siendo 32 celdas). El plugin genera un rectángulo mayor (100 % de margen
  por lado) para crecer por una cara sin regenerar; el grade, los rasgos y su
  densidad se escalan al mundo visible (`visibleWidth/visibleLength`).
- **Biomas activos** con **porcentaje del mundo** (`biomes.shares`). Los
  activos suman 100 % y cada uno tiene un mínimo del 10 % (salvo un solo
  bioma al 100 %).
- **Nieve**: no es una zona en planta. Su porcentaje cubre las **cumbres**.
- **Volcánico**: alrededor de las montañas, no manchas sueltas. El desierto
  usa regiones anisotrópicas (no un disco). Las montañas mezclan mesetas y
  picos. El suelo base tiene relieve amplio y poco ruido fino.
- **Inclinación**: la rampa cardinal escala con el tamaño del mundo. Los
  mundos pequeños usan el mínimo suave actual; los grandes se acercan al máximo
  anterior (más desnivel absoluto). El cálculo está en
  `scale::land_grade_amplitude`.
- **Lagos / valles / montañas**: huella elíptica con costa irregular. Las
  montañas combinan mesetas y picos con masa + crestas (la referencia
  de diseño no incluye lagos ni ríos).
- **Ríos**: nacen en ladera de montaña o en el borde alto del mundo y
  bajan (lago o salida del mapa). No se plantan en un punto interior.
- Semilla, colinas/llanuras/acantilados, cuevas, cascadas, scatter.

Límites de tamaño y biomas: `editor/worldgen.rs` (ventana nativa) y `scale.rs` /
`config.rs` (Rust).

## Biomas y splat

Con porcentajes, un campo de **regiones grandes** (`biomes/regions.rs`) se
reparte por cuantiles entre biomas de suelo. Trozo aislado &lt; 4 % del mundo
se funde con el vecino; cada bioma conserva su región mayor.

Splat por columna XZ (64×64, lod 0): R=hierba, G=roca, B=arena, A=nieve.
El engine **no** sube un splat por capa 1024² del `TextureArray` (256 capas
compartidas con modelos): las columnas van a un **atlas** (16×16 tiles de
64 px por página). Si el array GPU se llena, el chunk nuevo usa fallback;
**no** se pisa el splat de una columna ya en escena.

Fotos de suelo (Grass, Dirt, Clay, Sand, Stone, Snow, Mud) en
`worldgen/assets/terrain/`. Viajan en el ZIP del plugin (`terrain/` al lado
de la biblioteca nativa). El engine solo las sube a GPU; el shader `forward.frag`
mezcla esas capas con el splat. El vaso de un lago usa `Mud.jpg` (resto de los
pesos RGBA). Capa 0 del array = checker del editor; Grass no puede usarla.

## Consumo en el engine

`engine3d/config_3d/worldgen/`:

1. `world_gen_set_config` solo guarda el JSON. `world_gen_start` despacha al
   hilo `rer-worldgen`, que crea el handle (`wg_world_create`), barre columnas
   (`SCAN_COLUMNS_PER_FRAME` entre eventos `phase: scan`) y meshea
   (`wg_mesh_chunk` / `wg_chunk_textures`). El hilo de render no espera
   ese CPU.
2. `worldgen_tick` drena CPU y sube como máximo 2 mallas/frame
   (`CHUNKS_PER_FRAME`), progreso `phase: mesh`. Tras completar, el scatter
   spawnea como máximo `PLACEMENTS_PER_FRAME` instancias/frame.
3. Recorte rectangular a los límites visibles; UVs de splat acotadas a
   `[0, 1]`.
4. **Seguimiento automático**: si el plugin ya generó el mundo y se mueve una
   cara, el engine reacciona solo:
   - dentro del margen generado, meshea únicamente las columnas nuevas y
     re-meshea los chunks que tocan el borde movido;
   - al encoger, elimina los chunks y el scatter que quedan fuera;
   - si el mundo supera el área generada, regenera todo con un área nueva.

   Esta ruta vive en `config_3d/worldgen` y solo se activa con la biblioteca
   nativa cargada y un mundo ya generado: el motor 3D normal no genera ni recorta terreno.

Instalación local:
- Windows: `%APPDATA%\RER-ENGINE\plugins\world-gen\` con `rer_worldgen.dll`.
- Linux: `$XDG_CONFIG_HOME/RER-ENGINE/plugins/world-gen\` (o
  `~/.config/RER-ENGINE/plugins/world-gen\`) con `librer_worldgen.so`.

Ambas incluyen la carpeta `terrain/`. `yarn dev` y `yarn start` reconstruyen
el crate y copian la biblioteca + texturas (debug / release según el perfil).
A mano: `yarn rust:build:worldgen` o `yarn rust:build:worldgen:release`.
El editor no debe tener la biblioteca cargada durante la copia. Un cambio de
shader/host GPU pide reiniciar `yarn dev`. No reinstalar el plugin desde el
catálogo para una biblioteca local.

Generar el mundo es **CPU** (densidad, mesh, splat) en el hilo `rer-worldgen`.
El tick del engine solo sube GPU. Eso es normal.

## Fachada FFI (`plugin_ffi`, ABI v2)

La biblioteca `rer_worldgen.dll` (Windows) o `librer_worldgen.so` Linux
(`cdylib`) se instala desde Plugins (GitHub Releases) o desde el build local.
El engine **no depende del crate**: `libloading` + espejo en
`engine3d/config_3d/worldgen/abi.rs`.

Exports: `wg_abi_version`, `wg_world_create`, `wg_world_free`,
`wg_chunk_status`, `wg_mesh_chunk(lod)`, `wg_chunk_textures`,
`wg_scatter`, y los `wg_free_*`.

1. Cada export captura panics (`catch_unwind`).
2. Allocator único: la biblioteca allocó → `wg_free_*`.
3. Errores: `Status` + `wg_last_error`.
4. Cambio de firma → incrementar `ABI_VERSION`.

`wg_chunk_status` cachea el pruning Y por columna XZ (no recalcular 3×3
alturas por cada capa Y).

## Tests

- Determinismo (ruido, campo, plan, scatter, hash de mallas).
- Topología MC: costuras, normales hacia fuera, UV en `[0, 1]`.
- Planner: cuotas, lagos lejos de montañas, suelo de lagos/ríos.
- Biomas: porcentajes, nieve en cumbres, orilla de arena continua.
- FFI contra la biblioteca nativa; presupuestos en release (ver `tests/performance.rs`).

## Publicación

```powershell
powershell -File scripts\publish-worldgen.ps1 -Version "1.0.0"
```

```bash
node ./scripts/publish-worldgen.mjs 1.0.0
```

El script cross-platform genera `rer-worldgen-<versión>-win-x64.zip` con
`rer_worldgen.dll` o `rer-worldgen-<versión>-linux-x64.zip` con
`librer_worldgen.so`, más `terrain/*.jpg`, e imprime el SHA-256. Ese valor se
configura en `engine/src/editor/plugin_runtime.rs`; el artefacto Linux sigue
pendiente de publicación y no se instala on-demand hasta tener hash verificado.
