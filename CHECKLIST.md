# CHECKLIST — Tareas pendientes globales

Backlog transversal del monorepo (2D + 3D + Electron). Estado por motor: [CHECKLIST-2D.md](./CHECKLIST-2D.md), [CHECKLIST-3D.md](./CHECKLIST-3D.md). Modelo de recursos y `.save`: [docs/Save_Proyect_Model.yaml](./docs/Save_Proyect_Model.yaml).

**Última revisión:** junio 2026

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

- [ ] Crear boton de "Plugins" en el pie del espacio de accordiones.
- [ ] Al darle al boton abrir modal para seleccionar plugins descargables.
- [ ] Agregar la opcion (boton) del plugin de Asistente de IA local.
- [ ] Ventana de confirmacion con mas informacion del plugin.
- [ ] Este debe descargar un modelo ultra ligero de IA local para exclusivo uso del MCP en electron.
- [ ] Crear componente de burbuja flotante en el editor para al presionar abrir cuadro flotante de chat para pedir cosas a la IA.
- [ ] La IA debe de poder usar el MCP y decirle al usuario guiandolo abriendo accordiones o hacendo focus en botones y respondiendo en una burbuja flotante lo que necesita hacer de la manera mas resumida posible.
- [ ] Si funciona bien a futuro podriamos darle acceso al motor para realizar acciones directamente como crear entidades, asignar valores, etc.

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
