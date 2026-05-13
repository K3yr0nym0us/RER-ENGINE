# Arquitectura actual de `engine_2d`

Este documento fija el contrato tecnico actual del motor 2D para que el codigo no siga siendo la unica fuente de verdad implícita.

## Archivos fuente de verdad

El runtime real 2D vive principalmente en estos archivos:

- `src/engine/mod.rs`: estado central del runtime, caches, undo/redo, scripting y flags del editor.
- `src/engine/commands.rs`: frontera IPC y mutaciones principales del estado.
- `src/physics_2d.rs`: backend fisico 2D sobre Rapier, restringido al plano `XY`.
- `src/config_2d.rs`: utilidades y reglas de runtime/editor 2D (animacion, areas de ejecucion, overlays, helpers de personaje).
- `src/config_2d/assets.rs`: inicializacion de escena 2D y carga de sprites/fondos/blueprints.

## Que NO es runtime 2D real

Los modulos bajo `src/config_compat/` son shims de compatibilidad.

- `src/config_compat/physics.rs` no implementa la simulacion 2D real; solo conserva una API minima para rutas heredadas.
- Cualquier cambio de comportamiento de fisica 2D debe nacer en `src/physics_2d.rs`, no en `config_compat`.

## Contratos operativos que hoy deben respetarse

- El movimiento normal de gameplay debe pasar por `move_physics_entity()` o por las rutas kinematic del runtime.
- `teleport_entity()` solo debe usarse para teletransportes reales o para resincronizar el cuerpo fisico despues de una mutacion externa del `Transform`.
- `visual_offsets` afecta render/animacion, pero no es hoy la fuente de verdad universal de picking, triggers o fisica.
- Las `execution areas` siguen usando overlap AABB basado en `Transform`; alinearlas con fisica o con offsets visuales seria un cambio de comportamiento y debe tratarse aparte.

## Deuda tecnica separada de esta fase

Estos puntos ya estan identificados, pero no deben corregirse a ciegas en una pasada orientada a preservar comportamiento:

- Unificar la referencia espacial entre render, picking, triggers y fisica.
- Revisar la semantica de `SetGravity`, `apply_kinematic_gravity()` y la traduccion especial de `on_press` para cuerpos kinematic.
- Reducir o eliminar shims de `config_compat` cuando el runtime heredado deje de necesitarlos.
