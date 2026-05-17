use std::collections::HashMap;

use crate::ecs::Transform;
use crate::ipc::EngineCommand;
use crate::scripting::{EntitySnapshot, ScriptCmd};

use super::types::PendingSlide;
use super::State;

impl State {
    pub(crate) fn update_entity_facing_from_horizontal(&mut self, entity_id: u32, horizontal: f32) {
        const EPS: f32 = 0.0001;
        if horizontal.abs() <= EPS {
            return;
        }
        self.entity_facing_right.insert(entity_id, horizontal > 0.0);
    }

    /// Intenta inferir intención horizontal pura desde el input bruto.
    /// Se usa para pre-filtrar scripts de movimiento cuando el actor ya está
    /// bloqueado por colisión en esa dirección, evitando ejecutar Lua de más.
    pub(crate) fn infer_horizontal_input_dir(&self, device: &str, control_key: &str) -> Option<f32> {
        match device {
            "keyboard_mouse" => match control_key {
                "A" => Some(-1.0),
                "D" => Some(1.0),
                _ => None,
            },
            "gamepad" => match control_key {
                "D-LEFT" => Some(-1.0),
                "D-RIGHT" => Some(1.0),
                _ => None,
            },
            _ => None,
        }
    }

    pub(crate) fn clear_on_keep_horizontal_block_for_input(&mut self, device: &str, control_key: &str) {
        let Some(dir_x) = self.infer_horizontal_input_dir(device, control_key) else {
            return;
        };
        const EPS: f32 = 1e-6;
        self.blocked_on_keep_horizontal
            .retain(|_, sign| *sign * dir_x <= EPS);
    }

    pub(crate) fn clear_all_on_keep_horizontal_blocks(&mut self) {
        self.blocked_on_keep_horizontal.clear();
    }

    pub(crate) fn should_block_on_keep_script(
        &mut self,
        entity_id: u32,
        device: &str,
        control_key: &str,
    ) -> bool {
        let Some(dir_x) = self.infer_horizontal_input_dir(device, control_key) else {
            return false;
        };
        const EPS: f32 = 1e-6;

        if let Some(sign) = self.blocked_on_keep_horizontal.get(&entity_id).copied() {
            if sign * dir_x < -EPS {
                self.blocked_on_keep_horizontal.remove(&entity_id);
            } else if sign * dir_x > EPS {
                self.handle_command(EngineCommand::StopAnimation { id: entity_id });
                return true;
            }
        }

        if self.physics_2d.has_physics(entity_id) && self.physics_2d.is_horizontal_blocked(entity_id, dir_x) {
            self.blocked_on_keep_horizontal.insert(entity_id, dir_x.signum());
            self.handle_command(EngineCommand::StopAnimation { id: entity_id });
            return true;
        }

        false
    }

    pub(super) fn update_scripts(&mut self) {
        if !self.preview_playing {
            return;
        }

        // Build snapshots for entities that have scripts attached
        let snapshots: HashMap<u32, EntitySnapshot> = {
            let entity_ids: Vec<u32> = self.script_engine.entity_ids().to_vec();
            let mut map = HashMap::new();
            for id in entity_ids {
                let (x, y, scale_x, scale_y) = if let Some(t) = self.world.get::<Transform>(id) {
                    (t.position.x, t.position.y, t.scale.x, t.scale.y)
                } else {
                    (0.0, 0.0, 1.0, 1.0)
                };
                let facing_right = self.entity_facing_right.get(&id).copied().unwrap_or(true);
                let facing_sign = if facing_right { 1.0 } else { -1.0 };
                let animations: Vec<String> = self.animations
                    .get(&id)
                    .map(|m| m.keys().cloned().collect())
                    .unwrap_or_default();
                map.insert(id, EntitySnapshot { id, x, y, scale_x, scale_y, facing_right, facing_sign, animations });
            }
            map
        };

        let commands = self.script_engine.tick(self.delta_time, &snapshots);
        self.apply_script_commands(commands);
    }

