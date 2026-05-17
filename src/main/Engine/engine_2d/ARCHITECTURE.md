# Arquitectura actual de `engine_2d`

Este documento fija el contrato tecnico actual del motor 2D para que el codigo no siga siendo la unica fuente de verdad implicita.

## Relacion con `engine_3d`

- `rer_engine_2d` y `rer_engine_3d` son **binarios distintos** con runtimes distintos.
- Lo comun con Electron es el **protocolo IPC** (JSON por stdin/stdout) y crates de utilidades (`engine_shared`), no la logica de juego ni de editor.
- **No se copia runtime entre motores**: lo que el producto 3D necesitaba del stack 2D ya se extrajo; nuevas funciones 2D se implementan solo aqui (`config_2d/`, `physics_2d.rs`, `engine/`).
- Este documento no describe ni prescribe comportamiento del motor 3D.

## Archivos fuente de verdad

El runtime real 2D vive principalmente en estos archivos:

- `src/engine/mod.rs`: estado central del runtime, caches, undo/redo, scripting y flags del editor.
- `src/engine/commands.rs`: frontera IPC y mutaciones principales del estado.
- `src/physics_2d.rs`: backend fisico 2D sobre Rapier, restringido al plano `XY`.
- `src/config_2d.rs`: utilidades y reglas de runtime/editor 2D (animacion, areas de ejecucion, overlays, helpers de personaje).
- `src/config_2d/assets.rs`: inicializacion de escena 2D y carga de sprites/fondos/blueprints.

## Que NO es runtime 2D real

Los modulos bajo `src/config_compat/` son shims de compatibilidad **dentro de este crate** (firmas heredadas, rutas que ya no deben crecer).

- `src/config_compat/physics.rs` no implementa la simulacion 2D real; solo conserva una API minima para rutas heredadas.
- Cualquier cambio de comportamiento de fisica 2D debe nacer en `src/physics_2d.rs`, no en `config_compat`.

## Persistencia de escena (`.save`)

| Direccion | JSON | Rol |
|-----------|------|-----|
| Electron → motor | `export_save_snapshot` | Serializa mundo, entidades (scenario/character/collider/execution_area), jugador `[Player]`, camara 2D, almacenes de assets, animaciones y scripts. |
| Motor → Electron | `save_snapshot_ready` | Payload `scene` listo para el ZIP (main escribe el `.save`). |

Registro: `entity_save_meta` + inferencia desde `ScenarioMarker` / `CharacterMarker`. El renderer solo fusiona pestañas inactivas, blueprints e idioma. Ver `src/renderer/ARCHITECTURE.md`.

## Contratos operativos que hoy deben respetarse

- El movimiento normal de gameplay debe pasar por `move_physics_entity()` o por las rutas kinematic del runtime.
- `teleport_entity()` solo debe usarse para teletransportes reales o para resincronizar el cuerpo fisico despues de una mutacion externa del `Transform`.
- `visual_offsets` afecta render/animacion, pero no es hoy la fuente de verdad universal de picking, triggers o fisica.
- Las `execution areas` usan overlap AABB basado en `Transform`; cambiar esa semantica es decision de producto 2D, no alineacion con el motor 3D.

## Deuda tecnica (solo 2D)

- Unificar la referencia espacial entre render, picking, triggers y fisica en este binario.
- Revisar la semantica de `SetGravity`, `apply_kinematic_gravity()` y la traduccion especial de `on_press` para cuerpos kinematic.
- Reducir shims de `config_compat` cuando dejen de usarse rutas heredadas en **este** crate.
