# CHECKLIST — Tareas pendientes globales

Backlog transversal del monorepo (2D + 3D + Electron). Estado por motor: [CHECKLIST-2D.md](./CHECKLIST-2D.md), [CHECKLIST-3D.md](./CHECKLIST-3D.md). Modelo de recursos y `.save`: [docs/Save_Proyect_Model.yaml](./docs/Save_Proyect_Model.yaml).

**Última revisión:** 4 junio 2026 (plugin IA local — estado overlay y i18n)

---

## Pendiente

### Recursos por escena + “Cargar desde otra escena” (Resources)

- [ ] **Botón en Resources: “Cargar desde otra escena”**
  - Modal para elegir un recurso ya importado en **otra escena** del proyecto y usarlo en la **escena activa** (modelos, fuentes, imágenes HUD, sonidos, etc., según categoría del accordion).
  - Referencia al path del store; no reimportar el archivo desde disco.

- [ ] **Dos rutas de recursos en el `.save`**
  - **Globales** — recursos compartidos entre escenas (comportamiento actual de biblioteca a nivel proyecto).
  - **Por escena** — recursos usados solo en una escena concreta; empaquetados bajo ruta separada (p. ej. `scene-{id}/assets/`, `scene-{id}/hud-images/`, … alineado con categorías del accordion).
  - Actualizar `manifest.json` / `ProjectSaveData` para reflejar ambos ámbitos sin duplicar archivos.

- [ ] **Carga lazy de recursos al abrir `.save`**
  - El motor, al activar una escena, carga en memoria **solo** los recursos globales + los de **esa** escena.
  - Al cambiar de escena: Liberar memoria de recursos exclusivos de la escena anterior que no estén en uso global.

- [ ] **Promoción a global al guardar**
  - Si un recurso de escena A se referencia desde la escena activa B (vía modal “Cargar desde otra escena”), al **guardar** el `.save`:
    1. Quitar el recurso del bloque de recursos de la escena origen.
    2. Mover el archivo empaquetado a la ruta de **recursos globales** (o registrar en `sounds[]` / `fonts[]` / `hudImages[]` / `models[]` global según categoría).
    3. Actualizar referencias en ambas escenas al path global resuelto.
  - Objetivo: memoria y disco coherentes; un recurso compartido vive una sola vez en global.

- [ ] **Documentación**
  - Extender [docs/Save_Proyect_Model.yaml](./docs/Save_Proyect_Model.yaml) con el esquema dual global / por-escena cuando se implemente.

**Motivación:** hoy el motor tiende a registrar en memoria todos los recursos del manifest al cargar el proyecto. Separar por escena y promover a global solo lo compartido reduce uso de memoria y tiempo de carga en proyectos multi-escena.

### Métricas GPU en panel (Linux)

- [ ] **Compatibilizar métricas GPU fuera de Windows** (Electron + motor 2D/3D)
  - Hoy **Windows** usa contadores `GPU Engine` por PID (Electron en `gpuProcessUsage.ts`, motor en `engine_shared::process_metrics`); no refleja % global del SO.
  - En **Linux** el motor solo intenta `nvidia-smi` (NVIDIA); falta lectura GPU en Electron para Linux y soporte AMD/Intel en ambos procesos.
  - Criterio: panel **Métricas de uso** muestra uso GPU del editor de forma útil en Linux con la misma UX que en Windows (donde aplique).

---

### Auto-actualizaciones (Electron Updater)

- [ ] **Configurar electron-updater para publicar y detectar updates**
  - Integrar `electron-updater` en el proceso main (comprobación al arranque y/o en segundo plano; notificación al renderer).
  - Configurar `electron-builder` **publish** (p. ej. GitHub Releases, S3 u otro hosting) con URL de feed y canal de release (stable / beta).
  - Firmar instaladores según plataforma (Windows, macOS) para que el updater pueda aplicar parches de forma segura.
  - La **app instalada** debe detectar una versión nueva, informar al usuario y permitir descargar e instalar el update (o reiniciar para aplicarlo).
  - Documentar el flujo de release: bump de versión, build empaquetado, subida del artefacto y verificación end-to-end con una instalación previa.

---

### Asistente de IA Local y MCP en Electron

Referencia técnica: [docs/AI_Assistant_Plugin.yaml](./docs/AI_Assistant_Plugin.yaml), [docs/Plugins_Model.yaml](./docs/Plugins_Model.yaml).

#### Hecho

