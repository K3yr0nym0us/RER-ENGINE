use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use rer_engine_core::backend_policy::GpuInitError;
use winit::window::Window;

use crate::engine2d::config_2d::{ActiveTool, GridConfig, PhysicsWorld2D};
use crate::engine2d::config_compat::Camera;
use crate::engine2d::ecs::{MeshComponent, World};
use crate::engine2d::entity_save_meta::EntitySaveRegistry;
use crate::engine2d::gizmo;
use crate::engine2d::mesh;
use crate::engine2d::scripting::{ScriptEngine, ScriptEngineProfile};

use super::State;
use super::audio::start_audio_thread;

impl State {
    /// Inicializa el contexto Vulkan (Ash) y los recursos CPU del motor.
    pub fn new(window: Arc<Window>) -> Result<Self, GpuInitError> {
        let size = window.inner_size();

        let vulkan2d = Arc::new(
            crate::engine2d::vulkan::Vulkan2d::new(window.as_ref()).map_err(|error| {
                GpuInitError {
                    message: error.to_string(),
                }
            })?,
        );

        // ── Atlas de sprites ──────────────────────────────────────────────────
        let mut atlas = crate::engine2d::texture::TextureAtlas::new(vulkan2d.clone());
        let fallback_uv = crate::engine2d::texture::TextureAtlas::fallback_uv();

        // Checkerboard para el plano de suelo (tex_idx=0).
        // Patrón bakeado 128×128 con tiles de 8px para no depender del tiling con Repeat.
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
        let checker_uv = atlas.pack(&checker_pixels, 128, 128);
        let uv_rects = vec![checker_uv]; // idx 0 = checkerboard (plano de suelo)

        // Hint visual de snap (Ctrl): carga versión ES y EN del PNG en el atlas.
        let load_snap_hint = |atlas: &mut crate::engine2d::texture::TextureAtlas,
                              filename: &str|
         -> (Option<[f32; 4]>, (f32, f32)) {
            let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../assets")
                .join(filename);
            match std::fs::read(&path) {
                Ok(bytes) => {
                    use image::ImageReader;
                    match ImageReader::new(std::io::Cursor::new(&bytes))
                        .with_guessed_format()
                        .map_err(|e| e.to_string())
                        .and_then(|r| r.decode().map_err(|e| e.to_string()))
                    {
                        Ok(img) => {
                            let img = img.to_rgba8();
                            let (w, h) = img.dimensions();
                            let uv = atlas.pack(img.as_raw(), w, h);
                            (Some(uv), (w as f32, h as f32))
                        }
                        Err(e) => {
                            log::warn!(
                                "[snap-hint] Error decodificando '{}': {}",
                                path.display(),
                                e
                            );
                            (None, (0.0, 0.0))
                        }
                    }
                }
                Err(e) => {
                    log::warn!("[snap-hint] No se pudo leer '{}': {}", path.display(), e);
                    (None, (0.0, 0.0))
                }
            }
        };
        let (snap_hint_uv, snap_hint_size) =
            load_snap_hint(&mut atlas, "tooltip-btn-ctrl-to-auto-adjust.png");
        let (snap_hint_uv_en, snap_hint_size_en) =
            load_snap_hint(&mut atlas, "tooltip-btn-ctrl-to-auto-adjust-english.png");

        // ── Escenario base del editor 2D ─────────────────────────────────────
        let ground_plane = crate::engine2d::config_compat::mesh::create_ground_plane();
        let meshes = vec![ground_plane];
        let mut world = World::new();
        // Entidad del plano
        let plane_id = world.spawn(Some("Ground"));
        world.insert(
            plane_id,
            MeshComponent {
                mesh_idx: 0,
                tex_idx: 0,
            },
        );
        // Cámara base del editor para estados sin cámara ortográfica inicializada.
        let mut camera = Camera::new();
        camera.target = glam::Vec3::new(0.0, 1.75, 5.0);
        camera.pitch = 0.0;
        camera.yaw = -std::f32::consts::FRAC_PI_2;
        camera.distance = 0.01; // muy cerca — simula la posición del ojo

        // ── Overlays (CPU): el renderer Ash los sube cada frame ───────────────
        let tool_overlay_buffer_init = gizmo::build_from_vertices(&[]);
        let ui_work_grid_buffer = gizmo::build_from_vertices(&[]);
        let player_ui_text_overlay_buffer = gizmo::build_from_vertices(&[]);
        let player_ui_caret_buffer = gizmo::build_from_vertices(&[]);
        let player_ui_object_draw_overlay = gizmo::build_from_vertices(&[]);

        let grid_config = GridConfig::default();

        // ── Atlas HUD de pantalla + quad instanciado ──────────────────────────
        let player_ui_text_atlas =
            crate::engine2d::screen_hud_image::ScreenHudAtlas::new(vulkan2d.clone());
        let hud_quad_mesh = mesh::create_unit_quad_xy();

        // ── Audio: thread dedicado ──────────────────────────────────────────────
        // El thread de audio vive independiente del render thread para que la
        // creación/destrucción de Sinks de rodio (que puede bloquear en ALSA/PulseAudio)
        // nunca detenga el render loop ni cause drift en el timing de animaciones.
        let audio_slot = start_audio_thread();

        Ok(Self {
            window,
            size,
            clear_color: [0.06, 0.06, 0.10, 1.0],
            vulkan2d,
            atlas,
            uv_rects,
            fallback_uv,
            static_tex_cache: std::collections::HashMap::new(),
            canonical_quad_idx: 0,
            camera,
            camera_2d: None, // se activa al recibir SetScene { scene: "2D" }

            meshes,
            world,
            last_frame: Instant::now(),
            delta_time: 0.0,
            physics_2d: PhysicsWorld2D::new(),
            selected_entity: None,
            selected_entities: Vec::new(),
            hovered_entity: None,
            hovered_gizmo_axis: None,
            active_gizmo_axis: None,
            spatial_grid: crate::engine2d::spatial::SpatialGrid::new(),
            scenario_entities: Vec::new(),
            character_entities: Vec::new(),
            projectile_entities: Vec::new(),
            entity_projectile_config: std::collections::HashMap::new(),
            entity_hardness: std::collections::HashMap::new(),
            active_projectiles: Vec::new(),
            background_entity: None,
            background_path: None,
            grid_config,
            ctrl_held: false,
            shift_held: false,
            active_tool: ActiveTool::None,
            quick_build_ghost_id: None,
            quick_build_preview_path: None,
            quick_build_preview_kind: None,
            quick_build_preview_scale: None,
            preview_playing: false,
            play_mouse_px: None,
            player_ui_edit_active: false,
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
            player_ui_caret_blink_epoch: Instant::now(),
            player_ui_caret_buffer,
            player_ui_text_drag: None,
            player_ui_last_text_click: None,
            hud_quad_mesh,
            hud_image_store: HashMap::new(),
            debug_mode: false,
            vsync_enabled: false,
            tool_overlay_buffer: tool_overlay_buffer_init,
            snap_hint_uv,
            snap_hint_size,
            snap_hint_uv_en,
            snap_hint_size_en,
            snap_locale: "en".to_string(),
            show_snap_hint: false,
            snap_hint_alpha: 0.0,
            collider_entities: Vec::new(),
            execution_area_entities: Vec::new(),
            execution_overlaps: HashSet::new(),
            anim_saved_transforms: std::collections::HashMap::new(),
            visual_offsets: std::collections::HashMap::new(),
            pivot_edit_mode: None,
            logical_area_mode: None,
            audio_slot,
            anim_texture_cache: std::collections::HashMap::new(),
            anim_overrides: std::collections::HashMap::new(),
            animations: HashMap::new(),
            active_animations: HashMap::new(),
            default_animation_by_entity: HashMap::new(),
            anim_flip_overrides: HashMap::new(),
            entity_facing_right: HashMap::new(),
            script_engine: ScriptEngine::new(ScriptEngineProfile::Engine2d)
                .expect("Error al inicializar el motor de scripting Rhai"),
            control_bindings_by_entity: HashMap::new(),
            sprite_store: HashMap::new(),
            sound_store: HashMap::new(),
            font_store: HashMap::new(),
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
            target_fps: 60,
            blocked_on_keep_horizontal: HashMap::new(),
            pending_slides: HashMap::new(),
            save_registry: EntitySaveRegistry::new(),
            suppress_scene_setup_logs: false,
        })
    }
}
