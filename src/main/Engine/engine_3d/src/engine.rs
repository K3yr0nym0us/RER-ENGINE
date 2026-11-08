use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use glam::Vec3 as GlamVec3;
use winit::{dpi::PhysicalSize, window::Window};

use super::{ActiveAnimation, AnimationState, AudioSlot, UndoAction};
use crate::config_3d::model_animation::{
    ActiveModelClip, GpuSkinnedMeshEntry, ModelAnimationBinding,
};
use crate::config_3d::model_asset::ModelAsset;
use crate::config_3d::physics_3d::PhysicsWorld;
use crate::config_3d::{Camera, WorldBounds3D};
use crate::config_compat::{ActiveTool, GridConfig};
use crate::ecs::{EntityId, NameComponent, World};
use crate::entity_save_meta::EntitySaveRegistry;
use crate::gizmo::GizmoBuffer;
use crate::ipc::PlayCameraFollowMode;
use crate::mesh::Mesh;
use crate::scripting::ScriptEngine;

pub struct State {
    pub(crate) window: Arc<Window>,
    /// Contexto Ash (Vulkan) del motor 3D. Es dueÃ±o del swapchain y presenta el frame.
    pub(crate) vulkan: crate::vulkan::SharedGpu,
    /// Renderizador de frame en Ash (sombras, forward, TAA, SSR, reflejos RT/BVH, gizmo, HUD).
    pub(crate) frame_renderer: Option<crate::vulkan::FrameRenderer>,
    /// BVH software memoizado: `(hash de escena, blob con nodos/triángulos/materiales)`.
    pub(crate) rt_scene_cache: Option<(u64, std::sync::Arc<crate::bvh::RtSceneBlob>)>,
    pub(crate) size: PhysicalSize<u32>,
    pub(crate) clear_color: [f32; 4],
    pub(crate) vsync_enabled: bool,
    pub(crate) prev_view_proj: [[f32; 4]; 4],
    /// Nivel de calidad de sombras (resoluciÃ³n del shadow map por tier).
    pub(crate) shadow_tier: crate::config_3d::shadow_graphics::ShadowTier,
    /// ResoluciÃ³n actual del shadow map (px por lado); se recrea al cambiar de tier.
    pub(crate) shadow_map_size: u32,
    /// Nivel MSAA del pase forward (Low = Off).
    pub(crate) msaa_tier: crate::config_3d::msaa_graphics::MsaaTier,
    /// `sample_count` efectivo tras clamp al soporte del dispositivo.
    pub(crate) msaa_sample_count: u32,
    /// TAA del pase de escena (historial + velocity).
    pub(crate) taa_enabled: bool,
    pub(crate) taa_blend: f32,

