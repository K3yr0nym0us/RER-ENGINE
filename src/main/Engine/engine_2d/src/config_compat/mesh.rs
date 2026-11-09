use crate::mesh::{Mesh, create_cube};

pub(crate) fn create_ground_plane() -> Mesh {
    // Fallback liviano para mantener compatibilidad del setup base.
    create_cube()
}
