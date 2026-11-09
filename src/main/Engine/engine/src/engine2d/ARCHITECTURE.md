# Arquitectura del motor 2D (`engine2d`)

Este documento fija el contrato tecnico actual del motor 2D para que el codigo no siga siendo la unica fuente de verdad implicita. Las tareas de producto pendientes no viven en un checklist aparte: estan marcadas en el codigo (`pendiente`, `not ported`) y en las secciones `status: pendiente` de `docs/`.

## Relacion con `engine_3d`

- `rer_engine_2d` y `rer_engine_3d` son **binarios distintos** con runtimes distintos
  (features `engine2d` / `engine3d` del paquete `rer-engine`).
- Lo comun con Electron es el **canal** IPC (`engine:cmd` / `engine:event`) y `rer-engine-core`, no un enum de comandos unico ni la logica de juego.
- Contrato por motor: `src/engine2d/ipc.rs`; validacion en main (`engineCommandCatalog.ts`). Ver `docs/IPC_Protocol.yaml`.
- **No se copia runtime entre motores**: lo que el producto 3D necesitaba del stack 2D ya se extrajo; nuevas funciones 2D se implementan solo aqui (`config_2d/`, `physics_2d.rs`, `engine/`).
- Este documento no describe ni prescribe comportamiento del motor 3D.

## Politica GPU

- Perfil **TwoD**: siempre **Vulkan** mediante Ash (`rer_engine_core::vulkan::SharedGpu`). El crate no depende de wgpu.
- Núcleo compartido: `rer_engine_core::vulkan` (instancia, device, swapchain, `FrameTarget`, barreras, `GpuBuffer`/`GpuImage`); el 2D añade su renderer en `src/engine2d/vulkan/` (scene target + atlas + pipelines).
- **No** se usa OpenGL, EGL ni selección de backend por entorno. El 2D pide `VulkanOptions::default()` (sin ray tracing).
- Inicializacion: `engine2d::engine::State::new` → `engine2d::vulkan::Vulkan2d::new(window)`; fallo → `EngineEvent::Error` (sin `panic`, sin `ready`).
- **Una sola convención de Y** en todo el repo: viewport con altura negativa (y = `extent.height`, height = `-extent.height`); el blit del scene target muestrea `vec2(v_uv.x, 1.0 - v_uv.y)`.
- Electron elimina `RER_GPU_BACKEND` del entorno al hacer `spawn` del motor.

## Ventana única (editor)

- En el editor, el 2D se hospeda in-process sobre el `SharedGpu` de la ventana
  compartida: no crea instancia, device ni swapchain propios.
- `State::with_vulkan` + `Vulkan2d::with_gpu` reutilizan el contexto del editor;
  `render_into` graba en el sub-rect del viewport y deja la imagen para egui.
- En modo embebido `resize`/`set_bounds`/`set_vsync` no tocan la ventana ni el swapchain
  del editor; el tamaño del scene target lo fija `set_viewport_size`.
- `RER_LEGACY_2D_WINDOW=1` recupera el modo proceso con ventana overlay.

## Ventana overlay (integracion con Electron)

- Arranque: `--overlay <parent_id> <x> <y> <w> <h> [rel_x rel_y]` (alias legacy `--embed`). Ver `rer_engine_core::overlay`.
- **No** hay reparent (`with_embed_parent_window` eliminado): ventana hermana alineada por coordenadas de pantalla.
- Sincronizacion: `rer_engine_core::platform` (position-tracker Win32).
- IPC `set_bounds` redimensiona y actualiza offset del tracker.

## Archivos fuente de verdad

El runtime real 2D vive principalmente en estos archivos (rutas relativas a `engine/`):

- `core/src/vulkan/`: núcleo Ash compartido (contexto, swapchain, buffers/imágenes, barreras).
- `src/engine2d/vulkan/`: renderer Ash del 2D (escena offscreen, blit, atlas, rejilla, overlays y HUD).
- `src/engine2d/texture.rs`: shelf packing del atlas de sprites + subida a la imagen Vulkan.
- `src/engine2d/engine/mod.rs`: estado central del runtime, caches, undo/redo, scripting y flags del editor.
- `src/engine2d/engine/commands.rs`: frontera IPC y mutaciones principales del estado.
- `src/engine2d/physics_2d.rs`: backend fisico 2D sobre Rapier, restringido al plano `XY`.
- `src/engine2d/config_2d.rs`: utilidades y reglas de runtime/editor 2D (animacion, areas de ejecucion, overlays, helpers de personaje).
- `src/engine2d/config_2d/assets.rs`: inicializacion de escena 2D y carga de sprites/fondos/blueprints.
- `src/engine2d/config_2d/world_xy.rs`: contrato mundo XY vs pantalla; `screen_pixel_to_world_xy` para input del editor.

