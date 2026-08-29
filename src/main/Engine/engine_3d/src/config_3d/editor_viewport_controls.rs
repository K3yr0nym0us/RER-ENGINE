//! Controles del viewport de edición 3D (esquema por defecto de Blender).

use glam::Vec3;

use crate::ecs::Transform;
use crate::engine::State;

/// Índice virtual del mango central del gizmo (arrastre libre en plano de vista).
pub const GIZMO_CENTER_AXIS: usize = 3;
pub const GIZMO_CENTER_PICK_RADIUS_PX: f32 = 20.0;
pub const DRAG_PRECISION_FACTOR: f32 = 0.1;
pub const FRAME_AABB_MARGIN: f32 = 1.1;
/// Tope de movimiento por frame (evita saltos si falla la intersección rayo-plano).
pub const DRAG_MAX_DELTA_MIN: f32 = 0.25;
pub const DRAG_MAX_DELTA_MAX: f32 = 25.0;
pub const DRAG_MAX_DELTA_DISTANCE_FACTOR: f32 = 0.35;

pub const ROTATE_SNAP_DEG: f32 = 15.0;

const DOLLY_SENSITIVITY: f32 = 0.012;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorCameraNavMode {
    Orbit,
    Pan,
    Dolly,
}

/// Resuelve el modo de navegación de cámara según modificadores (MMB + drag).
pub fn resolve_editor_camera_nav_mode(shift_held: bool, ctrl_held: bool) -> EditorCameraNavMode {
    if ctrl_held {
        EditorCameraNavMode::Dolly
    } else if shift_held {
        EditorCameraNavMode::Pan
    } else {
        EditorCameraNavMode::Orbit
    }
}

/// Snap de posición mundial a la cuadrícula del mundo.
pub fn snap_vec3_to_grid(v: Vec3, cell: f32) -> Vec3 {
    if cell <= 1e-6 {
        return v;
    }
    Vec3::new(
        (v.x / cell).round() * cell,
        (v.y / cell).round() * cell,
        (v.z / cell).round() * cell,
    )
}

/// Distancia orbital para encuadrar un AABB con margen.
pub fn distance_to_frame_aabb(half_extents: Vec3, fov_y_rad: f32, aspect: f32, margin: f32) -> f32 {
    let radius = half_extents.length().max(1e-4) * margin;
    let tan_half_fov = (fov_y_rad * 0.5).tan().max(1e-4);
    let dist_v = radius / tan_half_fov;
    let dist_h = radius / (tan_half_fov * aspect.max(1e-4));
    dist_v.max(dist_h).clamp(0.5, 500.0)
}

pub(crate) fn ray_plane_intersect(
    ray_origin: Vec3,
    ray_dir: Vec3,
    plane_point: Vec3,
    plane_normal: Vec3,
    max_t: f32,
) -> Option<Vec3> {
    let denom = ray_dir.dot(plane_normal);
    if denom.abs() < 1e-6 {
        return None;
    }
    let t = (plane_point - ray_origin).dot(plane_normal) / denom;
    if t < 0.0 || t > max_t {
        return None;
    }
    Some(ray_origin + ray_dir * t)
}

/// Base ortonormal en el plano de rotación (estable para atan2).
pub(crate) fn rotation_plane_basis(axis: Vec3, view_forward: Vec3) -> (Vec3, Vec3) {
    let n = axis.normalize_or_zero();
    let mut u = view_forward.cross(n);
    if u.length_squared() < 1e-8 {
        u = if n.dot(Vec3::Y).abs() < 0.9 {
            Vec3::Y.cross(n)
        } else {
            Vec3::X.cross(n)
        };
    }
    u = u.normalize_or_zero();
    let v = n.cross(u);
    (u, v)
}

/// Vector en el plano de rotación bajo el cursor (desde el pivote).
pub(crate) fn rotation_plane_offset_from_ray(
    ray_origin: Vec3,
    ray_dir: Vec3,
    pivot: Vec3,
    axis: Vec3,
    plane_u: Vec3,
) -> Option<Vec3> {
    let n = axis.normalize_or_zero();
    if n.length_squared() < 1e-10 {
        return None;
    }

    let denom = ray_dir.dot(n);
    if denom.abs() > 1e-5 {
        let t = (pivot - ray_origin).dot(n) / denom;
        let offset = ray_origin + ray_dir * t - pivot;
        if offset.length_squared() >= 1e-10 {
            return Some(offset);
        }
    }

    let mut in_plane = ray_dir - n * ray_dir.dot(n);
    if in_plane.length_squared() < 1e-10 {
        return Some(plane_u);
    }
    in_plane = in_plane.normalize();
    Some(in_plane)
}

