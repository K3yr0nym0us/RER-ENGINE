# CHECKLIST — Motor 2D (`rer_engine_2d`)

Estado del runtime 2D y del editor en proyectos **2D**. Proyectos 3D: [CHECKLIST-3D.md](./CHECKLIST-3D.md). Contrato: [`engine_2d/ARCHITECTURE.md`](./src/main/Engine/engine_2d/ARCHITECTURE.md). Producto: [README.md](./README.md).

**Última revisión:** mayo 2026

---

## Monorepo (compartido)

```
Electron (React/TS)  ←→  IPC JSON  ←→  rer_engine_2d | rer_engine_3d
```

- [x] Workspace Cargo (`engine_2d`, `engine_3d`, `engine_shared`); binarios separados
- [x] IPC stdin/stdout, winit + wgpu overlay, ECS, scripting Lua, hot reload
- [x] GPU fija: Vulkan (`EngineGpuProfile::TwoD` en `engine_shared/src/gpu.rs`)
- [x] Editor: electron-vite, spawn del motor, escenas múltiples, `.save` ZIP
- [x] `set_bounds`, evento `ready`, multiplex `engine.off()` en preload
- [x] Undo/redo de **transformaciones** en editor (ambos motores)

| Plataforma | Viewport overlay |
|------------|------------------|
| Linux X11 | Ventana separada + position-tracker + `XSetTransientForHint` |
| Windows | Popup owned + position-tracker (`GWLP_HWNDPARENT`) |
| Wayland | XWayland o `ELECTRON_OZONE_PLATFORM_HINT=x11` |

| GPU | Siempre Vulkan (`engine_shared::gpu`, perfil TwoD); sin OpenGL |
| Arranque motor | `--overlay` (alias `--embed`); fallo GPU → evento `error` + overlay en editor |

**Nota:** Avisos del loader Vulkan en consola (Epic Games, Galaxy Overlay, `Unrecognized present mode`) son ruido habitual en Windows y **no** indican fallo si el motor envía `ready`.

---

## Implementado

### Motor 2D

- [x] Sprites, fondos, escenarios y personajes 2D
- [x] Animaciones por frames + espejo horizontal automático
- [x] Rapier 2D: `move_entity`, kinematic gravity/impulse, slide con shape-cast
- [x] Colliders y execution areas dibujados en editor
- [x] Triggers `on_trigger_enter` / eventos `trigger_entered` / `trigger_exited`
- [x] Cámara 2D (`set_camera2d`, `camera_2d_updated`)
- [x] Quick build con snap/escala calculados en Rust
- [x] Blueprints 2D (instanciar, actualizar desde plantilla)
- [x] API Lua 2D: `apply_kinematic_gravity`, `move_entity_slide`, `set_vsync`, etc.
- [x] Spatial grid para picking/consultas
- [x] Undo/redo de transformaciones, herramientas de dibujo y entidades (snapshot: escenario, personaje, colisionador, trigger)
- [x] Atlas: evento `atlas_exhausted` → consola del editor
- [x] `entity_removed` con snapshot de puntos (colliders / execution areas)
- [x] Export escena: `export_save_snapshot` → `save_snapshot_ready` (`entity_save_meta`, marcadores scenario/character/collider)
- [x] Asignación de IDs de entidad: `HashSet` + cola de IDs reciclados en `ecs.rs` (escenas grandes)

### Motor-first (2D)

- [x] `normalizeAnimations` solo en motor — `SetAnimation` resuelve `logical_w/h`; front vía `animation_logical_resolved`
- [x] Defaults `logical` / `pivot` en motor al cargar sprite y en `SetAnimation` / `PlayAnimationFrame`
- [x] Personaje nuevo ~1,5 celdas; colisión desde `tight_bounds`
- [x] `apply_entity_restore` IPC (restore post-carga por entidad)
- [x] Redo de `RemoveEntity` fiable — undo al borrar desde Propiedades y construcción rápida

### Editor e integración (proyecto 2D)

- [x] Herramientas 2D (dibujo colisionador / trigger, quick build, etc.)
- [x] Panel de propiedades y multi-selección en escena 2D
- [x] **`import_scene` completo** — un IPC carga la escena entera (entidades + restores); sin ráfagas IPC ni `pendingRestoresRef` en el front; evento `scene_imported`
- [x] **Flujo de restauración inicial** — `import_scene` en carga/pestaña 2D; `apply_entity_restore` para undo y casos puntuales
- [x] Guardado engine-first: `export_save_snapshot` + merge de pestañas/blueprints en el front

---

## Por implementar

- [ ] Unificar referencia espacial render / picking / triggers / física
- [ ] Revisar semántica `SetGravity`, `apply_kinematic_gravity` y `on_press` en cuerpos kinematic
- [ ] Reducir shims `config_compat` cuando no queden rutas heredadas

---

## Criterios «hecho»

| Ítem | Criterio |
|------|----------|
| `import_scene` | Un IPC carga escena + restores; `scene_imported` sincroniza React |
| Undo entidades | Crear/borrar escenario, personaje, colisionador y trigger con Ctrl+Z / Ctrl+Y simétricos |

---

## Aplazado (producto)

- Multiplayer
- IA generativa en assets
- partículas/shaders experimentales

---

## Notas IPC (solo 2D)

Comandos que **solo** ejecuta este binario (`play_animation_frame`, `create_collider_from_points`, etc.). `engine_3d` los stubbea en `config_compat`.
