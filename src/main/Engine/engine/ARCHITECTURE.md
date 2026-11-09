# Arquitectura actual de `engine_3d`

Este documento fija el contrato tecnico actual del motor 3D para que el codigo no siga siendo la unica fuente de verdad implicita. Las tareas de producto pendientes no viven en un checklist aparte: estan marcadas en el codigo (`pendiente`, `not ported`) y en las secciones `status: pendiente` de `docs/`.

## Terminologia (modo 3D y camara)

- **Proyecto 3D**: tipo en manifest (`type: "3D"`). Al crearlo, el editor abre directo; no hay pantalla de «estilo de juego».
- **`gameStyle` en manifest**: **modo de cámara** 3D (`first-person`, `third-person`, …). El usuario lo elige en el acordeón **Camera** del renderer.
- **`first-person`**: valor de `gameStyle` para el tipo de cámara «vista desde los ojos» (opción del select Camera).
- **Play character / vista play character**: jugador `[Player]` + IPC `set_play_character_view` / `play_character_view_changed`.

## Relacion con `engine_2d`

- `rer_engine_3d` y `rer_engine_2d` son **binarios distintos** con runtimes distintos.
- Lo comun con Electron es el **canal** IPC (`engine:cmd` / `engine:event`) y crates de utilidades (`core`), no un enum de comandos unico ni la logica de juego.
- Contrato por motor: `engine/src/engine3d/ipc/`; validacion en main (`engineCommandCatalog.ts`). Ver `docs/IPC_Protocol.yaml`.
- **No se copia runtime entre motores**: la fase de reutilizar ideas del 2D en el 3D ya cerró; el 3D funciona con su propia pila (`config_3d/`, Rapier3D, cámara play character, glTF/GLB). Nuevas funciones 3D se implementan solo aqui.
- Herramientas de editor 2D (colliders dibujados, execution areas, escenarios sprite, fisica XY, etc.) **no aplican** a este binario y no deben documentarse como deuda de portado.
- Este documento no describe ni prescribe comportamiento del motor 2D.

Runtime 3D: camara orbital en editor, play con jugador `[Player]`, Rapier3D, mallas glTF/GLB. El tipo de cámara en play (`gameStyle`) se elige en el acordeón Camera.

## Politica GPU

- **2D y 3D**: Vulkan crudo con `ash`. El núcleo compartido (instancia, device, swapchain,
  `FrameTarget`, `GpuBuffer`/`GpuImage`) vive en `rer-engine-core::vulkan`.
- El crate 3D conserva su renderer en `src/engine3d/vulkan/` (G-buffer, MSAA, TAA, SSR, sondas y
  sombras) y `accel.rs` (BLAS/TLAS).
- Shaders: GLSL en `engine/shaders/` (`raster/`, `rt/`, `bvh/`) compilados a SPIR-V
  por `build.rs` con `glslc` del Vulkan SDK. Son la única fuente: los `.wgsl` heredados del
  motor wgpu se borraron, y las variantes de pantalla del 2D viven en `engine_2d/`.
- Ray tracing: se piden juntas `VK_KHR_ray_tracing_pipeline`,
  `VK_KHR_acceleration_structure` y `VK_KHR_deferred_host_operations`. Si falta alguna
  extensión o sus features, `hw_rt = false` y los reflejos usan el BVH por software
  (`src/engine3d/bvh.rs` + `shaders/bvh/trace.comp`).
- Fallo de inicializacion Vulkan → `EngineEvent::Error`; Electron muestra overlay de ayuda.

## Tiers de calidad gráfica (qué hace cada uno de verdad)

Cada tier se aplica en `vulkan::FrameRenderer`; los valores pedidos se recortan a lo que admite
el adaptador, y el recorte se registra en el log (nada de números que no afectan al render).

- **Sombras** (`set_shadow_tier`): resuelve el lado del shadow map (Low 1024, Medium 2048,
  High 4096, Ultra 8192; `Off` no dibuja el pase). `record()` reasigna solo esa imagen cuando
  cambia (`Targets::recreate_shadow`), recortado a `max_image_dimension2_d`, y si la asignación
  falla baja a la mitad en vez de abortar cada frame. El texel del PCF y el snapping de la
  matriz de luz salen del tamaño real (`active_shadow_map_size`), y el radio PCF escala con la
  raíz del tamaño para que la penumbra no se afinde al subir resolución; por encima de 2.5
  texeles el kernel pasa de 3×3 a 5×5. Sigue siendo un único mapa sobre los bounds del mundo:
  no hay cascadas (CSM). Se intentó y se retiró: la evidencia de qué quedó descartado está en
  `docs/Shadows_3D.yaml` (`csm_attempt`) para no volver a empezar de cero.
