# Plan — Unificar editor y motores en un solo crate (`rer-engine`)

## Objetivo

Un solo directorio y un solo paquete de Cargo para el editor y los motores, manteniendo
aislamiento real entre 2D y 3D:

```text
hoy                                     objetivo
engine_editor  (bin)                    engine/  (paquete rer-engine)
engine_3d      (bin)  ──► shared           ├── editor/    bin rer_engine_editor
engine_2d      (bin)  ──► shared           ├── engine3d/  bin rer_engine_3d
engine_shared  (lib)                       ├── engine2d/  bin rer_engine_2d
engine_ipc_common (lib)                    └── (core sigue aparte)
```

`engine_shared` **no se fusiona**: sube a `rer-engine-core`. Es el único crate del que
dependen los motores, y fusionarlo con el editor crearía un ciclo de paquetes
(`editor → engine3d → core(=editor)`), justo cuando la ventana única pase a ser
in-process.

## Aislamiento 2D ↔ 3D

| Plano | Garantía |
|---|---|
| Ejecución | Total. Rust no ejecuta inicializadores de módulo; el binario llama solo a `engine2d::run()` o `engine3d::run()`. |
| Enlace | Por features de Cargo: `engine2d` no compila ni enlaza `engine3d`. |
| Ficheros | `engine2d/` y `engine3d/` separados; nada compartido salvo `core`. |

El bin del 2D se construye con `--features engine2d` y **no contiene** el módulo 3D ni
sus dependencias exclusivas (`ufbx`, `rayon`, `pollster`, `gpu-allocator`).
El editor activa ambos porque su función es hospedarlos.

No se usa un dispatcher único por defecto: un `rer_engine.exe` con subcomandos llevaría
los dos motores dentro. Puede añadirse después como alias de conveniencia.

## Medidas de partida (Fase 0)

Equipo actual, `cargo build` en debug salvo donde se indique.

| Crate | Incremental (1 fichero) |
|---|---|
| `rer-engine-shared` | 1,00 s |
| `rer-engine-ipc-common` | 0,89 s |
| `rer-engine-3d` | 4,07 s |
| `rer-engine-2d` | 3,19 s |
| `rer-engine-editor` | 2,23 s |
| **Suma** | **11,4 s** |

Rebuild conjunto de los 5 paquetes tras `cargo clean -p` (aproxima una unidad de
compilación única):

| Perfil | Tiempo |
|---|---|
| debug `cargo build --workspace` | **14,1 s** |
| release `cargo build --release --workspace` | **43,9 s** |

Tamaños actuales de binarios:

| Binario | debug | release |
|---|---|---|
| `rer_engine_2d.exe` | 31,1 MB | 17,1 MB |
| `rer_engine_3d.exe` | 34,2 MB | 17,0 MB |
| `rer_engine_editor.exe` | 35,0 MB | 10,2 MB |

### Puerta de decisión

Criterio acordado: si la recompilación incremental del crate unificado supera **90 s**,
no se fusiona. Medido: 14,1 s. **GO.**

Con features, el bucle de desarrollo no empeora: tocar `engine3d` y reconstruir solo su
bin cuesta ~5 s (3D + core); reconstruir el editor cuesta el conjunto (~14 s).

## Diseño objetivo

```text
src/main/Engine/
├── Cargo.toml                  # workspace: ["core", "engine"]
├── core/                       # rer-engine-core (antes engine_shared + ipc_common)
│   └── src/
│       ├── lib.rs
│       ├── vulkan/             # contexto, swapchain, memoria, buffers, imágenes, sync
│       ├── assets/  scripting/  player_ui/
│       ├── platform.rs  overlay.rs  editor_defaults.rs  backend_policy.rs
│       └── ipc/                # antes engine_ipc_common
└── engine/                     # rer-engine
    ├── Cargo.toml
    └── src/
        ├── lib.rs
        ├── engine3d/  (cfg engine3d)
        ├── engine2d/  (cfg engine2d)
        ├── editor/    (cfg editor)
        └── bin/
            ├── rer_engine_3d.rs
            ├── rer_engine_2d.rs
            └── rer_engine_editor.rs
```

