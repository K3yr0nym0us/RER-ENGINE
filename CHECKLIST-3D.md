# CHECKLIST — Motor 3D (`rer_engine_3d`)

Estado del runtime 3D y del editor en proyectos **3D** / primera persona. Proyectos 2D: [CHECKLIST-2D.md](./CHECKLIST-2D.md). Tareas globales: [CHECKLIST.md](./CHECKLIST.md). Contrato: [`engine_3d/ARCHITECTURE.md`](./src/main/Engine/engine_3d/ARCHITECTURE.md). Producto: [README.md](./README.md).

**Última revisión:** junio 2026

Documentación relacionada: [docs/Escenes_Model_3D.yaml](./docs/Escenes_Model_3D.yaml), [docs/Entities_Model_3D.yaml](./docs/Entities_Model_3D.yaml), [docs/Save_Proyect_Model.yaml](./docs/Save_Proyect_Model.yaml).

## Monorepo (compartido)

```
Electron (React/TS)  ←→  IPC JSON  ←→  rer_engine_2d | rer_engine_3d
```

- [x] Workspace Cargo (`engine_2d`, `engine_3d`, `engine_shared`); binarios separados
- [x] IPC stdin/stdout, winit + wgpu overlay, ECS, scripting Rhai, hot reload
- [x] GPU: Vulkan (`EngineGpuProfile::ThreeD`, Windows y Linux)
- [x] Editor: electron-vite, spawn del motor, escenas múltiples, `.save` ZIP
- [x] `set_bounds`, evento `ready`, multiplex `engine.off()` en preload
- [x] Undo/redo de **transformaciones** en editor (ambos motores)
- [x] Panel **Métricas de uso** (FPS, frame time, draw calls, CPU) + GPU **Windows** (contadores por PID, Electron + motor; no % global del SO)

| Plataforma | Viewport overlay |
|------------|------------------|
| Linux X11 | Ventana separada + position-tracker + `XSetTransientForHint` |
| Windows | Popup owned + position-tracker (`GWLP_HWNDPARENT`) |
| Wayland | XWayland o `ELECTRON_OZONE_PLATFORM_HINT=x11` |

| GPU | Vulkan (`EngineGpuProfile::ThreeD`) |
| Arranque motor | `--overlay` (alias `--embed`); fallo GPU → evento `error` + overlay en editor |

**Nota:** Avisos del loader Vulkan en consola pueden ser ruido habitual y **no** indican fallo si el motor envía `ready`.

---

## Implementado

### Motor 3D

- [x] Carga glTF/FBX, mallas normalizadas, materiales básicos + PBR en shader
- [x] Editor: cámara orbital; gizmo mover/rotar; preview FP con frustum de editor
- [x] Play mode primera persona (cápsula cinemática, shape cast; mesh del jugador oculto)
- [x] **Física 3D de producto** — Rapier en objetos (`set_entity_physics`, sync `Transform` en editor); jugador FP solo con shape cast; play sin traspasos (movimiento, salto, static/dynamic, límites del mundo)
- [x] Jugador FP: convención pies ↔ centro de cuerpo, forward de malla, sync cámara-cuerpo
- [x] IPC vista FP autoritativa: `set_first_person_view` → `first_person_view_changed`
- [x] HUD play: crosshair por defecto como **objetos Player UI** (barras H/V en `fp-hud-01`); tooltip Esc en play
- [x] **Player UI HUD** — pantallas del jugador, edición FPS, texto / botón / imagen / **objeto poligonal** (clicks + cruz en cursor), capas `z_index`, bloqueo, play de pantalla activa, persistencia en `.save`
- [x] **Player UI undo/redo** — Ctrl+Z / Ctrl+Y con snapshot por pantalla (`hud_undo.rs`, `RestorePlayerUiHud`)
- [x] Cubos de editor (`spawn_editor_box`), modelos en almacén (`load_model_asset`)
- [x] ECS 3D: `HashSet` + reciclado de IDs en `despawn` (`ecs.rs`)
- [x] Undo/redo de creación de entidad — snapshot en motor (`undo_entity.rs`, `spawn_with_id`)
- [x] API Rhai 3D FP: `fp_jump`, `fp_set_walk_speed`, etc.; tecla del binding auto-inyectada en control scripts
- [x] Demo `DEMO_3d_FIRST_PERSON.save`: scripts `.rhai` (WASD walk speed, SHIFT sprint, SPACE jump)
- [x] `replace_entity_model` con resync de orientación, escala y forward del jugador (FBX + GLB)
- [x] **Carga GLB/GLTF skinned** — esqueleto unificado (varios `skin` por archivo), paleta Khronos, piezas múltiples; clips embebidos en asset
- [x] **Animaciones embebidas 3D** — pipeline skinned GPU, `play_model_clip`, `set_default_animation`, evento `model_clips_ready`
- [x] **Orientación malla jugador (GLB/FBX)** — `upright_quat_from_vertices_bounds`, corrección yaw cadera Mixamo, forward skinned (`model_asset.rs`, `mesh_3d.rs`); aplicada al spawn/replace FP
- [x] Export escena: `export_save_snapshot` → `save_snapshot_ready` (`entity_save_meta`, jugador FP)
- [x] Scripts Rhai `update()` solo en play