    pub(super) fn execute_control_script(&mut self, id: u32, control_key: &str, path: &str, source: &str) {
        if !self.preview_playing {
            return;
        }

        let snapshot = self.build_script_snapshot(id);
        match self.script_engine.run_control_script(id, control_key, path, source, snapshot.as_ref()) {
            Ok(commands) => self.apply_script_commands(commands),
            Err(e) => log::error!("[control] Error ejecutando script '{}' ({}): {}", path, control_key, e),
        }
    }

    pub(crate) fn build_script_snapshot(&self, id: u32) -> Option<EntitySnapshot> {
        let (x, y, scale_x, scale_y) = if let Some(t) = self.world.get::<Transform>(id) {
            (t.position.x, t.position.y, t.scale.x, t.scale.y)
        } else {
            (0.0, 0.0, 1.0, 1.0)
        };
        let facing_right = self.entity_facing_right.get(&id).copied().unwrap_or(true);
        let facing_sign = if facing_right { 1.0 } else { -1.0 };

        let animations: Vec<String> = self.animations
            .get(&id)
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();

        Some(EntitySnapshot { id, x, y, scale_x, scale_y, facing_right, facing_sign, animations })
    }

    /// Aplica los comandos generados por los scripts al estado del motor.
    pub(crate) fn apply_script_commands(&mut self, commands: Vec<ScriptCmd>) {
        for cmd in commands {
            match cmd {
                ScriptCmd::SetPosition { id, x, y } => {
                    let current_pos = self.world.get::<Transform>(id)
                        .map(|t| (t.position.x, t.position.y));
                    let horizontal = current_pos
                        .map(|(cx, _)| x - cx)
                        .unwrap_or(0.0);
                    self.update_entity_facing_from_horizontal(id, horizontal);

                    // En modo juego con física activa, SetPosition debe respetar colisiones
                    // (estilo move_and_slide), no teletransportar atravesando obstáculos.
                    let uses_physics_move = self.preview_playing && self.physics_2d.has_physics(id);
                    if uses_physics_move {
                        if let Some((cx, cy)) = current_pos {
                            let dx = x - cx;
                            let dy = y - cy;
                            let dist = (dx * dx + dy * dy).sqrt();
                            if dist > 1e-6 {
                                let dt_safe = self.delta_time.max(1e-4);
                                let speed = dist / dt_safe;
                                let _ = self.physics_2d.move_physics_entity(id, speed, dx / dist, dy / dist, dt_safe);
                            }
                        }
                    } else {
                        // Editor o entidad sin física: mantener comportamiento anterior.
                        if let Some(t) = self.world.get_mut::<Transform>(id) {
                            t.position.x = x;
                            t.position.y = y;
                        }
                        // Sincronizar el origen de animación para que play_animation_frame
                        // no sobreescriba la posición con el valor pre-movimiento.
                        if let Some(saved) = self.anim_saved_transforms.get_mut(&id) {
                            saved.0.x = x;
                            saved.0.y = y;
                        }
                        // El script ya mutó el Transform fuera de la ruta de gameplay;
                        // aqui solo alineamos el body 2D para preservar el contrato actual.
                        self.sync_physics_2d_body_from_xy(id, x, y);
                    }
                }
                ScriptCmd::Translate { id, dx, dy } => {
                    self.update_entity_facing_from_horizontal(id, dx);
                    // En modo juego con física activa: aplicar translate vía movimiento físico
                    // para que respete colisiones y no atraviese colliders.
                    let uses_physics_move = self.preview_playing && self.physics_2d.has_physics(id);
                    if uses_physics_move {
                        let dist = (dx * dx + dy * dy).sqrt();
                        if dist > 1e-6 {
                            let dt_safe = self.delta_time.max(1e-4);
                            let speed = dist / dt_safe;
                            let _ = self.physics_2d.move_physics_entity(id, speed, dx / dist, dy / dist, dt_safe);
                        }
                    } else {
                        if let Some(t) = self.world.get_mut::<Transform>(id) {
                            t.position.x += dx;
                            t.position.y += dy;
                        }
                        // Propagar el desplazamiento al origen guardado de animación,
                        // de lo contrario cada frame de animación resetea la posición a orig_pos.
                        if let Some(saved) = self.anim_saved_transforms.get_mut(&id) {
                            saved.0.x += dx;
                            saved.0.y += dy;
                            log::debug!("[script/translate] entidad {} saved_x={:.3} (+{:.3})", id, saved.0.x, dx);
                        } else {
                            log::warn!("[script/translate] entidad {} SIN entrada en anim_saved_transforms — translate no acumulado", id);
                        }
                        // Mantener el contrato actual editor/script -> body fisico.
                        // Esta ruta existe para compatibilidad; el movimiento normal
                        // de gameplay debe seguir pasando por `move_physics_entity()`.
                        self.sync_physics_2d_body_from_transform(id);
                    }
                }
                ScriptCmd::SetScale { id, sx, sy } => {
                    if let Some(t) = self.world.get_mut::<Transform>(id) {
                        // Mantener compatibilidad actual: los scripts pueden escalar en
                        // editor/juego, pero eso no recompone automaticamente colliders.
                        t.scale.x = sx;
                        t.scale.y = sy;
                    }
                    // Mantener la escala base de animación en sync con scripts.
                    if let Some(saved) = self.anim_saved_transforms.get_mut(&id) {
                        saved.1.x = sx;
                        saved.1.y = sy;
                    }
                }
                ScriptCmd::PlayAnimation { id, name } => {
                    // Si la animación solicitada ya está activa en esa entidad,
                    // ignorar para evitar el bucle on_start → play_animation → on_start.
                    let already_active = self.active_animations.get(&id)
                        .map(|a| a.animation_name == name)
                        .unwrap_or(false);
                    if !already_active {
                        self.handle_command(EngineCommand::PlayAnimation { id, name });
                    }
                }
                ScriptCmd::SetDefaultAnimation { id, name } => {
                    self.handle_command(EngineCommand::SetDefaultAnimation { id, name });
                }
                ScriptCmd::StopAnimation { id } => {
                    self.handle_command(EngineCommand::StopAnimation { id });
                }
                ScriptCmd::SetPhysics { id, enabled, body_type } => {
                    // Evitar recrear el cuerpo Rapier si ya tiene el estado correcto.
                    // Destruir y recrear cada frame resetea la velocidad a 0, lo que
                    // impide que la gravedad acumule y que las colisiones funcionen.
                    let already_same = if enabled {
                        self.physics_2d.has_physics(id)
                            && self.physics_2d.get_body_type(id) == body_type
                    } else {
                        !self.physics_2d.has_physics(id)
                    };
                    if !already_same {
                        self.handle_command(EngineCommand::SetPhysics { id, enabled, body_type });
                    }
                }
                ScriptCmd::MoveEntity { id, speed, dir_x, dir_y } => {
                    self.update_entity_facing_from_horizontal(id, speed * dir_x);
                    // Aplica velocidad lineal al Rapier body usando shape cast para
                    // detectar obstáculos antes de aplicar. Si no tiene física activa,
                    // se aplica fallback por traslación directa para facilitar pruebas.
                    if self.preview_playing {
                        let moved = self.physics_2d.move_physics_entity(id, speed, dir_x, dir_y, self.delta_time);
                        if !moved {
                            let dx = speed * dir_x * self.delta_time;
                            let dy = speed * dir_y * self.delta_time;
                            if let Some(t) = self.world.get_mut::<Transform>(id) {
                                t.position.x += dx;
                                t.position.y += dy;
                            }
                            if let Some(saved) = self.anim_saved_transforms.get_mut(&id) {
                                saved.0.x += dx;
                                saved.0.y += dy;
                            }
                            log::warn!("[script/move_entity] entidad {} sin cuerpo físico activo — aplicado fallback translate", id);
                        }
                    } else {
                        // En modo editor no corremos el step de físicas; para pruebas
                        // manuales movemos por traslación directa respetando delta_time.
                        let dx = speed * dir_x * self.delta_time;
                        let dy = speed * dir_y * self.delta_time;
                        if let Some(t) = self.world.get_mut::<Transform>(id) {
                            t.position.x += dx;
                            t.position.y += dy;
                        }
                        if let Some(saved) = self.anim_saved_transforms.get_mut(&id) {
                            saved.0.x += dx;
                            saved.0.y += dy;
                        }
                    }
                }
                ScriptCmd::MoveEntityFacing { id, speed, amount_x, dir_y } => {
                    let facing_right = self.entity_facing_right.get(&id).copied().unwrap_or(true);
                    let facing_sign = if facing_right { 1.0 } else { -1.0 };
                    let dir_x = amount_x.abs() * facing_sign;

                    // Aplica velocidad lineal al Rapier body usando shape cast para
                    // detectar obstáculos antes de aplicar. Si no tiene física activa,
                    // se aplica fallback por traslación directa para facilitar pruebas.
                    if self.preview_playing {
                        let moved = self.physics_2d.move_physics_entity(id, speed, dir_x, dir_y, self.delta_time);
                        if !moved {
                            let dx = speed * dir_x * self.delta_time;
                            let dy = speed * dir_y * self.delta_time;
                            if let Some(t) = self.world.get_mut::<Transform>(id) {
                                t.position.x += dx;
                                t.position.y += dy;
                            }
                            if let Some(saved) = self.anim_saved_transforms.get_mut(&id) {
                                saved.0.x += dx;
                                saved.0.y += dy;
                            }
                            log::warn!("[script/move_entity_facing] entidad {} sin cuerpo físico activo — aplicado fallback translate", id);
                        }
                    } else {
                        // En modo editor no corremos el step de físicas; para pruebas
                        // manuales movemos por traslación directa respetando delta_time.
                        let dx = speed * dir_x * self.delta_time;
                        let dy = speed * dir_y * self.delta_time;
                        if let Some(t) = self.world.get_mut::<Transform>(id) {
                            t.position.x += dx;
                            t.position.y += dy;
                        }
                        if let Some(saved) = self.anim_saved_transforms.get_mut(&id) {
                            saved.0.x += dx;
                            saved.0.y += dy;
                        }
                    }
                }
                ScriptCmd::ApplyKinematicGravity { id, speed_x, jump_speed_y, gravity } => {
                    if self.preview_playing {
                        self.physics_2d.apply_kinematic_gravity(
                            id, speed_x, jump_speed_y, gravity, self.delta_time, None,
                        );
                    }
                }
                ScriptCmd::ApplyKinematicImpulse { id, dir_x, dir_y, impulse } => {
                    if self.preview_playing {
                        self.physics_2d.apply_kinematic_impulse(id, dir_x, dir_y, impulse);
                    }
                }
                ScriptCmd::SlideEntity { id, dx, dy, speed } => {
                    if self.preview_playing {
                        let (cx, cy) = if let Some(t) = self.world.get::<Transform>(id) {
                            (t.position.x, t.position.y)
                        } else {
                            (0.0, 0.0)
                        };
                        self.pending_slides.insert(id, PendingSlide {
                            target_x: cx + dx,
                            target_y: cy + dy,
                            speed:    speed.max(0.001),
                            keep_current_y: dy.abs() <= 1e-6,
                        });
                    }
                }
                ScriptCmd::Log { message } => {
                    log::info!("[script/log] {message}");
                }
                ScriptCmd::SetVsync { enabled } => {
                    self.set_vsync(enabled);
                }
            }
        }
    }
}
