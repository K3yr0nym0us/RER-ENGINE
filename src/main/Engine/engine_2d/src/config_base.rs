// ── Escena BASE — estado limpio del editor 2D ───────────────────────────────
//
// Se activa con:  SetScene { scene: "scratch" }
// Contiene únicamente la lógica de reinicio del estado del editor.

use crate::config_compat::Camera;
use crate::config_2d::Camera2D;
use crate::ecs::MeshComponent;
use crate::engine::State;
use crate::mesh;

impl State {
    /// Inicializa el estado BASE del editor.
    pub(crate) fn setup_scratch(&mut self) {
        self.world.clear();
        self.meshes.clear();
        self.uv_rects.clear();
        self.static_tex_cache.clear();
        self.anim_texture_cache.clear();
        self.atlas.reset(&self.queue);
        self.reload_snap_hint_assets();
        self.anim_overrides.clear();
        self.animations.clear();
        self.active_animations.clear();
        self.default_animation_by_entity.clear();
        self.anim_saved_transforms.clear();
        self.anim_flip_overrides.clear();
        self.entity_facing_right.clear();
        self.selected_entity = None;
        self.selected_entities.clear();
        self.hovered_entity  = None;
        self.camera_2d       = Some(Camera2D {
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
