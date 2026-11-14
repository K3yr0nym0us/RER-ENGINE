use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use rer_engine_core::backend_policy::GpuInitError;
use winit::window::Window;

use crate::engine3d::config_3d::physics_3d::PhysicsWorld;
use crate::engine3d::config_3d::{Camera, WorldBounds3D};
use crate::engine3d::config_compat::{ActiveTool, GridConfig};
use crate::engine3d::ecs::World;
use crate::engine3d::entity_save_meta::EntitySaveRegistry;
use crate::engine3d::scripting::{ScriptEngine, ScriptEngineProfile};

use super::{State, start_audio_thread};
use crate::engine3d::config_3d::directional_light::{
    DEFAULT_LIGHT_AMBIENT, DEFAULT_LIGHT_COLOR, DEFAULT_LIGHT_DIR, DEFAULT_LIGHT_INTENSITY,
    DEFAULT_SHADOW_DARKNESS,
};

impl State {
    /// Inicializa Ash (Vulkan) para el 3D.
    pub async fn new(window: Arc<Window>) -> Result<Self, GpuInitError> {
        // El 3D sí pide trazado por hardware (todo-o-nada) y `imageCubeArray` para las sondas.
        let vulkan = crate::engine3d::vulkan::SharedGpu::new(
            window.as_ref(),
            &crate::engine3d::vulkan::VulkanOptions::three_d(),
        )
        .map_err(|error| GpuInitError {
            message: error.to_string(),
        })?;
        Self::with_context(window, vulkan)
    }

