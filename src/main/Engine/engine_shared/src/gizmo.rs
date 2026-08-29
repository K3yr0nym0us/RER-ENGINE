//! Escala del gizmo de transformación: tamaño ~constante en pantalla (estilo Blender).

/// Longitud de cada eje en el mesh (`build_axes`).
pub const GIZMO_MESH_AXIS_LENGTH: f32 = 1.14;

/// Longitud visual objetivo de un eje en píxeles.
pub const GIZMO_AXIS_TARGET_PX: f32 = 72.0;

/// Separación mínima (px) entre el borde de la selección y el inicio de cada flecha.
pub const GIZMO_AXIS_GAP_PX: f32 = 8.0;

pub const GIZMO_WORLD_SCALE_MIN: f32 = 0.01;
pub const GIZMO_WORLD_SCALE_MAX: f32 = 80.0;

/// Máximo múltiplo del tamaño de la selección para no tapar objetos diminutos.
pub const GIZMO_SELECTION_EXTENT_FACTOR: f32 = 1.5;

/// Escala en mundo para cámara perspectiva 3D.
pub fn world_scale_perspective(
    camera_pos: glam::Vec3,
    anchor: glam::Vec3,
    fov_y_rad: f32,
    viewport_height_px: u32,
) -> f32 {
    let distance = (camera_pos - anchor).length().max(0.001);
    let vh = viewport_height_px.max(1) as f32;
    let world_per_px = 2.0 * distance * (fov_y_rad * 0.5).tan() / vh;
    (GIZMO_AXIS_TARGET_PX * world_per_px) / GIZMO_MESH_AXIS_LENGTH
}

/// Escala en mundo para cámara ortográfica 2D.
pub fn world_scale_ortho_2d(half_h: f32, viewport_height_px: u32) -> f32 {
    let vh = viewport_height_px.max(1) as f32;
    let world_per_px = (2.0 * half_h) / vh;
    (GIZMO_AXIS_TARGET_PX * world_per_px) / GIZMO_MESH_AXIS_LENGTH
}

/// Limita la escala global y, si hay selección, evita que el gizmo sea mucho más grande que el objeto.
pub fn clamp_scale_for_selection(screen_scale: f32, selection_max_extent: Option<f32>) -> f32 {
    let mut scale = screen_scale.clamp(GIZMO_WORLD_SCALE_MIN, GIZMO_WORLD_SCALE_MAX);
    if let Some(extent) = selection_max_extent.filter(|e| e.is_finite() && *e > 0.0) {
        let cap = (extent * GIZMO_SELECTION_EXTENT_FACTOR / GIZMO_MESH_AXIS_LENGTH)
            .max(GIZMO_WORLD_SCALE_MIN);
        scale = scale.min(cap);
    }
    scale
}

/// Longitud en mundo de un eje del gizmo (para picking alineado al render).
pub fn axis_world_length(scale: f32) -> f32 {
    GIZMO_MESH_AXIS_LENGTH * scale
}

/// Separación en mundo entre el AABB de la selección y el inicio de una flecha.
pub fn axis_gap_world(
    camera_pos: glam::Vec3,
    anchor: glam::Vec3,
    fov_y_rad: f32,
    viewport_height_px: u32,
) -> f32 {
    let distance = (camera_pos - anchor).length().max(0.001);
    let vh = viewport_height_px.max(1) as f32;
    let world_per_px = 2.0 * distance * (fov_y_rad * 0.5).tan() / vh;
    GIZMO_AXIS_GAP_PX * world_per_px
}

/// Inicio del eje en unidades del mesh del gizmo (antes de escalar el modelo).
pub fn axis_start_mesh_units(half_extent_world: f32, gap_world: f32, gizmo_scale: f32) -> f32 {
    (half_extent_world.max(0.0) + gap_world.max(0.0)) / gizmo_scale.max(1e-6)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perspective_scale_grows_with_distance() {
        let near = world_scale_perspective(
            glam::Vec3::new(0.0, 0.0, 5.0),
            glam::Vec3::ZERO,
            45_f32.to_radians(),
            1080,
        );
        let far = world_scale_perspective(
            glam::Vec3::new(0.0, 0.0, 50.0),
            glam::Vec3::ZERO,
            45_f32.to_radians(),
            1080,
        );
        assert!(far > near);
    }

    #[test]
    fn tiny_selection_caps_scale() {
        let screen = 2.0;
        let capped = clamp_scale_for_selection(screen, Some(0.02));
        assert!(capped < screen);
        assert!(axis_world_length(capped) <= 0.02 * GIZMO_SELECTION_EXTENT_FACTOR + 1e-4);
    }

    #[test]
    fn axis_start_scales_with_gizmo() {
        let gap = axis_gap_world(
            glam::Vec3::new(0.0, 0.0, 5.0),
            glam::Vec3::ZERO,
            45_f32.to_radians(),
            1080,
        );
        let start = axis_start_mesh_units(0.5, gap, 2.0);
        assert!(start > 0.25);
    }
}
