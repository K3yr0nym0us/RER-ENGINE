# Documentación RER-ENGINE

Índice de contratos y modelos del editor. Este directorio describe **lo ya implementado**; el trabajo pendiente se anota dentro de cada documento (`status: pendiente`) y en los comentarios del código, porque los antiguos `CHECKLIST*.md` se retiraron del repo.

**Última revisión:** noviembre 2026 (motores 2D y 3D sobre Ash/Vulkan; el 2D ya no depende de wgpu y comparte el núcleo Vulkan de `core`; reflejos, sondas, MSAA, sombras y TAA del 3D actualizados a la implementación real; nuevo LOD de mallas GLB estáticas por distancia en `yaml/rendering/Model_LOD_3D.yaml`). Revisión anterior: junio 2026 (reflejos: quality preset, RT espejo → SSR, docs SSR/Probes/RT).

## Estructura

- `yaml/engine/`: runtime, contexto del editor, IPC y política de plataforma.
- `yaml/persistence/`: `.save`, carga de proyecto y multi-escena.
- `yaml/assets/`: formato `.rerasset`.
- `yaml/rendering/`: reflejos, RT, sondas, TAA, MSAA y sombras.
- `yaml/gameplay/`: entidades, blueprints, física, animación, controles y proyectiles.
- `yaml/scripting/`: API Rhai y programación visual.
- `yaml/editor/`: modales, propiedades, UI del jugador, sockets y física de huesos.
- `yaml/plugins/`: sistema de plugins.
- `ai/`: asistente RER-AI (OpenCode) y prompts de contexto.
- Los documentos de calidad (`.md`) quedan en la raíz de `docs/`.

---

## yaml/engine/ — Fundamentos

| Documento | Contenido |
|-----------|-----------|
| [Engine_Runtime_Foundation.yaml](./yaml/engine/Engine_Runtime_Foundation.yaml) | Ventana única del editor, GPU Vulkan, métricas (Windows) |
| [Engine_Context.yaml](./yaml/engine/Engine_Context.yaml) | `EngineProvider`, refs, `createEngineActions` / `createEngineEventHandler` |
| [IPC_Protocol.yaml](./yaml/engine/IPC_Protocol.yaml) | `engine:cmd` / `engine:event`, catálogo por motor, `core/src/ipc` |

---

## yaml/persistence/ — Proyecto y persistencia

| Documento | Contenido |
|-----------|-----------|
| [Save_Proyect_Model.yaml](./yaml/persistence/Save_Proyect_Model.yaml) | `.save` ZIP, Resources, manifest, guardar/cargar, bibliotecas globales |
| [Project_Load_3D.yaml](./yaml/persistence/Project_Load_3D.yaml) | Carga `.save` 3D: burst, precarga, GLB, eventos `project_loaded_3d` |
| [Escenes_Model_3D.yaml](./yaml/persistence/Escenes_Model_3D.yaml) | Multi-escena editor 3D, dirty, `switch_editor_scene` |

---

## yaml/assets/ — Assets y formato

| Documento | Contenido |
|-----------|-----------|
| [Rerasset_Format.yaml](./yaml/assets/Rerasset_Format.yaml) | `.rerasset` / RTEX v1 (editor RGBA8); `export_profiles` reserva BC7/ASTC para el build final |

---

## yaml/rendering/ — Renderización 3D

| Documento | Contenido |
|-----------|-----------|
| [Reflections_3D.yaml](./yaml/rendering/Reflections_3D.yaml) | Índice reflejos: tiers, toggles, frame, IPC |
| [Reflections_SSR.yaml](./yaml/rendering/Reflections_SSR.yaml) | Screen-space reflections por tier |
| [Reflections_Probes.yaml](./yaml/rendering/Reflections_Probes.yaml) | Cubemap probes, captura, forward IBL |
| [Reflections_RT.yaml](./yaml/rendering/Reflections_RT.yaml) | Ray tracing HW (Ash), fallback BVH, denoise; historial wgpu |
| [Reflections_Diagnostics.yaml](./yaml/rendering/Reflections_Diagnostics.yaml) | Auditoría Ash con capa de validación + historial wgpu |
| [TAA_3D.yaml](./yaml/rendering/TAA_3D.yaml) | Anti-aliasing temporal: blend, historial ping-pong, formatos G-buffer, IPC |
| [MSAA_3D.yaml](./yaml/rendering/MSAA_3D.yaml) | MSAA forward 3D por tier, resolve a G-buffers 1×, IPC |
| [Shadows_3D.yaml](./yaml/rendering/Shadows_3D.yaml) | Shadow map único, tiers de resolución, IPC |
| [Model_LOD_3D.yaml](./yaml/rendering/Model_LOD_3D.yaml) | LOD de mallas GLB estáticas por distancia (vertex clustering, `set_model_lod`) |

