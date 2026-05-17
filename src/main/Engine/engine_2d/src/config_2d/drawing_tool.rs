// ── 2D Drawing Tool ─────────────────────────────────────────────────────────
//
// Creates a visual entity (quad) from 4 points in world space (XY).
// Does not add physics — the caller decides if the entity needs a collider.
// Reusable by any 2D editor tool.

use glam::Vec3 as GlamVec3;

use crate::ecs::{EntityId, MeshComponent, Transform};
use crate::engine::State;
use crate::mesh::{upload, Mesh, Vertex};

impl State {
    /// Crea una entidad visual (quad) desde 4 puntos; reutiliza `forced_id` si se indica (import de escena).
    pub(crate) fn create_box_entity_at(
        &mut self,
        pts:        &[[f32; 2]; 4],
        name:       &str,
        color:      [u8; 4],
        forced_id:  Option<EntityId>,
    ) -> Option<(EntityId, [f32; 3], [f32; 3])> {
        let (mesh, pos, scale) = create_mesh_from_4_points(pts, &self.device);
        let mesh_idx = self.meshes.len();
        self.meshes.push(mesh);

        let tex_idx = self.uv_rects.len();
        self.uv_rects.push(self.atlas.pack(&self.queue, &color, 1, 1));

        let entity = if let Some(id) = forced_id {
            if !self.world.spawn_with_id(id, Some(name)) {
                return None;
            }
            id
        } else {
            self.world.spawn(Some(name))
        };
        self.world.insert(entity, MeshComponent { mesh_idx, tex_idx });
        self.world.insert(entity, Transform {
            position: GlamVec3::from(pos),
            scale:    GlamVec3::from(scale),
            ..Default::default()
        });

        Some((entity, pos, scale))
    }
}

/// Creates a mesh from 4 arbitrary points in world space (XY plane).
/// The vertices are normalized with respect to the bounding box of the centroid so that
/// `Transform.position = centroid` and `Transform.scale = (bbox_w, bbox_h, 1)`
/// are consistent with the renderer and AABB picking.
/// Returns `(Mesh, position[3], scale[3])`.
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

    // Z = -0.5: between scenarios (Z=-1) and characters (Z=0).
    let position = [cx, cy, -0.5];
    let scale    = [bw, bh, 1.0];
    (upload(device, &vertices, &indices, "collider-quad"), position, scale)
}
