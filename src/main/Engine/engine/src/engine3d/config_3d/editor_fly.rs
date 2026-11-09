//! Navegación libre del viewport de edición: botón derecho para mirar, WASD para volar.
//!
//! La cámara del editor es ORBITAL (`editor_orbit_target` + `editor_viewport_yaw/pitch/distance`),
//! así que "mirar como en Minecraft" no es solo cambiar yaw/pitch: hay que girar manteniendo el
//! OJO fijo y recalcular el pivote. En el rig orbital el ojo está en
//! `pivote + back(yaw, pitch) * distancia`, con `back = -forward`, por lo que tras cambiar los
//! ángulos el pivote debe quedar en `ojo - back * distancia`.
//!
//! Se conserva el esquema Blender que ya existía (MMB órbita, Shift+MMB paneo, Ctrl+MMB dolly,
//! rueda zoom): esto se suma, no lo reemplaza.

use std::collections::HashSet;

use glam::Vec3;
use winit::keyboard::KeyCode;

use crate::engine3d::engine::State;

/// Radián por píxel. Igual que `orbit_editor_viewport` para que la sensibilidad no cambie de un
/// gesto a otro.
pub const FLY_LOOK_SENSITIVITY: f32 = 0.005;
/// Velocidad (m/s) en la distancia orbital de referencia.
pub const FLY_BASE_SPEED: f32 = 7.0;
pub const FLY_SPEED_REFERENCE: f32 = 8.0;
/// Teclas de vuelo se aceleran con Shift (sprint), como en un editor típico.
pub const FLY_SPRINT_FACTOR: f32 = 3.0;
/// Tope de cabeceo: deja 0.05 rad para que el `look_at` nunca quede degenerado.
pub const FLY_PITCH_LIMIT: f32 = std::f32::consts::FRAC_PI_2 - 0.05;
/// Rango útil de velocidad: cerca se navega fino, lejos rápido, sin volverse inutilizable.
pub const FLY_SPEED_MIN: f32 = 1.5;
pub const FLY_SPEED_MAX: f32 = 60.0;

/// Dirección de visionado para ángulos orbitales del editor.
///
/// Es el espejo de `Camera::view_forward`, que usa el yaw/pitch de la cámara DE JUEGO; el viewport
/// de edición tiene sus propios ángulos, así que no se puede reutilizar ese método.
pub fn orbit_forward(yaw: f32, pitch: f32) -> Vec3 {
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    Vec3::new(-cy * cp, -sp, -sy * cp).normalize_or_zero()
}

/// Vector "hacia atrás" del rig orbital: `ojo = pivote + back * distancia`.
pub fn orbit_back(yaw: f32, pitch: f32) -> Vec3 {
    -orbit_forward(yaw, pitch)
}

/// Suelo de la cámara: forward proyectado al plano XZ, con fallback para la vista cenital.
pub fn orbit_flat_forward(yaw: f32, pitch: f32) -> Vec3 {
    let fwd = orbit_forward(yaw, pitch);
    let mut flat = Vec3::new(fwd.x, 0.0, fwd.z);
    if flat.length_squared() < 1e-8 {
        // Mirando recto arriba/abajo: se usa el forward del plano XY para no quedarse sin eje.
        flat = Vec3::new(-yaw.cos(), 0.0, -yaw.sin());
    }
    flat.normalize_or_zero()
}

/// Derecha de la cámara (`forward × up`), la misma convención que `pan_offset_with_distance`.
pub fn orbit_right(yaw: f32, pitch: f32) -> Vec3 {
    let flat = orbit_flat_forward(yaw, pitch);
    let right = flat.cross(Vec3::Y);
    right.normalize_or_zero()
}

/// Velocidad de vuelo según la distancia orbital actual.
pub fn fly_speed_for_distance(distance: f32) -> f32 {
    let distance = distance.max(0.5);
    (FLY_BASE_SPEED * (distance / FLY_SPEED_REFERENCE)).clamp(FLY_SPEED_MIN, FLY_SPEED_MAX)
}

/// Ejes de vuelo pedidos por teclado: `(adelante, derecha, arriba, sprint)`, cada eje en `-1..=1`.
///
/// `Space` sube y `C` baja. No se usan Q/E porque la herramienta de plano los reserva para rotar.
pub fn fly_axes_from_keys(pressed: &HashSet<KeyCode>) -> (f32, f32, f32, bool) {
    let down = |code: KeyCode| pressed.contains(&code) as i8;
    let forward = down(KeyCode::KeyW) - down(KeyCode::KeyS);
    let right = down(KeyCode::KeyD) - down(KeyCode::KeyA);
    let up = down(KeyCode::Space) - down(KeyCode::KeyC);
    let sprint = down(KeyCode::ShiftLeft) + down(KeyCode::ShiftRight) > 0;
    (forward as f32, right as f32, up as f32, sprint)
}

impl State {
    /// RMB + arrastre: gira la vista alrededor del ojo actual (estilo FPS).
    pub(crate) fn editor_fly_look(&mut self, dx: f32, dy: f32) {
        if !self.editor_fly_active() {
            return;
        }
        let anchor = self.orbit_view_anchor();
        let (yaw, pitch, distance) = self.viewport_orbit_angles();
        let eye = self.camera.position_at_angles(anchor, yaw, pitch, distance);
        let yaw = yaw + dx * FLY_LOOK_SENSITIVITY;
        let pitch = (pitch - dy * FLY_LOOK_SENSITIVITY).clamp(-FLY_PITCH_LIMIT, FLY_PITCH_LIMIT);
        // El pivote se recalcula para que el ojo NO se desplace al mirar.
        let pivot = eye - orbit_back(yaw, pitch) * distance;
        self.editor_viewport_yaw = yaw;
        self.editor_viewport_pitch = pitch;
        self.editor_orbit_target = pivot
            - self.camera.orbit_pivot_offset
            - Vec3::Y * self.camera.eye_height_offset
            - self.camera.eye_offset_local;
        self.sync_editor_camera_entity_from_viewport();
    }

