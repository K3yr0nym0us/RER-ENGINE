# CHECKLIST — Tareas pendientes globales

Backlog transversal (2D + 3D + Electron). Por motor: [CHECKLIST-2D.md](./CHECKLIST-2D.md), [CHECKLIST-3D.md](./CHECKLIST-3D.md).

**Documentación de lo implementado:** [docs/README.md](./docs/README.md)

**Última revisión:** junio 2026

---

## Pendiente

### Formato editor vs exportación del juego (assets / texturas)

**Principio:** editor = RTEX RGBA8 en Win/Linux; compresión GPU solo al exportar el juego.

**Documentado (editor):** [docs/Rerasset_Format.yaml](./docs/Rerasset_Format.yaml)

#### Exportación del juego (aplazado)

> Pipeline completo **Editor → Save → Build → Ejecutable** antes de implementar.

- [ ] **Paso de build separado del `.save`** — `project.save` → Build → paquete con meshes/texturas transcodificadas.
- [ ] **Perfil Windows** — albedo BC7, normal BC5, ORM BC4.
- [ ] **Perfil Linux (Vulkan)** — mismo esquema que Windows.
- [ ] **Perfil Android (futuro)** — ASTC 6×6 / 8×8.
- [ ] **Sin duplicar en edición** — variantes GPU solo en build (*cook at build time*).
- [ ] **Herramientas de transcodificación** — texconv, Basis/ISPC, etc. en export Electron o binario Rust.

Referencia perfiles: [CHECKLIST-TEXTURAS-3D.md](./CHECKLIST-TEXTURAS-3D.md)

---

### Formato `.rersave` (reemplazo de `.save`)

**Principio:** contenedor binario `RERS`, sin ZIP, sin soporte legacy `.save`.

- [ ] **Documentar formato** — `docs/Rersave_Format.yaml` o sección en Save_Proyect_Model.
- [ ] **Read/write en Rust** — `engine_shared/rersave.rs`, escritura atómica.
- [ ] **Compresión por entrada** — zstd/deflate por blob.
- [ ] **Integrar en Electron main** — sustituir AdmZip.
- [ ] **Contrato runtime** — extraer a `extract_dir` al abrir.
- [ ] **Renombrar extensión y UI** — `.rersave`, diálogos, tipos TS.
- [ ] **`fileAssociations` en electron-builder.yml**
- [ ] **Abrir con doble clic** — `open-file`, second-instance (Win/Linux).
- [ ] **Probar en build instalado** — NSIS / AppImage / deb.
- [ ] **Actualizar docs** — Save_Proyect_Model, README, ARCHITECTURE.
- [ ] **Sin migrador automático** — proyectos `.save` antiguos fuera de alcance.

---

### Recursos por escena + “Cargar desde otra escena”

- [ ] **Botón Resources: “Cargar desde otra escena”** — modal, referencia por path sin reimportar.
- [ ] **Dos rutas en `.save`** — globales vs `scene-{id}/assets/…`
- [ ] **Carga lazy al abrir `.save`** — solo global + escena activa; liberar al cambiar escena.
- [ ] **Promoción a global al guardar** — recurso compartido entre escenas → mover a biblioteca global.
- [ ] **Documentar** — extender Save_Proyect_Model cuando exista el esquema.

---

### IPC por motor (2D / 3D)

**Documentado:** [docs/IPC_Protocol.yaml](./docs/IPC_Protocol.yaml) · `engine_ipc_common` · `engineCommandCatalog.ts`

- [ ] **Tipos TS discriminados** — `EngineCommand2D` / `EngineCommand3D` en shared-types; `send2d` / `send3d`.
- [ ] **Preload opcional** — `window.engine2d` / `window.engine3d` para enforcement en renderer.

---

### Métricas GPU en panel (Linux)

- [ ] **Compatibilizar métricas GPU fuera de Windows** — Electron + motor 2D/3D; AMD/Intel además de nvidia-smi.

---

### Auto-actualizaciones (Electron Updater)

- [ ] **electron-updater** — comprobación al arranque, publish en electron-builder, firma instaladores, flujo release documentado.

---

### Asistente de IA local

**Documentado:** [docs/AI_Assistant_Plugin.yaml](./docs/AI_Assistant_Plugin.yaml)

- [x] **QA end-to-end Windows** — instalación ~1.9 GB, inferencia, arrastre, idioma, cierre limpio.
- [ ] **Visual C++ Redistributable** — detectar MSVC faltante en instalación del plugin.
- [ ] **MCP SDK formal** / más tools.
- [ ] **Acceso al motor vía engine:cmd** — fuera de alcance v1.
- [ ] **macOS / Linux** — binario llama-server y empaquetado del plugin.

---

### Gameplay — proyectiles, enemigos e IA (2D + 3D)

- [ ] **Categoría entidad proyectil** — spawn, inspector, `.save`.
- [ ] **Disparo configurable** — velocidad, dirección, distancia.
- [ ] **Character tipo enemy** — categoría y defaults distintos del player.
- [ ] **IA enemigos y NPCs** — patrulla, persecución, ataque; Rhai / nodos visuales.

---

## Aplazado (producto)

- Multiplayer
- IA generativa en assets
- Partículas/shaders experimentales
