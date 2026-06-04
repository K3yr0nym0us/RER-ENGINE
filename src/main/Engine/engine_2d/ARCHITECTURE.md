# Arquitectura actual de `engine_2d`

Este documento fija el contrato tecnico actual del motor 2D para que el codigo no siga siendo la unica fuente de verdad implicita. Las tareas de producto pendientes están en [`CHECKLIST-2D.md`](../../../../CHECKLIST-2D.md).

## Relacion con `engine_3d`

- `rer_engine_2d` y `rer_engine_3d` son **binarios distintos** con runtimes distintos.
- Lo comun con Electron es el **protocolo IPC** (JSON por stdin/stdout) y crates de utilidades (`engine_shared`), no la logica de juego ni de editor.
- **No se copia runtime entre motores**: lo que el producto 3D necesitaba del stack 2D ya se extrajo; nuevas funciones 2D se implementan solo aqui (`config_2d/`, `physics_2d.rs`, `engine/`).
- Este documento no describe ni prescribe comportamiento del motor 3D.

## Politica GPU

- Perfil **TwoD**: siempre **Vulkan** (`EngineGpuProfile::TwoD`, `Backends::VULKAN`).
- Crate `wgpu`: `default-features = false`, `features = ["wgsl"]`; el 2D solicita Vulkan en runtime.
- **No** se usa OpenGL, EGL ni `Backends::all()`. No hay ramas de render por backend GL en pipelines.
- Inicializacion: `engine::State::new` → `init_gpu(_, TwoD)`; fallo → `EngineEvent::Error` (sin `panic`, sin `ready`).
- No se leen variables de entorno para elegir backend (política fija por perfil `TwoD`).
- Electron elimina `RER_GPU_BACKEND` del entorno al hacer `spawn` del motor.

## Ventana overlay (integracion con Electron)

- Arranque: `--overlay <parent_id> <x> <y> <w> <h> [rel_x rel_y]` (alias legacy `--embed`). Ver `engine_shared::overlay`.
- **No** hay reparent X11 (`with_embed_parent_window` eliminado): ventana hermana alineada por coordenadas de pantalla.
- Sincronizacion: `engine_shared::platform` (position-tracker Win32 / X11).
- IPC `set_bounds` redimensiona y actualiza offset del tracker.

## Archivos fuente de verdad

El runtime real 2D vive principalmente en estos archivos:

- `engine_shared/src/gpu.rs`: `resolve_backend(TwoD)` y `init_gpu(_, TwoD)`.
- `src/engine/mod.rs`: estado central del runtime, caches, undo/redo, scripting y flags del editor.
- `src/engine/commands.rs`: frontera IPC y mutaciones principales del estado.
- `src/physics_2d.rs`: backend fisico 2D sobre Rapier, restringido al plano `XY`.
- `src/config_2d.rs`: utilidades y reglas de runtime/editor 2D (animacion, areas de ejecucion, overlays, helpers de personaje).
- `src/config_2d/assets.rs`: inicializacion de escena 2D y carga de sprites/fondos/blueprints.
- `src/config_2d/world_xy.rs`: contrato mundo XY vs pantalla; `screen_pixel_to_world_xy` para input del editor.

## Que NO es runtime 2D real

Los modulos bajo `src/config_compat/` son shims minimos **dentro de este crate** (`camera` orbita legacy, `mesh` ground plane). La fisica 2D vive solo en `physics_2d.rs` / `PhysicsWorld2D` — no queda shim de fisica en `config_compat`.

## Persistencia de escena (`.save`)

| Direccion | JSON | Rol |
|-----------|------|-----|
| Electron → motor | `export_save_snapshot` | Serializa mundo, entidades (scenario/character/collider/execution_area), jugador `[Player]`, camara 2D, almacenes de assets, animaciones y scripts. |
| Motor → Electron | `save_snapshot_ready` | Payload `scene` listo para el ZIP (main escribe el `.save`). |

Registro: `entity_save_meta` + inferencia desde `ScenarioMarker` / `CharacterMarker`. El renderer solo fusiona escenas inactivas, blueprints e idioma. Ver `src/renderer/ARCHITECTURE.md`.

## Eventos de entidades

- `entity_removed` incluye `points` opcional para colisionadores y áreas de ejecución (cuadrilátero en espacio mundo), para sincronizar meta del editor y undo/redo.

## Scripting

- `update_scripts()` (`script.update`) solo corre con `preview_playing`. Los control scripts (`on_press` / `on_keep`) ya estaban acotados a play.

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
