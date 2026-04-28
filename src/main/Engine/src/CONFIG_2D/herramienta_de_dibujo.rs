// ── Herramienta de dibujo 2D ──────────────────────────────────────────────────
//
// Crea una entidad visual (quad) a partir de 4 puntos en espacio de mundo (XY).
// No añade física — el llamador decide si la entidad necesita un collider.
// Reutilizable por cualquier herramienta del editor 2D.

use glam::Vec3 as GlamVec3;

use crate::ecs::{EntityId, MeshComponent, Transform};
use crate::engine::State;
use crate::mesh::{upload, Mesh, Vertex};
use crate::texture::GpuTexture;

impl State {
    /// Crea una entidad de cuadro visual a partir de 4 puntos en espacio de mundo.
    /// Devuelve `(EntityId, posición_centro[3], escala[3])`.
    /// No añade física — el llamador decide si la entidad necesita un collider.
    pub(crate) fn create_box_entity(
        &mut self,
        pts:   &[[f32; 2]; 4],
        name:  &str,
        color: [u8; 4],
    ) -> (EntityId, [f32; 3], [f32; 3]) {
        let (mesh, pos, scale) = create_mesh_from_4_points(pts, &self.device);
        let mesh_idx = self.meshes.len();
        self.meshes.push(mesh);

        let tex = GpuTexture::from_rgba(&self.device, &self.queue, &color, 1, 1, "box-entity");
        self.textures.push(tex.create_bind_group(&self.device, &self.texture_bgl));

        let (buf, bg) = self.alloc_entity_uniform();
        self.entity_buffers.push(buf);
        self.entity_bind_groups.push(bg);

        let entity = self.world.spawn(Some(name));
        self.world.insert(entity, MeshComponent { mesh_idx });
        self.world.insert(entity, Transform {
            position: GlamVec3::from(pos),
            scale:    GlamVec3::from(scale),
            ..Default::default()
        });

        (entity, pos, scale)
    }
}

/// Crea un mesh a partir de 4 puntos arbitrarios en espacio de mundo (plano XY).
/// Los vértices se normalizan respecto al bounding box del centroide para que
/// `Transform.position = centroide` y `Transform.scale = (bbox_w, bbox_h, 1)`
/// sean coherentes con el renderizador y el picking por AABB.
/// Devuelve `(Mesh, posición[3], escala[3])`.
pub(super) fn create_mesh_from_4_points(pts: &[[f32; 2]; 4], device: &wgpu::Device) -> (Mesh, [f32; 3], [f32; 3]) {
    let cx = pts.iter().map(|p| p[0]).sum::<f32>() / 4.0;
    let cy = pts.iter().map(|p| p[1]).sum::<f32>() / 4.0;
    let min_x = pts.iter().map(|p| p[0]).fold(f32::INFINITY,     f32::min);
    let max_x = pts.iter().map(|p| p[0]).fold(f32::NEG_INFINITY, f32::max);
    let min_y = pts.iter().map(|p| p[1]).fold(f32::INFINITY,     f32::min);
    let max_y = pts.iter().map(|p| p[1]).fold(f32::NEG_INFINITY, f32::max);
    let bw = (max_x - min_x).max(0.01);
    let bh = (max_y - min_y).max(0.01);

    // Normalize to [-0.5, 0.5] space so the model matrix (scale = bbox) remaps correctly.
    let uvs = [[0.0_f32, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
    let vertices: Vec<Vertex> = pts.iter().enumerate().map(|(i, p)| Vertex {
        position: [(p[0] - cx) / bw, (p[1] - cy) / bh, 0.0],
        normal:   [0.0, 0.0, 1.0],
        uv:       uvs[i],
    }).collect();
    let indices = vec![0u32, 1, 2, 2, 3, 0];

    // Z = -0.5: entre los escenarios (Z=-1) y los personajes (Z=0).
    let position = [cx, cy, -0.5];
    let scale    = [bw, bh, 1.0];
    (upload(device, &vertices, &indices, "collider-quad"), position, scale)
}
