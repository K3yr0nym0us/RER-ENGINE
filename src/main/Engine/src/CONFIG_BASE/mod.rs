// ── Escena BASE — cubo de referencia (escenario vacío) ───────────────────────
//
// Se activa con:  SetScene { scene: "scratch" }
// Contiene únicamente la lógica de setup del escenario vacío por defecto.

use crate::config_3d::Camera;
use crate::ecs::MeshComponent;
use crate::engine::State;
use crate::mesh;

impl State {
    /// Inicializa la escena BASE: un cubo de referencia con cámara orbital.
    pub(crate) fn setup_scratch(&mut self) {
        self.world.clear();
        self.meshes.clear();
        self.uv_rects.clear();
        self.static_tex_cache.clear();
        self.anim_texture_cache.clear();
        self.anim_overrides.clear();
        self.selected_entity = None;
        self.hovered_entity  = None;
        self.camera_2d       = None;  // volver a modo 3D

        // Cubo central con textura blanca (fallback)
        let cube = mesh::create_cube(&self.device);
        self.meshes.push(cube);
        let white_px = [255u8, 255, 255, 255];
        let uv = self.atlas.pack(&self.queue, &white_px, 1, 1);
        let tex_idx = self.uv_rects.len();
        self.uv_rects.push(uv);
        let cube_id = self.world.spawn(Some("Cube"));
        self.world.insert(cube_id, MeshComponent { mesh_idx: 0, tex_idx });

        // Cámara orbital por defecto mirando el cubo
        self.camera = Camera::new();
        self.clear_color = wgpu::Color { r: 0.06, g: 0.06, b: 0.10, a: 1.0 };

        log::info!("Escena BASE cargada: cubo de referencia");
    }
}
