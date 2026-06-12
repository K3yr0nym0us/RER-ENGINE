# Arquitectura actual de `engine_3d`

Este documento fija el contrato tecnico actual del motor 3D para que el codigo no siga siendo la unica fuente de verdad implicita. Las tareas de producto pendientes están en [`CHECKLIST-3D.md`](../../../../CHECKLIST-3D.md).

## Relacion con `engine_2d`

- `rer_engine_3d` y `rer_engine_2d` son **binarios distintos** con runtimes distintos.
- Lo comun con Electron es el **protocolo IPC** (JSON por stdin/stdout) y crates de utilidades (`engine_shared`), no la logica de juego ni de editor.
- **No se copia runtime entre motores**: la fase de reutilizar ideas del 2D en el 3D ya cerró; el 3D funciona con su propia pila (`config_3d/`, Rapier3D, primera persona, glTF/GLB). Nuevas funciones 3D se implementan solo aqui.
- Herramientas de editor 2D (colliders dibujados, execution areas, escenarios sprite, fisica XY, etc.) **no aplican** a este binario y no deben documentarse como deuda de portado.
- Este documento no describe ni prescribe comportamiento del motor 2D.

Runtime 3D: camara orbital en editor, primera persona en play, Rapier3D, mallas glTF/GLB.

## Politica GPU

- Perfil **ThreeD** via `rer_engine_shared::gpu`:
  - **Windows y Linux**: Vulkan (`Backends::VULKAN`, `EngineGpuProfile::ThreeD`).
  - Sin variables de entorno: `resolve_backend(ThreeD)` no lee `RER_GPU_BACKEND`.
- Sin OpenGL ni otros backends wgpu. Shaders en **WGSL** (naga).
- Fallo de `init_gpu` → `EngineEvent::Error`; Electron muestra overlay de ayuda.

## Ventana overlay

- Identico al 2D: `--overlay`, ventana winit separada, tracker en `engine_shared::platform`.
- Ver `engine_2d/ARCHITECTURE.md` (seccion overlay) para detalle de plataformas.

## Archivos fuente de verdad

- `engine_shared/src/gpu.rs`: `resolve_backend(ThreeD)` y `init_gpu(_, ThreeD)`.
- `src/main.rs`: bucle winit, input, gizmo, play FP, setup overlay.
- `src/engine.rs` + `src/engine/mod.rs`: `State` (GPU, ECS, caches, undo/redo, scripting).
- `src/engine/init.rs`: instancia wgpu (Vulkan), pipelines WGSL, texture array, HUD/gizmo, TAA.
- `src/engine/commands.rs`: IPC y mutaciones de estado.
- `src/engine/render.rs`: mundo 3D, crosshair, tooltip Esc, gizmo de editor.
- `src/engine/tick.rs`: delta time, metricas, fade del hint Esc.
- `src/config_3d/mod.rs`: picking, raycast, gizmo, modelos, pantalla.
- `src/config_3d/camera_3d.rs`: camara orbital y uniforms.
- `src/config_3d/character_anchor.rs`: pies ↔ centro del personaje jugable.
- `src/config_3d/play_character.rs`: entidad `[Player]`, spawn y cuerpo placeholder.
- `src/config_3d/fps_camera.rs`: vista FPS acoplada, IPC `play_character_view_changed`.
- `src/config_3d/play_controller.rs`: movimiento en play (cápsula cinemática).
- `src/config_3d/player_ui/`: editor HUD del jugador (pantallas, texto, botones, imágenes, objetos poligonales, undo).
- `src/config_3d/mesh_3d.rs`: glTF/GLB, normalizacion, `forward_xz`.
- `src/config_3d/physics_3d.rs`: Rapier3D y shape cast del jugador.
- `src/config_3d/plane_tools.rs`: herramientas de muro invisible (colisionador) y trigger 3D (quad delgado, ghost, Q/E, colocación).
- `src/config_3d/execution_areas_3d.rs`: detección de triggers en play (OBB + cápsula jugador, `on_trigger_enter`, IPC).
- `src/config_3d/world_bounds.rs`: limites y culling AABB.
- `src/config_base.rs`: `setup_default_3d_scene`, `reset_runtime_scene_3d`.
- `src/ecs.rs`: `Transform`, `MeshComponent`, marcadores.
- `src/ipc.rs`: comandos y eventos JSON.