/// Ángulo de arrastre en un plano de rotación (misma lógica para X/Y/Z).
pub(crate) fn rotation_drag_angle(
    start_origin: Vec3,
    start_dir: Vec3,
    current_origin: Vec3,
    current_dir: Vec3,
    pivot: Vec3,
    axis: Vec3,
    plane_u: Vec3,
    plane_v: Vec3,
) -> Option<f32> {
    let off0 = rotation_plane_offset_from_ray(start_origin, start_dir, pivot, axis, plane_u)?;
    let off1 = rotation_plane_offset_from_ray(current_origin, current_dir, pivot, axis, plane_u)?;
    let a0 = off0.dot(plane_v).atan2(off0.dot(plane_u));
    let a1 = off1.dot(plane_v).atan2(off1.dot(plane_u));
    let mut delta = a1 - a0;
    while delta > std::f32::consts::PI {
        delta -= std::f32::consts::TAU;
    }
    while delta < -std::f32::consts::PI {
        delta += std::f32::consts::TAU;
    }
    Some(delta)
}

/// Limita el delta de un frame para evitar teletransportes.
pub fn clamp_drag_delta(delta: Vec3, max_len: f32) -> Vec3 {
    let max_len = max_len.max(1e-4);
    let len_sq = delta.length_squared();
    let max_sq = max_len * max_len;
    if len_sq <= max_sq {
        return delta;
    }
    delta * (max_len / len_sq.sqrt())
}

pub fn drag_max_delta_for_distance(view_distance: f32) -> f32 {
    (view_distance * DRAG_MAX_DELTA_DISTANCE_FACTOR).clamp(DRAG_MAX_DELTA_MIN, DRAG_MAX_DELTA_MAX)
}

/// Snap angular para Ctrl durante rotación en viewport.
pub fn snap_rotation_quat(quat: glam::Quat) -> glam::Quat {
    let (axis, angle) = quat.normalize().to_axis_angle();
    let step = ROTATE_SNAP_DEG.to_radians();
    let snapped = (angle / step).round() * step;
    glam::Quat::from_axis_angle(axis, snapped)
}

pub fn constrain_translation_delta(delta: Vec3, constraint_axis: Option<usize>) -> Vec3 {
    match constraint_axis {
        Some(0) => Vec3::X * delta.dot(Vec3::X),
        Some(1) => Vec3::Y * delta.dot(Vec3::Y),
        Some(2) => Vec3::Z * delta.dot(Vec3::Z),
        _ => delta,
    }
}

impl State {
    /// Dolly zoom (Ctrl + MMB): acerca/aleja a lo largo del eje de vista.
    pub(crate) fn dolly_editor_viewport(&mut self, dy: f32) {
        if !self.uses_editor_viewport_camera() {
            return;
        }
        let scale = self.editor_viewport_distance.max(0.5);
        self.zoom_editor_viewport(-dy * DOLLY_SENSITIVITY * scale);
    }

    /// Aplica navegación de cámara del editor según modo Blender.
    pub(crate) fn apply_editor_camera_nav(&mut self, mode: EditorCameraNavMode, dx: f32, dy: f32) {
        if !self.uses_editor_viewport_camera() {
            return;
        }
        match mode {
            EditorCameraNavMode::Orbit => self.orbit_editor_viewport(dx, dy),
            EditorCameraNavMode::Pan => self.pan_editor_viewport(dx, dy),
            EditorCameraNavMode::Dolly => self.dolly_editor_viewport(dy),
        }
    }

    /// Encuadra la selección en el viewport (Numpad `.` en Blender).
    pub(crate) fn frame_selected_in_viewport(&mut self) -> bool {
        if !self.uses_editor_viewport_camera() {
            return false;
        }

        let ids: Vec<u32> = if !self.selected_entities.is_empty() {
            self.selected_entities.clone()
        } else {
            self.selected_entity.into_iter().collect()
        };
        if ids.is_empty() {
            return false;
        }

        let mut bounds_min = Vec3::splat(f32::INFINITY);
        let mut bounds_max = Vec3::splat(f32::NEG_INFINITY);
        for id in ids {
            let Some(t) = self.world.get::<Transform>(id) else {
                continue;
            };
            let (center, half) = self.entity_world_pick_aabb(id, t);
            let mn = center - half;
            let mx = center + half;
            bounds_min = bounds_min.min(mn);
            bounds_max = bounds_max.max(mx);
        }

        if !bounds_min.is_finite() || !bounds_max.is_finite() {
            return false;
        }

        let center = (bounds_min + bounds_max) * 0.5;
        let half_extents = (bounds_max - bounds_min) * 0.5;
        let aspect = self.size.width as f32 / self.size.height.max(1) as f32;

        self.editor_orbit_target = center;
        self.editor_viewport_distance =
            distance_to_frame_aabb(half_extents, self.camera.fov_y, aspect, FRAME_AABB_MARGIN);
        self.ensure_editor_camera_entity();
        self.sync_editor_camera_entity_from_viewport();
        true
    }

