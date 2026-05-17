# Checklist técnico — RER-ENGINE

Estado de implementación del monorepo (editor + motores). Para ejecutar el proyecto y la filosofía de producto, ver [README.md](./README.md).

**Última revisión:** mayo 2026

---

## Arquitectura

```
┌─────────────────────────────────────────┐
│           Electron (BrowserWindow)      │
│  ┌──────────────┐  ┌───────────────────┐│
│  │  React + TS  │  │  Viewport Rust     ││
│  │  (editor)    │  │  wgpu embebido    ││
│  └──────────────┘  └───────────────────┘│
└─────────────────────────────────────────┘
         ↑ IPC — JSON por stdin/stdout
```

| Componente | Ubicación |
|------------|-----------|
| Workspace Cargo | `src/main/Engine/` — crates `engine_2d`, `engine_3d`, `engine_shared` |
| Editor | `src/main/`, `src/preload/`, `src/renderer/` |
| Tipos IPC | `src/shared-types/types.ts` |
| Docs de contrato | `engine_2d/ARCHITECTURE.md`, `engine_3d/ARCHITECTURE.md`, `src/renderer/ARCHITECTURE.md` |

Selección de binario en runtime: `rer_engine_2d` o `rer_engine_3d` según tipo de proyecto / `GameStyle`.

---

## Implementado

### Workspace y motores

- [x] Workspace Cargo con tres crates (`engine_2d`, `engine_3d`, `engine_shared`)
- [x] Binarios **separados** 2D y 3D (sin runtime híbrido en un solo proceso)
- [x] `engine_shared` — utilidades IPC comunes
- [x] `config_compat/` en cada crate — stubs de comandos del protocolo que ese binario no ejecuta
- [x] Documentación de contrato por crate + renderer engine-first

### Motor — base (ambos binarios)

- [x] Ventana winit + superficie wgpu embebida en Electron (`--embed` + handle nativo)
- [x] IPC: líneas JSON stdin → comandos; stdout → eventos
- [x] Hilo de lectura stdin + canal al loop principal
- [x] Comandos `ping`, `shutdown`, `resize`, `set_bounds`, `set_clear_color`
- [x] Delta time y bucle update/render
- [x] ECS con queries multi-componente
- [x] Undo/redo de transformaciones, herramientas de dibujo y entidades 2D (snapshot completo)
- [x] Scripting Lua (mlua) con sandbox y lifecycle `on_start` / `update` / `on_stop`
- [x] Hooks `on_press`, `on_trigger_enter` (2D), control bindings desde input nativo
- [x] Hot reload de scripts
- [x] Debug overlay (FPS, frame time, draw calls, cuerpos físicos)
- [x] Instanced rendering, texture atlas, frustum culling, capas de render
- [x] Spatial grid para picking/consultas (2D)

### Motor 2D (`rer_engine_2d`)

- [x] Sprites, fondos, escenarios y personajes 2D
- [x] Animaciones por frames + espejo horizontal automático
- [x] Rapier 2D: `move_entity`, kinematic gravity/impulse, slide con shape-cast
- [x] Colliders y execution areas dibujados en editor
- [x] Triggers `on_trigger_enter` / eventos `trigger_entered` / `trigger_exited`
- [x] Cámara 2D (`set_camera2d`, `camera_2d_updated`)
- [x] Quick build con snap/escala calculados en Rust
- [x] Blueprints 2D (instanciar, actualizar desde plantilla)
- [x] API Lua 2D: `apply_kinematic_gravity`, `move_entity_slide`, `set_vsync`, etc.

### Motor 3D (`rer_engine_3d`)

- [x] Carga glTF/FBX, mallas normalizadas, materiales básicos + PBR en shader
- [x] Editor: cámara orbital; gizmo mover/rotar; preview FP con frustum de editor
- [x] Play mode primera persona (cápsula cinemática, shape cast; mesh del jugador oculto)
- [x] Rapier3D en **objetos** de escena (`set_entity_physics`, sync con `Transform`)
- [x] Jugador FP: convención pies ↔ centro de cuerpo, forward de malla, sync cámara-cuerpo
- [x] IPC vista FP autoritativa: `set_first_person_view` → `first_person_view_changed`
- [x] HUD play: crosshair + tooltip Esc (texturas en `Engine/assets/`)
- [x] Cubos de editor (`spawn_editor_box`), modelos en almacén (`load_model_asset`)
- [x] API Lua 3D FP: `fp_press_key`, `fp_jump`, `fp_set_walk_speed`, etc.
- [x] `replace_entity_model` con resync de orientación y escala del jugador
- [x] Export escena para `.save`: `export_save_snapshot` → `save_snapshot_ready` (`entity_save_meta`, placeholders FP incluidos)

### Motor 2D (`rer_engine_2d`) — guardado

- [x] Export escena para `.save`: `export_save_snapshot` → `save_snapshot_ready` (`entity_save_meta`, marcadores scenario/character/collider)

### Editor (Electron + React)

