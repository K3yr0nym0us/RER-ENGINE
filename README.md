# RER-ENGINE

> **R**eact + **E**lectron + **R**ust ENGINE

Motor de videojuegos 2D/3D con editor integrado. La idea central: **crear comportamiento de forma directa**, sin repartir la lógica de cada entidad entre muchos sistemas abstractos.

---

## Idea del motor

RER-ENGINE separa el producto en dos capas que hablan por JSON (stdin/stdout):

| Capa | Rol |
|------|-----|
| **Editor** (Electron + React + TypeScript) | Interfaz, escenas, guardado `.save`, herramientas de autoría. |
| **Motor** (Rust + wgpu) | Render, física, scripting, play mode. **API gráfica: wgpu; backend activo: solo Vulkan.** |

Hay **dos motores independientes**, no uno híbrido:

- `rer_engine_2d` — sprites, plano XY, colliders y execution areas.
- `rer_engine_3d` — mallas glTF/FBX, cámara orbital, primera persona en play.

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
│  Vulkan vía wgpu — misma posición/tamaño que el hueco   │
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
| **Hoy** | Solo **Vulkan** (`Backends::VULKAN` en `engine_shared::gpu`). |
| **Prohibido** | OpenGL, EGL, `Backends::all()`, fallback silencioso a otros backends. |
| **Futuro (Windows)** | **DirectX 12** como alternativa de arranque (`RER_GPU_BACKEND=dx12`), **una** API por sesión; no mezclar Vulkan y DX12 en el mismo proceso. |
| **Shaders** | WGSL compilado con naga; no implica backend OpenGL. |
| **Si Vulkan falla** | El editor Electron **sí** abre; el viewport muestra ayuda (drivers, WSL). El motor emite `{"event":"error"}` y no envía `ready`. |

Requisito de sistema: drivers Vulkan funcionales (`vulkaninfo` / GPU en WSLg si usas WSL2).

---

## Lenguajes y stack

| Área | Tecnologías |
|------|-------------|
| Motor | Rust, **wgpu (Vulkan)**, winit, glam, Rapier, mlua, gltf/image |
| Editor | Electron, React, TypeScript, Vite (electron-vite) |
| Scripts de juego | Lua (sandbox: sin `io`, `os`, `require`) |
| Contrato IPC | JSON — tipos en `src/shared-types/types.ts` |

---

## Requisitos

- **Node.js** 20+ y **Yarn**
- **Rust** (toolchain estable) y **Cargo**
- **Windows 11** o **Linux con X11** (viewport overlay; en Wayland usar XWayland)
- **GPU con soporte Vulkan** (drivers actualizados; en WSL2: WSLg + `mesa-vulkan-drivers` o drivers NVIDIA para WSL)

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
| [LUA_API.md](./LUA_API.md) | API de scripting Lua (2D y 3D), resumida |
| [CHECKLIST-2D.md](./CHECKLIST-2D.md) | Hecho y pendiente — motor / editor 2D |
| [CHECKLIST-3D.md](./CHECKLIST-3D.md) | Hecho y pendiente — motor / editor 3D |
| [`src/main/Engine/engine_2d/ARCHITECTURE.md`](src/main/Engine/engine_2d/ARCHITECTURE.md) | Contrato del motor 2D |
| [`src/main/Engine/engine_3d/ARCHITECTURE.md`](src/main/Engine/engine_3d/ARCHITECTURE.md) | Contrato del motor 3D |
| [`src/renderer/ARCHITECTURE.md`](src/renderer/ARCHITECTURE.md) | Rol del frontend y qué no duplicar |

---

## Proyectos `.save`

Los proyectos se guardan como ZIP portable (`manifest.json`, `assets/`, `sounds/`, `scripting/`) para moverlos entre máquinas sin romper rutas.

---

## Licencia

MIT — ver autor en `package.json`.