    /// Traslada el pivote orbital (y por tanto la cámara) en el espacio de la vista.
    pub(crate) fn editor_fly_translate(&mut self, forward: f32, right: f32, up: f32, dt: f32) {
        if !self.uses_editor_viewport_camera() {
            return;
        }
        if forward == 0.0 && right == 0.0 && up == 0.0 {
            return;
        }
        let (yaw, pitch, distance) = self.viewport_orbit_angles();
        let step = orbit_flat_forward(yaw, pitch) * forward
            + orbit_right(yaw, pitch) * right
            + Vec3::Y * up;
        if step.length_squared() < 1e-8 {
            return;
        }
        let speed = fly_speed_for_distance(distance);
        self.editor_orbit_target += step.normalize() * speed * dt.max(0.0);
        self.sync_editor_camera_entity_from_viewport();
    }

    /// Aplica el vuelo del viewport desde el estado de teclado de la ventana.
    pub(crate) fn apply_editor_fly_input(&mut self, pressed: &HashSet<KeyCode>, dt: f32) {
        let (forward, right, up, sprint) = fly_axes_from_keys(pressed);
        if forward == 0.0 && right == 0.0 && up == 0.0 {
            return;
        }
        let factor = if sprint { FLY_SPRINT_FACTOR } else { 1.0 };
        self.editor_fly_translate(forward, right, up, dt * factor);
    }

    /// El vuelo del viewport (mirar y trasladarse) solo aplica en edición pura: durante la edición
    /// de HUD el viewport muestra el ojo del jugador, así que mover la órbita no se vería.
    pub(crate) fn editor_fly_active(&self) -> bool {
        self.uses_editor_viewport_camera() && !self.player_ui_edit_active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(list: &[KeyCode]) -> HashSet<KeyCode> {
        list.iter().copied().collect()
    }

    #[test]
    fn forward_at_zero_angles_looks_down_minus_x() {
        let fwd = orbit_forward(0.0, 0.0);
        assert!((fwd.x + 1.0).abs() < 1e-5);
        assert!(fwd.y.abs() < 1e-5);
        assert!(fwd.z.abs() < 1e-5);
    }

    #[test]
    fn positive_pitch_looks_downhill() {
        // `position_at_angles` pone el ojo en `pivote + back*dist`; con pitch>0 el ojo sube, así
        // que la vista (=-back) tiene que bajar.
        assert!(orbit_forward(0.0, 0.6).y < 0.0);
        assert!(orbit_forward(0.0, -0.6).y > 0.0);
    }

    #[test]
    fn back_is_the_opposite_of_forward() {
        let (yaw, pitch) = (0.7, -0.3);
        assert!((orbit_back(yaw, pitch) + orbit_forward(yaw, pitch)).length() < 1e-6);
    }

    #[test]
    fn flat_forward_ignores_pitch_and_stays_unitary() {
        for pitch in [-1.4, -0.2, 0.0, 0.9] {
            let flat = orbit_flat_forward(0.3, pitch);
            assert!(flat.y.abs() < 1e-6);
            assert!((flat.length() - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn right_is_perpendicular_to_flat_forward() {
        let flat = orbit_flat_forward(1.1, 0.2);
        let right = orbit_right(1.1, 0.2);
        assert!(flat.dot(right).abs() < 1e-5);
        assert!((right.length() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn speed_grows_with_distance_and_stays_bounded() {
        assert!(fly_speed_for_distance(40.0) > fly_speed_for_distance(4.0));
        assert!(fly_speed_for_distance(0.1) >= FLY_SPEED_MIN - 1e-5);
        assert!(fly_speed_for_distance(9999.0) <= FLY_SPEED_MAX + 1e-5);
    }

    #[test]
    fn opposed_keys_cancel_out() {
        let (f, r, u, sprint) = fly_axes_from_keys(&keys(&[KeyCode::KeyW, KeyCode::KeyS]));
        assert_eq!((f, r, u), (0.0, 0.0, 0.0));
        assert!(!sprint);
    }

    #[test]
    fn wasd_and_space_map_to_expected_axes() {
        let (f, r, u, _) = fly_axes_from_keys(&keys(&[KeyCode::KeyW, KeyCode::KeyD]));
        assert_eq!((f, r, u), (1.0, 1.0, 0.0));
        let (f, _, u, _) = fly_axes_from_keys(&keys(&[KeyCode::KeyS, KeyCode::Space]));
        assert_eq!((f, u), (-1.0, 1.0));
        let (f, _, u, _) = fly_axes_from_keys(&keys(&[KeyCode::KeyC]));
        assert_eq!((f, u), (0.0, -1.0));
    }

    #[test]
    fn either_shift_enables_sprint() {
        assert!(fly_axes_from_keys(&keys(&[KeyCode::ShiftLeft])).3);
        assert!(fly_axes_from_keys(&keys(&[KeyCode::ShiftRight])).3);
    }

    #[test]
    fn pitch_limit_avoids_degenerate_look_at() {
        // Un pitch de ±90° haría `look_at` degenerado (forward paralelo a up).
        let limit = FLY_PITCH_LIMIT;
        let fwd = orbit_forward(0.0, limit);
        let flat = Vec3::new(fwd.x, 0.0, fwd.z);
        assert!(flat.length_squared() > 1e-4);
    }
}
