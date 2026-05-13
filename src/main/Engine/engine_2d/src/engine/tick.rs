use std::time::Instant;

use crate::ecs::Transform;
use crate::ipc::{send_event, EngineEvent};

use super::types::AUTOSAVE_INTERVAL;
use super::State;

impl State {
    pub fn update(&mut self) {
        let now         = Instant::now();
        self.delta_time = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;
        self.update_snap_hint_alpha();

        // Emitir métricas de debug ~1 vez por segundo.
        self.metrics_frame_count += 1;
        if now.duration_since(self.metrics_last_emit) >= std::time::Duration::from_secs(1) {
            let elapsed_secs = now.duration_since(self.metrics_last_emit).as_secs_f32();
            let fps = self.metrics_frame_count as f32 / elapsed_secs;
            let physics_bodies = if self.camera_2d.is_some() {
                self.physics_2d.body_count()
            } else {
                self.physics.body_count()
            };
            send_event(&EngineEvent::DebugMetrics {
                fps,
                frame_time_ms:  self.delta_time * 1000.0,
                draw_calls:     self.last_draw_calls,
                physics_bodies,
            });
            self.metrics_last_emit   = now;
            self.metrics_frame_count = 0;
        }
        if self.autosave_enabled && now.duration_since(self.autosave_last_tick) >= AUTOSAVE_INTERVAL {
            send_event(&EngineEvent::AutosaveTick);
            self.autosave_last_tick = now;
        }
        if self.camera_2d.is_some() {
            // Scripts corren siempre (editor + juego) para facilitar pruebas rápidas.
            self.update_scripts();
            if self.preview_playing {
                // Los deslizamientos pendientes (on_press slide) se avanzan antes del paso
                // de física para que la velocidad esté lista cuando Rapier integra el cuerpo.
                self.advance_pending_slides();
                // En modo juego sí aplicamos físicas completas.
                self.physics_2d.step(self.delta_time, &mut self.world);
                // Sincronizar anim_saved_transforms con la posición post-physics (ya bloqueada
                // por colisiones) para que update_animations() no restaure la posición original.
                self.sync_physics_anim_origins();
                self.update_execution_areas_2d();
            }
        } else {
            self.update_scripts();
            if self.preview_playing {
                self.physics.step(self.delta_time, &mut self.world);
            }
        }
    }

    /// Sincroniza anim_saved_transforms desde la posición actual del Transform
    /// para entidades que tienen física activa y están en medio de una animación.
    /// Necesario para que move_physics_entity funcione con animaciones de pivot.
    fn sync_physics_anim_origins(&mut self) {
        let ids: Vec<u32> = self.anim_saved_transforms.keys().copied().collect();
        for id in ids {
            if self.physics_2d.has_physics(id) && self.physics_2d.get_body_type(id) != "static" {
                if let Some(t) = self.world.get::<Transform>(id) {
                    let (px, py) = (t.position.x, t.position.y);
                    if let Some(saved) = self.anim_saved_transforms.get_mut(&id) {
                        saved.0.x = px;
                        saved.0.y = py;
                    }
                }
            }
        }
    }

    /// Avanza los deslizamientos pendientes (`engine.move_entity_slide`) frame a frame.
    /// Debe llamarse ANTES del paso de física para que la velocidad esté lista cuando
    /// Rapier integra el cuerpo. Respeta colisiones usando el shape-cast kinematic.
    fn advance_pending_slides(&mut self) {
        let dt = self.delta_time;
        let ids: Vec<u32> = self.pending_slides.keys().copied().collect();
        for id in ids {
            let Some(slide) = self.pending_slides.get(&id).copied() else { continue; };
            let (cx, cy) = if let Some(t) = self.world.get::<Transform>(id) {
                (t.position.x, t.position.y)
            } else {
                self.pending_slides.remove(&id);
                continue;
            };
            let delta_x = slide.target_x - cx;
            let delta_y = if slide.keep_current_y {
                0.0
            } else {
                slide.target_y - cy
            };
            let dist = (delta_x * delta_x + delta_y * delta_y).sqrt();
            if dist <= 0.02 {
                self.pending_slides.remove(&id);
                continue;
            }
            // Limitar velocidad para no sobrepasar el destino en el último frame.
            let max_speed = dist / dt.max(0.001);
            let effective_speed = slide.speed.min(max_speed);
            let dir_x = delta_x / dist;
            let dir_y = delta_y / dist;
            let moved = self.physics_2d.move_physics_entity(id, effective_speed, dir_x, dir_y, dt);
            if !moved {
                // Fallback: traslación directa para entidades sin física activa.
                let step_x = dir_x * effective_speed * dt;
                let step_y = dir_y * effective_speed * dt;
                if let Some(t) = self.world.get_mut::<Transform>(id) {
                    t.position.x += step_x;
                    t.position.y += step_y;
                }
            }
        }
    }
}
