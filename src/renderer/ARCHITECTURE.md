# Arquitectura del renderer (Electron + React) — SOLO launcher

> **Estado actual:** el editor fue migrado a Rust/egui (`rer_engine_editor`).
> Este renderer **ya no es el editor**. Para la arquitectura del editor consulta
> `src/main/Engine/engine/ARCHITECTURE.md` y `docs/yaml/editor/`.

## Rol real

Electron/React es el *shell* que:

1. Muestra el selector de tipo de proyecto (2D/3D).
2. Abre un diálogo para elegir un `.save` y entrega **solo la ruta**; Rust extrae el ZIP y decide 2D/3D desde el manifest.
3. Envía `setGameStyle` por IPC y lanza `rer_engine_editor` con el tipo, cámara y rutas.
4. Mantiene el idioma elegido en el launcher y lo pasa al editor como `--locale` al arrancar.

Todo lo demás —escenas, entidades, transforms, física, cámaras, colisiones, HUD,
programación visual, propiedades y guardado— vive en **Rust egui**, en proceso con el motor.

## Archivos

| Archivo | Rol |
|---------|-----|
| `src/App.tsx` | Única pantalla: selector de proyecto / apertura de `.save`. |
| `src/context/LanguageContext.tsx` | Idioma de la UI; `window.electronAPI.setLocale()`. |
| `src/components/LanguageToggleButton.tsx` | Botón de idioma. |

## Regla sobre `src/shared-types/`

`src/shared-types/` contiene **únicamente** los tipos del launcher: selector de proyecto,
diálogo de `.save` e idioma. **No es el contrato del editor, ni de plugins, ni un catálogo de comandos.**

- Un cambio en Rust **nunca** debe agregar ni modificar tipos ahí.
- Los comandos/eventos del motor se definen en Rust (`engine/src/**/ipc/`).
- Ver `.cursor/rules/shared-types.mdc`.

## No hacer

- No reintroducir paneles, modales ni hosts React del editor.
- No duplicar tipos de entidades/escenas del motor en TypeScript.
- No "sincronizar" `shared-types` después de un cambio en Rust.

## Calidad

`yarn lint`, `yarn typecheck`, `yarn test`, `yarn quality`.
Ver `.cursor/rules/frontend-quality.mdc` y `docs/FRONTEND_QUALITY.md`.
