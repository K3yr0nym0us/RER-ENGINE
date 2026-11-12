# RER-ENGINE

> **R**eact + **E**lectron + **R**ust ENGINE

Motor de videojuegos 2D/3D con editor integrado. La idea central: **crear comportamiento de forma directa**, sin repartir la lógica de cada entidad entre muchos sistemas abstractos.

---

## Idea del motor

RER-ENGINE separa el producto en Electron (UI, persistencia y modales) y un **editor nativo Rust** que abre la ventana winit y hospeda el motor 2D/3D in-process. Electron y el editor hablan por JSON (stdin/stdout).

| Capa | Rol |
|------|-----|
| **Editor** (Electron + React + TypeScript) | Interfaz, escenas, guardado `.save`, herramientas de autoría. |
| **Editor nativo** (Rust + egui) | Ventana winit que hospeda el motor 2D/3D in-process sobre Ash/Vulkan. Electron solo lanza el editor y abre las modales. |

Hay **dos motores independientes**, no uno híbrido:

- `rer_engine_2d` — sprites, plano XY, colliders y execution areas.
- `rer_engine_3d` — mallas glTF/GLB, cámara orbital en editor, cámara play character en play (modo `first-person` en manifest), editor **Player UI HUD** (pantallas, texto, botones, imágenes, objetos 2D en NDC).

Electron lanza el editor (`rer_engine_editor`); el editor hospeda el motor 2D o 3D in-process según el tipo de proyecto. Comparten protocolo IPC y utilidades (`rer-engine-core`), pero **no comparten runtime de juego**.

### Viewport: ventana única del editor

El editor egui (`rer_engine_editor`) abre su propia ventana winit y hospeda el motor 2D/3D **in-process** sobre el mismo `SharedGpu` y swapchain. Electron queda como host del selector de proyecto y de las modales; ya no hay ventana overlay ni tracker Win32.

Los binarios `rer_engine_2d` / `rer_engine_3d` siguen existiendo como standalone de debug, con ventana propia y sin overlay.

Principio **engine-first**: posiciones, cámaras, física y convenciones espaciales las resuelve el motor; el frontend envía intención y refleja eventos. Detalle en [`src/renderer/ARCHITECTURE.md`](src/renderer/ARCHITECTURE.md) y en los `ARCHITECTURE.md` de cada crate.

---

## Política gráfica (GPU)

| Regla | Detalle |
|-------|---------|
| **Motor 2D y 3D** | Siempre **Vulkan** (Windows) con **Ash**. |
| **Prohibido** | OpenGL, EGL, DX12, `wgpu`/`Backends::all()` y cualquier selección de backend por entorno. |
| **Shaders** | GLSL en `engine/shaders/` (`2d/`, `raster/`, `rt/`, `bvh/`), compilado a SPIR-V con `glslc`. |
| **Si la GPU falla** | El editor abre y muestra el error; el motor emite `{"event":"error"}` y no envía `ready`. |

Requisitos: **Vulkan** en Windows (`vulkaninfo` o drivers actualizados).

### Implementación (`core/src/vulkan/gpu/`)

| Pieza | Rol |
|-------|-----|
| `EngineGpuProfile::TwoD` / `ThreeD` | Perfil del motor standalone (2D sin RT; 3D con opciones RT). |
| `SharedGpu::new(window, options)` | Un `VkInstance`/`VkDevice`/swapchain por ventana; 2D y 3D comparten el núcleo. |
| `VulkanOptions` | `default()` para 2D; `three_d()` para el 3D. |
| `GpuInitError` | Fallo de arranque → IPC `{"event":"error"}`. |
| Editor `EditorGpu` | Crea el `SharedGpu` de la ventana y lo comparte con el motor in-process. |
| `EngineGpuErrorOverlay` | Ayuda en el viewport si el motor no envía `ready`. |

`yarn dev` / `yarn start` **no** definen backend GPU; solo compilan motores y arrancan electron-vite (`RER_ENGINE_PROFILE` opcional para release).

---

## Lenguajes y stack

| Área | Tecnologías |
|------|-------------|
| Motor | Rust, **Ash/Vulkan**, winit, glam, Rapier, **rhai**, gltf/image |
| Editor | Electron, React, TypeScript, Vite (electron-vite) |
| Scripts de juego | Rhai (sandbox: API `engine.*` registrada por el motor) |
| Contrato IPC | JSON — tipos en `src/shared-types/types.ts` |

---

## Requisitos

- **Node.js** 20+ y **Yarn**
- **Rust** (toolchain estable) y **Cargo**
- **Windows 10/11**
- **GPU**: **Vulkan** para el editor y los binarios standalone `rer_engine_2d` / `rer_engine_3d` (drivers actualizados).

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
cargo build --manifest-path src/main/Engine/Cargo.toml -p rer-engine --bin rer_engine_2d --no-default-features --features engine2d
cargo build --manifest-path src/main/Engine/Cargo.toml -p rer-engine --bin rer_engine_3d --no-default-features --features engine3d
```

Los ejecutables quedan en `src/main/Engine/target/debug/` o `target/release/`.

Logs del motor (opcional): `RUST_LOG=info` o `RUST_LOG=rer_engine::engine2d=debug`.

---

## Documentación

| Archivo | Contenido |
|---------|-----------|
| [docs/yaml/scripting/RHAI_API.yaml](./docs/yaml/scripting/RHAI_API.yaml) | API de scripting Rhai (2D y 3D), resumida |
| [docs/yaml/scripting/Programing_Model.yaml](./docs/yaml/scripting/Programing_Model.yaml) | Modelo de programación visual (nodos → Rhai, UI, persistencia, IPC modal) |
| [docs/yaml/plugins/Plugins_Model.yaml](./docs/yaml/plugins/Plugins_Model.yaml) | Plugins opcionales (catálogo, IPC, persistencia) |
| [docs/ai/AI_Assistant_Plugin.yaml](./docs/ai/AI_Assistant_Plugin.yaml) | Plugin asistente RER-AI (OpenCode free, sandbox docs/ai, tools de UI) |
| [docs/README.md](./docs/README.md) | Índice de documentación (funcionalidad implementada) |
| [`src/main/Engine/engine/src/engine2d/ARCHITECTURE.md`](src/main/Engine/engine/src/engine2d/ARCHITECTURE.md) | Contrato del motor 2D |
| [`src/main/Engine/engine/ARCHITECTURE.md`](src/main/Engine/engine/ARCHITECTURE.md) | Contrato del motor 3D |
| [`src/renderer/ARCHITECTURE.md`](src/renderer/ARCHITECTURE.md) | Rol del frontend y qué no duplicar |
| [`.cursor/rules/project.mdc`](.cursor/rules/project.mdc) | Reglas del proyecto (índice de reglas: `engine`, `frontend`, `rust-quality`, `frontend-quality`) |

---

## Proyectos `.save`

Los proyectos se guardan como ZIP portable (`manifest.json`, `assets/`, `sounds/`, `scripting/`) para moverlos entre máquinas sin romper rutas.

---

## Licencia

MIT — ver autor en `package.json`.
