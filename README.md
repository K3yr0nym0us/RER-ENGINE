# RER-ENGINE

> **R**eact + **E**lectron + **R**ust ENGINE

Motor gráfico 2D y 3D experimental construido con Rust embebido dentro de Electron. El objetivo es crear un editor de escenas interactivo donde la UI (React) y el motor de render (Rust/wgpu) coexisten en la misma ventana, comunicándose mediante un protocolo IPC ligero.

---

## Idea del proyecto

Electron actúa como shell: gestiona la ventana principal, la UI del editor y el ciclo de vida de la aplicación. Rust se lanza como proceso hijo y renderiza en una **ventana nativa embebida** directamente dentro del viewport de Electron. La comunicación entre ambos procesos se realiza mediante **JSON lines por stdin/stdout**, sin WebSocket ni servidor intermedio.

- **Linux (X11):** embedding mediante XEMBED (`with_embed_parent_window`).
- **Windows:** embedding mediante owned popup Win32 (`SetWindowLongPtrW` + `GWLP_HWNDPARENT`) para evitar la intercepción de eventos por parte de Chromium.

```
┌─────────────────────────────────────────────┐
│             Electron (BrowserWindow)         │
│  ┌──────────────────┐  ┌───────────────────┐ │
│  │   React + TS     │  │  Viewport nativo  │ │
│  │   (UI/Editor)    │  │  ← Rust / wgpu    │ │
│  └──────────────────┘  └───────────────────┘ │
└─────────────────────────────────────────────┘
          ↑  IPC — JSON lines stdin/stdout
```

---

## Tecnologías

### Motor (Rust)

| Crate | Uso |
|---|---|
| `wgpu 22` | API gráfica multiplataforma (Vulkan/GL/Metal) |
| `winit 0.30` | Gestión de ventanas y eventos de entrada |
| `glam 0.28` | Matemáticas 2D/3D (Vec, Mat, Quat) |
| `gltf 1.x` | Carga de modelos `.glb` / `.gltf` |
| `image 0.25` | Carga de texturas (PNG, JPEG, etc.) |
| `rapier3d` | Motor de física para modos 2D y 3D |
| `mlua` | Scripting Lua 5.4 embebido en el motor |
| `serde` / `serde_json` | Serialización del protocolo IPC |
| `bytemuck` | Casting seguro de structs a bytes para wgpu |

### Editor (Electron + React)

| Tecnología | Uso |
|---|---|
| Electron | Shell nativo, gestión de ventanas y procesos |
| electron-vite 2.3.0 | Build tool y dev server |
| React 18 + TypeScript | Interfaz del editor |
| Bootstrap 5.3.8 | Componentes y estilos UI |
| react-bootstrap 2.10.10 | Componentes Bootstrap para React |
| yarn | Gestión de dependencias |

---

## Arquitectura interna

### Motor Rust (`src/main/Engine/src/`)

- **`engine.rs`** — Estado principal del motor: render loop, entidades, comandos IPC y sincronización editor/juego
- **`main.rs`** — Event loop `winit`, embedding Linux/Windows, input de mouse/teclado y dispatch de comandos
- **`CONFIG_2D/`** — Lógica 2D (cámara, grilla, herramientas, picking, física 2D)
- **`CONFIG_3D/`** — Lógica 3D (cámara, mallas, picking, física 3D)
- **`CONFIG_BASE/`** — Escena base y setup inicial compartido
- **`CONFIG_SHARED/`** — Helpers y utilidades comunes entre modos
- **`ecs.rs`** — ECS propio: `EntityId`, `World`, `Transform`, `MeshComponent`, etc.
- **`gizmo.rs` + `gizmo.wgsl`** — Render y control de gizmos de transformación
- **`shader.wgsl`** — Shader principal de render
- **`ipc.rs`** — Protocolo JSON lines: `EngineCommand` (entrada) y `EngineEvent` (salida)
- **`mesh.rs`** — Carga de mallas/modelos
- **`texture.rs`** — Carga de texturas y fallback
- **`scripting.rs`** — Runtime Lua (attach/detach scripts, tick y comandos de script)

### Editor React (`src/renderer/src/`)

- **`App.tsx`** — Flujo de entrada: selector de tipo de proyecto, carga de `.save` y entrada al editor
- **`pages/EngineView/`** — Vista principal del editor (sidebar, topbar, viewport y consola)
- **`context/useContextEngine/`** — Estado global del motor en frontend (reducer + acciones + event handler)
- **`hooks/`** — Hooks de UI/editor (autosave, herramientas de dibujo, sprites, scripting)

---

## Protocolo IPC

**Electron → Motor (comandos)**
```jsonc
{ "cmd": "ping" }
{ "cmd": "load_model", "path": "/ruta/modelo.glb" }
{ "cmd": "set_bounds", "x": 268, "y": 0, "width": 1012, "height": 680 }
{ "cmd": "set_transform", "id": 0, "position": [0,0,0], "rotation": [0,0,0,1], "scale": [1,1,1] }
{ "cmd": "set_preview_playing", "playing": true }
{ "cmd": "undo" }
{ "cmd": "shutdown" }
```

**Motor → Electron (eventos)**
```jsonc
{ "event": "ready" }
{ "event": "model_loaded", "id": 0 }
{ "event": "entity_selected", "id": 0, "name": "Cube", "position": [...], "rotation": [...], "scale": [...] }
{ "event": "entity_deselected" }
{ "event": "collider_created", "id": 12, "points": [[...],[...],[...],[...]] }
{ "event": "error", "message": "..." }
```

---

## Requisitos

- **Linux:** X11 o XWayland (Wayland puro no soportado). Dependencias: `libgl1`, `libgles2`, `libx11-dev`
- **Windows:** Windows 10/11 con drivers GPU actualizados
- Rust toolchain estable (`rustup`)
- Node.js ≥ 18
- yarn (`npm i -g yarn`)

---

## Desarrollo

```bash
# Instalar dependencias JS
yarn

# Iniciar en modo desarrollo (compila Rust automáticamente)
yarn dev

# Build de producción
yarn build

# Empaquetar (AppImage / deb)
yarn dist
```

> `yarn dev` ejecuta `cargo build` automáticamente antes de iniciar Electron gracias al script `predev` en `package.json`.

---

## Estado del proyecto

| Área | Estado |
|---|---|
| Motor Rust embebido en Electron | ✅ Completo |
| Render wgpu (PBR, texturas, mallas) | ✅ Completo |
| Cámara interactiva (2D/3D) | ✅ Completo |
| ECS (entidades, componentes) | ✅ Completo |
| Carga de modelos .glb | ✅ Completo |
| Gizmos y manipulación de entidades | ✅ Completo |
| UI editor (sidebar/topbar/modales) | ✅ Completo |
| IPC JSON lines | ✅ Completo |
| Física Rapier (2D y 3D) | ✅ Completo (por entidad: dinámico / estático / cinemático) |
| Herramientas 2D (grilla, colisionadores, drawing tool) | ✅ Completo |
| Modo Play/Stop para prueba en editor | ✅ Completo |
| Undo básico de editor | ✅ Implementado |
| Scripting (Lua / mlua) | ✅ Implementado |
| Soporte Linux Ubuntu | ✅ Probado en Ubuntu (X11 / WSL2 + WSLg) |
| Soporte Windows | ✅ Probado en Windows 11 |


