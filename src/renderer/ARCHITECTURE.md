# Arquitectura del renderer (Electron + React)

Este documento fija el rol del **frontend** en RER-ENGINE para que futuras IAs y contribuidores no dupliquen logica que pertenece al motor.

## Integracion con el viewport del motor

El renderer **no renderiza** el mundo de juego. Solo reserva un hueco visual y reporta su geometria:

| Pieza | Archivo / canal |
|-------|-----------------|
| Hueco en UI | `EngineView.tsx` → `<main class="engine-viewport-area">` (transparente) |
| Bounds | `useContextEngine` → `reportBounds()` → `electronAPI.sendViewportBounds` |
| Main | `src/main/index.ts` → `viewport-bounds`, `viewportToScreenBounds`, spawn `--overlay` |
| Motor | Ventana winit separada, alineada por IPC (`set_bounds`) + tracker nativo en Rust |

**GPU:** wgpu con **Vulkan** en ambos binarios (`EngineGpuProfile` 2D/3D). El front no elige backend ni usa WebGL para el viewport. Al hacer `spawn`, el main **elimina** `RER_GPU_BACKEND` del entorno del proceso motor.

Si la GPU no arranca: evento `error` del motor → `engineError` → `EngineGpuErrorOverlay` (drivers / WSL / Vulkan). El resto del editor sigue usable.

## Principio: engine-first

El renderer es **cascarón de editor**: UI, persistencia (`.save`), enrutado IPC y estado React para paneles. **No** es un segundo motor de juego.

| Capa | Responsabilidad |
|------|-----------------|
| **Motor** (`engine_2d` / `engine_3d`) | Fisica, camaras, transforms reales, picking, play mode, convenciones de mallas y quaterniones. |
| **`shared-types`** | Contrato JSON tipado (comandos/eventos) compartido con preload y main. |
| **Renderer** | Enviar **intencion** (`window.engine.send`), reflejar **eventos** del motor, serializar escenas, formularios. |

Si una feature necesita trigonometria espacial, conversion pies↔centro, forward de mesh o sincronizar cuerpo con camara → va en el motor correspondiente, no en TypeScript.

## Que SI puede hacer el frontend

- Formularios y validacion de entrada (numeros finitos, rangos de UI).
- Montar listas de entidades, lista de escenas (acordeón Scenes), undo de **metadatos** que el motor no posee (nombres de escena, rutas de assets en el proyecto).
- Colas de restore al cargar `.save` (`pendingRestoresRef`, `load_character`, etc.) **sin** recalcular poses 3D.
- Constantes de **presentacion** compartidas con el save (p. ej. `FIRST_PERSON_PLAYER_BODY_SCALE` en `@shared-types`) solo como valor por defecto de archivo, no como fuente de verdad en runtime.
- Helpers 2D acotados al editor de sprites (recortes de canvas, frames) donde no existe motor implicado.

## Que NO debe hacer el frontend

- Duplicar reglas de gameplay o de camara del motor (anti-ejemplo corregido: `quatFromCameraYaw`, `bodyCenterFromFeet` en el front).
- Enviar `set_transform` al jugador FP con rotacion/centro **calculados en TS** para “corregir” al motor.
- Inferir estado autoritativo a partir de `entity_selected` o `debug_metrics` cuando existe un evento dedicado del motor.
- Portar herramientas de un motor al otro (colliders 2D, execution areas, etc.) solo porque comparten nombres IPC.

## Primera persona 3D (contrato vigente)

Referencia completa en `src/main/Engine/engine_3d/ARCHITECTURE.md` (seccion *Vista FP autoritativa*).

Flujo **vista / cámara** (editor y paneles; no es el guardado):

1. UI / carga de escena → `set_play_character_view` (pies, yaw, pitch, FOV, frustum).
2. Motor → `play_character_view_changed` (alias legacy `first_person_view_changed`).
3. `applyPlayCharacterViewFromEngine()` en `src/defaults/playCharacterSceneRestore.ts` actualiza `playCharacterViewRef` y `entityTransformsRef` para la UI (p. ej. `CameraAccordion`), sin recalcular poses en TS.

En proyectos **3D**, `playCharacterViewRef` **no** alimenta el `.save`: el motor exporta `player_transform` en el snapshot (pies, yaw, pitch, FOV, frustum, `visual_model_path`). Ver sección *Guardado*.

Archivos clave:

- `src/defaults/playCharacterSceneRestore.ts` — envio de vista y aplicacion del evento (sin matematica de poses).
- `src/context/useContextEngine/hooks/createEngineEventHandler.ts` — handler de `play_character_view_changed`; carga de jugador con `skipTransform` si hay vista guardada.
- `src/pages/EngineView/components/sidebar/CameraAccordion.tsx` — dispara `set_play_character_view`; refresca UI con `playCharacterViewSyncSeq`.
- `src/shared-types/types.ts` — `PlayCharacterViewChanged`, comando `set_play_character_view`.

## Player UI HUD (proyectos 3D)

Editor de **pantallas HUD del jugador** en el acordeón UI (scope `player`). El motor posee geometría NDC, capas, play y undo; el renderer envía comandos y refleja eventos.

