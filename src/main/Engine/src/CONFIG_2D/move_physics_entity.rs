// ── Movimiento físico de entidades 2D ────────────────────────────────────────
//
// Mueve una entidad 2D con física activa aplicando velocidad lineal al Rapier
// body. Diseño:
//
//   1. SHAPE CAST (proactivo): se proyecta el desplazamiento completo del frame
//      (speed * dt) como vector de movimiento. El toi devuelto es una fracción
//      [0..1] de ese vector: toi=0.5 significa "choca a mitad de camino".
//      Esto elimina la conversión frágil toi/dt de la versión anterior.
//
//   2. stop_at_penetration: false — ignora penetraciones YA existentes (p.ej.
//      el personaje apoyado en el suelo). Solo detecta nuevos impactos hacia
//      adelante, lo que permite moverse estando en contacto con una superficie.
//
//   3. target_distance: 0.01 — margen de piel. El cast se detiene 0.01 u antes
//      de la superficie para evitar floating point issues en el borde exacto.
//
//   4. SLIDING: si toi ≤ ε (ya pegado a la pared), proyecta la velocidad sobre
//      el plano perpendicular a la normal del obstáculo (normal2), igual que
//      Godot move_and_collide. El personaje desliza en lugar de bloquearse.
//
//   5. CCD (seguro de red): Rapier garantiza no-traversal aunque el shape cast
//      subestime la distancia.
//
// Parámetros:
//   entity       — EntityId del cuerpo a mover.
//   speed        — Velocidad en unidades de mundo por segundo.
//   dir_x / dir_y — Vector dirección (se normaliza internamente).
//   dt           — Delta time del frame actual.
//
// Retorna true si la entidad tiene cuerpo físico activo.

use rapier3d::prelude::*;
use parry3d::query::ShapeCastOptions;

use super::PhysicsWorld2D;
use crate::ecs::EntityId;

impl PhysicsWorld2D {
    pub(crate) fn move_physics_entity(
        &mut self,
        entity: EntityId,
        speed:  f32,
        dir_x:  f32,
        dir_y:  f32,
        dt:     f32,
    ) -> bool {
        let Some(&body_handle) = self.entity_bodies.get(&entity) else {
            return false;
        };

        // ── Dirección nula: detener componente horizontal, conservar vertical ─
        let len = (dir_x * dir_x + dir_y * dir_y).sqrt();
        if len <= 1e-6 {
            if let Some(body) = self.bodies.get_mut(body_handle) {
                let vy = body.linvel().y;
                body.set_linvel(vector![0.0, vy, 0.0], true);
            }
            return true;
        }
        let (nx, ny) = (dir_x / len, dir_y / len);
        let controls_y = ny.abs() > 1e-6;

        // ── Leer estado actual del body (inmutable) ───────────────────────────
        let current_vy = self.bodies.get(body_handle).map(|b| b.linvel().y).unwrap_or(0.0);
        let shape_pos  = match self.bodies.get(body_handle) {
            Some(b) => *b.position(),
            None    => return false,
        };
        let col_handle = match self.entity_colliders.get(&entity).copied() {
            Some(h) => h,
            None    => {
                // Sin colisionador: aplicar velocidad directamente
                if let Some(body) = self.bodies.get_mut(body_handle) {
                    let vy = if controls_y { ny * speed } else { current_vy };
                    body.set_linvel(vector![nx * speed, vy, 0.0], true);
                }
                return true;
            }
        };

        // ── Shape cast con desplazamiento real del frame ──────────────────────
        //
        // shape_vel = desplazamiento completo que el personaje haría en este dt.
        // Con max_toi = 1.0, el toi devuelto es una fracción [0..1]:
        //   toi = 0.0  →  ya en contacto (o penetración)
        //   toi = 0.5  →  choca a mitad del desplazamiento
        //   toi = 1.0  →  choca justo al final
        // Esto hace que effective_speed = toi * speed sea directamente correcto
        // sin ninguna conversión extra.
        let dt_safe = dt.max(1e-4);
        let shape_vel = vector![nx * speed * dt_safe, ny * speed * dt_safe, 0.0];

        let filter = QueryFilter::default().exclude_collider(col_handle);

        let hit = if let Some(col) = self.colliders.get(col_handle) {
            self.query_pipeline.cast_shape(
                &self.bodies,
                &self.colliders,
                &shape_pos,
                &shape_vel,
                col.shape(),
                ShapeCastOptions {
                    // Fracción máxima del desplazamiento a buscar (1.0 = todo el frame)
                    max_time_of_impact:                    1.0,
                    // Margen de piel: no llegamos exactamente a la superficie
                    target_distance:                       0.01,
                    // false = ignorar penetraciones existentes (cuerpo apoyado en suelo).
                    // Permite moverse mientras se está en contacto con una superficie.
                    stop_at_penetration:                   false,
                    compute_impact_geometry_on_penetration: false,
                },
                filter,
            )
        } else {
            None
        };

        let (final_vx, final_vy) = match hit {
            None => {
                // Camino libre — velocidad completa
                let vy = if controls_y { ny * speed } else { current_vy };
                (nx * speed, vy)
            }
            Some((_, hit_data)) => {
                let toi = hit_data.time_of_impact;

                if toi <= 1e-3 {
                    // Ya pegado a la superficie — deslizar sobre la normal del obstáculo.
                    // normal2 = normal exterior del obstáculo, apunta hacia el personaje.
                    // slide(v, n) = v - (v·n)*n  →  componente perpendicular a la pared.
                    let wall_normal = hit_data.normal2.into_inner();
                    let vx_req = nx * speed;
                    let vy_req = if controls_y { ny * speed } else { 0.0 };
                    let vel    = vector![vx_req, vy_req, 0.0f32];
                    let dot    = vel.dot(&wall_normal);
                    let slide  = vel - dot * wall_normal;

                    log::debug!(
                        "[move_entity] entidad {} deslizando en pared \
                         (normal=({:.2},{:.2}), slide=({:.2},{:.2}))",
                        entity, wall_normal.x, wall_normal.y, slide.x, slide.y
                    );

                    let vy = if controls_y { slide.y } else { current_vy };
                    (slide.x, vy)
                } else {
                    // Obstáculo a toi fracción del frame — avanzar hasta el borde.
                    // effective_speed = toi * speed (sin divisiones frágiles por dt).
                    let safe_speed = toi * speed;
                    log::debug!(
                        "[move_entity] entidad {} vel limitada {:.2} → {:.2} (toi={toi:.3})",
                        entity, speed, safe_speed
                    );
                    let vy = if controls_y { ny * safe_speed } else { current_vy };
                    (nx * safe_speed, vy)
                }
            }
        };

        if let Some(body) = self.bodies.get_mut(body_handle) {
            body.set_linvel(vector![final_vx, final_vy, 0.0], true);
        }
        true
    }
}

