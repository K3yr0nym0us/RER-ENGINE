# Arquitectura actual de `engine_3d`

Este documento fija el contrato tecnico actual del motor 3D para que el codigo no siga siendo la unica fuente de verdad implicita.

## Relacion con `engine_2d`

- `rer_engine_3d` y `rer_engine_2d` son **binarios distintos** con runtimes distintos.
- Lo comun con Electron es el **protocolo IPC** (JSON por stdin/stdout) y crates de utilidades (`engine_shared`), no la logica de juego ni de editor.
- **No se copia runtime entre motores**: la fase de reutilizar ideas del 2D en el 3D ya cerró; el 3D funciona con su propia pila (`config_3d/`, Rapier3D, primera persona, glTF/FBX). Nuevas funciones 3D se implementan solo aqui.
- Herramientas de editor 2D (colliders dibujados, execution areas, escenarios sprite, fisica XY, etc.) **no aplican** a este binario y no deben documentarse como deuda de portado.
- Este documento no describe ni prescribe comportamiento del motor 2D.

Runtime 3D: camara orbital en editor, primera persona en play, Rapier3D, mallas glTF/FBX.

## Archivos fuente de verdad

- `src/main.rs`: bucle winit, input, gizmo, play FP.
- `src/engine.rs` + `src/engine/mod.rs`: `State` (GPU, ECS, caches, undo/redo, scripting).
- `src/engine/init.rs`: WGPU, pipelines, atlas, HUD/gizmo.
- `src/engine/commands.rs`: IPC y mutaciones de estado.
- `src/engine/render.rs`: mundo 3D, crosshair, tooltip Esc, gizmo de editor.
- `src/engine/tick.rs`: delta time, metricas, fade del hint Esc.
- `src/config_3d/mod.rs`: picking, raycast, gizmo, modelos, pantalla.
- `src/config_3d/camera_3d.rs`: camara orbital y uniforms.
- `src/config_3d/first_person.rs`: controller FP, pies/centro, sync cuerpo-camara.
- `src/config_3d/mesh_3d.rs`: glTF/FBX, normalizacion, `forward_xz`.
- `src/config_3d/physics_3d.rs`: Rapier3D y shape cast del jugador.
- `src/config_3d/world_bounds.rs`: limites y culling AABB.
- `src/config_base.rs`: `setup_first_person`, `reset_runtime_scene_3d`, spawn `[Player]`.
- `src/ecs.rs`: `Transform`, `MeshComponent`, marcadores.
- `src/ipc.rs`: comandos y eventos JSON.

Soporte: `mesh.rs`, `shader.wgsl`, `gizmo.rs`, `gizmo.wgsl`, `texture.rs`, `scripting.rs`.

## Que NO es runtime 3D real

`src/config_compat/` solo cumple variantes del **enum IPC compartido** que este binario no ejecuta: stubs, vacios o `warn`. No es un submotor 2D ni un lugar para pegar logica copiada de `engine_2d`.

- Implementacion 3D nueva → `config_3d/` o `engine/`.
- No ampliar `config_compat` con comportamiento de gameplay o editor.

`src/config_shared.rs` reexporta utilidades de `engine_shared`.

## Modos de camara y render

- **Editor 3D**: `camera_2d` es `None`; `Camera` orbital; gizmo y frustum FP con `preview_playing == false`.
- **Play FP**: `preview_playing`, vista FPS, mesh del jugador oculto; capsula cinematica en `first_person.rs`.
- **HUD** (crosshair, Esc): NDC + `hud_scene_bind_group` (identidad). No mezclar ese uniform con `scene_bind_group`.

## Contratos operativos (3D)

### Jugador primera persona

- `Transform` = centro del cuerpo; pies via `FIRST_PERSON_BODY_HEIGHT`; `camera.target` = pies.
- `replace_entity_model`: actualizar `first_person_mesh_forward_xz`, escala 1.7 m, `sync_player_rotation_from_look()`.
- `SetTransform` del jugador con rotacion: recalcular centro desde pies (`feet_from_player_transform` / `player_center_from_feet` en `commands.rs`).
- Yaw: mantener alineadas `look_xz_from_mesh_yaw` y `mesh_yaw_from_camera_and_forward` con `glam::Quat::from_rotation_y`.

### Fisica

- Objetos: Rapier3D (`set_entity_physics`, sync con `Transform` al editar).
- Jugador en play: shape cast; sin rigid body Rapier en el mesh visual del player.

### IPC

- Comandos que el protocolo define solo para 2D: no-op o warn; no son “pendientes de implementar en 3D”.
- Proyectos 3D en el frontend deben evitar emitirlos.

### Assets

- Hints HUD: `../assets/tooltip-btn-esc-*.png`; `snap_locale` EN/ES.

## Deuda tecnica (solo 3D)

- `canonical_quad_idx` / `hud_quad_mesh`: no usar `meshes[0]` (suelo) para overlays HUD futuros.
- Coherencia render / picking / fisica 3D respecto a `Transform` en el editor.