### Features y bins

```toml
[features]
engine2d = []
engine3d = []
editor   = ["engine2d", "engine3d"]   # el editor hospeda ambos

[[bin]]
name = "rer_engine_2d"
path = "src/bin/rer_engine_2d.rs"
required-features = ["engine2d"]

[[bin]]
name = "rer_engine_3d"
path = "src/bin/rer_engine_3d.rs"
required-features = ["engine3d"]

[[bin]]
name = "rer_engine_editor"
path = "src/bin/rer_engine_editor.rs"
required-features = ["editor"]
```

Cada bin se compila por separado (como ya hace `predev` con cuatro invocaciones). Las
dependencias exclusivas de un motor entran por feature con `optional = true`.

## Fases

### Fase 1 — Núcleo

- `engine_shared/` → `core/`, paquete `rer-engine-core`.
- `engine_ipc_common/` → `core/src/ipc/`.
- `rer_engine_shared::` → `rer_engine_core::` (146 usos).
- `wgpu_surface.rs` → `surface.rs`; `gpu.rs` (política) → `backend_policy.rs`.
- Eliminar el bin de salud `rer_engine_shared` (sustituido por `rer_engine status` si
  hiciera falta).

Gate: `cargo check/test --workspace`, 2D y 3D arrancan igual. Commit.

### Fase 2 — `engine/` con el 3D

> Estado: completada. El host sigue en un único `host.rs`; su división en
> `app`/`input`/`overlay`/`preview` se hace en la Fase 6, cuando el editor
> reutilice el input.

- Mover `engine_3d/src` a `engine/src/engine3d/`.
- `crate::` → `crate::engine3d::` (813 usos); `super::` intacto.
- `main.rs` → `engine3d/host.rs` (`run()`); bin de 6 líneas.
- El macro `impl_player_ui_text_input!` recibe ahora la ruta al `State` del motor
  en vez de asumir `crate::engine::State`.
- Borrar ficheros-eco que solo reexportan al core.
- `engine/Cargo.toml` con la feature `engine3d` y sus dependencias opcionales.

Gate: standalone 3D idéntico (escena, FPS, modales, overlay). Commit.

### Fase 3 — Editor dentro del mismo crate

> Estado: completada. El editor usa `SharedGpu` y la feature `editor`; su `gpu.rs`
> de 679 líneas se sustituyó por `editor/render.rs` (~250) sobre el núcleo compartido.

- Mover `engine_editor/src` a `engine/src/editor/`; `crate::` → `crate::editor::` (65 usos).
- Borrar `editor/gpu.rs` (679 líneas de Ash duplicado): usa
  `rer_engine_core::vulkan::SharedGpu`.
- El renderer de egui pasa a `RenderMode::DynamicRendering` y graba el pase de UI
  en el command buffer de `SharedGpu`; un solo submit por frame.
- `engine_host.rs` sigue lanzando el `.exe` en esta fase (no se mezcla con ventana única).

Gate: editor ash + modales Electron idénticos. Commit.

### Fase 4 — Empaquetado

- `predev`, `prebuild`, `predist` pasan a construir cada bin con su feature.
- `electron-builder.yml`: sin cambios de nombres.
- `src/main/index.ts` y `engineStartupScene.ts` siguen igual (mismos nombres de exe).

Gate: `yarn dev` con proyecto 2D y 3D; `electron-builder --dir`.

### Fase 5 — Ventana única 3D

Sin la fase de "convertir el 3D en librería" (ya son módulos del mismo crate):

1. `core/vulkan`: `FrameTarget.offset: vk::Offset2D`; `begin_frame` a cero.
2. `engine3d/render`: `render_area`/viewport/scissor con offset; barrera final a
   `PRESENT_SRC_KHR` al dueño del present.
3. `scene/render.rs`: `render_into(&FrameTarget)`; `State::with_context`;
   `set_viewport_size` sin tocar swapchain.