Soporte: `mesh.rs`, `shader.wgsl`, `gizmo.rs`, `gizmo.wgsl`, `texture.rs`, `scripting.rs`.

## Que NO es runtime 3D real

`src/config_compat/` solo cumple variantes del **enum IPC compartido** que este binario no ejecuta: stubs, vacios o `warn`. No es un submotor 2D ni un lugar para pegar logica copiada de `engine_2d`.

- Implementacion 3D nueva → `config_3d/` o `engine/`.
- No ampliar `config_compat` con comportamiento de gameplay o editor.

`src/config_shared.rs` reexporta utilidades de `engine_shared`.

## Modos de camara y render

- **Editor 3D**: solo `Camera` orbital + viewport desacoplado del jugador; gizmo y frustum FP con `preview_playing == false`. No hay modo cámara 2D en este binario.
- **Play FP**: `preview_playing`, vista desde acordeón Cámara; mesh del jugador visible; capsula cinematica en `play_controller.rs`.
- **HUD play** (crosshair, Esc): NDC + `hud_scene_bind_group` (identidad). PNG de pantalla **solo** vía `screen_hud_image` + `screen_hud_pipeline` (no `TextureArray`).
- **HUD Player UI** (autoría + play): pantallas con elementos en NDC (`config_3d/player_ui/`); en play se muestra la pantalla activa; en edición UI la cámara del jugador sin `preview_playing`.

## Contratos operativos (3D)

### Personaje jugable (`[Player]`)

- `Transform` = centro del cuerpo; pies via `PLAY_CHARACTER_BODY_HEIGHT`.
- **Editor 3D (sin play)**: viewport orbital en `editor_orbit_target` + `editor_viewport_*`; `SetTransform` del jugador **no** mueve ese viewport (`body_rotation_only` / `apply_play_character_transform_editor`). Rotación o escala recalculan centro desde pies fijos.
- **Play FP**: `camera.target` = pies; cuerpo y cámara acoplados vía `set_play_character_feet_position` / mouse look.
- **Carga `.save` FP** (`restoring_save_manifest`): mesh sin escala 1.7 m; `place_play_character_at_world_feet` usa bounds skinned (bind pose) y la misma fórmula que `align_*` (`position = pies − feet_world_offset`); cápsula/cámara usan pies del manifest; `player.position` en save = pies.
- `replace_entity_model` (editor / cambio de modelo en vivo): actualizar `play_character_mesh_forward_xz`, escala 1.7 m, `sync_player_rotation_from_look()`. Con `restoring_save_manifest` el jugador omite ese pipeline.
- Yaw: mantener alineadas `look_xz_from_mesh_yaw` y `mesh_yaw_from_camera_and_forward` con `glam::Quat::from_rotation_y`.

#### Vista FP autoritativa (IPC)

El frontend **no** debe derivar centro de cuerpo, quaterniones ni forward de malla. Solo envia intencion y refleja lo que el motor confirma.

| Direccion | JSON (`snake_case`) | Responsabilidad |
|-----------|---------------------|-----------------|
| Electron → motor | `set_play_character_view` | Pies, `yaw`, `pitch`; opcional `fov_y`, `frustum_distance`. Aplica camara + cuerpo y emite evento. |
| Motor → Electron | `play_character_view_changed` | Estado confirmado: pies, orientacion de camara, FOV/frustum, `body_center`, `body_rotation`, `body_scale`, `player_id`. |

Aliases legacy (misma semantica): `set_first_person_view`, `set_first_person_spawn`, `first_person_view_changed`, `set_fp_editor_frustum_distance`.

Implementacion: `apply_play_character_view()` / `emit_play_character_view_changed()` en `config_3d/fps_camera.rs`; handler en `engine/commands.rs`.

