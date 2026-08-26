# Documentación RER-ENGINE

Índice de contratos y modelos del editor. Los checklists (`CHECKLIST*.md`) listan **solo trabajo pendiente**; lo ya implementado vive aquí.

**Última revisión:** junio 2026 (reflejos: quality preset, RT espejo → SSR, docs SSR/Probes/RT)

---

## Fundamentos

| Documento | Contenido |
|-----------|-----------|
| [Engine_Runtime_Foundation.yaml](./Engine_Runtime_Foundation.yaml) | Viewport overlay winit, GPU Vulkan, métricas (Windows) |
| [Engine_Context.yaml](./Engine_Context.yaml) | `EngineProvider`, refs, `createEngineActions` / `createEngineEventHandler` |
| [IPC_Protocol.yaml](./IPC_Protocol.yaml) | `engine:cmd` / `engine:event`, catálogo por motor, `engine_ipc_common` |
| [MODAL_ELECTRON.yaml](./MODAL_ELECTRON.yaml) | Ventanas hijas (propiedades, blueprint, player UI, scripting) |

---

## Proyecto y persistencia

| Documento | Contenido |
|-----------|-----------|
| [Save_Proyect_Model.yaml](./Save_Proyect_Model.yaml) | `.save` ZIP, Resources, manifest, guardar/cargar, bibliotecas globales |
| [Rerasset_Format.yaml](./Rerasset_Format.yaml) | `.rerasset` / RTEX v1 (editor RGBA8), códigos reservados para export |
| [Project_Load_3D.yaml](./Project_Load_3D.yaml) | Carga `.save` 3D: burst, precarga, GLB, eventos `project_loaded_3d` |
| [Escenes_Model_3D.yaml](./Escenes_Model_3D.yaml) | Multi-escena editor 3D, dirty, `switch_editor_scene` |

---

## Renderización 3D

| Documento | Contenido |
|-----------|-----------|
| [Reflections_3D.yaml](./Reflections_3D.yaml) | Índice reflejos: tiers, toggles, frame, IPC |
| [Reflections_SSR.yaml](./Reflections_SSR.yaml) | Screen-space reflections por tier |
| [Reflections_Probes.yaml](./Reflections_Probes.yaml) | Cubemap probes, captura, forward IBL |
| [Reflections_RT.yaml](./Reflections_RT.yaml) | Ray tracing HW, quality preset, denoise |
| [Reflections_Diagnostics.yaml](./Reflections_Diagnostics.yaml) | Auditoría y pruebas manuales |
| [TAA_3D.yaml](./TAA_3D.yaml) | Anti-aliasing temporal: blend, jitter, pipeline, IPC |
| [Shadows_3D.yaml](./Shadows_3D.yaml) | Shadow map único, tiers de resolución, IPC |

---

## Entidades y gameplay

| Documento | Contenido |
|-----------|-----------|
| [Entities_Model_2D.yaml](./Entities_Model_2D.yaml) | Tipos 2D, entidades, sprites |
| [Blueprints_Model_2D.yaml](./Blueprints_Model_2D.yaml) | Blueprints 2D, quick build, snap, propagación, `.save` |
| [Entities_Model_3D.yaml](./Entities_Model_3D.yaml) | Tipos 3D, entidades, modelos GLB/GLTF |
| [Blueprints_Model_3D.yaml](./Blueprints_Model_3D.yaml) | Blueprints 3D, model_id, quick build, propagación, `.save` |
| [Entities_Model_3D_extras.yaml](./Entities_Model_3D_extras.yaml) | Cámara play character, colisión por tipo, orientación GLB jugador |
| [Physics_Model.yaml](./Physics_Model.yaml) | Rapier 2D/3D, herramientas plano 3D, jugador shape cast |
| [Animation_Model.yaml](./Animation_Model.yaml) | Sprite 2D, clips GLTF skinned, `play_animation`, blueprints |
| [Controls_Model.yaml](./Controls_Model.yaml) | Bindings, control scripts Rhai |
| [RHAI_API.yaml](./RHAI_API.yaml) | API scripting 2D y play character 3D |

---

## Editor UI

| Documento | Contenido |
|-----------|-----------|
| [Entity_Properties_Modal.yaml](./Entity_Properties_Modal.yaml) | Modal propiedades entidad (transform, animaciones, texturas, blueprint) |
| [Sockets_3D.yaml](./Sockets_3D.yaml) | Sockets en huesos skinned, vínculos objeto↔socket, modal Configuración de sockets |
| [Bone_Physics_3D.yaml](./Bone_Physics_3D.yaml) | Física secundaria por hueso (jiggle), modal Editar física de huesos |
| [Entity_Textures.yaml](./Entity_Textures.yaml) | LOD texturas embebidas GLB (pestaña Texturas 3D) |
| [Player_UI_Model.yaml](./Player_UI_Model.yaml) | HUD jugador, pantallas, undo/redo, NDC |
| [Programing_Model.yaml](./Programing_Model.yaml) | Programación visual (escena + entidad), nodos → Rhai |

---

## Plugins e IA

| Documento | Contenido |
|-----------|-----------|
| [Plugins_Model.yaml](./Plugins_Model.yaml) | Sistema de plugins descargables |
| [AI_Assistant_Plugin.yaml](./AI_Assistant_Plugin.yaml) | Asistente IA local (Qwen GGUF, overlay, herramientas v1) |
| [AI_Assistant_Editor_Guide.es.prompt.txt](./AI_Assistant_Editor_Guide.es.prompt.txt) | Contexto FAQ español para el modelo |
| [AI_Assistant_Editor_Guide.en.prompt.txt](./AI_Assistant_Editor_Guide.en.prompt.txt) | Contexto FAQ inglés para el modelo |

---

## Exportación futura (referencia)

| Documento | Contenido |
|-----------|-----------|
| [CHECKLIST-TEXTURAS-3D.md](../CHECKLIST-TEXTURAS-3D.md) | Perfiles BC7/ASTC recomendados para build final (no editor) |

---

## Motores (ARCHITECTURE en código)

- [`engine_2d/ARCHITECTURE.md`](../src/main/Engine/engine_2d/ARCHITECTURE.md)
- [`engine_3d/ARCHITECTURE.md`](../src/main/Engine/engine_3d/ARCHITECTURE.md)
- [`renderer/ARCHITECTURE.md`](../src/renderer/ARCHITECTURE.md)
