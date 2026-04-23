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

# 🧱 FASE 1 — Motor Rust (standalone, para validar render)

## Setup

* ✅ Crear workspace Cargo en `Engine/` (`cargo init --name rer-engine`)
* ✅ Dependencias: `winit`, `wgpu`, `raw-window-handle`, `serde`, `serde_json`, `gltf`, `image`
* ✅ Estructura de módulos: `main.rs`, `engine.rs`, `ipc.rs`, `shader.wgsl`

## Ventana y render (modo standalone)

* ✅ Crear ventana propia con `winit::EventLoop` y `WindowBuilder`
* ✅ Inicializar `wgpu`: `Instance` → `Surface` → `Adapter` → `Device` + `Queue`
* ✅ Renderizar clear color configurable
* ✅ Renderizar triángulo con vertex/fragment shader (WGSL)
* ✅ Manejar evento `Resized` y reconfigurar `SurfaceConfiguration`

## Game loop

* ✅ `EventLoop::run` con separación `update()` / `render()`
* ✅ Manejar `WindowEvent::CloseRequested`
* ✅ Delta time básico (`std::time::Instant`)

## Cámara básica

* ✅ Struct `Camera` con posición, yaw, pitch
* ✅ Matriz View (`glam::Mat4::look_at_rh`)
* ✅ Matriz Projection (perspectiva, `glam::Mat4::perspective_rh`)
* ✅ Uniform buffer en wgpu + bind group

---

# 🔌 FASE 2 — Soporte para ventana embebida (clave)

## Modo dual de arranque

* ✅ Parsear args: `--standalone` vs `--embed <window_id> <x> <y> <width> <height>`
* ✅ En modo `--embed`: crear ventana hija usando el handle recibido
  * Linux (X11): usar `XID` con `raw-window-handle::XlibWindowHandle`
  * Windows: usar `HWND` con `raw-window-handle::Win32WindowHandle`
* ✅ `wgpu::Surface` creada desde el handle de la ventana hija
* ✅ Sin decoraciones (`window.set_decorations(false)`)
* ✅ Sin barra de título, borderless, no resizable (Electron controla el tamaño)

## IPC — Protocolo stdin/stdout (JSON lines)

* ✅ Hilo dedicado para leer stdin (`BufReader<stdin>`)
* ✅ Deserializar cada línea como `EngineCommand` con `serde_json`
* ✅ Enviar eventos al loop principal vía `mpsc::channel`
* ✅ Responder por stdout con `println!("{}", serde_json::to_string(...))`

### Comandos mínimos a soportar

```jsonc
// Motor → Electron (eventos)
{ "event": "ready" }
{ "event": "error", "message": "..." }

// Electron → Motor (comandos)
{ "cmd": "ping" }
{ "cmd": "load_model", "path": "assets/cube.glb" }
{ "cmd": "resize", "width": 800, "height": 600 }
{ "cmd": "set_clear_color", "r": 0.1, "g": 0.1, "b": 0.15 }
{ "cmd": "shutdown" }
```

---

# 📦 FASE 3 — Assets y escena

## Carga de modelos

* ✅ Loader `.glb` con crate `gltf` (solo mallas + materiales básicos)
* ✅ Subir vértices e índices a `wgpu::Buffer`
* ✅ Renderizar modelo con shader básico (Blinn-Phong o flat shading)

## Texturas

* ✅ Cargar imagen con crate `image` → `wgpu::Texture`
* ✅ Bind group con sampler + texture view
* ✅ Aplicar textura al modelo cargado

## Escena mínima

* ✅ Struct `Entity` con ID + `Transform` (posición, rotación, escala)
* ✅ Struct `Scene` con `Vec<Entity>` + `HashMap<EntityId, MeshHandle>`
* ✅ Crear y eliminar entidades desde comandos IPC

---

# 🪟 FASE 4 — Editor en Electron (React + TypeScript)

## Setup del proyecto

* ✅ Inicializar en `UI/` con `electron-vite` → template `react-ts`
* ✅ Configurar `tsconfig.json` (strict mode)
* ✅ Instalar: `@types/node`, `electron-builder`
* ✅ Estructura: `src/main/`, `src/preload/`, `src/renderer/`

## Main process (`src/main/index.ts`)

* ✅ Crear `BrowserWindow` principal (frameless o con frame, a elección)
* ✅ Obtener el XID nativo de la ventana principal: `mainWindow.getNativeWindowHandle()`
* ✅ Spawner del motor: `child_process.spawn('./engine', ['--embed', xid, x, y, w, h])`
* ✅ Pipe de stdin/stdout: `engine.stdin.write(...)` / `engine.stdout.on('data', ...)`
* ✅ Reenviar comandos del renderer al motor via `ipcMain.on('engine:cmd', ...)`
* ✅ Reenviar eventos del motor al renderer via `mainWindow.webContents.send('engine:event', ...)`
* ✅ Al cerrar la app: enviar `{ "cmd": "shutdown" }` y esperar cierre del proceso

