# RER-ENGINE

> **R**eact + **E**lectron + **R**ust ENGINE

Motor de videojuegos 2D/3D con editor integrado. La idea central: **crear comportamiento de forma directa**, sin repartir la lógica de cada entidad entre muchos sistemas abstractos.

---

## Idea del motor

RER-ENGINE separa el producto en un **launcher Electron** y un **editor nativo Rust** que abre la ventana winit y hospeda el motor 2D/3D in-process. Electron y el editor hablan por JSON (stdin/stdout).

| Capa | Rol |
|------|-----|
| **Launcher** (Electron + React + TypeScript) | Selector de proyecto 2D/3D, apertura de `.save`, idioma de la UI y arranque del editor. **No es el editor.** |
| **Editor** (Rust + egui) | Ventana winit que hospeda el motor 2D/3D in-process sobre Ash/Vulkan. Posee escenas, entidades, propiedades, HUD, scripting y guardado. |

Hay **dos motores independientes**, no uno híbrido:

- `rer_engine_2d` — sprites, plano XY, colliders y execution areas.
- `rer_engine_3d` — mallas glTF/GLB, cámara orbital en editor, cámara play character en play (modo `first-person` en manifest), editor **Player UI HUD** (pantallas, texto, botones, imágenes, objetos 2D en NDC).

Electron lanza el editor (`rer_engine_editor`); el editor hospeda el motor 2D o 3D in-process según el tipo de proyecto. Comparten el protocolo de arranque y utilidades (`rer-engine-core`), pero **no comparten runtime de juego**. Los tipos del motor viven en Rust; `src/shared-types/` solo contiene tipos del shell Electron.

### Viewport: ventana única del editor

El editor egui (`rer_engine_editor`) abre su propia ventana winit y hospeda el motor 2D/3D **in-process** sobre el mismo `SharedGpu` y swapchain. Electron queda como host del selector de proyecto y de las modales; ya no hay ventana overlay ni tracker Win32.

Los binarios `rer_engine_2d` / `rer_engine_3d` siguen existiendo como standalone de debug, con ventana propia y sin overlay.

Principio **engine-first**: posiciones, cámaras, física y convenciones espaciales las resuelve el motor dentro del editor egui. El launcher solo elige proyecto y arranca el editor. Detalle en [`src/renderer/ARCHITECTURE.md`](src/renderer/ARCHITECTURE.md) y en los `ARCHITECTURE.md` de cada crate.

> **Importante:** un cambio en Rust **no** debe agregar tipos a `src/shared-types/`. Ver [`.cursor/rules/shared-types.mdc`](.cursor/rules/shared-types.mdc).

---

## Política gráfica (GPU)

| Regla | Detalle |
|-------|---------|
| **Motor 2D y 3D** | Siempre **Vulkan** (Windows y Linux) con **Ash**. |
| **Prohibido** | OpenGL, EGL, DX12, `wgpu`/`Backends::all()` y cualquier selección de backend por entorno. |
| **Shaders** | GLSL en `engine/shaders/` (`2d/`, `raster/`, `rt/`, `bvh/`), compilado a SPIR-V con `glslc`. |
| **Si la GPU falla** | El editor abre y muestra el error; el motor emite `{"event":"error"}` y no envía `ready`. |

Requisitos: **Vulkan** (loader + ICD) en Windows y Linux (`vulkaninfo` o drivers actualizados).

### Implementación (`core/src/vulkan/gpu/`)

| Pieza | Rol |
|-------|-----|
| `EngineGpuProfile::TwoD` / `ThreeD` | Perfil del motor standalone (2D sin RT; 3D con opciones RT). |
| `SharedGpu::new(window, options)` | Un `VkInstance`/`VkDevice`/swapchain por ventana; 2D y 3D comparten el núcleo. |
| `VulkanOptions` | `default()` para 2D; `three_d()` para el 3D. |
| `GpuInitError` | Fallo de arranque → IPC `{"event":"error"}`. |
| Editor `EditorGpu` | Crea el `SharedGpu` de la ventana y lo comparte con el motor in-process. |
| `EngineGpuErrorOverlay` | Ayuda en el viewport si el motor no envía `ready`. |