    /// Punto en el plano de arrastre libre bajo el cursor.
    pub(crate) fn free_drag_world_point(
        &self,
        pixel_x: f32,
        pixel_y: f32,
        plane_point: Vec3,
        plane_normal: Vec3,
    ) -> Option<Vec3> {
        let (ray_origin, ray_dir) = self.viewport_ray(pixel_x, pixel_y)?;
        let cam_dist = (self.camera_world_position() - plane_point).length();
        let max_t = cam_dist.max(1.0) * 4.0;
        ray_plane_intersect(ray_origin, ray_dir, plane_point, plane_normal, max_t)
    }

    pub(crate) fn editor_viewport_drag_max_delta(&self) -> f32 {
        drag_max_delta_for_distance(self.editor_viewport_distance)
    }

    /// Restaura transforms desde snapshots (p. ej. cancelar rotación en viewport).
    pub(crate) fn restore_transform_snapshots(
        &mut self,
        snapshots: &[crate::engine::types::EntityTransformSnapshot],
    ) {
        for &(id, pos, rot, scl) in snapshots {
            if let Some(t) = self.world.get_mut::<crate::ecs::Transform>(id) {
                t.position = glam::Vec3::from_array(pos);
                t.rotation = glam::Quat::from_xyzw(rot[0], rot[1], rot[2], rot[3]).normalize();
                t.scale = glam::Vec3::from_array(scl);
            }
            if self.is_plane_wall_entity(id) && self.collider_entities.contains(&id) {
                self.sync_plane_wall_physics(id);
            } else if self.physics.has_physics(id) {
                self.sync_entity_physics_collider(id);
            }
            if self.sun_entity == Some(id) {
                self.sync_directional_light_from_sun();
            }
        }
    }