    /// Construye el estado reutilizando un contexto Ash ya creado por otro host (el editor).
    pub fn with_context(
        window: Arc<Window>,
        vulkan: crate::engine3d::vulkan::SharedGpu,
    ) -> Result<Self, GpuInitError> {
        let size = window.inner_size();

        let shadow_tier = crate::engine3d::config_3d::shadow_graphics::DEFAULT_SHADOW_TIER;
        let shadow_map_size = shadow_tier.shadow_map_size();
        let msaa_tier = crate::engine3d::config_3d::msaa_graphics::DEFAULT_MSAA_TIER;
        let msaa_sample_count = msaa_tier.desired_sample_count();
        let identity_vp = glam::Mat4::IDENTITY.to_cols_array_2d();

        let mut texture_array =
            crate::engine3d::texture::TextureArray::new(&vulkan).map_err(|error| GpuInitError {
                message: error.to_string(),
            })?;
        let fallback_layer = crate::engine3d::texture::TextureArray::fallback_layer();

        let checker_pixels = {
            const S: u32 = 128;
            const TILE: u32 = 8;
            let mut px: Vec<u8> = Vec::with_capacity((S * S * 4) as usize);
            for y in 0..S {
                for x in 0..S {
                    let light = (x / TILE + y / TILE).is_multiple_of(2);
                    let (r, g, b): (u8, u8, u8) = if light { (58, 61, 80) } else { (30, 32, 48) };
                    px.extend_from_slice(&[r, g, b, 255]);
                }
            }
            px
        };
        let checker_layer = texture_array
            .pack(&vulkan, &checker_pixels, 128, 128)
            .map_err(|error| GpuInitError {
                message: error.to_string(),
            })?;
        let tex_layers = vec![checker_layer];

        let frame_renderer = Some(
            crate::engine3d::vulkan::FrameRenderer::new(&vulkan, texture_array.image()).map_err(
                |error| GpuInitError {
                    message: error.to_string(),
                },
            )?,
        );

        let ground_plane =
            crate::engine3d::config_3d::mesh_3d::create_ground_plane(&vulkan, 100.0, 100.0, 1.0);
        let hud_quad_mesh = crate::engine3d::mesh::create_unit_quad_xy(&vulkan);
        let player_ui_text_atlas = crate::engine3d::screen_hud_image::HudAtlas::new(&vulkan)
            .map_err(|error| GpuInitError {
                message: error.to_string(),
            })?;
        let meshes = vec![ground_plane];
        let world = World::new();
        let mut camera = Camera::new();
        camera.target = glam::Vec3::new(0.0, 1.75, 5.0);
        camera.pitch = 0.0;
        camera.yaw = -std::f32::consts::FRAC_PI_2;
        camera.distance = 0.01;

        let gizmo_buffer = crate::engine3d::gizmo::build_axes(&vulkan, 1.14);
        let gizmo_rotate_buffer = crate::engine3d::gizmo::build_rotation_rings(
            &vulkan,
            crate::engine3d::gizmo::GIZMO_ROTATION_RING_RADIUS,
        );
        let ui_work_grid_buffer = crate::engine3d::gizmo::build_from_vertices(&vulkan, &[]);
        let player_ui_text_overlay_buffer =
            crate::engine3d::gizmo::build_from_vertices(&vulkan, &[]);
        let player_ui_object_draw_overlay =
            crate::engine3d::gizmo::build_from_vertices(&vulkan, &[]);
        let tool_overlay_buffer_init = crate::engine3d::gizmo::build_from_vertices(&vulkan, &[]);

        let world_bounds_3d = WorldBounds3D::default();
        let grid_config = GridConfig::default();
        let world_bounds_buffer = world_bounds_3d.build_buffer(&vulkan);
        let world_sky_buffer = world_bounds_3d.build_sky_buffer(&vulkan);

        let audio_slot = start_audio_thread();

        let (model_preload_tx, model_preload_rx) =
            crate::engine3d::config_3d::static_model_cache::create_model_preload_channel();
        let state = Self {
            window,
            vulkan: vulkan.clone(),
            size,
            clear_color: [0.72, 0.86, 0.98, 1.0],
            frame_renderer,
            rt_scene_cache: None,
            vsync_enabled: false,
            prev_view_proj: identity_vp,
            texture_array,
            tex_layers,
            tex_layer_albedo: vec![[0.75, 0.75, 0.75]],
            fallback_layer,
            hud_quad_mesh,
            camera,
            editor_orbit_target: glam::Vec3::ZERO,
            editor_viewport_yaw: -std::f32::consts::FRAC_PI_4,
            editor_viewport_pitch:
                crate::engine3d::config_3d::character_anchor::EDITOR_DEFAULT_ORBIT_PITCH,
            editor_viewport_distance: 3.0,
            fps_editor_frustum_distance: 2.5,
            play_camera_eye_position: glam::Vec3::ZERO,
            play_camera_follow_mode: crate::engine3d::ipc::PlayCameraFollowMode::MoveWithCharacter,
            play_camera_follow_offset: glam::Vec3::ZERO,
            play_camera_follow_offset_local: glam::Vec3::ZERO,
            meshes,
            world,
            last_frame: Instant::now(),
            delta_time: 0.0,
            gizmo_buffer,
            gizmo_rotate_buffer,
            physics: PhysicsWorld::new(),
            selected_entity: None,
            selected_entities: Vec::new(),
            hovered_entity: None,
            hovered_gizmo_axis: None,
            active_gizmo_axis: None,
            transform_gizmo_mode:
                crate::engine3d::config_3d::transform_gizmo::TransformGizmoMode::Translate,
            scenario_entities: Vec::new(),
            character_entities: Vec::new(),
            collider_entities: Vec::new(),
            execution_area_entities: Vec::new(),
            execution_overlaps: std::collections::HashSet::new(),
            background_entity: None,
            background_path: None,
            grid_config,
            world_bounds_3d,
            world_bounds_buffer,
            world_sky_buffer,
            collision_overlay_buffer: crate::engine3d::gizmo::build_lines_from_vertices(
                &vulkan,
                &[],
            ),
            skeleton_overlay_buffer: crate::engine3d::gizmo::build_lines_from_vertices(
                &vulkan,
                &[],
            ),
            socket_overlay_buffer: crate::engine3d::gizmo::build_lines_from_vertices(&vulkan, &[]),
            fps_frustum_buffer: crate::engine3d::gizmo::build_lines_from_vertices(&vulkan, &[]),
            probe_gizmo_buffer: crate::engine3d::gizmo::build_lines_from_vertices(&vulkan, &[]),
            ctrl_held: false,
            shift_held: false,
            active_tool: ActiveTool::None,
            quick_build_ghost_id: None,
            plane_tool_ghost_id: None,
            plane_tool_preview_scale: None,
            plane_tool_tex_cache: HashMap::new(),
            plane_tool_rotate_left: false,
            plane_tool_rotate_right: false,
            engine_window_focused: true,
            embedded: false,
            tool_cursor_pixels: None,
            quick_build_preview_path: None,
            quick_build_preview_kind: None,
            quick_build_preview_scale: None,
            quick_build_blueprint: None,
            blueprint_registry: HashMap::new(),
            entity_blueprint_ids: HashMap::new(),
            entity_colision: HashMap::new(),
            entity_hardness: HashMap::new(),
            entity_attachments: HashMap::new(),
            entity_sockets: HashMap::new(),
            bone_index_cache: HashMap::new(),
            socket_bone_pick_entity: None,
            socket_bone_pick_hovered_joint: None,
            bone_physics_editor_entity: None,
            bone_physics_pick_entity: None,
            bone_physics_pick_hovered_joint: None,
            entity_bone_physics: HashMap::new(),
            entity_projectile_config: HashMap::new(),
            active_projectiles: Vec::new(),
            bone_physics_sim: HashMap::new(),
            bone_physics_entity_motion: HashMap::new(),
            preview_playing: false,
            player_ui_edit_active: false,
            player_ui_edit_restore: None,
            ui_work_grid_buffer,
            player_ui_edit_scope: None,
            player_ui_edit_screen_id: None,
            player_ui_active_player_screen_id: None,
            player_ui_player_screen_names: HashMap::new(),
            player_ui_text_boxes: HashMap::new(),
            player_ui_buttons: HashMap::new(),
            player_ui_images: HashMap::new(),
            player_ui_objects: HashMap::new(),
            player_ui_object_draw: None,
            player_ui_object_draw_overlay,
            player_ui_text_next_id: 1,
            player_ui_text_overlay_buffer,
            player_ui_text_atlas,
            player_ui_hud_texture_cache: HashMap::new(),
            player_ui_font_cache: HashMap::new(),
            player_ui_glyph_instances: Vec::new(),
            player_ui_selected_text_id: None,
            player_ui_selected_button_id: None,
            player_ui_selected_image_id: None,
            player_ui_selected_object_id: None,
            player_ui_text_editing_id: None,
            player_ui_text_caret: 0,
            player_ui_caret_blink_epoch: std::time::Instant::now(),
            player_ui_caret_buffer: crate::engine3d::gizmo::build_from_vertices(&vulkan, &[]),
            player_ui_text_drag: None,
            player_ui_last_text_click: None,

            debug_mode: false,
            preview_entity_transform_snapshots: HashMap::new(),
            preview_fp_view_snapshot: None,
            play_controller_velocity: glam::Vec3::ZERO,
            play_controller_on_floor: true,
            play_controller_jump_queued: false,
            play_controller_jump_request_active: false,
            play_controller_jump_request_prev: false,
            play_controller_script_input: HashSet::new(),
            play_controller_script_walk_speed: None,
            play_controller_script_sprint_multiplier: None,
            play_controller_script_jump_speed: None,
            play_character_entity: None,
            editor_camera_entity: None,
            play_character_mesh_forward_xz: glam::Vec2::new(0.0, 1.0),
            play_character_mesh_extents: None,
            play_session_body_yaw_baseline: 0.0,
            play_session_camera_yaw_baseline: 0.0,
            tool_overlay_buffer: tool_overlay_buffer_init,
            snap_locale: "en".to_string(),
            anim_saved_transforms: std::collections::HashMap::new(),
            pivot_edit_mode: None,
            logical_area_mode: None,
            audio_slot,
            animations: HashMap::new(),
            active_animations: HashMap::new(),
            default_animation_by_entity: HashMap::new(),
            entity_facing_right: HashMap::new(),
            script_engine: ScriptEngine::new(ScriptEngineProfile::Engine3d)
                .expect("Error al inicializar el motor de scripting Rhai"),
            control_bindings_by_entity: HashMap::new(),
            sprite_store: HashMap::new(),
            model_store: HashMap::new(),
            imported_model_registry: crate::engine3d::assets::ImportedModelRegistry::default(),
            rerasset_material_tex: std::collections::HashMap::new(),
            static_model_cache: HashMap::new(),
            worldgen: crate::engine3d::config_3d::worldgen::StateWorldgenState::default(),
            model_preload_rx,
            model_preload_tx,
            model_preload_inflight: HashSet::new(),
            model_preload_gpu_queue: Vec::new(),
            pending_load_models: Vec::new(),
            pending_entity_model_replaces: Vec::new(),
            sound_store: HashMap::new(),
            font_store: HashMap::new(),
            hud_image_store: HashMap::new(),
            background_store: HashMap::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            is_applying_undo: false,
            process_metrics_sampler: rer_engine_core::process_metrics::ProcessMetricsSampler::new(),
            metrics_last_emit: Instant::now(),
            metrics_frame_count: 0,
            last_draw_calls: 0,
            autosave_enabled: false,
            autosave_last_tick: Instant::now(),
            save_registry: EntitySaveRegistry::new(),
            target_fps: 60,
            graphics_texture_tier:
                crate::engine3d::config_3d::texture_graphics::TextureGraphicsTier::Medium,
            reflection_tier:
                crate::engine3d::config_3d::reflection_graphics::DEFAULT_REFLECTION_TIER,
            reflection_probes_enabled: false,
            reflection_raytracing_enabled: false,

            reflection_debug_view:
                crate::engine3d::config_3d::reflection_graphics::ReflectionDebugView::Final,

            probe_capture_cursor: 0,
            probe_capture_burst_all: false,
            probe_entity_slots: std::collections::HashMap::new(),
            last_probe_capture_ids: None,
            probe_capture_cooldown: 0,
            rt_skinned_triangles: Vec::new(),
            rt_skinned_materials: Vec::new(),
            rt_skinned_vertices: std::collections::HashMap::new(),
            rt_skinned_instances: Vec::new(),
            rt_skinned_rebuild: false,
            rt_skinned_tick: 0,

            shadow_tier,
            shadow_map_size,
            msaa_tier,
            msaa_sample_count,
            taa_enabled: false,
            taa_blend: 0.2,
            taa_jitter_scale: 1.0,
            taa_frame: 0,
            texture_detail_near_m:
                crate::engine3d::config_3d::entity_textures::default_texture_detail_near_m(),
            model_lod_enabled: crate::engine3d::config_3d::model_lod::DEFAULT_MODEL_LOD_ENABLED,
            model_lod1_distance_m:
                crate::engine3d::config_3d::model_lod::DEFAULT_MODEL_LOD1_DISTANCE_M,
            model_lod2_distance_m:
                crate::engine3d::config_3d::model_lod::DEFAULT_MODEL_LOD2_DISTANCE_M,
            mesh_lod_variants: HashMap::new(),
            glb_texture_catalog_cache: HashMap::new(),
            entity_texture_effective_cap: HashMap::new(),
            texture_lod_last_update: Instant::now(),
            sun_entity: None,
            sun_icon_mesh_idx: None,
            sun_icon_tex_idx: None,
            editor_box_mesh_idx: None,
            editor_box_tex_idx: None,
            mat_val_label_mesh_idx: None,
            mat_val_texture_cache: HashMap::new(),
            plane_tool_wall_mesh_idx: None,
            directional_light_dir: DEFAULT_LIGHT_DIR.normalize(),
            directional_light_color: DEFAULT_LIGHT_COLOR,
            directional_light_ambient: DEFAULT_LIGHT_AMBIENT,
            light_intensity: DEFAULT_LIGHT_INTENSITY,
            shadow_darkness: DEFAULT_SHADOW_DARKNESS,
            texture_path_layers: HashMap::new(),
            model_assets: std::collections::HashMap::new(),
            model_animation_bindings: std::collections::HashMap::new(),
            active_model_clips: std::collections::HashMap::new(),
            model_clip_defaults: std::collections::HashMap::new(),
            skinned_gpu_meshes: Vec::new(),
            mount_save_on_empty_world: false,
            restoring_save_manifest: false,
            fp_baseline_defer_frames: 0,
            editor_scenes: crate::engine3d::engine::editor_scenes::EditorSceneStore::new(),
        };
        Ok(state)
    }
}