| Pieza | Archivo |
|-------|---------|
| Pantallas y edición | `UIAccordion/PlayerUiAccordion.tsx`, `UiScreensAccordion.tsx` |
| Lista de elementos | `EditingUiElementGroups.tsx` (texto, objeto, imagen; botones en lista IPC) |
| Dibujo de polígonos | `hooks/usePlayerUiObjectDrawing.ts` → `set_player_ui_object_draw` |
| Estado | `useContextEngine/types.ts` — `editingUiElements`, `SET_EDITING_UI_OBJECTS` vía `player_ui_text_boxes_list` |
| IPC | `createEngineActions.ts`, `createEngineEventHandler.ts`, `shared-types/types.ts` |
| Save | `buildProjectSaveFromEngine.ts` — `playerUiObjects` y resto de capas HUD del snapshot |

Flujo típico:

1. **Editar pantalla** → `set_player_ui_edit_mode` (scope `player`, id de pantalla).
2. **Añadir contenido** → modales de fuente/imagen o modo dibujo de objeto; el motor emite `player_ui_text_boxes_list` para sincronizar el sidebar.
3. **Play** → pantalla marcada activa en `playerUiScreens` + `sync_player_ui_screens`; el motor dibuja el HUD en NDC.
4. **Undo** → Ctrl+Z en el viewport (motor `Undo` / `RestorePlayerUiHud`); no duplicar pila de undo en React.

Al salir de edición (`endUiScreenEdit`), cancelar dibujo de objeto en el hook para alinear estado local con el motor.

## Abrir `.save`

1. Main extrae el ZIP a un directorio temporal (`extractDir`) y lee solo metadatos (`type`, `gameStyle`) del `manifest.json`.
2. El renderer recibe `initialExtractDir` + `initialSavePath`; **no** recibe `ProjectSaveData` en memoria.
3. Al arrancar el motor, `set_scene` / spawn usan `extract_dir`; Rust carga `manifest.json` y emite `project_loaded_2d` / `project_loaded_3d` + `project_load_*_complete`.
4. El front sincroniza la lista de escenas, modelos y entidades desde esos eventos (y `get_models_list` si hace falta), sin reenviar `load_model_asset` ni reconstruir la escena en el handler `ready`.

Proyecto **nuevo** (sin `extractDir`): en `ready` se envía `set_scene` con plantilla vacía; la carga de escenas inactivas sigue siendo vía `import_scene` al activar otra escena en el acordeón (flujo legacy 2D / 3D no-FP).

## Guardado `.save`

El **ZIP** lo escribe Electron/main; el **contenido de la escena activa** depende del tipo de proyecto:

| Tipo | Quién serializa la escena | Archivos |
|------|---------------------------|----------|
| **2D / 3D** | Motor (`export_save_snapshot` → `save_snapshot_ready`) | `useAutoSave.ts`, `buildProjectSaveFromEngine.ts`, `ScenesAccordion` |

Flujo (ambos motores):

1. Front → `{ cmd: 'export_save_snapshot' }`.
2. Motor recorre la escena (entidades placeholder del template FP incluidas), mundo, jugador FP, cámara 2D si aplica, stores de assets y scripts registrados.
3. Motor → `save_snapshot_ready` con `scene`.
4. Front arma `ProjectSaveData`: fusiona blueprints, idioma, sonidos/fondos del contexto, escenas **inactivas** desde `sceneStateStore`, y `blueprint_id` / `entity_category` de entidades desde `entityMetaRef` cuando exista. La **categoría de biblioteca** (accordion Resources: character / environment / object) la guarda el motor en `model_store` y va en el snapshot (`models[].category`).

El front **no** decide qué entidades ni qué modelos precargados van al save ni arma `playerTransform` desde refs locales (salvo metadatos de editor: blueprints, animaciones de celda, escenas inactivas).

## Relacion con los dos motores

- Proyecto **2D** → proceso `rer_engine_2d`; UI usa herramientas 2D (colliders, areas de ejecucion, `camera_2d_updated`).
- Proyecto **3D** FP → proceso `rer_engine_3d`; UI de camara FP y jugador sigue el contrato anterior.
- `set_scene` / `projectType` eligen binario en main; el renderer no mezcla semantica de ambos en un mismo modulo de runtime.

Documentacion de motores:

- `src/main/Engine/engine_2d/ARCHITECTURE.md`
- `src/main/Engine/engine_3d/ARCHITECTURE.md`

## Archivos fuente de verdad (renderer)

- `src/context/useContextEngine/` — estado del editor, refs, reducer, envio IPC.
- `src/context/useContextEngine/hooks/createEngineEventHandler.ts` — unico lugar que deberia interpretar eventos del motor para estado global.
- `src/pages/EngineView/` — layout del editor, sidebars, `ScenesAccordion`.
- `src/defaults/` — plantillas y restauracion de escena (intencion, no fisica).
- `src/hooks/useAutoSave.ts` — snapshot del motor (2D y 3D).
- `src/hooks/usePlayerUiObjectDrawing.ts` — modo dibujo de objetos HUD (progreso `toolProgress`).
- `src/defaults/buildProjectSaveFromEngine.ts` — IPC `export_save_snapshot` y merge a `ProjectSaveData`.
- `src/defaults/defaultSceneName.ts` — nombres `Scene-NN` locales (misma regla que `editor_defaults` en Rust).
- `src/shared-types/types.ts` — contrato IPC; ampliar aqui al anadir comandos/eventos nuevos.

## Checklist para cambios nuevos

Antes de anadir logica en TS, preguntar:

1. ¿El motor ya expone un comando/evento para esto?
2. Si no, ¿deberia añadirse al motor en lugar del front?
3. ¿El cambio es solo UI/persistencia? → renderer.
4. ¿Afecta posicion, rotacion, fisica o camara en play? → motor obligatorio.

Mantener `cargo check` en el crate del motor y `tsc` en el monorepo tras tocar tipos IPC.