    /// Aplica traslación de viewport (tecla G) desde snapshots iniciales.
    pub(crate) fn apply_viewport_grab(
        &mut self,
        plane_point: Vec3,
        plane_normal: Vec3,
        start_snapshots: &[crate::engine::types::EntityTransformSnapshot],
        start_world: Vec3,
        current_mouse: (f32, f32),
        constraint_axis: Option<usize>,
        shift_held: bool,
        ctrl_held: bool,
    ) {
        let Some(hit) =
            self.free_drag_world_point(current_mouse.0, current_mouse.1, plane_point, plane_normal)
        else {
            return;
        };

        let mut delta = hit - start_world;
        delta = constrain_translation_delta(delta, constraint_axis);
        if shift_held {
            delta *= DRAG_PRECISION_FACTOR;
        }
        delta = clamp_drag_delta(delta, self.editor_viewport_drag_max_delta());
        self.apply_selection_translation_from_snapshots(start_snapshots, delta, ctrl_held);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nav_mode_blender_defaults() {
        assert_eq!(
            resolve_editor_camera_nav_mode(false, false),
            EditorCameraNavMode::Orbit
        );
        assert_eq!(
            resolve_editor_camera_nav_mode(true, false),
            EditorCameraNavMode::Pan
        );
        assert_eq!(
            resolve_editor_camera_nav_mode(false, true),
            EditorCameraNavMode::Dolly
        );
    }

    #[test]
    fn frame_distance_grows_with_aabb() {
        let small = distance_to_frame_aabb(Vec3::splat(0.5), 45_f32.to_radians(), 16.0 / 9.0, 1.1);
        let large = distance_to_frame_aabb(Vec3::splat(5.0), 45_f32.to_radians(), 16.0 / 9.0, 1.1);
        assert!(large > small);
        assert!(small >= 0.5);
    }

    #[test]
    fn snap_vec3_rounds_to_cell() {
        let snapped = snap_vec3_to_grid(Vec3::new(1.04, 2.49, -0.01), 1.0);
        assert!((snapped.x - 1.0).abs() < 1e-5);
        assert!((snapped.y - 2.0).abs() < 1e-5);
        assert!((snapped.z - 0.0).abs() < 1e-5);
    }

    #[test]
    fn clamp_drag_delta_caps_large_jumps() {
        let delta = Vec3::new(100.0, 0.0, 0.0);
        let capped = clamp_drag_delta(delta, 2.0);
        assert!((capped.length() - 2.0).abs() < 1e-4);
    }

    #[test]
    fn translation_constraint_zeros_other_axes() {
        let delta = Vec3::new(1.0, 2.0, 3.0);
        let x_only = constrain_translation_delta(delta, Some(0));
        assert!((x_only.x - 1.0).abs() < 1e-6);
        assert!(x_only.y.abs() < 1e-6);
        assert!(x_only.z.abs() < 1e-6);
    }

    #[test]
    fn rotation_plane_offset_parallel_ray_lies_in_yz_plane() {
        let pivot = Vec3::ZERO;
        let axis = Vec3::X;
        let (u, v) = rotation_plane_basis(axis, Vec3::new(0.0, 0.0, -1.0));
        let origin = Vec3::new(0.0, 0.0, 5.0);
        let dir = Vec3::new(0.1, -0.2, -1.0).normalize();
        let off = rotation_plane_offset_from_ray(origin, dir, pivot, axis, u).unwrap();
        assert!(off.x.abs() < 1e-5);
        assert!(off.length() > 0.99);
        let _ = off.dot(v).atan2(off.dot(u));
    }

    #[test]
    fn rotation_drag_angle_pure_spin_keeps_zero_translation_delta() {
        let pivot = Vec3::ZERO;
        let axis = Vec3::Y;
        let (u, v) = rotation_plane_basis(axis, Vec3::new(0.0, 0.0, -1.0));
        let ro = Vec3::new(0.0, 3.0, 8.0);
        let rd0 = (Vec3::new(1.0, -0.5, -1.0)).normalize();
        let rd1 = (Vec3::new(0.5, -0.5, -1.0)).normalize();
        let angle = rotation_drag_angle(ro, rd0, ro, rd1, pivot, axis, u, v).unwrap();
        assert!(angle.abs() > 1e-4);

        let pos = Vec3::new(2.0, 0.0, 1.0);
        let q = glam::Quat::from_axis_angle(axis, angle);
        let rotated = pivot + q * (pos - pivot);
        let dist_before = (pos - pivot).length();
        let dist_after = (rotated - pivot).length();
        assert!((dist_before - dist_after).abs() < 1e-4);
    }

    #[test]
    fn rotation_plane_offset_intersection_stays_in_plane() {
        let pivot = Vec3::ZERO;
        let axis = Vec3::Y;
        let (u, _) = rotation_plane_basis(axis, Vec3::X);
        let origin = Vec3::new(0.0, 2.0, 5.0);
        let dir = Vec3::new(0.0, -0.4, -1.0).normalize();
        let off = rotation_plane_offset_from_ray(origin, dir, pivot, axis, u).unwrap();
        assert!(off.y.abs() < 1e-5);
    }

    #[test]
    fn rotate_about_pivot_preserves_mesh_visual_center() {
        use crate::config_3d::{
            physics_body_world_center, physics_half_extents_for_model,
            rotate_entity_transform_about_visual_center, transform_position_for_visual_center,
        };
        use crate::ecs::Transform;

        let bounds = ([-0.5, 0.0, -0.5], [0.5, 2.0, 0.5]);
        let scale = Vec3::ONE;
        let start_rot = glam::Quat::IDENTITY;
        let path = "character.glb";
        let half = physics_half_extents_for_model(scale.to_array(), Some(bounds));
        let pivot = Vec3::new(0.0, 1.0, 0.0);
        let start_pos =
            transform_position_for_visual_center(pivot, start_rot, scale, path, Some(bounds));
        let start_transform = Transform {
            position: start_pos,
            rotation: start_rot,
            scale,
        };
        let start_visual = Vec3::from_array(physics_body_world_center(
            &start_transform,
            Some(bounds),
            path,
            half,
        ));
        assert!((start_visual - pivot).length() < 1e-4);

        let delta = glam::Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
        let (new_pos, new_rot) = rotate_entity_transform_about_visual_center(
            &start_transform,
            start_visual,
            pivot,
            delta,
            path,
            Some(bounds),
        );
        let new_transform = Transform {
            position: new_pos,
            rotation: new_rot,
            scale,
        };
        let new_visual = Vec3::from_array(physics_body_world_center(
            &new_transform,
            Some(bounds),
            path,
            half,
        ));
        assert!((new_visual - pivot).length() < 1e-4);
        assert!((start_pos - new_pos).length() > 1e-4);
    }
}
