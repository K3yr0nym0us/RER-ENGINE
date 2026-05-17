# Arquitectura del renderer (Electron + React)

Este documento fija el rol del **frontend** en RER-ENGINE para que futuras IAs y contribuidores no dupliquen logica que pertenece al motor.

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
- Montar listas de entidades, tabs de escena, undo de **metadatos** que el motor no posee (nombres de pestaña, rutas de assets en el proyecto).
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

Flujo correcto:

1. UI / carga de escena → `set_first_person_view` (pies, yaw, pitch, FOV, frustum).
2. Motor → `first_person_view_changed`.
3. `applyFirstPersonViewFromEngine()` en `src/defaults/firstPersonSceneRestore.ts` actualiza `firstPersonViewRef` y `entityTransformsRef`.
4. Autoguardado y tabs usan `firstPersonViewRef.current.position` (pies), no derivaciones locales.

Archivos clave:

- `src/defaults/firstPersonSceneRestore.ts` — envio de vista y aplicacion del evento (sin matematica de poses).
- `src/context/useContextEngine/hooks/createEngineEventHandler.ts` — handler de `first_person_view_changed`; carga de jugador con `skipTransform` si hay vista guardada.
- `src/pages/EngineView/components/sidebar/CameraAccordion.tsx` — solo dispara `set_first_person_view`; refresca UI con `fpViewSyncSeq`.
- `src/shared-types/types.ts` — `FirstPersonViewChanged`, comando `set_first_person_view`.

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
- `src/pages/EngineView/` — layout del editor, sidebars, `SceneTabsBar`.
- `src/defaults/` — plantillas y restauracion de escena (intencion, no fisica).
- `src/hooks/useAutoSave.ts` — persistencia; leer refs alimentadas por eventos del motor.
- `src/shared-types/types.ts` — contrato IPC; ampliar aqui al anadir comandos/eventos nuevos.

## Checklist para cambios nuevos

Antes de anadir logica en TS, preguntar:

1. ¿El motor ya expone un comando/evento para esto?
2. Si no, ¿deberia añadirse al motor en lugar del front?
3. ¿El cambio es solo UI/persistencia? → renderer.
4. ¿Afecta posicion, rotacion, fisica o camara en play? → motor obligatorio.

Mantener `cargo check` en el crate del motor y `tsc` en el monorepo tras tocar tipos IPC.