## Que NO es runtime 2D real

Los modulos bajo `src/engine2d/config_compat/` son shims minimos **dentro de este modulo** (`camera` orbita legacy, `mesh` ground plane). La fisica 2D vive solo en `physics_2d.rs` / `PhysicsWorld2D` — no queda shim de fisica en `config_compat`.

## Persistencia de escena (`.save`)

| Direccion | JSON | Rol |
|-----------|------|-----|
| Electron → motor | `export_save_snapshot` | Serializa mundo, entidades (scenario/character/collider/execution_area), jugador `[Player]`, camara 2D, almacenes de assets, animaciones y scripts. |
| Motor → Electron | `save_snapshot_ready` | Payload `scene` listo para el ZIP (main escribe el `.save`). |

Registro: `entity_save_meta` + inferencia desde `ScenarioMarker` / `CharacterMarker`. El renderer solo fusiona escenas inactivas, blueprints e idioma. Ver `src/renderer/ARCHITECTURE.md`.

## Plantilla por defecto (proyecto nuevo)

- Sin `.save` / `extract_dir`: `set_scene` 2D carga la demo embebida (`include_bytes!` de `assets/DEMO_2d.save` → extract a `%TEMP%` → `load_proyect_from_save_path`).
- No hay archivo `.save` externo en runtime ni en el empaquetado Electron; el ZIP vive en el binario `rer_engine_2d`.
- Fallback: si la extracción falla, `emit_default_2d_boot_loaded` (escena vacía Scene-01).

## Eventos de entidades

- `entity_removed` incluye `points` opcional para colisionadores y áreas de ejecución (cuadrilátero en espacio mundo), para sincronizar meta del editor y undo/redo.

## Scripting

- `update_scripts()` (`script.update`) solo corre con `preview_playing`. Los control scripts (`on_press` / `on_keep`) ya estaban acotados a play.
- Callbacks Rhai usan **invoke pattern** (`run_ast_with_scope` + `update!(entity, dt);`, etc.) para que `engine` del preámbulo sea visible dentro de `fn update` / `on_trigger_enter` / controles.
- **2D controles:** `engine.move_control(id, speed)` — el motor guarda la tecla del binding en `ScriptApiContext.control_binding_key`; no comparar `control_key` en el script.
- **Triggers:** `on_trigger_enter(trigger, actor)` en execution areas.
- Referencia API: [`docs/RHAI_API.yaml`](../../../../docs/RHAI_API.yaml). Implementación: `core/src/scripting/`.

## Atlas de texturas

- Si el atlas 4096×4096 no puede empacar una imagen, el motor emite `atlas_exhausted` (una vez hasta `reset` del atlas) y el front lo muestra en la consola de log.

## Contratos operativos que hoy deben respetarse

- El movimiento normal de gameplay debe pasar por `move_physics_entity()` o por las rutas kinematic del runtime.
- `teleport_entity()` solo debe usarse para teletransportes reales o para resincronizar el cuerpo fisico despues de una mutacion externa del `Transform`.
- **Centro visual** (`Transform.position + visual_offsets`): render, spatial grid, picking, hover y overlap de execution areas con el actor. Ver `config_2d/world_xy.rs`.
- **Body Rapier** sigue en `Transform.position` (pivot); la física no sigue el offset de dibujo por frame.

## Restore post-carga

- `apply_entity_restore`: transform, física, animaciones, scripts y bindings en un solo comando IPC (undo y casos puntuales).
- `import_scene`: reset 2D + mundo + fondo + entidades con ids del `.save` y restores en un solo IPC; evento `scene_imported` al terminar.
- Undo/redo de entidades: snapshot en `undo_entity.rs` (`RemoveEntity` / `RestoreEntity`) para escenario, personaje, colisionador y execution area.