- **MSAA** (`set_msaa_tier`): Low 1× (por defecto, sin multisampling), Medium 2×, High 4×,
  Ultra 8×, recortado por `SharedGpu::max_gbuffer_samples` (intersección de
  `framebufferColorSampleCounts` y `framebufferDepthSampleCounts`). Con >1× se crean copias
  multisampleadas de los 5 adjuntos del G-buffer que se resuelven (`resolveMode = AVERAGE`)
  sobre las imágenes 1× que ya consumen TAA/SSR/present. El fondo trazado **no se copia** al
  adjunto multisampleado (`vkCmdCopyImage` exige las mismas muestras en origen y destino,
  VUID-vkCmdCopyImage-srcImage-00136): se dibuja un quad full-screen con `sky.frag`, sin prueba ni
  escritura de profundidad, que escribe las cinco salidas del G-buffer para que ningún adjunto
  quede sin definir. La profundidad multisampleada se descarta y `targets.depth` se rellena con un
  segundo pase a 1× con los pipelines de `depth_only.frag` (sin salidas de color, solo el `discard`
  del alfa cortado, para que la profundidad coincida con la imagen resuelta). Tras un resize hay que
  recrear el G-buffer MSAA (`ensure_ms`), porque `Targets::recreate` lo destruye y `ensure_msaa`
  saldría por su early-return. La captura de sondas sigue dibujando con los pipelines de 1×.
- **Reflejos** (`set_reflection_tier`): `Off` (por defecto) deja solo el forward+TAA; cualquier
  otro tier activa la cadena SSR → denoise → composite. El switch adicional de ray tracing
  (`set_reflection_raytracing`) hace que el trazado use `VK_KHR_ray_tracing_pipeline` si el
  device lo expone, o el BVH por software en caso contrario.
- **Sondas de reflejo**: un `texture_cube_array` con 6 caras por ranura
  (`probe_registry::MAX_PROBES` = 8, caras de `PROBE_FACE_SIZE` px). `capture_probe` escribe en
  la capa `slot * 6 + cara`, así varias sondas conviven (antes todas pisaban las caras 0..5 y
  la última del frame ganaba). Cada instancia lleva su ranura en `InstanceData.tex_layer_pad[2]`
  → `v_tex.z`, elegida en CPU por `probe_registry::select_probe_slot` según el radio de
  influencia (`scale.x * 0.5`); `-1` = fuera de todo volumen → el shader cae al cielo
  procedural. Se captura en burst cuando cambia el conjunto de sondas y luego una sonda cada
  ~30 frames.
- **Geometría trazable skinned**: las entidades animadas entran en **ambos** caminos, BVH por
  software y TLAS de hardware. Sus triángulos se deforman en CPU con el mismo palette que usa
  `skinned.vert` (`model * (Σ pesos·palette[joint]·posición)`) y se cachean, refrescándose cada
  `SKINNED_TRACE_REFRESH_FRAMES` (4) frames y con un tope de `SKINNED_TRACE_MAX_TRIANGLES`
  (24 000): rehacer el BVH cada frame con un personaje animado cuesta más de lo que un reflejo
  agradece. El hash de la escena cacheada incluye el tick del refresco.
  Para el TLAS se sube además un buffer de sólo posiciones ya poseadas (`create_trace_vertex_buffer`,
  12 bytes por vértice, mismo orden de vértices que el VBO skinned, así se reutiliza su índice) y
  `refresh_skinned_rt_geometry` arma una `RtBatch` por entidad y pieza, con clave
  `(gpu_mesh_idx) | (1 << 63)` para no colisionar con las mallas estáticas; el BLAS sólo se
  reconstruye en el frame en que cambió la pose. Las poses las calcula el mismo bucle que alimenta
  el BVH, así que HW y SW trazan exactamente la misma silueta.
- **Vista de debug** (`set_reflection_debug_view`): solo hay `final` y `ssr_debug`. `ssr_debug`
  presenta la imagen SSR/trazada en crudo como fuente del `present`, saltando el composite y el TAA,
  para ver qué devuelve el pase. Los nombres de la era wgpu (`trace_strength`, `probe_layers`,
  `metallic`) ya no existen: cayeron con `src/engine3d/reflections/`.

## Ventana overlay

- Identico al 2D: `--overlay`, ventana winit separada, tracker en `rer_engine_core::platform`.
- Ver `engine_2d/ARCHITECTURE.md` (seccion overlay) para detalle Win32.