---

## yaml/gameplay/ — Entidades y gameplay

| Documento | Contenido |
|-----------|-----------|
| [Entities_Model_2D.yaml](./yaml/gameplay/Entities_Model_2D.yaml) | Tipos 2D, entidades, sprites |
| [Blueprints_Model_2D.yaml](./yaml/gameplay/Blueprints_Model_2D.yaml) | Blueprints 2D, quick build, snap, propagación, `.save` |
| [Entities_Model_3D.yaml](./yaml/gameplay/Entities_Model_3D.yaml) | Tipos 3D, entidades, modelos GLB/GLTF |
| [Blueprints_Model_3D.yaml](./yaml/gameplay/Blueprints_Model_3D.yaml) | Blueprints 3D, model_id, quick build, propagación, `.save` |
| [Entities_Model_3D_extras.yaml](./yaml/gameplay/Entities_Model_3D_extras.yaml) | Cámara play character, colisión por tipo, orientación GLB jugador |
| [Projectiles_3D.yaml](./yaml/gameplay/Projectiles_3D.yaml) | Proyectiles 3D: config, gravedad opcional, muzzle, hits/rebote, IPC/Rhai |
| [Projectiles_2D.yaml](./yaml/gameplay/Projectiles_2D.yaml) | Proyectiles 2D: config, gravedad, pierce/rebote por dureza, IPC/Rhai |
| [Physics_Model.yaml](./yaml/gameplay/Physics_Model.yaml) | Rapier 2D/3D, herramientas plano 3D, jugador shape cast |
| [Animation_Model.yaml](./yaml/gameplay/Animation_Model.yaml) | Sprite 2D, clips GLTF skinned, `play_animation`, blueprints |
| [Controls_Model.yaml](./yaml/gameplay/Controls_Model.yaml) | Bindings, control scripts Rhai |

---

## yaml/scripting/ — Scripting

| Documento | Contenido |
|-----------|-----------|
| [RHAI_API.yaml](./yaml/scripting/RHAI_API.yaml) | API scripting 2D y play character 3D |
| [Programing_Model.yaml](./yaml/scripting/Programing_Model.yaml) | Programación visual (escena + entidad), nodos → Rhai |

---

## yaml/editor/ — Editor UI

| Documento | Contenido |
|-----------|-----------|
| [MODAL_ELECTRON.yaml](./yaml/editor/MODAL_ELECTRON.yaml) | Ventanas hijas (propiedades, blueprint, player UI, scripting) |
| [Editor_Native_Shell.yaml](./yaml/editor/Editor_Native_Shell.yaml) | Ventana nativa, foco, cursor en play, arranque y DevTools |
| [Entity_Properties_Modal.yaml](./yaml/editor/Entity_Properties_Modal.yaml) | Modal propiedades entidad (transform, animaciones, texturas, blueprint) |
| [Sockets_3D.yaml](./yaml/editor/Sockets_3D.yaml) | Sockets en huesos skinned, vínculos objeto↔socket, modal Configuración de sockets |
| [Bone_Physics_3D.yaml](./yaml/editor/Bone_Physics_3D.yaml) | Física secundaria por hueso (jiggle), modal Editar física de huesos |
| [Entity_Textures.yaml](./yaml/editor/Entity_Textures.yaml) | LOD texturas embebidas GLB (pestaña Texturas 3D) |
| [Player_UI_Model.yaml](./yaml/editor/Player_UI_Model.yaml) | HUD jugador, pantallas, undo/redo, NDC |

---

## yaml/plugins/ — Plugins

| Documento | Contenido |
|-----------|-----------|
| [Plugins_Model.yaml](./yaml/plugins/Plugins_Model.yaml) | Sistema de plugins descargables |

---

## ai/ — Asistente IA

| Documento | Contenido |
|-----------|-----------|
| [AI_Assistant_Plugin.yaml](./ai/AI_Assistant_Plugin.yaml) | Asistente RER-AI (OpenCode free, sandbox docs/ai, herramientas v2) |
| [AI_Assistant_Editor_Guide.es.prompt.txt](./ai/AI_Assistant_Editor_Guide.es.prompt.txt) | Contexto FAQ español para el modelo |
| [AI_Assistant_Editor_Guide.en.prompt.txt](./ai/AI_Assistant_Editor_Guide.en.prompt.txt) | Contexto FAQ inglés para el modelo |

---

## Motores (ARCHITECTURE en código)

- [`engine/src/engine2d/ARCHITECTURE.md`](../src/main/Engine/engine/src/engine2d/ARCHITECTURE.md)
- [`engine/ARCHITECTURE.md`](../src/main/Engine/engine/ARCHITECTURE.md)
- [`renderer/ARCHITECTURE.md`](../src/renderer/ARCHITECTURE.md)