    pub(crate) texture_array: crate::texture::TextureArray,
    /// `tex_idx` de `MeshComponent` â†’ capa en `texture_array`.
    pub(crate) tex_layers: Vec<crate::texture::TextureLayer>,
    /// Albedo promedio por `tex_idx` (paralelo a `tex_layers`).
    pub(crate) tex_layer_albedo: Vec<[f32; 3]>,
    pub(crate) fallback_layer: crate::texture::TextureLayer,
    /// Quad para tooltips en pantalla (HUD); no usar `meshes[0]` (suelo).
    pub(crate) hud_quad_mesh: Mesh,
    pub camera: Camera,
    /// Blanco de Ã³rbita del editor 3D: no sigue al transform del jugador FP.
    pub editor_orbit_target: glam::Vec3,
    /// Yaw/pitch/distancia del viewport orbital en editor (desacoplados de `camera` y del jugador).
    pub editor_viewport_yaw: f32,
    pub editor_viewport_pitch: f32,
    pub editor_viewport_distance: f32,
    /// Profundidad del gizmo de frustum FPS en modo editor (metros).
    pub(crate) fps_editor_frustum_distance: f32,
    /// PosiciÃ³n absoluta del ojo de la cÃ¡mara FPS en editor. Independiente del transform
    /// del jugador salvo modo de seguimiento activo en el panel CÃ¡mara.
    pub(crate) play_camera_eye_position: glam::Vec3,
    /// Modo de seguimiento del ojo FPS respecto al jugador en editor.
    pub(crate) play_camera_follow_mode: PlayCameraFollowMode,
    /// Offset mundo ojoâˆ’cabeza (legacy / depuraciÃ³n).
    pub(crate) play_camera_follow_offset: glam::Vec3,
    /// Offset ojoâˆ’cabeza en espacio local del jugador (yaw del cuerpo); `FollowCharacter`.
    pub(crate) play_camera_follow_offset_local: glam::Vec3,
    pub(crate) meshes: Vec<Mesh>,
    pub(crate) world: World,
    pub(crate) last_frame: Instant,
    pub delta_time: f32,
    pub(crate) gizmo_buffer: GizmoBuffer,
    pub(crate) gizmo_rotate_buffer: GizmoBuffer,
    pub physics: PhysicsWorld,
    pub selected_entity: Option<EntityId>,
    pub selected_entities: Vec<EntityId>,
    pub hovered_entity: Option<EntityId>,
    pub hovered_gizmo_axis: Option<usize>,
    pub active_gizmo_axis: Option<usize>,
    pub transform_gizmo_mode: crate::config_3d::transform_gizmo::TransformGizmoMode,
    pub(crate) scenario_entities: Vec<EntityId>,
    pub(crate) character_entities: Vec<EntityId>,
    pub(crate) collider_entities: Vec<EntityId>,
    pub(crate) execution_area_entities: Vec<EntityId>,
    /// Pares (trigger, actor) dentro de un execution area en play (3D).
    pub(crate) execution_overlaps: std::collections::HashSet<(EntityId, EntityId)>,
    pub(crate) background_entity: Option<EntityId>,
    pub(crate) background_path: Option<String>,
    pub(crate) grid_config: GridConfig,
    pub(crate) world_bounds_3d: WorldBounds3D,
    pub(crate) world_bounds_buffer: GizmoBuffer,
    pub(crate) world_sky_buffer: GizmoBuffer,
    /// Buffers reutilizables de overlays dinámicos (evitan asignar Vulkan por frame).
    pub(crate) collision_overlay_buffer: GizmoBuffer,
    pub(crate) skeleton_overlay_buffer: GizmoBuffer,
    pub(crate) socket_overlay_buffer: GizmoBuffer,
    pub(crate) fps_frustum_buffer: GizmoBuffer,
    pub(crate) probe_gizmo_buffer: GizmoBuffer,
    pub(crate) ctrl_held: bool,
    pub(crate) shift_held: bool,
    pub active_tool: ActiveTool,
    pub(crate) quick_build_ghost_id: Option<EntityId>,
    pub(crate) plane_tool_ghost_id: Option<EntityId>,
    pub(crate) plane_tool_preview_scale: Option<[f32; 3]>,
    pub(crate) plane_tool_tex_cache: std::collections::HashMap<[u8; 4], usize>,
    /// Reservado (obsoleto): rotaciÃ³n Q/E solo vÃ­a polling OS en el tick del motor.
    pub(crate) plane_tool_rotate_left: bool,
    pub(crate) plane_tool_rotate_right: bool,
    /// Ventana nativa con foco: rotaciÃ³n vÃ­a OS. Sin foco: flags IPC (sidebar Electron).
    pub(crate) engine_window_focused: bool,
    /// HWND/XID de la ventana Electron (padre del overlay); devolver foco al colocar herramientas plano.
    pub(crate) editor_parent_id: u64,
    /// Ãšltima posiciÃ³n del cursor en pÃ­xeles del viewport (ghost de herramientas).
    pub(crate) tool_cursor_pixels: Option<(f32, f32)>,
    pub(crate) quick_build_preview_path: Option<String>,
    pub(crate) quick_build_preview_kind: Option<String>,
    pub(crate) quick_build_preview_scale: Option<[f32; 3]>,
    /// Metadatos del blueprint activo (nombre, fÃ­sica, rotaciÃ³nâ€¦) para colocar instancias.
    pub(crate) quick_build_blueprint: Option<crate::config_3d::quick_build::QuickBuildBlueprint>,
    /// Registro IPC de blueprints (`register_blueprint`) por id.
    pub(crate) blueprint_registry:
        std::collections::HashMap<String, crate::ipc::BlueprintPlacementMeta>,
    /// Blueprint de origen por entidad (construcciÃ³n rÃ¡pida u otras vÃ­as del motor).
    pub(crate) entity_blueprint_ids: std::collections::HashMap<EntityId, String>,
    /// ColisiÃ³n de malla Rapier on/off por entidad (`docs/Entities_Model_3D.yaml`).
    pub(crate) entity_colision: std::collections::HashMap<EntityId, bool>,
    /// Dureza de gameplay por entidad (0 = no afecta proyectiles).
    pub(crate) entity_hardness: std::collections::HashMap<EntityId, u32>,
    /// Hijo â†’ ancla + transform local (fusiÃ³n de entidades o socket en editor).
    pub(crate) entity_attachments: std::collections::HashMap<
        EntityId,
        crate::config_3d::entity_attachments::EntityAttachmentLocal,
    >,
    /// Sockets nombrados por entidad host con esqueleto.
    pub(crate) entity_sockets:
        std::collections::HashMap<EntityId, Vec<crate::config_3d::entity_sockets::EntitySocket>>,
    /// Cache hueso â†’ Ã­ndice joint (invalidar al cambiar modelo).
    pub(crate) bone_index_cache: std::collections::HashMap<(EntityId, String), usize>,
    /// Modo editor: seleccionar hueso con click para un socket.
    pub(crate) socket_bone_pick_entity: Option<EntityId>,
    pub(crate) socket_bone_pick_hovered_joint: Option<usize>,
    /// Entidad con esqueleto visible en editor de fÃ­sica por hueso.
    pub(crate) bone_physics_editor_entity: Option<EntityId>,
    pub(crate) bone_physics_pick_entity: Option<EntityId>,
    pub(crate) bone_physics_pick_hovered_joint: Option<usize>,
    /// ConfiguraciÃ³n persistida de fÃ­sica por hueso.
    pub(crate) entity_bone_physics:
        std::collections::HashMap<EntityId, Vec<crate::config_3d::bone_physics::BonePhysicsEntry>>,
    /// Config de proyectil (plantillas categorÃ­a `projectile`).
    pub(crate) entity_projectile_config:
        std::collections::HashMap<EntityId, crate::config_3d::projectiles::ProjectileConfig>,
    /// Instancias de proyectil en play (no persistidas).
    pub(crate) active_projectiles: Vec<crate::config_3d::projectiles::ActiveProjectile>,
    /// Estado de simulaciÃ³n jiggle (no persistido).
    pub(crate) bone_physics_sim: std::collections::HashMap<
        (EntityId, usize),
        crate::config_3d::bone_physics::BonePhysicsSimState,
    >,
    pub(crate) bone_physics_entity_motion: std::collections::HashMap<
        EntityId,
        crate::config_3d::bone_physics::BonePhysicsEntityMotion,
    >,
    pub preview_playing: bool,
    /// EdiciÃ³n de UI del jugador (vista play + cuadrÃ­cula de trabajo).
    pub(crate) player_ui_edit_active: bool,
    pub(crate) player_ui_edit_restore:
        Option<crate::config_3d::player_ui::edit::PlayerUiEditViewportRestore>,
    pub(crate) ui_work_grid_buffer: GizmoBuffer,
    pub(crate) player_ui_edit_scope: Option<String>,
    pub(crate) player_ui_edit_screen_id: Option<String>,
    /// Pantalla Player UI marcada como activa (play y scripts Rhai).
    pub(crate) player_ui_active_player_screen_id: Option<String>,
    /// id â†’ nombre de pantallas Player UI (sincronizado desde el editor).
    pub(crate) player_ui_player_screen_names: HashMap<String, String>,
    pub(crate) player_ui_text_boxes:
        std::collections::HashMap<String, Vec<crate::config_3d::player_ui::PlayerUiTextBox>>,
    pub(crate) player_ui_buttons:
        std::collections::HashMap<String, Vec<crate::config_3d::player_ui::PlayerUiButton>>,
    pub(crate) player_ui_images:
        std::collections::HashMap<String, Vec<crate::config_3d::player_ui::PlayerUiImage>>,
    pub(crate) player_ui_objects:
        std::collections::HashMap<String, Vec<crate::config_3d::player_ui::PlayerUiObject>>,
    pub(crate) player_ui_object_draw:
        Option<crate::config_3d::player_ui::object::PlayerUiObjectDrawSession>,
    pub(crate) player_ui_object_draw_overlay: GizmoBuffer,
    pub(crate) player_ui_text_next_id: u32,
    pub(crate) player_ui_text_overlay_buffer: crate::gizmo::GizmoBuffer,

