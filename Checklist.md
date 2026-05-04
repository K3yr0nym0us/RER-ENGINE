# 🚀 Checklist — RER-ENGINE (React + Electron + Rust)

## Arquitectura objetivo

```
┌─────────────────────────────────────────┐
│           Electron (BrowserWindow)      │
│  ┌──────────────┐  ┌───────────────────┐│
│  │  React + TS  │  │  Viewport nativo  ││
│  │  (UI/Editor) │  │  (child window)   ││
│  │              │  │  ← Rust/wgpu      ││
│  └──────────────┘  └───────────────────┘│
└─────────────────────────────────────────┘
         ↑ IPC (stdin/stdout JSON)
```

- **Electron** actúa como shell: gestiona layout, UI y ciclo de vida
- **Rust** renderiza en una ventana hija (`child_window`) embebida en Electron
- **Comunicación**: comandos JSON por stdin/stdout (sin WebSocket en MVP)

---

# ✅ IMPLEMENTADO

## Motor Rust — Setup y render base

* ✅ Crear workspace Cargo en `Engine/` (`cargo init --name rer-engine`)
* ✅ Dependencias: `winit`, `wgpu`, `raw-window-handle`, `serde`, `serde_json`, `gltf`, `image`
* ✅ Estructura de módulos: `main.rs`, `engine.rs`, `ipc.rs`, `shader.wgsl`
* ✅ Crear ventana propia con `winit::EventLoop` y `WindowBuilder`
* ✅ Inicializar `wgpu`: `Instance` → `Surface` → `Adapter` → `Device` + `Queue`
* ✅ Renderizar clear color configurable
* ✅ Renderizar triángulo con vertex/fragment shader (WGSL)
* ✅ Manejar evento `Resized` y reconfigurar `SurfaceConfiguration`
* ✅ `EventLoop::run` con separación `update()` / `render()`
* ✅ Manejar `WindowEvent::CloseRequested`
* ✅ Delta time básico (`std::time::Instant`)

## Motor Rust — Cámara

* ✅ Struct `Camera` con posición, yaw, pitch
* ✅ Matriz View (`glam::Mat4::look_at_rh`)
* ✅ Matriz Projection (perspectiva, `glam::Mat4::perspective_rh`)
* ✅ Uniform buffer en wgpu + bind group

## Motor Rust — Ventana embebida e IPC

* ✅ Parsear args: `--standalone` vs `--embed <window_id> <x> <y> <width> <height>`
* ✅ En modo `--embed`: crear ventana hija usando el handle recibido (X11/Win32)
* ✅ `wgpu::Surface` creada desde el handle de la ventana hija
* ✅ Sin decoraciones, borderless, no resizable (Electron controla el tamaño)
* ✅ Hilo dedicado para leer stdin (`BufReader<stdin>`)
* ✅ Deserializar cada línea como `EngineCommand` con `serde_json`
* ✅ Enviar eventos al loop principal vía `mpsc::channel`
* ✅ Responder por stdout con `println!("{}", serde_json::to_string(...))`

## Motor Rust — Assets y escena

* ✅ Loader `.glb` con crate `gltf` (solo mallas + materiales básicos)
* ✅ Subir vértices e índices a `wgpu::Buffer`
* ✅ Renderizar modelo con shader básico (Blinn-Phong o flat shading)
* ✅ Cargar imagen con crate `image` → `wgpu::Texture`
* ✅ Bind group con sampler + texture view
* ✅ Aplicar textura al modelo cargado
* ✅ Struct `Entity` con ID + `Transform` (posición, rotación, escala)
* ✅ Struct `Scene` con `Vec<Entity>` + `HashMap<EntityId, MeshHandle>`
* ✅ Crear y eliminar entidades desde comandos IPC

## Motor Rust — ECS y física

* ✅ ECS funcional con almacenamiento denso por componente y query simple por tipo
* ✅ ECS avanzado: queries multi-componente y evaluación de archetypes
* ✅ Sistema de física (Rapier3D) integrado como `PhysicsWorld`
* ✅ Físicas y colisiones 2D
* ✅ Iluminación PBR (GGX + Fresnel-Schlick + Smith en `shader.wgsl`)

## Motor Rust — Optimizaciones de render

* ✅ Instanced rendering — batch by (mesh_idx, bind_group)
* ✅ Texture atlas — single 4096×4096 GPU texture via shelf packing
* ✅ Frustum culling — 2D AABB + 3D sphere-plane testing
* ✅ Render layers — explicit layer control + z-sorting
* ✅ Particionado espacial (spatial grid, 5.0 world units per cell, O(k) lookup)
* ✅ Rebuild de spatial grid en cada frame; `update_hover_2d` usa spatial grid para picking

## Motor Rust — Debug y herramientas

* ✅ Debug overlay runtime: FPS, frame time, draw calls, estado de física
* ✅ El overlay puede activarse/desactivarse en runtime
* ✅ Gizmos 3D interactivos (mover/rotar entidades desde la UI)
* ✅ Hint visual de snap a grilla en viewport durante drag de gizmo

## Motor Rust — Scripting y animaciones

* ✅ Scripting Lua con sandbox (sin `io`, `os`, `require`) y lifecycle (`on_start`, `update`, `on_stop`)
* ✅ Hooks de scripting para input/control y triggers (`on_press`, `on_trigger_enter`)
* ✅ Hot reload de scripts/shaders/assets
* ✅ Animaciones a base de frames para 2D