### Editor e integración (proyecto 3D)

- [x] Acordeón cámara FP (envía `set_play_character_view`, no calcula poses en TS)
- [x] `playCharacterViewRef` vía `play_character_view_changed` (UI editor; save 3D vía snapshot del motor)
- [x] Herramientas 3D (gizmo, spawn caja/modelo, play FP)
- [x] **Herramientas plano 3D** — muro invisible (`[Colisionador]`) y trigger (`[ExecutionArea]`): toggle, ghost, Q/E, colocación con click, foco editor al colocar, rotación en `.save`, colisión orientada Rapier, ocultos en play, sin sombras
- [x] **Triggers 3D en play** — `update_execution_areas_3d`, `trigger_entered` / `trigger_exited`, `on_trigger_enter`, log panel `[trigger]`
- [x] Eliminar entidades 3D (modelos/cajas) — `remove_entity` + panel Propiedades; sync listas vía `entity_removed`
- [x] Carga de escena 3D al abrir `.save` — burst/precarga en Rust (`load_proyect.rs`); front refleja `project_loaded_3d` / `project_load_3d_complete` (`pendingRestores` solo en rutas puntuales del handler, no carga principal)
- [x] Guardado engine-first: `export_save_snapshot` + merge en el front
- [x] Panel **Animaciones** en Propiedades (clips embebidos GLB/FBX en 3D; hojas/sprites en 2D)
- [x] **Reemplazar modelo del jugador** — overlay de carga + ocultar viewport durante importación síncrona
- [x] Acordeón Modelos: botón y diálogo **`.glb` / `.gltf` / `.fbx`** (almacén + sustitución jugador)
- [x] **Blueprints / prefabs 3D** — convertir entidad (`PropertiesAccordion`), `register_blueprint` en motor, construcción rápida (`quick_build` + `BlueprintPlacementMeta`), propagación a instancias (transform, física, scripts, animaciones), persistencia en `project.blueprints[]`
- [x] **Multi-escena editor 3D** — registro en motor (`editor_scenes.rs`), dirty vía undo, `switch_editor_scene`, baselines boot/placeholder/saved; UI acordeón Scenes (`docs/Escenes_Model_3D.yaml`)
- [x] **Programación visual** — lógica de escena y entidad (nodos → Rhai); panel de variables con entidades por categoría y animaciones; resolución jugador FP + `entityMeta` para modal Electron

### Carga `.save` 3D y rendimiento (mayo 2026)

- [x] **Carga de proyecto 3D en Rust** — `engine_3d/src/engine/load_proyect.rs`: manifest, burst de entidades, jugador FP, sonidos/fondos; front solo refleja eventos (`project_loaded_3d`, `project_load_3d_complete`)
- [x] **Loader de escena** — overlay hasta `project_load_3d_complete`; `ready` tras escena vacía al abrir `.save` (`RER_3D_START_FROM_SAVE`); logs `[Carga]` / `load_progress` en panel
- [x] **Precarga y burst acotados** — solo paths GLB/FBX requeridos por entidades + `playerTransform.visual_model_path` (sin precargar toda la biblioteca `view.models`)
- [x] **Vaciado de escena al abrir save** — `clear_scene_entities_for_save_load` sin segundo `reset_runtime_scene_3d` duplicado
- [x] **EditorBox compartido en GPU** — `ensure_editor_box_gpu_assets` (una mesh/textura para todos los `[EditorBox]`)
- [x] **Precarga async** — hilos de fondo + `poll_model_preloads`; burst espera caché sin bloquear arranque del motor
- [x] **Import GLB optimizado (precarga)** — un solo `gltf::import` por archivo (`preload_model_cpu_bundle`); sin `ModelAsset` si no hay skins ni animaciones; sin `try_warm` variante jugador en props/entorno
- [x] **Texturas GLB en editor** — `gltf_texture_load.rs`, modo por defecto `SmallestEmbedded`: solo decodifica la `baseColor` embebida de menor resolución; ignora normal/metallic y el resto del pack; `AllEmbedded` reservado para módulo de calidad futuro
- [x] **Main Electron** — reenvío stderr `[load_proyect]`; spawn 3D con `extract_dir` / `save_path`
- [x] **`kind` en `ModelLoaded` (IPC)** — `load_model` acepta `kind`; el motor lo reenvía en `model_loaded`; el front usa `loaded.kind` (sin `pendingSpawnKindRef`)
- [x] **Spawn personaje 3D desde Characters** — `entity_label_for_spawn` → nombre `Character_*`; `entity_category` + `character_entities` en motor; modal y acordeón Entidades reconocen personaje (no `Object_*`)