    /// Atlas Vulkan compartido por texto, botones, imágenes y objetos del HUD.
    pub(crate) player_ui_text_atlas: crate::screen_hud_image::HudAtlas,
    /// UV empaquetados en `player_ui_text_atlas` por ruta de imagen (evita releer disco en cada frame).
    pub(crate) player_ui_hud_texture_cache:
        std::collections::HashMap<String, crate::screen_hud_image::ScreenHudPackedImage>,
    pub(crate) player_ui_font_cache:
        std::collections::HashMap<String, std::sync::Arc<ab_glyph::FontArc>>,
    pub(crate) player_ui_glyph_instances: Vec<crate::mesh::InstanceData>,
    pub(crate) player_ui_selected_text_id: Option<u32>,
    pub(crate) player_ui_selected_button_id: Option<u32>,
    pub(crate) player_ui_selected_image_id: Option<u32>,
    pub(crate) player_ui_selected_object_id: Option<u32>,
    pub(crate) player_ui_text_editing_id: Option<u32>,
    /// Ãndice de carÃ¡cter (UTF-8) del cursor en el cuadro en ediciÃ³n.
    pub(crate) player_ui_text_caret: usize,
    pub(crate) player_ui_caret_blink_epoch: std::time::Instant,
    pub(crate) player_ui_caret_buffer: crate::gizmo::GizmoBuffer,
    pub(crate) player_ui_text_drag: Option<crate::config_3d::player_ui::PlayerUiTextDrag>,
    pub(crate) player_ui_last_text_click: Option<(u32, std::time::Instant)>,
    /// Wireframes de colisiÃ³n en editor (IPC `set_debug_mode`).
    pub debug_mode: bool,
    pub(crate) preview_entity_transform_snapshots:
        HashMap<EntityId, crate::config_3d::preview_editor::PreviewEntityTransform>,
    pub(crate) preview_fp_view_snapshot:
        Option<crate::config_3d::preview_editor::PreviewFpEditorView>,
    /// Velocidad del personaje (m/s), patrÃ³n character controller.
    pub(crate) play_controller_velocity: GlamVec3,
    pub(crate) play_controller_on_floor: bool,
    pub(crate) play_controller_jump_queued: bool,
    /// DetecciÃ³n de flanco de input: true mientras el script SPACE
    /// estÃ¡ pulsando este frame; comparado con `_prev` para detectar la transiciÃ³n.
    pub(crate) play_controller_jump_request_active: bool,
    pub(crate) play_controller_jump_request_prev: bool,
    /// Teclas acumuladas por scripts de control en el frame actual (play).
    pub(crate) play_controller_script_input: HashSet<String>,
    /// ParÃ¡metros opcionales fijados por scripts Rhai de control (play).
    pub(crate) play_controller_script_walk_speed: Option<f32>,
    pub(crate) play_controller_script_sprint_multiplier: Option<f32>,
    pub(crate) play_controller_script_jump_speed: Option<f32>,
    /// Entidad `[Player]` principal de la escena 3D.
    pub(crate) play_character_entity: Option<EntityId>,
    /// CÃ¡mara orbital del editor 3D (ECS separada del jugador FP).
    pub(crate) editor_camera_entity: Option<EntityId>,
    /// Forward local (plano XZ) del mesh del jugador; se recalcula al reemplazar el modelo.
    pub(crate) play_character_mesh_forward_xz: glam::Vec2,
    /// AABB de la malla visual del jugador (pies en `local_min_y`, tÃ­p. 0 tras normalizar).
    pub(crate) play_character_mesh_extents:
        Option<crate::config_3d::character_anchor::PlayCharacterMeshExtents>,
    pub(crate) play_session_body_yaw_baseline: f32,
    pub(crate) play_session_camera_yaw_baseline: f32,
    pub(crate) tool_overlay_buffer: GizmoBuffer,
    pub(crate) snap_locale: String,
    pub(crate) anim_saved_transforms: std::collections::HashMap<u32, (GlamVec3, GlamVec3)>,
    pub pivot_edit_mode: Option<(u32, String, u32, u32)>,
    pub logical_area_mode: Option<u32>,
    pub(crate) audio_slot: Option<AudioSlot>,
    pub(crate) animations: HashMap<u32, HashMap<String, AnimationState>>,
    pub(crate) active_animations: HashMap<u32, ActiveAnimation>,
    pub(crate) default_animation_by_entity: HashMap<u32, String>,
    pub(crate) entity_facing_right: HashMap<u32, bool>,
    pub(crate) script_engine: ScriptEngine,
    pub(crate) control_bindings_by_entity: HashMap<u32, crate::ipc::ControlBindingsData>,
    pub(crate) sprite_store: HashMap<String, (String, u32, u32)>,
    /// Modelos 3D precargados: clave de ruta normalizada â†’ nombre + categorÃ­a de biblioteca.
    pub(crate) model_store: HashMap<String, crate::ipc::ModelStoreEntry>,
    /// Assets importados indexados por `model_id` (.rerasset).
    pub(crate) imported_model_registry: crate::assets::ImportedModelRegistry,
    /// `model_id` â†’ (`material_index` â†’ `texture_chunk`) del Ãºltimo `.rerasset` cargado.
    pub(crate) rerasset_material_tex:
        std::collections::HashMap<String, std::collections::HashMap<u32, u32>>,
    /// Mallas estÃ¡ticas en GPU indexadas por ruta (precarga al registrar recurso).
    pub(crate) static_model_cache: crate::config_3d::static_model_cache::StaticModelCache,
    pub(crate) model_preload_rx: crate::config_3d::static_model_cache::ModelPreloadRx,
    pub(crate) model_preload_tx: crate::config_3d::static_model_cache::ModelPreloadTx,
    pub(crate) model_preload_inflight: std::collections::HashSet<String>,
    pub(crate) model_preload_gpu_queue:
        Vec<crate::config_3d::static_model_cache::PendingGpuModelPreload>,
    pub(crate) pending_load_models: Vec<crate::config_3d::static_model_cache::PendingLoadModel>,
    pub(crate) pending_entity_model_replaces:
        Vec<crate::config_3d::static_model_cache::PendingEntityModelReplace>,
    pub(crate) sound_store: HashMap<String, String>,
    pub(crate) font_store: HashMap<String, String>,
    /// ImÃ¡genes HUD (Resources â†’ Images): path â†’ metadatos para UI del jugador.
    pub(crate) hud_image_store: HashMap<String, crate::hud_image_asset::HudImageAssetMeta>,
    pub(crate) background_store: HashMap<String, String>,
    pub(crate) undo_stack: Vec<UndoAction>,
    pub(crate) redo_stack: Vec<UndoAction>,
    pub(crate) is_applying_undo: bool,
    pub(crate) process_metrics_sampler: rer_engine_shared::process_metrics::ProcessMetricsSampler,
    pub(crate) metrics_last_emit: Instant,
    pub(crate) metrics_frame_count: u32,
    pub(crate) last_draw_calls: u32,
    pub(crate) autosave_enabled: bool,
    pub(crate) autosave_last_tick: Instant,
    /// Metadatos de persistencia (rutas, tipo) y fuentes de scripts por entidad.
    pub(crate) save_registry: EntitySaveRegistry,
    /// PrÃ³ximo `load_proyect` monta el manifest sin vaciar (arranque con `.save`).
    pub(crate) mount_save_on_empty_world: bool,
    /// Carga 3D desde manifest: el jugador no usa pipeline editor (`sync_scale` / `align`).
    pub(crate) restoring_save_manifest: bool,
    /// Tras `set_directional_light` en cambio de escena activa: 1 frame para ver si llegan spawns del front.
    pub(crate) fp_baseline_defer_frames: u8,
    /// Registro multi-escena del editor (baselines, dirty, cambio de escena activa).
    pub(crate) editor_scenes: crate::engine::editor_scenes::EditorSceneStore,
    /// LÃ­mite de FPS del bucle (sincronizado con `set_target_fps`).
    pub(crate) target_fps: u64,
    /// Nivel grÃ¡fico global de texturas GLB embebidas.
    pub(crate) graphics_texture_tier: crate::config_3d::texture_graphics::TextureGraphicsTier,
    /// Nivel global de reflejos (Off / Low / Medium / High).
    pub(crate) reflection_tier: crate::config_3d::reflection_graphics::ReflectionTier,
    /// Captura y muestreo de reflection probes (toggle editor, independiente del SSR).
    pub(crate) reflection_probes_enabled: bool,
    /// Ray tracing HW (toggle editor, independiente del tier).
    pub(crate) reflection_raytracing_enabled: bool,
    /// Vista debug de reflejos (editor).
    pub(crate) reflection_debug_view: crate::config_3d::reflection_graphics::ReflectionDebugView,

