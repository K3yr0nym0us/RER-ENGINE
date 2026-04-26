// ── Escena BASE — cubo de referencia (escenario vacío) ───────────────────────
//
// Se activa con:  SetScene { scene: "scratch" }
// Contiene únicamente la lógica de setup del escenario vacío por defecto.

use crate::config_3d::Camera;
use crate::ecs::MeshComponent;
use crate::engine::State;
use crate::mesh;
use crate::texture::GpuTexture;

impl State {
    /// Inicializa la escena BASE: un cubo de referencia con cámara orbital.
    pub(crate) fn setup_scratch(&mut self) {
        self.world.clear();
        self.meshes.clear();
        self.textures.clear();
        self.anim_texture_cache.clear();
        self.anim_overrides.clear();
        self.entity_buffers.clear();
        self.entity_bind_groups.clear();
        self.selected_entity = None;
        self.hovered_entity  = None;
        self.camera_2d       = None;  // volver a modo 3D

        // Cubo central con textura blanca (fallback)
        let cube = mesh::create_cube(&self.device);
        self.meshes.push(cube);
        self.textures.push(
            GpuTexture::white(&self.device, &self.queue)
                .create_bind_group(&self.device, &self.texture_bgl),
        );
        let (b, bg) = self.alloc_entity_uniform();
        self.entity_buffers.push(b);
        self.entity_bind_groups.push(bg);
        let cube_id = self.world.spawn(Some("Cube"));
        self.world.insert(cube_id, MeshComponent { mesh_idx: 0 });

        // Cámara orbital por defecto mirando el cubo
        self.camera = Camera::new();
        self.clear_color = wgpu::Color { r: 0.06, g: 0.06, b: 0.10, a: 1.0 };

        log::info!("Escena BASE cargada: cubo de referencia");
    }
}
