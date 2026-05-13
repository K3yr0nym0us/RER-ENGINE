// ── Escena BASE — estado limpio del editor 2D ───────────────────────────────
//
// Se activa con:  SetScene { scene: "scratch" }
// Contiene únicamente la lógica de reinicio del estado del editor.

use crate::config_compat::Camera;
use crate::config_2d::{ActiveTool, Camera2D};
use crate::ecs::MeshComponent;
use crate::engine::State;
use crate::gizmo;
use crate::mesh;
use crate::scripting::ScriptEngine;

impl State {
    /// Limpieza compartida para cualquier escena 2D del editor.
    ///
    /// Mantiene el comportamiento visible actual, pero garantiza que el cambio
    /// de escena no deje caches, scripts o referencias colgando entre escenas.
    pub(crate) fn reset_runtime_scene_2d(&mut self) {
        self.stop_audio_internal();
        self.physics.clear();
        self.physics_2d.clear();
        self.world.clear();
        self.meshes.clear();
        self.uv_rects.clear();
        self.static_tex_cache.clear();
        self.anim_texture_cache.clear();
        self.atlas.reset(&self.queue);
        self.reload_snap_hint_assets();

        self.scenario_entities.clear();
        self.character_entities.clear();
        self.collider_entities.clear();
        self.execution_area_entities.clear();
        self.execution_overlaps.clear();
        self.background_entity = None;
        self.background_path = None;

        self.anim_overrides.clear();
        self.animations.clear();
        self.active_animations.clear();
        self.default_animation_by_entity.clear();
        self.anim_saved_transforms.clear();
        self.anim_flip_overrides.clear();
        self.entity_facing_right.clear();
        self.visual_offsets.clear();

        self.selected_entity = None;
        self.selected_entities.clear();
        self.hovered_entity = None;
        self.hovered_gizmo_axis = None;
        self.active_gizmo_axis = None;
        self.ctrl_held = false;

        self.active_tool = ActiveTool::None;
        self.tool_overlay_buffer = gizmo::build_from_vertices(&self.device, &[]);
        self.quick_build_ghost_id = None;
        self.quick_build_preview_path = None;
        self.quick_build_preview_kind = None;
        self.quick_build_preview_scale = None;
        self.show_snap_hint = false;
        self.snap_hint_alpha = 0.0;

        self.pivot_edit_mode = None;
        self.logical_area_mode = None;

        self.script_engine = ScriptEngine::new()
            .expect("Error al reinicializar el motor de scripting Lua");
        self.control_bindings_by_entity.clear();
        self.blocked_on_keep_horizontal.clear();
        self.pending_slides.clear();

        self.undo_stack.clear();
        self.redo_stack.clear();
        self.is_applying_undo = false;
    }

    /// Inicializa el estado BASE del editor.
    pub(crate) fn setup_scratch(&mut self) {
        self.reset_runtime_scene_2d();
        self.camera_2d = Some(Camera2D {
            x:      0.0,
            y:      0.0,
            half_h: 3.5,
            near:  -100.0,
            far:    100.0,
        });

        // Cubo central con textura blanca (fallback)
        let cube = mesh::create_cube(&self.device);
        self.meshes.push(cube);
        let white_px = [255u8, 255, 255, 255];
        let uv = self.atlas.pack(&self.queue, &white_px, 1, 1);
        let tex_idx = self.uv_rects.len();
        self.uv_rects.push(uv);
        let cube_id = self.world.spawn(Some("Cube"));
        self.world.insert(cube_id, MeshComponent { mesh_idx: 0, tex_idx });

        // Cámara base del editor (fallback de uniforms).
        self.camera = Camera::new();
        self.clear_color = wgpu::Color { r: 0.06, g: 0.06, b: 0.10, a: 1.0 };

        log::info!("Escena BASE cargada: estado limpio del editor 2D");
    }
}