    /// Modo depuraciÃ³n SSR: emite logs del pipeline cada frame.

    /// GPU expone `EXPERIMENTAL_RAY_QUERY` (para UI / IPC).

    /// BVH CPU + TLAS hardware (actualizado cuando RT estÃ¡ activo).

    /// Ãšltimo tier emitido por IPC (evita spam).
    pub(crate) probe_capture_cursor: usize,
    pub(crate) probe_capture_burst_all: bool,
    pub(crate) probe_entity_slots: std::collections::HashMap<crate::ecs::EntityId, usize>,
    pub(crate) last_probe_capture_ids: Option<Vec<crate::ecs::EntityId>>,
    /// Frames que faltan para volver a capturar una probe (evita capturar cada frame).
    pub(crate) probe_capture_cooldown: u32,
    /// Distancia (m) a la cÃ¡mara con textura al tope del tier activo.
    pub(crate) texture_detail_near_m: f32,
    pub(crate) glb_texture_catalog_cache:
        HashMap<String, Vec<crate::config_3d::entity_textures::MaterialTexturesCatalog>>,
    pub(crate) entity_texture_effective_cap: HashMap<EntityId, u32>,
    pub(crate) texture_lod_last_update: Instant,
    /// Entidad icono del sol (luz direccional).
    pub(crate) sun_entity: Option<EntityId>,
    /// Mesh y textura compartidos del icono esfÃ©rico del sol.
    pub(crate) sun_icon_mesh_idx: Option<usize>,
    pub(crate) sun_icon_tex_idx: Option<usize>,
    /// Cubo blanco compartido para `[EditorBox]` (plantilla y `.save`).
    pub(crate) editor_box_mesh_idx: Option<usize>,
    pub(crate) editor_box_tex_idx: Option<usize>,
    /// Quad + texturas cacheadas para la escena MatVal.
    pub(crate) mat_val_label_mesh_idx: Option<usize>,
    pub(crate) mat_val_texture_cache: HashMap<String, usize>,
    /// Quad delgado compartido para muros/triggers 3D (colisionador / execution area).
    pub(crate) plane_tool_wall_mesh_idx: Option<usize>,
    pub(crate) directional_light_dir: GlamVec3,
    pub(crate) directional_light_color: GlamVec3,
    pub(crate) directional_light_ambient: f32,
    pub(crate) light_intensity: f32,
    pub(crate) shadow_darkness: f32,
    /// path absoluto de imagen â†’ capa en `texture_array` (dedup).
    pub(crate) texture_path_layers: HashMap<String, crate::texture::TextureLayer>,
    /// Assets de animaciÃ³n 3D por ruta de archivo (parseo aparte de mesh_3d).
    pub(crate) model_assets: std::collections::HashMap<String, Arc<ModelAsset>>,
    pub(crate) model_animation_bindings: std::collections::HashMap<u32, ModelAnimationBinding>,
    pub(crate) active_model_clips: std::collections::HashMap<u32, ActiveModelClip>,
    pub(crate) model_clip_defaults: std::collections::HashMap<u32, String>,
    pub(crate) skinned_gpu_meshes: Vec<GpuSkinnedMeshEntry>,
}

