use crate::mesh::{Mesh, create_cube};

pub(crate) fn create_ground_plane(device: &wgpu::Device) -> Mesh {
    // Fallback liviano para mantener compatibilidad del setup base.
    create_cube(device)
}
