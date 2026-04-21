// ── CONFIG_SHARED — utilidades compartidas entre todos los modos ──────────────
//
// Contiene funciones usadas por más de un modo de escena (2D, 3D, BASE):
//  · alloc_entity_uniform  — crea el buffer wgpu de uniforms para una entidad
//  · point_to_segment_2d   — distancia de un punto a un segmento 2D (picking)

use glam::Mat4;

use crate::engine::{SceneUniforms, State};

impl State {
    /// Crea un nuevo buffer de uniforms por entidad y su bind group asociado.
    ///
    /// wgpu aplica todos los `write_buffer` al hacer submit; si dos entidades
    /// compartieran buffer, el último write ganaría. Un buffer por malla
    /// garantiza que cada draw call vea sus propios datos.
    pub(crate) fn alloc_entity_uniform(&self) -> (wgpu::Buffer, wgpu::BindGroup) {
        use wgpu::util::DeviceExt;

        let identity = SceneUniforms {
            view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            model:     Mat4::IDENTITY.to_cols_array_2d(),
            cam_pos:   [0.0; 4],
        };
        let buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label:    Some("entity-uniforms"),
            contents: bytemuck::cast_slice(&[identity]),
            usage:    wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:   Some("entity-bg"),
            layout:  &self.scene_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding:  0,
                resource: buf.as_entire_binding(),
            }],
        });
        (buf, bg)
    }
}

/// Distancia 2D desde el punto `(px, py)` al segmento `[(ax,ay), (bx,by)]`.
/// Usada por el picking de ejes de gizmo tanto en modo 2D como 3D.
pub(crate) fn point_to_segment_2d(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let dx = bx - ax;
    let dy = by - ay;
    let len_sq = dx * dx + dy * dy;
    if len_sq < 1e-6 {
        return ((px - ax).powi(2) + (py - ay).powi(2)).sqrt();
    }
    let t  = ((px - ax) * dx + (py - ay) * dy) / len_sq;
    let t  = t.clamp(0.0, 1.0);
    let cx = ax + t * dx;
    let cy = ay + t * dy;
    ((px - cx).powi(2) + (py - cy).powi(2)).sqrt()
}
