# CHECKLIST — Motor 3D (`rer_engine_3d`)

Estado del runtime 3D y del editor en proyectos **3D** / primera persona. Proyectos 2D: [CHECKLIST-2D.md](./CHECKLIST-2D.md). Contrato: [`engine_3d/ARCHITECTURE.md`](./src/main/Engine/engine_3d/ARCHITECTURE.md). Producto: [README.md](./README.md).

**Última revisión:** mayo 2026

---

## Monorepo (compartido)

```
Electron (React/TS)  ←→  IPC JSON  ←→  rer_engine_2d | rer_engine_3d
```

- [x] Workspace Cargo (`engine_2d`, `engine_3d`, `engine_shared`); binarios separados
- [x] IPC stdin/stdout, winit + wgpu overlay, ECS, scripting Lua, hot reload
- [x] GPU por plataforma: Vulkan (Linux), DirectX 12 fijo (Windows, `EngineGpuProfile::ThreeD`)
- [x] Editor: electron-vite, spawn del motor, escenas múltiples, `.save` ZIP
- [x] `set_bounds`, evento `ready`, multiplex `engine.off()` en preload
- [x] Undo/redo de **transformaciones** en editor (ambos motores)

| Plataforma | Viewport overlay |
|------------|------------------|
| Linux X11 | Ventana separada + position-tracker + `XSetTransientForHint` |
| Windows | Popup owned + position-tracker (`GWLP_HWNDPARENT`) |
| Wayland | XWayland o `ELECTRON_OZONE_PLATFORM_HINT=x11` |

| GPU | Linux: Vulkan; Windows: DX12 fijo (`EngineGpuProfile::ThreeD`) |
| Arranque motor | `--overlay` (alias `--embed`); fallo GPU → evento `error` + overlay en editor |

**Nota:** En Windows el 3D usa DX12 (menos ruido de capas Vulkan). En Linux, avisos del loader Vulkan en consola pueden ser ruido habitual y **no** indican fallo si el motor envía `ready`.

---

## Implementado

### Motor 3D

- [x] Carga glTF/FBX, mallas normalizadas, materiales básicos + PBR en shader
- [x] Editor: cámara orbital; gizmo mover/rotar; preview FP con frustum de editor
- [x] Play mode primera persona (cápsula cinemática, shape cast; mesh del jugador oculto)
- [x] **Física 3D de producto** — Rapier en objetos (`set_entity_physics`, sync `Transform` en editor); jugador FP solo con shape cast; play sin traspasos (movimiento, salto, static/dynamic, límites del mundo)
- [x] Jugador FP: convención pies ↔ centro de cuerpo, forward de malla, sync cámara-cuerpo
- [x] IPC vista FP autoritativa: `set_first_person_view` → `first_person_view_changed`
- [x] HUD play: crosshair + tooltip Esc (texturas en `Engine/assets/`)
- [x] Cubos de editor (`spawn_editor_box`), modelos en almacén (`load_model_asset`)
- [x] ECS 3D: `HashSet` + reciclado de IDs en `despawn` (`ecs.rs`)
- [x] Undo/redo de creación de entidad — snapshot en motor (`undo_entity.rs`, `spawn_with_id`)
- [x] API Lua 3D FP: `fp_press_key`, `fp_jump`, `fp_set_walk_speed`, etc.
- [x] `replace_entity_model` con resync de orientación, escala y forward del jugador (FBX + GLB)
- [x] **Carga GLB/GLTF skinned** — esqueleto unificado (varios `skin` por archivo), paleta Khronos, piezas múltiples; clips embebidos en asset
- [x] **Animaciones embebidas 3D** — pipeline skinned GPU, `play_model_clip`, `set_default_animation`, evento `model_clips_ready`
- [x] Export escena: `export_save_snapshot` → `save_snapshot_ready` (`entity_save_meta`, jugador FP)
- [x] Scripts Lua `update()` solo en play

### Editor e integración (proyecto 3D)

- [x] Acordeón cámara FP (envía `set_play_character_view`, no calcula poses en TS)
- [x] `playCharacterViewRef` vía `play_character_view_changed` (UI editor; save 3D vía snapshot del motor)
- [x] Herramientas 3D (gizmo, spawn caja/modelo, play FP)
- [x] Eliminar entidades 3D (modelos/cajas) — `remove_entity` + panel Propiedades; sync listas vía `entity_removed`
- [x] Carga de escena 3D + `pendingRestores` / vista FP desde motor al abrir `.save`
- [x] Guardado engine-first: `export_save_snapshot` + merge en el front
- [x] Panel **Animaciones** en Propiedades (clips embebidos GLB/FBX en 3D; hojas/sprites en 2D)
- [x] **Reemplazar modelo del jugador** — overlay de carga + ocultar viewport durante importación síncrona
- [x] Acordeón Modelos: botón y diálogo **`.glb` / `.gltf` / `.fbx`** (almacén + sustitución jugador)

---

## Por implementar

### Prioridad media — funcionalidad

- [ ] **Orientación vertical jugador GLB** — validar en FP con GLB Mixamo reales (rest pose esqueleto + bake por pieza; pendiente prueba manual)
- [ ] **Animaciones 3D (avanzado)** — state machine / hojas tipo Blender; más allá de clips embebidos + reproducción básica
- [ ] **Blueprints / prefabs 3D** — equivalente unificado al flujo 2D

### Prioridad baja

- [ ] Picking 3D: AABB del `Transform` vs silueta fina del modelo (mejora UX de selección)
- [ ] `kind` en `ModelLoaded` (IPC) — sync front sin `spawnKind` local (motor-first)

---

## Criterios «hecho»

| Ítem | Criterio |
|------|----------|
| Física 3D producto | Play FP + colisiones Rapier sin traspasos; sync editor en gizmo/`set_transform` |
| Animaciones embebidas 3D | GLB/FBX con skinning reproducen clips en editor/play; panel y `model_clips_ready` |
| Orientación GLB jugador FP | Modelo de pie de frente a cámara, altura ~1.7 m, sin regresión skinning |
| Blueprints 3D | Crear/instanciar/actualizar desde editor en proyecto 3D |

---

## Aplazado (producto)

- Multiplayer 
- IA generativa en assets 
- partículas/shaders experimentales

---

## Notas IPC (3D / FP)

| Comando / evento | Uso |
|------------------|-----|
| `set_play_character_view` | Front → motor: pies, yaw, pitch, FOV, frustum |
| `play_character_view_changed` | Motor → front: estado confirmado (`body_center`, `body_rotation`, etc.) |
| `model_clips_ready` | Motor → front: clips embebidos tras cargar GLB/FBX skinned en entidad |
| `entity_model_replaced` | Motor → front: fin de `replace_entity_model` (quita overlay de carga) |

Aliases legacy: `set_first_person_view`, `first_person_view_changed`.

El frontend **no** deriva poses FP para el `.save`; el snapshot del motor es autoritativo. Ver `src/renderer/ARCHITECTURE.md`.

Comandos del protocolo **solo 2D** están stubbeados en este binario; el front 3D no debe depender de ellos.