## Archivos fuente de verdad

- `core/src/backend_policy.rs`: política de backend del motor (siempre Vulkan).
- `src/engine3d/host.rs`: bucle winit, input, gizmo, play (cámara play character), setup overlay.
- `src/engine3d/engine.rs` + `src/engine3d/engine/mod.rs`: `State` (GPU Ash, ECS, caches, undo/redo, scripting).
- `src/engine3d/engine/init.rs`: `SharedGpu::new` (Ash), texture array y `FrameRenderer::new`.
- `src/engine3d/vulkan/`: `FrameRenderer` (sombras, forward, TAA, SSR, denoise, reflejos RT/BVH,
  gizmo y HUD) y `accel.rs` (BLAS/TLAS). El contexto y la swapchain viven en
  `rer-engine-core::vulkan`.
- `src/engine3d/bvh.rs`: BVH binario en CPU y layouts GPU del fallback por software.
- `src/engine3d/engine/commands/`: IPC y mutaciones de estado, por dominio.
- `src/engine3d/engine/render.rs`: mundo 3D, batches estáticos y skinned, escena RT cacheada
  (instancias/materiales/BVH), overlays gizmo, HUD y `FrameInput`.
- `src/engine3d/engine/tick.rs`: delta time, metricas, fade del hint Esc.
- `src/engine3d/config_3d/mod.rs`: picking, raycast, gizmo, modelos, pantalla.
- `src/engine3d/config_3d/camera_3d.rs`: camara orbital y uniforms.
- `src/engine3d/config_3d/character_anchor.rs`: pies ↔ centro del personaje jugable.
- `src/engine3d/config_3d/play_character.rs`: entidad `[Player]`, spawn y cuerpo placeholder.
- `src/engine3d/config_3d/editor_viewport_controls.rs`: navegación orbital (esquema Blender), arrastre/gizmo.
- `src/engine3d/config_3d/editor_fly.rs`: navegación libre del viewport de edición (RMB mirar, WASD volar).
- `src/engine3d/config_3d/fps_camera.rs`: vista play character acoplada, IPC `play_character_view_changed`.
- `src/engine3d/config_3d/play_controller.rs`: movimiento en play (cápsula cinemática).
- `src/engine3d/config_3d/player_ui/`: editor HUD del jugador (pantallas, texto, botones, imágenes, objetos poligonales, undo).
- `src/engine3d/config_3d/mesh_3d.rs`: glTF/GLB, normalizacion, `forward_xz`.
- `src/engine3d/config_3d/physics_3d.rs`: Rapier3D y shape cast del jugador.
- `src/engine3d/config_3d/plane_tools.rs`: herramientas de muro invisible (colisionador) y trigger 3D (quad delgado, ghost, Q/E, colocación).
- `src/engine3d/config_3d/execution_areas_3d.rs`: detección de triggers en play (OBB + cápsula jugador, `on_trigger_enter`, IPC).
- `src/engine3d/config_3d/world_bounds.rs`: limites y culling AABB.
- `src/engine3d/config_base.rs`: `setup_default_3d_scene`, `reset_runtime_scene_3d`.
- `src/engine3d/ecs.rs`: `Transform`, `MeshComponent`, marcadores.
- `src/engine3d/ipc/`: comandos y eventos JSON (`types` y `events`).

Soporte: `mesh.rs`, `gizmo.rs`, `texture.rs`, `scripting.rs`, `build.rs` y `shaders/`.

## Mapa de modulos (Fase 6)

Los ficheros grandes se dividieron por funcionalidad. Para un cambio concreto se abre el
submodulo, no el arbol completo:

| Modulo | Contenido |
|---|---|
| `src/engine3d/vulkan/frame/` | `mod` (FrameRenderer/record), `targets`, `forward`, `present`, `rt`, `reflections`, `shadow`, `commands`, `layouts`, `pipelines` |
| `src/engine3d/engine/commands/` | `system`, `scenes`, `entities`, `assets`, `graphics`, `world`, `player_ui`, `animation`, `scripts`, `sockets` |
| `src/engine3d/engine/load_proyect/` | `schema`, `manifest`, `paths`, `restore`, `scene`, `events`, `snapshot` |
| `src/engine3d/config_3d/model_asset/` | `types`, `material`, `import`, `asset`, `animation`, `skeleton`, `play` |
| `src/engine3d/config_3d/static_model_cache/` | `types`, `registry`, `preload`, `spawn` |
| `src/engine3d/config_3d/` (raíz) | `mod.rs` (declaraciones), `helpers`, `model_ops`, `picking`, `gizmo_drag` |
| `src/engine3d/engine/render/` | `mod` (frame), `batches`, `rt`, `skinned`, `frustum` |
| `src/engine3d/ipc/` | `types` (payloads), `events` (EngineEvent y transporte) |
| `core/vulkan/gpu/` (crate `core`) | `context`, `swapchain`, `memory`, `frame` |
| `core/src/player_ui/text_input/` (crate `core`) | `mod` (macro `impl_player_ui_text_input!`), `methods/{state,edit,drag,emit}` (sub-macros `__rer_player_ui_text_*!`) |