impl State {
    pub(crate) fn texture_layer_for(&self, tex_idx: usize) -> crate::texture::TextureLayer {
        self.tex_layers
            .get(tex_idx)
            .copied()
            .unwrap_or(self.fallback_layer)
    }

    /// Empaqueta RGBA en el array GPU; reutiliza capa si `cache_key` ya existe.
    pub(crate) fn pack_texture_layer(
        &mut self,
        cache_key: Option<&str>,
        rgba: &[u8],
        w: u32,
        h: u32,
    ) -> crate::texture::TextureLayer {
        if let Some(key) = cache_key
            && let Some(&layer) = self.texture_path_layers.get(key)
        {
            return layer;
        }
        let layer = self.texture_array.pack(&self.vulkan, rgba, w, h).unwrap();
        if layer >= crate::texture::TextureArray::MAX_LAYERS - 1 {
            crate::ipc::send_event(&crate::ipc::EngineEvent::TextureArrayExhausted {
                max_layers: crate::texture::TextureArray::MAX_LAYERS,
            });
        }
        if let Some(key) = cache_key {
            self.texture_path_layers.insert(key.to_string(), layer);
        }
        self.tex_layers.push(layer);
        let albedo = crate::texture::rgba_mip0_average_linear(rgba);
        self.tex_layer_albedo.push(albedo);
        layer
    }