**Instalación y runtime**
- [x] Botón **Plugins** en el pie del espacio de acordeones + modal de plugins descargables.
- [x] Plugin **Asistente de IA local** con confirmación inline (modelo [Qwen/Qwen3-1.7B-GGUF](https://huggingface.co/Qwen/Qwen3-1.7B-GGUF)).
- [x] Descarga on-demand del modelo GGUF + `llama-server` (Windows x64; no empaquetado en instalador base ni motor Rust).
- [x] Instalación / desinstalación / estado en `%APPDATA%/rer-engine/plugins/`.
- [x] Arranque y parada de `llama-server` desde Electron main; probe HTTP; `--reasoning-format deepseek`.
- [x] Auto-arranque del servidor si el plugin quedó habilitado en sesión anterior.
- [x] Parada de `llama-server` al cerrar la aplicación.

**Chat e inferencia**
- [x] IPC `plugins:chat` → `assistantChat.ts` (API OpenAI-compatible en `127.0.0.1:8765`).
- [x] Contexto al modelo desde `docs/AI_Assistant_Editor_Guide.prompt.txt` (`editorDocsIndex.ts`, slice por idioma) + hints de UI.
- [x] Herramientas v1: `OPEN_ACCORDION:*` y `HIGHLIGHT:*` parseadas en main; reenvío al renderer (`plugins:ui-action`).
- [x] Filtrado de bloques `thinking` / `redacted_thinking`; texto fallback si el modelo solo devuelve tags.
- [x] Log de depuración en `%APPDATA%/rer-engine/plugins/ai-assistant-chat.log`.
- [x] Prompt de idioma explícito (en/es) según locale del editor en cada petición de chat.

**Overlay UI (ventana Electron dedicada)**
- [x] Ventana frameless, transparente, `alwaysOnTop` sobre el viewport winit (`aiAssistantWindow.ts`).
- [x] Mascot FAB con assets `RER-AI.png`, `RER-AI-THINKING.png`, `RER-AI-POINTING.png`.
- [x] Flujo por fases: **idle** (saludo) → **input** (pregunta lateral) → **thinking** (globo arriba, sin input) → **answer** (respuesta abajo, avatar señalando).
- [x] Botón cerrar (×) en globo de respuesta; vuelve a avatar idle; nuevo clic abre chat de nuevo.
- [x] Redimensionado de ventana por fase (`intro` | `thinking` | `input` | `answer`).
- [x] Arrastre: región nativa `app-region: drag` (padding) + arrastre del personaje por IPC (bucle en main); posición libre en pantalla (sin clamp al viewport).
- [x] `resizable: true` con min/max fijos (requisito Windows para `app-region: drag` en ventanas frameless).
- [x] Ventana hija del editor (`parent`); destrucción del overlay y parada del motor al cerrar la ventana principal.
- [x] Sincronización de idioma: UI del overlay (`translations.json`) + `locale` en chat al cambiar ES/EN o al cargar proyecto (`set_locale` / `project_loaded_*`).

**Editor (renderer principal)**
- [x] `SidebarAccordionContext` + `data-plugin-target` en acordeones y controles clave.
- [x] `useAiAssistantOverlaySync` — muestra/oculta overlay según plugin habilitado e idioma.
- [x] Traducciones ES para strings del asistente en `translations.json`.

#### Pendiente

- [ ] **QA end-to-end en Windows:** instalación ~1.9 GB, primera inferencia, flujo “¿dónde cargo un modelo?”, arrastre, cambio de idioma, cierre limpio de procesos.
- [ ] **Visual C++ Redistributable (plugin IA):** detectar si falta MSVC 2015–2022 (fallo silencioso de `llama-server`); ofrecer o ejecutar el redistributable en el flujo de instalación del plugin.
- [x] **Documentación de contexto para la IA:** prompt compacto `docs/AI_Assistant_Editor_Guide.prompt.txt` (FAQ densa, slice ES/EN); respuestas con **negrita** y pasos en líneas separadas.
- [ ] **MCP SDK formal** / más tools (sub-acordeones, búsqueda dedicada en docs, herramientas estructuradas).
- [ ] **Acceso al motor** (futuro): crear entidades, asignar valores, etc. vía `engine:cmd` — fuera de alcance v1.
- [ ] **macOS / Linux:** binario `llama-server` y empaquetado del plugin fuera de Windows.

---

### Gameplay — proyectiles, enemigos e IA (2D + 3D)

- [ ] **Categoría y entidad tipo proyectil**
  - Crear categoría de recurso/entidad **proyectil** en el editor (spawn, inspector, persistencia en `.save`).
  - Diferenciarla de `character`, `object`, etc., en motor y renderer.

- [ ] **Disparo de proyectil configurable**
  - Acción/comportamiento de disparo con parámetros editables en UI:
    - **Velocidad**
    - **Dirección**
    - **Distancia** (alcance o recorrido máximo)
  - Sincronizar valores con el motor y reflejarlos en el snapshot de entidad.

- [ ] **Character tipo enemy**
  - Variante de personaje **enemy** (categoría, etiquetas, defaults de editor).
  - Propiedades y filtros en sidebar / inspector distintos del player.

- [ ] **IA de enemigos y NPCs**
  - Sistema de comportamiento para enemigos y NPCs (patrulla, persecución, ataque, estados básicos).
  - Integración con scripts Rhai y/o nodos visuales según el flujo del motor.

---

## Aplazado (producto)

Ítems fuera de alcance actual; compartidos por 2D y 3D.

- Multiplayer
- IA generativa en assets
- partículas/shaders experimentales

---

## Completado (global)

_(Vacío — mover aquí ítems al cerrarlos.)_