---

## Por implementar

### Prioridad media — funcionalidad

- [ ] **Herramienta 3D: física por hueso (bone physics)**
  - Botón/herramienta exclusiva del acordeón **Tools** en proyectos **3D** (no visible en 2D).
  - Al activarla y seleccionar una entidad basada en modelo 3D skinned (GLB/GLTF/FBX), visualizar el **esqueleto / huesos** de la entidad en el viewport.
  - Panel o inspector para asignar **tipo de física por hueso** (p. ej. estático, dinámico, kinematic, sin simulación, etc.).
  - Persistencia en `.save` por entidad/hueso; sincronización motor ↔ editor.
  - Objetivo: dinamismo en personajes y escenario (pelo, pechos, ropa suelta, accesorios colgantes, elementos blandos del entorno, etc.) sin depender solo de animación de clips embebidos.
- [ ] **QA orientación jugador GLB Mixamo** — la lógica de bake/orientación ya está en motor; falta validación manual con varios GLB Mixamo reales en FP (altura ~1.7 m, de pie de frente, sin regresión skinning)
- [ ] **Animaciones 3D (avanzado)** — state machine / hojas tipo Blender; más allá de clips embebidos + reproducción básica

### Prioridad baja

- [x] **Calidad de texturas GLB (UI)** — pestaña **Texturas** en Propiedades de entidad (3D, GLB/GLTF): niveles Bajo/Medio/Alto/Ultra, variantes embebidas por resolución y material; IPC `list_entity_textures` / `set_entity_texture_lod` / `set_entity_texture_preview_tier`; persistencia `texture_lod` en `.save`
- [ ] Picking 3D: AABB del `Transform` vs silueta fina del modelo (hoy `entity_world_pick_aabb` + ray-AABB en `config_3d/mod.rs`)

---

## Criterios «hecho»

| Ítem | Criterio |
|------|----------|
| Física 3D producto | Play FP + colisiones Rapier sin traspasos; sync editor en gizmo/`set_transform` |
| Animaciones embebidas 3D | GLB/FBX con skinning reproducen clips en editor/play; panel y `model_clips_ready` |
| Orientación GLB jugador FP | Código en motor aplicado; QA con GLB Mixamo variados confirma pie de frente ~1.7 m sin regresión skinning |
| Blueprints 3D | Crear/instanciar/actualizar desde editor; quick build; propagación; `.save` |
| Carga `.save` 3D | Abrir demo/proyecto 3D: loader coherente, escena en motor, sin duplicar eventos `model_loaded`; tiempo de carga aceptable en props pesados |

---

## Notas IPC (3D / FP)

| Comando / evento | Uso |
|------------------|-----|
| `set_play_character_view` | Front → motor: pies, yaw, pitch, FOV, frustum |
| `play_character_view_changed` | Motor → front: estado confirmado (`body_center`, `body_rotation`, etc.) |
| `model_clips_ready` | Motor → front: clips embebidos tras cargar GLB/FBX skinned en entidad |
| `entity_model_replaced` | Motor → front: fin de `replace_entity_model` (quita overlay de carga) |
| `project_loaded_3d` | Motor → front: escena activa tras parsear manifest (tabs, mundo, entidades) |
| `project_load_3d_complete` | Motor → front: fin de burst/precarga al abrir `.save` (cierra loader) |
| `load_progress` | Motor → front: mensaje de progreso durante carga (`step_ms`, `total_ms` opcionales) |
| `load_model` (`kind`, `entity_category`) | Front → motor: instancia modelo precargado; `kind` (`model` \| `character`) define prefijo de nombre y meta |
| `model_loaded` (`kind`, `entity_category`) | Motor → front: entidad creada; el front clasifica sin estado local `pendingSpawnKindRef` |
| `plane_tool_ready` / `tool_cancelled` | Motor → front: herramienta plano 3D activa / cancelada (silenciosos en panel) |
| `trigger_entered` | Motor → front: actor entró en execution area (`has_attached_script` opcional); log `[trigger]` |
| `collider_created` / `execution_area_created` | Motor → front: plano 3D colocado (`position` + `scale`); Electron devuelve foco al editor |

Aliases legacy: `set_first_person_view`, `first_person_view_changed`.

El frontend **no** deriva poses FP para el `.save`; el snapshot del motor es autoritativo. Ver `src/renderer/ARCHITECTURE.md`.

Comandos del protocolo **solo 2D** están stubbeados en este binario; el front 3D no debe depender de ellos.