Cada `mod.rs` reexporta las rutas publicas anteriores (`load_proyect::SavedScene`,
`model_asset::MAX_JOINTS`, `commands::handle_command`, etc.), asi que el resto del
codigo no cambia. Las reglas de cada split:

- Un fichero = una responsabilidad (pase, dominio de comandos, fase de carga).
- Los helpers compartidos viven en el `mod.rs` de su grupo o en un submodulo comun.
- El dispatcher de comandos pasa el `EngineCommand` de dominio en dominio; cada
  dominio devuelve `None` si lo atendio o `Some(cmd)` si no era suyo.

## Soporte interno 3D (no es IPC del otro motor)

`src/engine3d/config_compat/` agrupa tipos de herramientas del editor 3D (`ActiveTool`, `GridConfig`) y utilidades de quick-build/plane tools. **No** es un submotor 2D ni un lugar para stubs de comandos IPC ajenos; esos comandos ya no existen en `ipc/` 3D.

- Implementacion 3D nueva → `config_3d/` o `engine/`.
- No reintroducir variantes 2D en el enum IPC 3D “por compatibilidad”.

`src/engine3d/config_shared.rs` reexporta utilidades de `core`.

## Modos de camara y render

- **Editor 3D**: solo `Camera` orbital + viewport desacoplado del jugador; gizmo y frustum de cámara play character con `preview_playing == false`. No hay modo cámara 2D en este binario.
  - Navegación (toda en `!preview_playing` y fuera del editor de HUD): **RMB + arrastre** mira alrededor del ojo (estilo Minecraft, `editor_fly_look`), **W/A/S/D** vuela, **Space/C** suben y bajan, **Shift** acelera (×3), y se conserva el esquema Blender: **MMB** órbita, **Shift+MMB** paneo, **Ctrl+MMB** dolly, **rueda** zoom, **Numpad.** encuadra la selección.
  - La cámara del editor es orbital, así que "mirar" no es solo cambiar yaw/pitch: hay que girar manteniendo el ojo fijo y recolocar el pivote (`pivote = ojo − back·distancia`). Ver `config_3d/editor_fly.rs`.
  - La ventana del motor es `WS_EX_NOACTIVATE` en modo overlay: `main.rs` llama `focus_window()` al presionar cualquier botón en edición, si no el teclado (WASD y los atajos G/R/.) nunca llega al motor.
- **Play 3D**: `preview_playing`, vista desde acordeón Camera; mesh del jugador visible; capsula cinematica en `play_controller.rs`.
- **HUD play** (crosshair, Esc): NDC + `hud_scene_bind_group` (identidad). PNG de pantalla **solo** vía `screen_hud_image` + `screen_hud_pipeline` (no `TextureArray`).
- **HUD Player UI** (autoría + play): pantallas con elementos en NDC (`config_3d/player_ui/`); en play se muestra la pantalla activa; en edición UI la cámara del jugador sin `preview_playing`.

## Contratos operativos (3D)

### Personaje jugable (`[Player]`)

- `Transform` = centro del cuerpo; pies via `PLAY_CHARACTER_BODY_HEIGHT`.
- **Editor 3D (sin play)**: viewport orbital en `editor_orbit_target` + `editor_viewport_*`; `SetTransform` del jugador **no** mueve ese viewport (`body_rotation_only` / `apply_play_character_transform_editor`). Rotación o escala recalculan centro desde pies fijos.
- **Play 3D**: `camera.target` = pies; cuerpo y cámara acoplados vía `set_play_character_feet_position` / mouse look.
- **Carga `.save` 3D** (`restoring_save_manifest`): mesh sin escala 1.7 m; `place_play_character_at_world_feet` usa bounds skinned (bind pose) y la misma fórmula que `align_*` (`position = pies − feet_world_offset`); cápsula/cámara usan pies del manifest; `player.position` en save = pies.
- `replace_entity_model` (editor / cambio de modelo en vivo): actualizar `play_character_mesh_forward_xz`, escala 1.7 m, `sync_player_rotation_from_look()`. Con `restoring_save_manifest` el jugador omite ese pipeline.
- Yaw: mantener alineadas `look_xz_from_mesh_yaw` y `mesh_yaw_from_camera_and_forward` con `glam::Quat::from_rotation_y`.

