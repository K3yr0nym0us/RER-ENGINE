use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use glam::Vec3 as GlamVec3;
use winit::{dpi::PhysicalSize, window::Window};

use super::{ActiveAnimation, AnimationState, AudioSlot, UndoAction};
use crate::config_3d::physics_3d::PhysicsWorld;
use crate::config_3d::{Camera, WorldBounds3D};
use crate::config_compat::{ActiveTool, Camera2D, GridBuffer, GridConfig, PhysicsWorld2D};
use crate::ecs::{EntityId, NameComponent, World};
use crate::gizmo::GizmoBuffer;
use crate::mesh::Mesh;
use crate::entity_save_meta::EntitySaveRegistry;
use crate::scripting::ScriptEngine;

pub struct State {
    pub(crate) window: Arc<Window>,
    pub(crate) surface: wgpu::Surface<'static>,
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) config: wgpu::SurfaceConfiguration,
    pub(crate) size: PhysicalSize<u32>,
    pub(crate) clear_color: wgpu::Color,
    pub(crate) render_pipeline: wgpu::RenderPipeline,
    /// Pipeline para modo 2D: sin depth-write, CompareFunction::Always.
    /// Permite que el alpha blending funcione correctamente con back-to-front sort.
    pub(crate) render_pipeline_2d: wgpu::RenderPipeline,
    pub(crate) render_pipeline_overlay: wgpu::RenderPipeline,
    pub(crate) shadow_pipeline: wgpu::RenderPipeline,
    pub(crate) _shadow_texture: wgpu::Texture,
    pub(crate) shadow_map_view: wgpu::TextureView,
    pub(crate) depth_view: wgpu::TextureView,
    pub(crate) taa: rer_engine_shared::taa::TaaPass,
    pub(crate) scene_buffer: wgpu::Buffer,
    pub(crate) scene_bind_group: wgpu::BindGroup,
    /// Solo uniformes; evita leer el shadow map mientras se escribe en el pase de sombras.
    pub(crate) shadow_pass_bind_group: wgpu::BindGroup,
    pub(crate) hud_scene_bind_group: wgpu::BindGroup,
    pub(crate) atlas: crate::texture::TextureAtlas,
    pub(crate) uv_rects: Vec<[f32; 4]>,
    pub(crate) fallback_uv: [f32; 4],
    pub(crate) static_tex_cache: std::collections::HashMap<String, [f32; 4]>,
    pub(crate) canonical_quad_idx: usize,
    /// Quad para tooltips en pantalla (HUD); no usar `meshes[0]` (suelo).
    pub(crate) hud_quad_mesh: Mesh,
    pub camera: Camera,
    /// Profundidad del gizmo de frustum FPS en modo editor (metros).
    pub(crate) fps_editor_frustum_distance: f32,
    /// Cámara 2D ortográfica activa cuando se carga una escena 2D.
    /// `None` = modo 3D (usa `camera`).
    pub camera_2d: Option<Camera2D>,
    pub(crate) meshes: Vec<Mesh>,
    pub(crate) world: World,
    pub(crate) last_frame: Instant,
    pub delta_time: f32,
    pub(crate) gizmo_pipeline: wgpu::RenderPipeline,
    pub(crate) gizmo_buffer: GizmoBuffer,
    pub(crate) gizmo_bind_group: wgpu::BindGroup,
    pub(crate) gizmo_buffer_uni: wgpu::Buffer,
    pub physics: PhysicsWorld,
    pub physics_2d: PhysicsWorld2D,
    pub selected_entity: Option<EntityId>,
    pub selected_entities: Vec<EntityId>,
    pub hovered_entity: Option<EntityId>,
    pub hovered_gizmo_axis: Option<usize>,
    pub active_gizmo_axis: Option<usize>,
    pub(crate) spatial_grid: crate::spatial::SpatialGrid,
    pub(crate) scenario_entities: Vec<EntityId>,
    pub(crate) character_entities: Vec<EntityId>,
    pub(crate) background_entity: Option<EntityId>,
    pub(crate) background_path: Option<String>,
    pub(crate) grid_config: GridConfig,
    pub(crate) grid_pipeline: wgpu::RenderPipeline,
    pub(crate) grid_buffer: GridBuffer,
    pub(crate) grid_bind_group: wgpu::BindGroup,
    pub(crate) grid_buffer_uni: wgpu::Buffer,
    pub(crate) world_bounds_3d: WorldBounds3D,
    pub(crate) world_bounds_buffer: GizmoBuffer,
    pub(crate) crosshair_buffer: GizmoBuffer,
    pub(crate) ctrl_held: bool,
    pub active_tool: ActiveTool,
    pub(crate) quick_build_ghost_id: Option<EntityId>,
    pub(crate) quick_build_preview_path: Option<String>,
    pub(crate) quick_build_preview_kind: Option<String>,
    pub(crate) quick_build_preview_scale: Option<[f32; 3]>,
    pub preview_playing: bool,
    /// Velocidad del personaje (m/s), patrón Godot CharacterBody3D / Unity CharacterController.
    pub(crate) play_controller_velocity: GlamVec3,
    pub(crate) play_controller_on_floor: bool,
    pub(crate) play_controller_jump_queued: bool,
    /// Detección de flanco (Godot `is_action_just_pressed`): true mientras el script SPACE
    /// está pulsando este frame; comparado con `_prev` para detectar la transición.
    pub(crate) play_controller_jump_request_active: bool,
    pub(crate) play_controller_jump_request_prev: bool,
    /// Teclas acumuladas por scripts de control en el frame actual (play).
    pub(crate) play_controller_script_input: HashSet<String>,
    /// Parámetros opcionales fijados por scripts Lua de control (play).
    pub(crate) play_controller_lua_walk_speed: Option<f32>,
    pub(crate) play_controller_lua_sprint_multiplier: Option<f32>,
    pub(crate) play_controller_lua_jump_speed: Option<f32>,
    /// Entidad `[Player]` principal de la escena 3D.
    pub(crate) play_character_entity: Option<EntityId>,
    /// Forward local (plano XZ) del mesh del jugador; se recalcula al reemplazar el modelo.
    pub(crate) play_character_mesh_forward_xz: glam::Vec2,
    pub(crate) tool_overlay_buffer: GizmoBuffer,
    pub(crate) snap_hint_uv: Option<[f32; 4]>,
    pub(crate) snap_hint_size: (f32, f32),
    pub(crate) snap_hint_uv_en: Option<[f32; 4]>,
    pub(crate) snap_hint_size_en: (f32, f32),
    pub(crate) snap_locale: String,
    pub(crate) show_snap_hint: bool,
    pub(crate) snap_hint_alpha: f32,
    /// Tooltip «pulsa Esc para salir» en play FPS 3D.
    pub(crate) fps_exit_hint_uv: Option<[f32; 4]>,
    pub(crate) fps_exit_hint_size: (f32, f32),
    pub(crate) fps_exit_hint_uv_en: Option<[f32; 4]>,
    pub(crate) fps_exit_hint_size_en: (f32, f32),
    pub(crate) fps_exit_hint_alpha: f32,
    pub(crate) collider_entities: Vec<EntityId>,
    pub(crate) execution_area_entities: Vec<EntityId>,
    pub(crate) execution_overlaps: HashSet<(EntityId, EntityId)>,
    pub(crate) anim_saved_transforms: std::collections::HashMap<u32, (GlamVec3, GlamVec3)>,
    pub pivot_edit_mode: Option<(u32, String, u32, u32)>,
    pub logical_area_mode: Option<u32>,
    pub(crate) audio_slot: Option<AudioSlot>,
    pub(crate) anim_texture_cache: std::collections::HashMap<String, ([f32; 4], u32, u32)>,
    pub(crate) anim_overrides: std::collections::HashMap<usize, [f32; 4]>,
    pub(crate) animations: HashMap<u32, HashMap<String, AnimationState>>,
    pub(crate) active_animations: HashMap<u32, ActiveAnimation>,
    pub(crate) default_animation_by_entity: HashMap<u32, String>,
    pub(crate) anim_flip_overrides: HashMap<u32, bool>,
    pub(crate) entity_facing_right: HashMap<u32, bool>,
    pub(crate) script_engine: ScriptEngine,
    pub(crate) control_bindings_by_entity: HashMap<u32, crate::ipc::ControlBindingsData>,
    pub(crate) sprite_store: HashMap<String, (String, u32, u32)>,
    /// Modelos 3D precargados: ruta absoluta → nombre visible.
    pub(crate) model_store: HashMap<String, String>,
    pub(crate) sound_store: HashMap<String, String>,
    pub(crate) background_store: HashMap<String, String>,
    pub(crate) undo_stack: Vec<UndoAction>,
    pub(crate) redo_stack: Vec<UndoAction>,
    pub(crate) is_applying_undo: bool,
    pub(crate) metrics_last_emit: Instant,
    pub(crate) metrics_frame_count: u32,
    pub(crate) last_draw_calls: u32,
    pub(crate) autosave_enabled: bool,
    pub(crate) autosave_last_tick: Instant,
    /// Metadatos de persistencia (rutas, tipo) y fuentes de scripts por entidad.
    pub(crate) save_registry: EntitySaveRegistry,
    /// Límite de FPS del bucle (sincronizado con `set_target_fps`).
    pub(crate) target_fps: u64,
    /// Entidad icono del sol (luz direccional).
    pub(crate) sun_entity: Option<EntityId>,
    pub(crate) directional_light_dir: GlamVec3,
    pub(crate) directional_light_color: GlamVec3,
    pub(crate) directional_light_ambient: f32,
    pub(crate) light_intensity: f32,
    pub(crate) shadow_darkness: f32,
}

impl State {
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
        let clean_base = base.trim();
        let prefix = format!("{}_", clean_base);
        let mut max_suffix: u32 = 0;

        for (_id, c) in self.world.query::<NameComponent>() {
            let current = c.name.trim();
            if let Some(rest) = current.strip_prefix(&prefix) {
                if let Ok(n) = rest.parse::<u32>() {
                    if n > max_suffix {
                        max_suffix = n;
                    }
                }
            }
        }

        format!("{}_{:02}", clean_base, max_suffix.saturating_add(1))
    }
}