- [x] electron-vite, preload con `contextBridge`, tipos compartidos
- [x] Spawn del motor correcto según proyecto; reintento si el proceso muere
- [x] Escenas múltiples (crear, renombrar, duplicar, cambiar activa)
- [x] Guardado/carga `.save` (ZIP: manifest, assets, sounds, scripting)
- [x] Guardado engine-first (2D y 3D): `export_save_snapshot`; front fusiona pestañas inactivas, blueprints e idioma
- [x] Multi-selección, panel de propiedades, herramientas 2D/3D según proyecto
- [x] Acordeón cámara FP (envía `set_first_person_view`, no calcula poses en TS)
- [x] Refactor engine-first FP: `firstPersonViewRef` para UI vía `first_person_view_changed` (no para serializar save 3D)
- [x] Empaquetado `electron-builder` (extraResources con binarios release)

### Integración

- [x] `set_bounds` al redimensionar el viewport
- [x] Timeout si no llega evento `ready`
- [x] Flujo carga escena → `load_character` / colas `pendingRestores` (orquestación en front; poses FP vía motor)
- [x] `apply_entity_restore` IPC (restore post-carga por entidad 2D; paso hacia `import_scene`)
- [x] Tamaño lógico de animación resuelto en motor (`animation_logical_resolved`; front sin pre-cálculo)
- [x] Defaults pivot/lógico en motor al cargar sprite y en `SetAnimation` / `PlayAnimationFrame`
- [x] Personaje nuevo ~1,5 celdas; colisión 2D desde `tight_bounds` (no solo offset del frame)

---

## Por implementar

### Prioridad alta

- [ ] **Física 3D de producto completo** — Rapier en objetos está; falta cerrar colisiones/gameplay FP (interacción uniforme, edge cases, pruebas de regresión)

### Prioridad media — motor-first

- [x] **`normalizeAnimations` solo en motor** — `SetAnimation` resuelve `logical_w/h`; front sincroniza vía `animation_logical_resolved`
- [ ] **`import_scene` completo en motor** — hoy: `apply_entity_restore` por entidad; falta cargar escena entera en un comando
- [x] **Defaults `logical` / `pivot` en motor** — pivot opcional en frames; `CharacterLoaded`/`ScenarioLoaded` con dimensiones
- [x] **`entity_removed` con snapshot de puntos** (2D: colliders / execution areas en el evento IPC)
- [x] **Scripts Lua `update()` solo en play** (2D y 3D)

### Prioridad media — funcionalidad

- [ ] **Animaciones 3D** (clips / state machine; pipeline compatible con Blender)
- [ ] **Blueprints / prefabs 3D** — flujo 2D parcial; falta equivalente unificado en 3D
- [x] **Redo de `RemoveEntity` fiable** — snapshot + mismo id (escenario, personaje, colisionador, trigger); undo al borrar desde Propiedades
- [x] **Atlas: señalizar agotamiento** (2D: evento `atlas_exhausted` → consola del editor)

### Prioridad baja

- [ ] Jerarquía parent/child de entidades (evaluar diseño)
- [ ] Optimizar `new_entity_id()` para escenas muy grandes
- [x] Renombrar helpers con sufijo `_x11` a nombres neutros multiplataforma (`query_ctrl_held_os`)
- [~] Flujo dedicado de restauración inicial — `applyPendingRestoreToEngine.ts` + `apply_entity_restore`; aún hay colas `pendingRestores` en front
- [x] Revisar `window.engine.off()` vs múltiples listeners (multiplex en preload; `off(cb)` por suscriptor)

### Descartado por ahora

- Multiplayer
- IA generativa en pipeline de assets
- Partículas / shaders experimentales sin caso de producto

---

## Notas técnicas

### Embedding del viewport

| Plataforma | Handle | Mecanismo |
|------------|--------|-----------|
| Linux X11 | `XID` | Ventana hija / reparent |
| Windows | `HWND` | `SetParent` (Win32) |
| Linux Wayland | — | Usar XWayland o `ELECTRON_OZONE_PLATFORM_HINT=x11` |

### IPC — contratos recientes (3D FP)

| Comando / evento | Uso |
|------------------|-----|
| `set_first_person_view` | Front → motor: pies, yaw, pitch, FOV, frustum |
| `first_person_view_changed` | Motor → front: estado confirmado (incl. `body_center`, `body_rotation`) |

### Deuda 3D documentada en crate

- No usar `meshes[0]` para overlays HUD (`hud_quad_mesh` dedicado)
- Coherencia render / picking / física respecto a `Transform` en editor

### Comandos IPC “solo 2D”

El protocolo compartido incluye variantes que `engine_3d` ignora o stubbea (`create_collider_from_points`, `play_animation_frame`, etc.). Proyectos 3D no deben depender de ellos.

---

## Criterios “hecho” (referencia)

| Ítem | Criterio |
|------|----------|
| Blueprints 3D | Crear/instanciar/actualizar desde editor en proyecto 3D |
| Física 3D producto | Play FP + objetos dinámicos/estáticos sin desincronías transform/collider |