    pub(crate) fn is_entity_name_taken(&self, name: &str, except_id: Option<u32>) -> bool {
        let target = name.trim().to_lowercase();
        if target.is_empty() {
            return false;
        }

        self.world.query::<NameComponent>().any(|(id, c)| {
            if except_id == Some(id) {
                return false;
            }
            c.name.trim().eq_ignore_ascii_case(&target)
        })
    }

    pub(crate) fn next_numbered_entity_name(&self, base: &str) -> String {
        let names = self
            .world
            .query::<NameComponent>()
            .map(|(_, c)| c.name.clone());
        rer_engine_shared::editor_defaults::next_numbered_entity_label(base, names)
    }

    pub(crate) fn resolve_entity_display_name(
        &self,
        requested: &str,
        default_base: &str,
    ) -> String {
        let names = self
            .world
            .query::<NameComponent>()
            .map(|(_, c)| c.name.clone());
        rer_engine_shared::editor_defaults::resolve_entity_display_name(
            requested,
            default_base,
            names,
        )
    }

    /// Activa o desactiva V-Sync reconfigurando el swapchain de Ash.
    pub fn set_vsync(&mut self, enabled: bool) {
        self.vsync_enabled = enabled;
        if let Err(e) = self.vulkan.set_vsync(enabled) {
            log::warn!("[vsync] no se pudo reconfigurar el swapchain: {e}");
        } else {
            log::info!(
                "[vsync] V-Sync {}",
                if enabled { "activado" } else { "desactivado" }
            );
        }
    }

    /// Activa o desactiva TAA (sombra + escena) y opcionalmente ajusta blend/jitter.
    pub fn set_taa(&mut self, enabled: bool, blend: Option<f32>, jitter_scale: Option<f32>) {
        if let Some(blend) = blend {
            self.taa_blend = blend.clamp(0.0, 1.0);
        }
        self.taa_enabled = enabled;
        log::info!(
            "[taa] TAA: enabled={enabled}, blend={:.2}, jitter={:.2}",
            self.taa_blend,
            jitter_scale.unwrap_or(1.0)
        );
    }
}