## Preload (`src/preload/index.ts`)

* ✅ Exponer API segura con `contextBridge`:
  ```ts
  window.engine = {
    send: (cmd: EngineCommand) => ipcRenderer.send('engine:cmd', cmd),
    on: (cb: (event: EngineEvent) => void) => ipcRenderer.on('engine:event', (_, e) => cb(e))
  }
  ```
* ✅ Tipos `EngineCommand` y `EngineEvent` en `src/shared/types.ts`

## Renderer (`src/renderer/`)

* ✅ Layout con flexbox: sidebar izquierdo + área de viewport central
* ✅ El área de viewport es un `<div>` que reporta bounds vía `ResizeObserver` → el motor X11 renderiza encima
* ✅ Estado: `engineReady: boolean`, `engineError: string|null`, `log: string[]` en `App.tsx`
* ✅ Botón "Ping motor" y "Color de fondo aleatorio"
* ✅ Panel de log: muestra eventos del motor en tiempo real
* ✅ Hook `useEngine()` extraído como módulo separado
* ✅ Botón "Cargar modelo (.glb)" (abre `dialog.showOpenDialog`)
* ✅ Componente `<SceneTree>`: lista entidades de la escena (stub)
* ✅ Deshabilitar botones hasta que `engineReady === true`

---

# 🔗 FASE 5 — Integración completa

## Sincronización de viewport

* ✅ Al redimensionar la ventana principal: calcular nuevo tamaño del viewport
* ✅ Enviar `{ "cmd": "set_bounds", "x": x, "y": y, "width": w, "height": h }` al motor
* ✅ Motor reconfigura `SurfaceConfiguration` y redibuja

## Flujo de carga de modelo

* ✅ Click "Load Model" → `dialog.showOpenDialog` filtra `.glb/.gltf`
* ✅ Enviar `{ "cmd": "load_model", "path": "..." }` al motor
* ✅ Motor responde `{ "event": "model_loaded", "id": 0 }` o `{ "event": "error" }`
* ✅ UI actualiza `<SceneTree>` con la entidad creada

## Manejo de errores

* ✅ Si el proceso Rust muere: mostrar overlay de error en UI + botón "Reintentar"
* ✅ Si el motor no envía `ready` en 5s: timeout + mensaje de error

---

# 🎯 MVP FINAL

El sistema debe permitir:

* ✅ Abrir Electron → motor Rust se inicia automáticamente embebido
* ✅ Motor renderiza dentro del área de viewport (sin ventana separada)
* ✅ Click "Load Model" → seleccionar `.glb` → modelo aparece en el motor
* ✅ Redimensionar ventana → viewport se ajusta correctamente
* ✅ Cerrar Electron → proceso del motor termina limpiamente

---

# 🧠 POST-MVP (no implementar aún)

* ✅ ECS completo (entidades/componentes desacoplados)
* ✅ Sistema de física (Rapier3D) — integrado como `PhysicsWorld`, sin step activo en game loop aún
* ✅ Iluminación PBR (physically based rendering) — GGX + Fresnel-Schlick + Smith en `shader.wgsl`
* ✅ Gizmos 3D interactivos (mover/rotar entidades desde la UI)
* ✅ Empaquetado multiplataforma (`electron-builder`)
* ✅ Fisicas y colisiones 2D
* [ ] Fisicas y colisiones 3D
* [ ] Animaciones a base de frames para el 2D
* [ ] Animaciones para el 3D (creo que con que sea compatible con las de blender basta)
* [ ] Scripting (Lua o Python embebido)

👉 Solo cuando el MVP esté estable y probado.

---

# ⚠️ Notas técnicas importantes

## Window embedding por plataforma

| Plataforma | Handle | Mecanismo |
|---|---|---|
| Linux X11 | `XID` (u64) | `XReparentWindow` o child window |
| Linux Wayland | No soportado directamente | Usar XWayland como fallback |
| Windows | `HWND` (isize) | `SetParent` via Win32 API |

> **En Linux**: Si el sistema usa Wayland, lanzar Electron con `--ozone-platform=x11` o `ELECTRON_OZONE_PLATFORM_HINT=x11` para forzar X11.

## Alternativa de fallback si el embedding falla

Si el embedding nativo resulta demasiado complejo en una plataforma:
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

---

# ✅ Regla de oro

Si puedes:

* Abrir la app
* Ver el motor renderizando dentro de la ventana
* Cargar un modelo con un click

👉 Ya tienes una base sólida para crecer 🚀
