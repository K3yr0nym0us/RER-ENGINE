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
│       ├── platform.rs  editor_defaults.rs  backend_policy.rs
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

> Estado: completada. El motor 3D se construye con `State::with_context` sobre el
> `SharedGpu` del editor, graba en el rectángulo del `CentralPanel` y egui se dibuja
> encima en el mismo command buffer. Los eventos del motor salen por el sink IPC al
> mismo JSON que consume Electron. Pendiente de pulido: `ClipCursor` explícito al rect
> (hoy el motor usa `CursorGrabMode::Locked/Confined` sobre la ventana compartida).

Sin la fase de "convertir el 3D en librería" (ya son módulos del mismo crate):

1. `core/vulkan`: `FrameTarget.offset: vk::Offset2D`; `begin_frame` a cero.
2. `engine3d/render`: `render_area`/viewport/scissor con offset; `FrameRenderer::finish_present`
   separa la barrera a `PRESENT_SRC_KHR` para que egui pinte antes.
3. `scene/render.rs`: `render_into(&FrameTarget)`; `State::with_context`;
   `set_viewport_size` sin tocar swapchain.
4. `core/vulkan`: `queue()`, `instance()`, `physical_device()` y `queue_family_index()`;
   pool de comandos propio del editor para egui.
5. `editor/render.rs`: egui en `RenderMode::DynamicRendering`, `in_flight_frames = 2`,
   `loadOp = LOAD` a ventana completa sobre la escena.
6. `ipc`: sink `OnceLock<mpsc::Sender<String>>`; sin sink → stdout. El JSON que recibe
   `bridge::forward_line` no cambia, así que las modales Electron no se tocan.
7. `host`: `App::new_in_editor`, `handle_command`, `handle_window_event`, `tick`,
   `frame_update` reutilizados por el editor; input traducido al rect del viewport.

### Fase 6 — Repartir ficheros grandes

> Estado: completada. Hecho: `vulkan/frame`, `ipc`, `core/vulkan/gpu`, `commands`,
> `load_proyect`, `config_3d/model_asset`, `static_model_cache`, `engine/render`,
> `config_3d/mod`, los sidebars, `app` del editor y `core/player_ui/text_input`.

| Antes | Ahora |
|---|---|
| `engine3d/vulkan/frame.rs` 3.995 | `vulkan/frame/{mod,commands,layouts,pipelines,targets,forward,present,rt,reflections,shadow}.rs` |
| `engine3d/engine/commands.rs` 2.171 | `engine/commands/{mod,system,scenes,entities,assets,graphics,world,player_ui,animation,scripts,sockets}.rs` |
| `engine3d/engine/load_proyect.rs` 2.360 | `engine/load_proyect/{mod,schema,manifest,paths,restore,scene,events,snapshot}.rs` |
| `config_3d/model_asset.rs` 2.360 | `config_3d/model_asset/{mod,types,material,import,asset,animation,skeleton,play}.rs` |
| `core/vulkan/gpu.rs` 1.757 | `core/vulkan/gpu/{mod,context,swapchain,memory,frame}.rs` |
| `engine3d/config_3d/mod.rs` 1.397 | `config_3d/{mod,helpers,model_ops,picking,gizmo_drag}.rs` |
| `engine3d/ipc.rs` 1.372 | `engine3d/ipc/{mod,types,events}.rs` |
| `engine3d/engine/render.rs` 1.256 | `engine/render/{mod,batches,rt,skinned,frustum}.rs` |
| `config_3d/static_model_cache.rs` 1.084 | `config_3d/static_model_cache/{mod,types,registry,preload,spawn}.rs` |
| `editor/ui/sidebar_left.rs` 1.432 | `ui/sidebar_left/{mod,scenes,camera,resources,entities,interface,tools,controls}.rs` |
| `editor/ui/sidebar_right.rs` 1.170 | `ui/sidebar_right/{mod,workspace,grid,lighting,textures,reflections,shadows,msaa,taa,gravity,fps,tiers,metrics}.rs` |
| `editor/app.rs` 969 | `editor/app/{mod,events,logs,save}.rs` |
| `core/player_ui/text_input/mod.rs` 1.056 | `text_input/{mod,methods/{state,edit,drag,emit}}.rs` |

Regla: los `mod.rs` conservan los re-exports de las rutas públicas, así que los
consumidores no cambian aunque el código viva en otro fichero. En `text_input` el
macro principal mantiene su firma y expande sub-macros `__rer_player_ui_text_*!`
(exportados con `#[macro_export]` y resueltos vía `$crate::`).

### Fase 7 — Sumar el 2D

> Estado: completada. `engine_2d/` vive en `engine/src/engine2d/`; los 250 usos de
> `crate::` se reescribieron a `crate::engine2d::`, el bin `rer_engine_2d` usa
> `required-features = ["engine2d"]` y `build.rs` compila los shaders 2D con prefijo
> `2d_`. El paquete `rer-engine-2d` desaparece del workspace.

- Feature `engine2d` (en `default` para el build local completo; cada bin se compila
  con `--no-default-features --features <motor>`).
- `engine2d/host.rs` conserva el host winit; el bin solo llama a `host::run()`.
- Shaders en `engine/shaders/2d/`, compilados solo si la feature está activa.
- Ventana única completada: `editor` activa `engine3d` + `engine2d`;
  `InProcessEngine` construye el motor del proyecto sobre el `SharedGpu` del editor y
  graba en el sub-rect del `CentralPanel`. El input se traduce al rect del viewport y
  los comandos de la UI se drenan antes de actualizar el motor.
- Fallback 2D por proceso eliminado en la Fase 8; el motor siempre va in-process.

> Aislamiento de compilación: `engine2d` no compila `ufbx`, `rayon`, `pollster`, `gltf` ni
> `egui*`/`rfd`; `engine3d` no compila `egui*`/`rfd`. `gpu-allocator` queda compartido vía
> `rer-engine-core` (memoria Vulkan).

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