## Editor (Electron + React + TypeScript)

* ✅ Inicializar con `electron-vite` → template `react-ts`; `tsconfig.json` strict mode
* ✅ Estructura: `src/main/`, `src/preload/`, `src/renderer/`
* ✅ Crear `BrowserWindow` principal; obtener handle nativo con `getNativeWindowHandle()`
* ✅ Spawner del motor: `child_process.spawn('./engine', ['--embed', ...])`
* ✅ Pipe de stdin/stdout; reenvío de comandos y eventos via `ipcMain`/`webContents.send`
* ✅ Al cerrar la app: enviar `{ "cmd": "shutdown" }` y esperar cierre del proceso
* ✅ Exponer API segura con `contextBridge` en preload
* ✅ Tipos `EngineCommand` y `EngineEvent` en `src/shared/types.ts`
* ✅ Layout con flexbox: sidebar + área de viewport central con `ResizeObserver`
* ✅ Hook `useEngine()` extraído como módulo separado
* ✅ Panel de log con eventos del motor en tiempo real
* ✅ Botones: "Ping motor", "Color de fondo aleatorio", "Cargar modelo (.glb)"
* ✅ Componente `<SceneTree>` con lista de entidades de la escena
* ✅ Deshabilitar botones hasta `engineReady === true`

## Editor — Funcionalidades avanzadas

* ✅ Sistema de escenas múltiples: crear, renombrar, duplicar, eliminar y cambiar escena activa
* ✅ Guardado/carga de proyecto como `.save` (ZIP con `manifest.json` + `assets/` + `sounds/` + `scripting/`)
* ✅ Remapeo de rutas para portabilidad Windows/Linux al guardar/cargar
* ✅ Multi-selección en editor (ctrl + click)
* ✅ Undo/Redo para transformaciones y dibujo de colliders/triggers
* ✅ Empaquetado multiplataforma (`electron-builder`)

## Integración completa

* ✅ Al redimensionar ventana: enviar `{ "cmd": "set_bounds", ... }` → motor reconfigura y redibuja
* ✅ Flujo completo: click "Load Model" → `.glb` → modelo en motor → `<SceneTree>` actualizado
* ✅ Si el proceso Rust muere: overlay de error + botón "Reintentar"
* ✅ Si el motor no envía `ready` en 5s: timeout + mensaje de error

---

# 🔧 POR IMPLEMENTAR

> Ordenado por prioridad descendente.

## Prioridad alta

* [ ] **Físicas y colisiones 3D** — equivalente a lo ya implementado en 2D con Rapier3D
* [ ] **Versionado y migraciones de formato `.save`** — el campo `version` existe pero faltan migraciones automáticas (⚠️ crítico en cuanto existan proyectos persistentes reales)
  * DONE cuando cada cambio de formato incrementa `version`
  * DONE cuando existe migración automática y testeada entre al menos dos versiones

## Prioridad media

* [ ] **Animaciones 3D** (clips/animator/state machine) — compatible con Blender; consolidadas en pipeline de proyecto
* [ ] **Prefabs/blueprints** — guardar, instanciar y actualizar entidades reutilizables desde el editor sin trabajo manual repetitivo
  * DONE cuando el flujo es visible y usable desde el editor

## Prioridad baja

* [ ] **Jerarquía de entidades parent/child** — a evaluar si se incorpora al diseño
* [ ] **Evaluar optimizaciones de transporte IPC** — solo si aparecen bottlenecks medidos en escenas grandes

## Descartado por ahora

* ⏸️ Multiplayer
* ⏸️ IA generativa (posible uso futuro en interpolación de animaciones)
* ⏸️ Partículas / shaders experimentales (sin necesidad de producto concreta)

---

# ⚠️ Notas técnicas

## Window embedding por plataforma

| Plataforma | Handle | Mecanismo |
|---|---|---|
| Linux X11 | `XID` (u64) | `XReparentWindow` o child window |
| Linux Wayland | No soportado directamente | Usar XWayland como fallback |
| Windows | `HWND` (isize) | `SetParent` via Win32 API |

> **En Linux**: Si el sistema usa Wayland, lanzar Electron con `--ozone-platform=x11` o `ELECTRON_OZONE_PLATFORM_HINT=x11` para forzar X11.

## Alternativa de fallback si el embedding falla

1. Motor Rust abre su propia ventana (`--standalone`)
2. Electron la reposiciona junto a la UI usando coordenadas de pantalla
3. Migrar a embedding real post-MVP

## Crate `raw-window-handle`

```toml
# Engine/Cargo.toml
raw-window-handle = "0.6"
winit = { version = "0.30", features = ["rwh_06"] }
wgpu = "22"
glam = "0.29"
gltf = "1"
image = "0.25"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

## Estado actual del engine (Mayo 2026)

Nivel: base funcional + inicio de madurez.

* Editor usable: ✅
* Pipeline de assets/guardado: ⚠️ funcional, pero aún básico para crecimiento grande
* Rendimiento: ✅ (instanced rendering, atlas, frustum culling implementados)
* Escalabilidad de escena: ✅ (spatial grid implementado)

## Decisión de arquitectura vigente

* ✅ Mantener arquitectura Electron (editor) + Rust (motor) por IPC
* ⚠️ Vigilar volumen de eventos IPC para evitar cuellos de botella en escenas grandes
