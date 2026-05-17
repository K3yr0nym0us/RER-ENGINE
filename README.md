# RER-ENGINE

> **R**eact + **E**lectron + **R**ust ENGINE

Motor de videojuegos 2D/3D con editor integrado. La idea central: **crear comportamiento de forma directa**, sin repartir la lógica de cada entidad entre muchos sistemas abstractos.

---

## Idea del motor

RER-ENGINE separa el producto en dos capas que hablan por JSON (stdin/stdout):

| Capa | Rol |
|------|-----|
| **Editor** (Electron + React + TypeScript) | Interfaz, escenas, guardado `.save`, herramientas de autoría. |
| **Motor** (Rust + wgpu) | Render, física, scripting, play mode. |

Hay **dos motores independientes**, no uno híbrido:

- `rer_engine_2d` — sprites, plano XY, colliders y execution areas.
- `rer_engine_3d` — mallas glTF/FBX, cámara orbital, primera persona en play.

Electron arranca el binario que corresponda según el tipo de proyecto. Comparten protocolo IPC y utilidades (`engine_shared`), pero **no comparten runtime de juego**.

```
┌─────────────────────────────────────────────┐
│             Electron (BrowserWindow)        │
│  ┌──────────────────┐  ┌───────────────────┐│
│  │   React + TS     │  │  Viewport Rust    ││
│  │   (editor UI)    │  │  wgpu embebido    ││
│  └──────────────────┘  └───────────────────┘│
└─────────────────────────────────────────────┘
          ↑  IPC — una línea JSON por mensaje
```

Principio **engine-first**: posiciones, cámaras, física y convenciones espaciales las resuelve el motor; el frontend envía intención y refleja eventos. Detalle en `src/renderer/ARCHITECTURE.md` y en los `ARCHITECTURE.md` de cada crate.

---

## Lenguajes y stack

| Área | Tecnologías |
|------|-------------|
| Motor | Rust, wgpu, winit, glam, Rapier, mlua, gltf/image |
| Editor | Electron, React, TypeScript, Vite (electron-vite) |
| Scripts de juego | Lua (sandbox: sin `io`, `os`, `require`) |
| Contrato IPC | JSON — tipos en `src/shared-types/types.ts` |

---

## Requisitos

- **Node.js** 20+ y **Yarn**
- **Rust** (toolchain estable) y **Cargo**
- **Windows 11** o **Linux con X11** (el viewport se embebe vía handle nativo; en Wayland usar XWayland / `ELECTRON_OZONE_PLATFORM_HINT=x11`)

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

---

## Documentación

| Archivo | Contenido |
|---------|-----------|
| [LUA_API.md](./LUA_API.md) | API de scripting Lua (2D y 3D), resumida |
| [CHECKLIST-2D.md](./CHECKLIST-2D.md) | Hecho y pendiente — motor / editor 2D |
| [CHECKLIST-3D.md](./CHECKLIST-3D.md) | Hecho y pendiente — motor / editor 3D |
| `src/main/Engine/engine_2d/ARCHITECTURE.md` | Contrato del motor 2D |
| `src/main/Engine/engine_3d/ARCHITECTURE.md` | Contrato del motor 3D |
| `src/renderer/ARCHITECTURE.md` | Rol del frontend y qué no duplicar |

---

## Proyectos `.save`

Los proyectos se guardan como ZIP portable (`manifest.json`, `assets/`, `sounds/`, `scripting/`) para moverlos entre máquinas sin romper rutas.

---

## Licencia

MIT — ver autor en `package.json`.
