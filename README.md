# RER-ENGINE

> **R**eact + **E**lectron + **R**ust ENGINE

Motor de videojuegos 2D/3D con editor integrado. La idea central: **crear comportamiento de forma directa**, sin repartir la lógica de cada entidad entre muchos sistemas abstractos.

---

## Idea del motor

RER-ENGINE separa el producto en dos capas que hablan por JSON (stdin/stdout):

| Capa | Rol |
|------|-----|
| **Editor** (Electron + React + TypeScript) | Interfaz, escenas, guardado `.save`, herramientas de autoría. |
| **Motor** (Rust + wgpu) | Render, física, scripting, play mode. **wgpu** con backend según binario (ver [Política gráfica](#política-gráfica-gpu)). |

Hay **dos motores independientes**, no uno híbrido:

- `rer_engine_2d` — sprites, plano XY, colliders y execution areas.
- `rer_engine_3d` — mallas glTF/GLB, cámara orbital en editor, cámara play character en play (modo `first-person` en manifest), editor **Player UI HUD** (pantallas, texto, botones, imágenes, objetos 2D en NDC).

Electron arranca el binario que corresponda según el tipo de proyecto. Comparten protocolo IPC y utilidades (`engine_shared`), pero **no comparten runtime de juego**.

### Viewport: overlay (no embebido en Chromium)

El render **no** ocurre dentro del DOM de Electron. Es un **proceso hijo** con ventana nativa (winit) alineada al hueco del editor:

```
┌────────────────────────────────────────────────────────────┐
│  Electron (BrowserWindow — UI React)                       │
│  ┌─────────────┐  ┌─────────────────────────────────────┐  │
│  │ Sidebars,   │  │ <main class="engine-viewport-area"> │  │
│  │ tabs, log   │  │  (div transparente — “hueco”)       │  │
│  └─────────────┘  └─────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────┘
         │  getBoundingClientRect × DPR
         ▼  IPC viewport-bounds / set_bounds
┌─────────────────────────────────────────────────────────┐
│  rer_engine_2d | rer_engine_3d (ventana winit overlay)  │
│  rer_engine_2d | rer_engine_3d — Vulkan (Windows/Linux) │
└─────────────────────────────────────────────────────────┘
```

| Plataforma | Comportamiento |
|------------|----------------|
| **Windows** | Popup separado; `GWLP_HWNDPARENT` + position-tracker (WinEventHook). |
| **Linux X11** | Ventana separada; `XSetTransientForHint` + position-tracker (`ConfigureNotify`). |
| **Wayland** | No soportado nativamente; usar XWayland / `ELECTRON_OZONE_PLATFORM_HINT=x11`. |

Principio **engine-first**: posiciones, cámaras, física y convenciones espaciales las resuelve el motor; el frontend envía intención y refleja eventos. Detalle en [`src/renderer/ARCHITECTURE.md`](src/renderer/ARCHITECTURE.md) y en los `ARCHITECTURE.md` de cada crate.

---

## Política gráfica (GPU)

| Regla | Detalle |
|-------|---------|
| **Motor 2D y 3D** | Siempre **Vulkan** (Windows y Linux). |
| **Prohibido** | OpenGL, EGL, `Backends::all()`, otros backends wgpu (p. ej. DX12). |
| **Shaders** | WGSL compilado con naga; portable entre backends wgpu. |
| **Si la GPU falla** | El editor Electron **sí** abre; el viewport muestra ayuda. El motor emite `{"event":"error"}` y no envía `ready`. |

Requisitos: **Vulkan** en Windows y Linux (`vulkaninfo` o drivers actualizados).

### Implementación (`engine_shared/src/gpu.rs`)

| Pieza | Rol |
|-------|-----|
| `EngineGpuProfile::TwoD` / `ThreeD` | Lo fija cada binario al llamar `init_gpu(window, profile)`. |
| `resolve_backend(profile)` | Siempre Vulkan; **no** lee variables de entorno. |
| `init_gpu` | Una API wgpu por proceso; fallo → `GpuInitError` → IPC `{"event":"error"}`. |
| Electron `startEngine` | Elimina `RER_GPU_BACKEND` del entorno del hijo para que no pueda anular la política. |
| `EngineGpuErrorOverlay` | Ayuda en el viewport si el motor no envía `ready`. |

`yarn dev` / `yarn start` **no** definen backend GPU; solo compilan motores y arrancan electron-vite (`RER_ENGINE_PROFILE` opcional para release).

---

## Lenguajes y stack

| Área | Tecnologías |
|------|-------------|
| Motor | Rust, **wgpu** (Vulkan), winit, glam, Rapier, **rhai**, gltf/image |
| Editor | Electron, React, TypeScript, Vite (electron-vite) |
| Scripts de juego | Rhai (sandbox: API `engine.*` registrada por el motor) |
| Contrato IPC | JSON — tipos en `src/shared-types/types.ts` |

---

## Requisitos

- **Node.js** 20+ y **Yarn**
- **Rust** (toolchain estable) y **Cargo**
- **Windows 11** o **Linux con X11** (viewport overlay; en Wayland usar XWayland)
- **GPU**: **Vulkan** para `rer_engine_2d` y `rer_engine_3d` (drivers + `vulkaninfo` en WSL2 si aplica).

---

## Cómo ejecutar

Desde la raíz del repositorio:

```bash
# Instalar dependencias (primera vez)
yarn

# Desarrollo: compila motores (debug) y abre el editor
yarn dev

# Desarrollo con motores en release (más rápido al renderizar)
yarn start

# Build del editor (sin empaquetar instalador)
yarn build

# Vista previa del build
yarn preview

# Instalador (compila motores release + electron-builder)
yarn dist
```

Compilar solo los motores (opcional):

```bash
cargo build --manifest-path src/main/Engine/Cargo.toml -p rer-engine-2d -p rer-engine-3d
```

Los ejecutables quedan en `src/main/Engine/target/debug/` o `target/release/`.

Logs del motor (opcional): `RUST_LOG=info` o `RUST_LOG=rer_engine_2d=debug`.

---

## Documentación

| Archivo | Contenido |
|---------|-----------|
| [docs/RHAI_API.yaml](./docs/RHAI_API.yaml) | API de scripting Rhai (2D y 3D), resumida |
| [docs/Programing_Model.yaml](./docs/Programing_Model.yaml) | Modelo de programación visual (nodos → Rhai, UI, persistencia, IPC modal) |
| [docs/Plugins_Model.yaml](./docs/Plugins_Model.yaml) | Plugins opcionales (catálogo, IPC, persistencia) |
| [docs/AI_Assistant_Plugin.yaml](./docs/AI_Assistant_Plugin.yaml) | Plugin asistente IA local (Qwen3, llama.cpp, chat) |
| [docs/README.md](./docs/README.md) | Índice de documentación (funcionalidad implementada) |
| [CHECKLIST.md](./CHECKLIST.md) | Backlog global pendiente |
| [CHECKLIST-2D.md](./CHECKLIST-2D.md) | Pendiente motor / editor 2D |
| [CHECKLIST-3D.md](./CHECKLIST-3D.md) | Pendiente motor / editor 3D |
| [`src/main/Engine/engine_2d/ARCHITECTURE.md`](src/main/Engine/engine_2d/ARCHITECTURE.md) | Contrato del motor 2D |
| [`src/main/Engine/engine_3d/ARCHITECTURE.md`](src/main/Engine/engine_3d/ARCHITECTURE.md) | Contrato del motor 3D |
| [`src/renderer/ARCHITECTURE.md`](src/renderer/ARCHITECTURE.md) | Rol del frontend y qué no duplicar |
| [`.cursor/rules/rer-engine-project.mdc`](.cursor/rules/rer-engine-project.mdc) | Convenciones permanentes para agentes Cursor (inglés) |

---

## Proyectos `.save`

Los proyectos se guardan como ZIP portable (`manifest.json`, `assets/`, `sounds/`, `scripting/`) para moverlos entre máquinas sin romper rutas.

---

## Licencia

MIT — ver autor en `package.json`.