`yarn dev` / `yarn start` **no** definen backend GPU; solo compilan el binario del editor y arrancan electron-vite (`RER_ENGINE_PROFILE` fija el perfil release).

---

## Lenguajes y stack

| Área | Tecnologías |
|------|-------------|
| Motor | Rust, **Ash/Vulkan**, winit, glam, Rapier, **rhai**, gltf/image |
| Editor | Rust, egui, winit |
| Launcher | Electron, React, TypeScript, Vite (electron-vite) |
| Scripts de juego | Rhai (sandbox: API `engine.*` registrada por el motor) |
| Comandos/eventos | JSON definido en Rust (`engine/src/**/ipc/`); `src/shared-types/` es solo del shell |

---

## Plataformas soportadas

- **Windows 10/11** y **Linux (X11 y Wayland)**: mismo código y mismos comandos
  (`yarn dev`, `yarn start`, `yarn build`, `yarn dist`, `yarn quality`).
- **Fuera de alcance:** macOS y WSL.

`yarn dist` genera un instalador NSIS (`.exe`) en Windows y `AppImage` + `.deb`
en Linux, con el mismo icono PNG y los binarios del editor en `extraResources`.

## Requisitos

- **Node.js 22** e **Yarn** (ambas plataformas).
- **Rust 1.100** y **Cargo** (versión fijada en `rust-toolchain.toml`).
- **GPU con Vulkan**: loader e ICD instalados; comprobar con `vulkaninfo`.
- **Windows 10/11**: drivers/Vulkan SDK (proporciona `glslc` para los shaders).
- **Linux (X11/Wayland)**, además de lo anterior:
  - `libasound2` (ALSA) para audio en runtime; `libasound2-dev` si compilas.
  - `libudev-dev` para compilar el backend de gamepads (`gilrs`).
  - `glslc` de `glslang-tools` para compilar los shaders a SPIR-V (o
    `$VULKAN_SDK/bin/glslc` si usas el SDK).
  - `vulkan-tools` y una ICD como `mesa-vulkan-drivers` (loader/ICD; también
    se usa en CI headless con `xvfb`).
  - `xdg-desktop-portal` más un backend (GTK/KDE) para los diálogos nativos
    de abrir/guardar (`rfd`), especialmente bajo Wayland.
  - En Debian/Ubuntu: `sudo apt-get install -y pkg-config libasound2-dev libudev-dev glslang-tools vulkan-tools mesa-vulkan-drivers xvfb`.

### Limitaciones conocidas en Linux

- **Métrica GPU % por PID:** solo Windows (contadores PDH). En Linux el panel
  muestra **N/A** y el editor arranca con normalidad.
- **Ctrl/Shift globales vía OS** (`GetAsyncKeyState`): solo Windows. En Linux
  se usa el estado de teclado que rastrea la ventana winit; se pierde la
  consulta global cuando la ventana no tiene el foco.
- **Publicación de plugins Linux:** el artefacto/hash Linux del plugin
  worldgen/OpenCode aún no está publicado; no se da por terminada esa
  publicación. En desarrollo el plugin se instala desde el build local.

---

## Cómo ejecutar

Desde la raíz del repositorio:

```bash
# Instalar dependencias (primera vez)
yarn

# Desarrollo: compila el editor (debug) de forma incremental y abre la app
yarn dev

# Desarrollo con el editor en release (más rápido al renderizar)
yarn start

# Si solo cambiaste TS/React (Rust intacto), salta la compilación del motor:
yarn dev:local
yarn start:local

# Build del editor (sin empaquetar instalador)
yarn build

# Vista previa del build
yarn preview

# Instalador (compila los 3 binarios release + electron-builder):
# Windows → NSIS .exe; Linux → AppImage + .deb
yarn dist
```

### Builds de Rust

