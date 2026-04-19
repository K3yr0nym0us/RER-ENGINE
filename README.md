# Oxide Engine

Motor gráfico 3D experimental construido con Rust embebido dentro de Electron. El objetivo es crear un editor de escenas interactivo donde la UI (React) y el motor de render (Rust/wgpu) coexisten en la misma ventana, comunicándose mediante un protocolo IPC ligero.

---

## Idea del proyecto

Electron actúa como shell: gestiona la ventana principal, la UI del editor y el ciclo de vida de la aplicación. Rust se lanza como proceso hijo y renderiza en una **ventana X11 hija** embebida directamente dentro del viewport de Electron. La comunicación entre ambos procesos se realiza mediante **JSON lines por stdin/stdout**, sin WebSocket ni servidor intermedio.

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
| `glam 0.29` | Matemáticas 3D (Vec3, Mat4, Quat) |
| `gltf 1.x` | Carga de modelos `.glb` / `.gltf` |
| `image 0.25` | Carga de texturas (PNG, JPEG, etc.) |
| `rapier3d` | Motor de física (integrado, en desarrollo) |
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

- **`engine.rs`** — Estado principal wgpu: pipelines, render loop, picking de entidades, uniforms
- **`main.rs`** — Event loop winit, manejo de mouse/teclado, drag de gizmos
- **`camera.rs`** — Cámara órbita con yaw/pitch/zoom controlados por ratón
- **`ecs.rs`** — ECS propio: `EntityId`, `ComponentStorage<T>`, `World`, `Transform`, `MeshComponent`
- **`gizmo.rs`** — Flechas 3D (eje caja + cabeza pirámide) para mover entidades
- **`gizmo.wgsl`** — Shader de gizmos con tint por hover/active
- **`shader.wgsl`** — Shader PBR: distribución GGX, Fresnel-Schlick, geometría Smith
- **`ipc.rs`** — Protocolo JSON lines: `EngineCommand` (entrada) / `EngineEvent` (salida)
- **`mesh.rs`** — Loader .glb y cubo por defecto, upload a buffers wgpu
- **`physics.rs`** — `PhysicsWorld` con Rapier3D (estructura lista, sin step activo aún)
- **`texture.rs`** — Carga de imágenes a `wgpu::Texture`, fallback blanco 1×1

### Editor React (`src/renderer/src/`)

- **`App.tsx`** — Layout principal: sidebar con acordeón (Assets / Escena / Propiedades) + viewport + consola
- **`SceneTree.tsx`** — Lista de entidades de la escena con selección activa
- **`useEngine.ts`** — Hook que gestiona el ciclo de vida del IPC, estado del motor y entidades

---

## Protocolo IPC

**Electron → Motor (comandos)**
```jsonc
{ "cmd": "ping" }
{ "cmd": "load_model", "path": "/ruta/modelo.glb" }
{ "cmd": "set_bounds", "x": 268, "y": 0, "width": 1012, "height": 680 }
{ "cmd": "set_transform", "id": 0, "position": [0,0,0], "rotation": [0,0,0,1], "scale": [1,1,1] }
{ "cmd": "shutdown" }
```

**Motor → Electron (eventos)**
```jsonc
{ "event": "ready" }
{ "event": "model_loaded", "id": 0 }
{ "event": "entity_selected", "id": 0, "name": "Cube", "position": [...], "rotation": [...], "scale": [...] }
{ "event": "entity_deselected" }
{ "event": "error", "message": "..." }
```

---

## Requisitos

- Linux con X11 (Wayland no soportado directamente; usa XWayland)
- Rust toolchain estable (`rustup`)
- Node.js ≥ 18
- yarn (`npm i -g yarn`)
- Dependencias de sistema para wgpu/GL: `libgl1`, `libgles2`, `libx11-dev`

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
| Cámara órbita interactiva | ✅ Completo |
| ECS (entidades, componentes) | ✅ Completo |
| Carga de modelos .glb | ✅ Completo |
| Gizmos 3D (mover por eje) | ✅ Completo |
| UI con Bootstrap (acordeón) | ✅ Completo |
| IPC JSON lines | ✅ Completo |
| Física Rapier3D | 🔧 Integrada, sin activar |
| Scripting (Lua/Python) | ⏳ Pendiente |
| Soporte Windows | ⏳ Sin probar |