El motor emite `play_character_view_changed` tras, entre otros:

- `set_play_character_view` (y spawn legacy),
- `set_transform` del personaje jugable,
- salir de play (`set_preview_playing` false),
- `set_camera_fov` / `set_fps_editor_frustum_distance` con personaje activo,
- `replace_entity_model` del jugador.

El renderer **no** arma `playerTransform` del `.save` desde refs locales: lo incluye el snapshot (`export_save_snapshot`). La vista en editor sigue `play_character_view_changed` (`position` = pies). Ver `src/renderer/ARCHITECTURE.md`.

#### Persistencia de escena (`.save`)

| Direccion | JSON | Rol |
|-----------|------|-----|
| Electron → motor | `export_save_snapshot` | El motor serializa mundo, entidades, jugador FP, camara, almacenes de assets y scripts. |
| Motor → Electron | `save_snapshot_ready` | Payload `scene` listo para empaquetar en ZIP (main sigue escribiendo el `.save`). |

Registro de rutas/tipos: `entity_save_meta` + actualizacion en spawn/load/replace. El front solo fusiona escenas inactivas, blueprints, idioma y `blueprint_id` por entidad.

### Scripting

- `update_scripts()` solo ejecuta el tick Rhai de entidades cuando `preview_playing` es true. Los control scripts (`on_press` / `on_keep`) ya estaban acotados a play.
- Callbacks Rhai usan **invoke pattern** en `engine_shared/src/scripting/script_engine.rs` (mismo contrato que 2D).
- **Triggers 3D:** `on_trigger_enter(trigger, actor)` en execution areas planas; detección en `execution_areas_3d.rs`.
- **FP control scripts:** tras ejecutar el script del binding, `engine/scripts.rs` auto-inyecta `fp_press_key` con la tecla del binding para el play character cuando el script no lo hace. Scripts típicos: solo `fp_set_walk_speed`, `fp_set_sprint_multiplier`, `fp_jump`.
- Referencia: [`docs/RHAI_API.yaml`](../../../../docs/RHAI_API.yaml).

### Fisica

- Objetos: Rapier3D (`set_entity_physics`, sync con `Transform` al editar y al usar gizmo).
- Muros invisibles 3D (`[Colisionador]`): caja Rapier orientada al yaw del plano (`set_entity_physics_oriented`, `sync_plane_wall_physics`); colisión en play, ocultos salvo `debug_mode`.
- Jugador en play: shape cast (`move_character_capsule_at_feet`); sin rigid body Rapier en el mesh visual del player.
- En play, `physics.step` sincroniza cuerpos dynamic con ECS; el id del jugador FP se excluye de ese sync.

### Herramientas plano 3D (colisionador / trigger)

Herramientas del acordeón **Tools** en proyectos 3D (no son el dibujo por puntos del 2D).

| Aspecto | Comportamiento |
|---------|----------------|
| Toggle | `set_active_tool` → ghost semitransparente en el motor; click en viewport coloca **una** vez y desactiva la herramienta. |
| Rotación | **Q / E** en el motor (polling OS + swallow winit); yaw persistido en `Transform.rotation`. |
| Colisionador | Marcador `[Colisionador]`; Rapier static orientado; invisible en play (salvo debug). |
| Trigger | Marcador `[ExecutionArea]`; sin física; invisible en play; scripts/nodos vía Rhai. |
| Foco | Al activar herramienta → foco ventana overlay; al colocar → `focus_overlay_parent_window` + `mainWindow.focus()` en Electron. |
| `.save` | Posición, escala y **rotación** (quaternion) en snapshot; restauración vía `restore_*_plane_from_save` + `apply_entity_restore`. |
| Render | `render_kind = 0.25`: iluminación normal, sin recibir ni proyectar sombras (excluidos del shadow pass). |
| Play | `update_execution_areas_3d` en tick; eventos `trigger_entered` / `trigger_exited`; panel front `[trigger]` con `has_attached_script`. |