4. `core/vulkan`: exponer `queue()`; pool de comandos propio del editor para egui.
5. `editor/render/ui_pass.rs`: egui en `RenderMode::DynamicRendering`, `in_flight_frames = 2`,
   `loadOp = LOAD` a ventana completa.
6. `ipc/send`: sink `OnceLock<mpsc::Sender<Value>>`; sin sink → stdout. El JSON que recibe
   `bridge::forward_line` no cambia, así que las modales Electron no se tocan.
7. `host/input` reutilizado por el editor; `ClipCursor` de Win32 al rect.

### Fase 6 — Repartir ficheros grandes

| Fichero | Líneas | Destino |
|---|---|---|
| `engine3d/render/frame.rs` | 3995 | `render/{targets,pipelines,descriptors}.rs` + `render/passes/*` |
| `config_3d/model_asset.rs` | 2360 | `config/model/{store,clips,materials,textures,fbx}.rs` |
| `engine/load_proyect.rs` | 2360 | `loader/{manifest,entities,scripts,assets,scenes}.rs` |
| `engine3d/ipc/commands.rs` | 2171 | `ipc/commands/{scene,assets,camera,physics,ui,graphics}.rs` |
| `core/vulkan/gpu.rs` | 1757 | `vulkan/{context,surface,swapchain,memory,buffer,image,upload,frame_sync}.rs` |
| `engine_3d/main.rs` | 1436 | `host/{app,input,overlay,preview}.rs` (Fase 2/5) |
| `editor/ui/sidebar_left.rs` | 1432 | `ui/left/*.rs` por sección |
| `config_3d/mod.rs` | 1397 | `config/{camera,world,bounds,execution}.rs` |
| `engine3d/scene/render.rs` | 1256 | `scene/render/{gather,record,debug}.rs` |
| `editor/ui/sidebar_right.rs` | 1170 | `ui/right/*.rs` por acordeón |
| `static_model_cache.rs` | 1084 | `assets/model_cache/{store,preload,states}.rs` |
| `editor/app.rs` | 969 | `app/{state,events,save,logs,selection,metrics}.rs` |
| `core/player_ui/text_input` | 1056 | `text_input/{types,hit_test,drag,render}.rs` |

### Fase 7 — Sumar el 2D

- `engine2d/` con `crate::` → `crate::engine2d::` (237 usos).
- Bin `rer_engine_2d` con `required-features = ["engine2d"]`.
- Solo después de que termine la migración Ash del 2D: hoy hay 17 nombres de módulo en
  conflicto con el 3D (`engine`, `vulkan`, `ipc`, `ecs`, `mesh`, `gizmo`, `scripting`,
  `texture`, …), resueltos por prefijo, pero reescribirlos mientras se edita ese crate
  garantiza conflictos.

## Riesgos

| Riesgo | Mitigación |
|---|---|
| Ciclo de paquetes si `core` se fusiona | `core` queda como crate independiente |
| Recompilación del crate unificado | Medido 14,1 s debug / 43,9 s release; límite 90 s |
| Reescritura de 878 rutas `crate::` | Por fichero + `cargo check` tras cada movimiento |
| Features unificadas en `cargo build --workspace` | Los artefactos se construyen por bin, no con `--workspace` |
| Pisarse con el agente del 2D | Fases 1–6 no tocan `engine_2d/**`; la 7 va aparte |
| Perder historial en los `git mv` | Movimientos en commits separados y `git log --follow` |

## Criterios de aceptación

1. Un solo directorio `engine/` con editor + motores; `core/` separado.
2. Un binario por motor y ninguno contiene el otro (`--features`).
3. El editor compila con ambos y ejecuta el que pida el proyecto.
4. `predev` construye los tres bins sin `--workspace`.
5. `cargo test --workspace`, `clippy -D warnings` y `fmt --check` limpios.
6. 2D y 3D standalone funcionan igual que antes; modales Electron intactas.
7. Recompilación incremental del conjunto por debajo de 90 s.