La app Electron lanza **solo** `rer_engine_editor` (`startEditorFrame` en
`src/main/index.ts`). Por eso `yarn dev` y `yarn start` compilan únicamente ese
binario; compilar también los standalone 2D/3D era reutilizar la cadena de
empaquetado para otra cosa. Todos los builds pasan por
`scripts/build-rust.mjs` (fuente única, con feature sets aislados):

```bash
# Editor debug / release: lo que usan dev / start
yarn rust:build:editor
yarn rust:build:editor:release

# Los 3 binarios (rer_engine_2d, rer_engine_3d, rer_engine_editor):
# para dist y depuración standalone
yarn rust:build
yarn rust:build:release
```

`rer-engine-core` no se compila aparte: es dependencia de `rer-engine` y cargo
ya lo construye con el conjunto de features correcto en cada invocación.

Los ejecutables quedan en `src/main/Engine/target/debug/` o `target/release/`.

`yarn dev` y `yarn start` pasan antes por `scripts/prune-rust-target.mjs`: mide
`target/` (~1 s) y **no toca nada** mientras esté por debajo del tope (18 GB).
Cargo recompila solo los crates que cambiaron: sin cambios en Rust el build
tarda ~0,6 s; con un `.rs` modificado, ~5 s en debug. Las variantes
`dev:local` / `start:local` omiten directamente el build de Rust (útiles cuando
solo tocaste el frontend).

Cuando `target/` supera el tope, el guardia poda primero las cachés puras
(`doc/` e `incremental/` de cada perfil) y solo si aún excede el límite ejecuta
un `cargo clean` completo: un rebuild lento de vez en cuando, no uno en cada
arranque. El tope se ajusta con `RER_TARGET_MAX_GB` (default 18).

Los comandos de tests/quality (`rust:test*`, `rust:check`, `rust:clippy`,
`quality:rust`) compilan en `src/main/Engine/target-ci/`, separado de los
binarios de dev/dist. Así las tres variantes de features y los binarios de test
no dejan residuos en `target/`. Ese directorio se limpia con `yarn rust:clean:ci`.

Si tras cambiar de toolchain aparece el error del enlazador de MSVC
`LNK1318: Unexpected PDB error`, la caché incremental quedó incompatible: haz
**una** limpieza del perfil afectado y vuelve a compilar. Para una
reconstrucción total (toolchain nuevo, binario obsoleto, etc.):

```bash
yarn rust:prune         # poda cachés / limpia solo si se supera el tope
yarn rust:clean         # todo target/
yarn rust:clean:dev     # solo perfil debug
yarn rust:clean:release # solo perfil release
yarn rust:clean:ci      # artefactos de tests/quality
```

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
| [`src/main/Engine/branding/README.md`](src/main/Engine/branding/README.md) | Intro 4K del motor para el build final del juego (`status: pendiente`) |
| [`src/main/Engine/engine/src/engine2d/ARCHITECTURE.md`](src/main/Engine/engine/src/engine2d/ARCHITECTURE.md) | Contrato del motor 2D |
| [`src/main/Engine/engine/ARCHITECTURE.md`](src/main/Engine/engine/ARCHITECTURE.md) | Contrato del motor 3D |
| [`src/renderer/ARCHITECTURE.md`](src/renderer/ARCHITECTURE.md) | Rol del launcher (ya no es el editor) y regla de `shared-types` |
| [`.cursor/rules/project.mdc`](.cursor/rules/project.mdc) | Reglas del proyecto (índice: `engine`, `frontend`, `shared-types`, `rust-quality`, `frontend-quality`) |

---

## Proyectos `.save`

Los proyectos se guardan como ZIP portable (`manifest.json`, `assets/`, `sounds/`, `scripting/`) para moverlos entre máquinas sin romper rutas.

El launcher Electron **no abre** el `.save`: solo entrega la ruta al editor Rust. `rer_engine_editor` descomprime el archivo (`editor/save_open.rs`), lee `type` / `gameStyle` del manifest y arranca el motor 2D o 3D correspondiente.

---

## Licencia

MIT — ver autor en `package.json`.