Implementación front: `usePlaneToolPlacement`, `PlaneToolContext`, botones `ColliderToolButton` / `ExecutionAreaToolButton`.

### IPC

- Comandos del protocolo compartido que solo aplican al binario 2D: stub o `warn` en este crate (`config_compat`).
- Proyectos 3D en el frontend no deben depender de variantes solo 2D.

### Assets

- Hints HUD: `../assets/tooltip-btn-esc-*.png`; `snap_locale` EN/ES.

### Render HUD

- Overlays en play (crosshair, tooltip Esc) usan `hud_quad_mesh`, `screen_hud_atlas` y `screen_hud_pipeline`; no reutilizar `texture_array` ni el mesh del suelo para PNG de UI.

### Player UI HUD (editor y play)

Sistema de **pantallas HUD** para el jugador en proyectos 3D FP. Clave de almacenamiento `scope:screen_id` (p. ej. `player:<uuid>`). El renderer edita vía acordeón **UI del jugador**; el motor es autoritativo en geometría NDC, capas y persistencia.

| Modo | Comportamiento |
|------|----------------|
| **Edición UI** | `set_player_ui_edit_mode` activo: vista FPS del jugador, cuadrícula NDC, picking solo sobre HUD (el mundo 3D no recibe hover/selección). |
| **Play** | `preview_playing` + pantalla activa (`sync_player_ui_screens` / `set_active_player_ui_screen`): dibuja texto, botones, imágenes y relleno de polígonos. |

**Tipos de elemento** (orden por `z_index` en `hud_layers.rs`):

| Tipo | Módulo | Notas |
|------|--------|--------|
| Texto | `text_input.rs`, `text_render.rs` | Cajas con fuente TTF; doble clic para editar inline. |
| Botón | `button.rs`, `button_render.rs` | Forma, color, textura HUD opcional. |
| Imagen | `image.rs`, `image_render.rs` | Asset de biblioteca HUD (`load_hud_image`). |
| Objeto | `object.rs` | Polígono por clicks (≥3 vértices); cerrar clicando cerca del primer punto; preview con cruz en cursor y segmentos; relleno en play y edición. |

**Dibujo de objetos:** `set_player_ui_object_draw` → clicks en viewport → `player_ui_object_added` / `player_ui_object_draw_ended`; overlay con `gizmo_pipeline` (`draw_player_ui_object_draw_overlay`). Ctrl+grid: snap al colocar y al mover.

**Undo/redo (Ctrl+Z / Ctrl+Y):** snapshots de la pantalla en edición (`hud_undo.rs`, `UndoAction::RestorePlayerUiHud`). Se registra antes de cada punto de polígono, alta/baja de elementos, props (`set_player_ui_hud_element_props`) e inicio de arrastre/resize.

**IPC relevante (ver `ipc.rs`):**

| Comando | Rol |
|---------|-----|
| `set_player_ui_edit_mode` | Entrar/salir edición de una pantalla (`scope`, `screen_id`). |
| `add_player_ui_text_box` / `remove_player_ui_text_box` | Texto. |
| `add_player_ui_button` / `remove_player_ui_button` | Botón. |
| `add_player_ui_image` / `remove_player_ui_image` | Imagen. |
| `set_player_ui_object_draw` / `remove_player_ui_object` | Dibujo y borrado de objetos. |
| `set_player_ui_hud_element_props` | `locked`, `z_index` (`text` \| `button` \| `image` \| `object`). |
| `sync_player_ui_screens` / `set_active_player_ui_screen` | Catálogo y pantalla activa en play. |

Lista unificada al editor: evento `player_ui_text_boxes_list` (texto, botones, imágenes y objetos de la pantalla en edición).

**Persistencia (`.save`):** `export_save_snapshot` incluye `player_ui_text_boxes`, `player_ui_buttons`, `player_ui_images`, `player_ui_objects` y `playerUiScreens`; restauración en `load_proyect.rs`.

**Scripting:** `set_active_player_ui`, `set_active_player_ui_by_name`, `clear_active_player_ui` (`scripting.rs`).