#### Vista play character autoritativa (IPC)

El frontend **no** debe derivar centro de cuerpo, quaterniones ni forward de malla. Solo envia intencion y refleja lo que el motor confirma.

| Direccion | JSON (`snake_case`) | Responsabilidad |
|-----------|---------------------|-----------------|
| Electron → motor | `set_play_character_view` | Pies, `yaw`, `pitch`; opcional `fov_y`, `frustum_distance`. Aplica camara + cuerpo y emite evento. |
| Motor → Electron | `play_character_view_changed` | Estado confirmado: pies, orientacion de camara, FOV/frustum, `body_center`, `body_rotation`, `body_scale`, `player_id`. |

Implementacion: `apply_play_character_view()` / `emit_play_character_view_changed()` en `config_3d/fps_camera.rs`; handler en `engine/commands/`.

El motor emite `play_character_view_changed` tras, entre otros:

- `set_play_character_view` (y spawn),
- `set_transform` del personaje jugable,
- salir de play (`set_preview_playing` false),
- `set_camera_fov` / `set_play_editor_frustum_distance` con personaje activo,
- `replace_entity_model` del jugador.

El renderer **no** arma `playerTransform` del `.save` desde refs locales: lo incluye el snapshot (`export_save_snapshot`). La vista en editor sigue `play_character_view_changed` (`position` = pies). Ver `src/engine3d/renderer/ARCHITECTURE.md`.

#### Persistencia de escena (`.save`)

| Direccion | JSON | Rol |
|-----------|------|-----|
| Electron → motor | `export_save_snapshot` | El motor serializa mundo, entidades, jugador FP, camara, almacenes de assets y scripts. |
| Motor → Electron | `save_snapshot_ready` | Payload `scene` listo para empaquetar en ZIP (main sigue escribiendo el `.save`). |

Registro de rutas/tipos: `entity_save_meta` + actualizacion en spawn/load/replace. El front solo fusiona escenas inactivas, blueprints, idioma y `blueprint_id` por entidad.

### Scripting

- `update_scripts()` solo ejecuta el tick Rhai de entidades cuando `preview_playing` es true. Los control scripts (`on_press` / `on_keep`) ya estaban acotados a play.
- Callbacks Rhai usan **invoke pattern** en `core/src/scripting/script_engine.rs` (mismo contrato que 2D).
- **Triggers 3D:** `on_trigger_enter(trigger, actor)` en execution areas planas; detección en `execution_areas_3d.rs`.
- **Control scripts play character:** tras ejecutar el script del binding, `engine/scripts.rs` auto-inyecta `fp_press_key` con la tecla del binding cuando el script no lo hace. Scripts típicos: solo `fp_set_walk_speed`, `fp_set_sprint_multiplier`, `fp_jump`.
- Referencia: [`docs/RHAI_API.yaml`](../../../../docs/RHAI_API.yaml).

### Fisica

- Objetos: Rapier3D (`set_entity_physics`, sync con `Transform` al editar y al usar gizmo).
- Muros invisibles 3D (`[Colisionador]`): caja Rapier orientada al yaw del plano (`set_entity_physics_oriented`, `sync_plane_wall_physics`); colisión en play, ocultos salvo `debug_mode`.
- Jugador en play: shape cast (`move_character_capsule_at_feet`); sin rigid body Rapier en el mesh visual del player.
- En play, `physics.step` sincroniza cuerpos dynamic con ECS; el id del jugador play character se excluye de ese sync.

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

Sistema de **pantallas HUD** para el jugador en proyectos **3D** (cámara play character). Clave de almacenamiento `scope:screen_id` (p. ej. `player:<uuid>`). El renderer edita vía acordeón **UI del jugador**; el motor es autoritativo en geometría NDC, capas y persistencia.

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

**IPC relevante (ver `ipc/`):**

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

**Persistencia (`.save`):** `export_save_snapshot` incluye `player_ui_text_boxes`, `player_ui_buttons`, `player_ui_images`, `player_ui_objects` y `playerUiScreens`; restauración en `load_proyect/`.

**Scripting:** `set_active_player_ui`, `set_active_player_ui_by_name`, `clear_active_player_ui` (`scripting.rs`).
