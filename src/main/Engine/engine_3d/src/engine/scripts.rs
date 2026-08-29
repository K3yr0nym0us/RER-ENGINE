use std::collections::HashMap;

use crate::ecs::Transform;
use crate::ipc::{EngineCommand, EngineCommandCommon};
use crate::scripting::{EntitySnapshot, PlayScriptInput, ScriptCmd};

use super::State;

impl State {
    pub(crate) fn build_play_script_input(&self) -> PlayScriptInput {
        let play_aim_dir_3d = if self.preview_playing && self.is_play_controller_active() {
            let dir = self.camera.view_forward();
            if dir.length_squared() > f32::EPSILON {
                Some((dir.x, dir.y, dir.z))
            } else {
                None
            }
        } else {
            None
        };
        PlayScriptInput {
            is_play: self.preview_playing,
            mouse_world_2d: None,
            play_aim_dir_3d,
        }
    }

    pub(crate) fn prepare_control_script_input(&mut self) {
        let input = self.build_play_script_input();
        self.script_engine.set_play_script_input(input);
    }

    /// Ejecuta un tick del motor de scripting y aplica los comandos generados.
    pub(crate) fn update_scripts(&mut self) {
        if !self.preview_playing {
            return;
        }

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
                let animations: Vec<String> = self
                    .animations
                    .get(&id)
                    .map(|m| m.keys().cloned().collect())
                    .unwrap_or_default();
                map.insert(
                    id,
                    EntitySnapshot {
                        id,
                        x,
                        y,
                        scale_x,
                        scale_y,
                        facing_right,
                        facing_sign,
                        animations,
                    },
                );
            }
            map
        };

        let active_ui = self.player_ui_active_player_screen_id.clone();
        let commands = self
            .script_engine
            .tick(self.delta_time, &snapshots, active_ui.as_deref());
        self.apply_script_commands(commands);
    }

    pub(crate) fn execute_control_script(
        &mut self,
        id: u32,
        control_key: &str,
        path: &str,
        source: &str,
    ) {
        if !self.preview_playing {
            return;
        }

        self.prepare_control_script_input();

        let snapshot = self.build_script_snapshot(id);
        match self.script_engine.run_control_script(
            id,
            control_key,
            path,
            source,
            snapshot.as_ref(),
        ) {
            Ok(commands) => self.apply_script_commands(commands),
            Err(e) => log::error!(
                "[control] Error ejecutando script '{}' ({}): {}",
                path,
                control_key,
                e
            ),
        }

        // Play character: la tecla del binding se aplica en el motor (como en b9de03b).
        // El script solo personaliza velocidad/sprint/salto; no hace falta fp_press_key("W") etc.
        if self.play_character_entity == Some(id) {
            self.play_controller_script_input
                .insert(control_key.to_string());
        }
    }

    /// Una sola vez por pulsación (borde ascendente). Ejecuta `on_press` en Rhai.
    pub fn dispatch_on_press(&mut self, device: &str, control_key: &str) {
        if !self.preview_playing {
            return;
        }

        self.prepare_control_script_input();
        log::info!("[on_press] tecla {} detectada", control_key);

        let bindings: Vec<(u32, String, String)> = self
            .control_bindings_by_entity
            .iter()
            .filter_map(|(&id, b)| {
                let script = match device {
                    "keyboard_mouse" => b.keyboard_mouse.get(control_key),
                    "gamepad" => b.gamepad.get(control_key),
                    _ => None,
                }?;
                Some((id, script.name.clone(), script.source.clone()))
            })
            .collect();

        for (id, path, source) in bindings {
            let snapshot = self.build_script_snapshot(id);
            match self.script_engine.run_control_script_just_pressed(
                id,
                control_key,
                &path,
                &source,
                snapshot.as_ref(),
            ) {
                Ok(commands) => self.apply_script_commands(commands),
                Err(e) => log::error!("[on_press] Error en '{}' ({}): {}", path, control_key, e),
            }
            if self.play_character_entity == Some(id) {
                self.play_controller_script_input
                    .insert(control_key.to_string());
            }
        }
    }

    pub fn dispatch_on_keep_key_down(&self, control_key: &str) {
        if !self.preview_playing {
            return;
        }
        log::info!("[on_keep] tecla {} bajó", control_key);
    }

    pub fn dispatch_on_keep_key_up(&mut self, device: &str, control_key: &str) {
        if !self.preview_playing {
            return;
        }
        let _ = device;
        log::info!("[on_keep] tecla {} subió", control_key);
    }

    /// Cada frame mientras la tecla/botón está sostenido. Ejecuta `on_keep` en Rhai.
    pub fn dispatch_on_keep_frame(&mut self, device: &str, control_key: &str) {
        if !self.preview_playing {
            return;
        }

        let bindings: Vec<(u32, String, String)> = self
            .control_bindings_by_entity
            .iter()
            .filter_map(|(&id, b)| {
                let script = match device {
                    "keyboard_mouse" => b.keyboard_mouse.get(control_key),
                    "gamepad" => b.gamepad.get(control_key),
                    _ => None,
                }?;
                Some((id, script.name.clone(), script.source.clone()))
            })
            .collect();

        for (id, path, source) in bindings {
            let snapshot = self.build_script_snapshot(id);
            match self.script_engine.run_control_script(
                id,
                control_key,
                &path,
                &source,
                snapshot.as_ref(),
            ) {
                Ok(mut commands) => {
                    commands.retain(|c| !matches!(c, ScriptCmd::Log { .. }));
                    self.apply_script_commands(commands);
                }
                Err(e) => log::error!("[on_keep] Error en '{}' ({}): {}", path, control_key, e),
            }
            if self.play_character_entity == Some(id) {
                self.play_controller_script_input
                    .insert(control_key.to_string());
            }
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

        let animations: Vec<String> = self
            .animations
            .get(&id)
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();

        Some(EntitySnapshot {
            id,
            x,
            y,
            scale_x,
            scale_y,
            facing_right,
            facing_sign,
            animations,
        })
    }

    /// Aplica los comandos generados por los scripts al estado del motor.
    pub(crate) fn apply_script_commands(&mut self, commands: Vec<ScriptCmd>) {
        for cmd in commands {
            match cmd {
                ScriptCmd::SetPosition { id, x, y } => {
                    let horizontal = self
                        .world
                        .get::<Transform>(id)
                        .map(|t| x - t.position.x)
                        .unwrap_or(0.0);
                    self.update_entity_facing_from_horizontal(id, horizontal);
                    if let Some(t) = self.world.get_mut::<Transform>(id) {
                        t.position.x = x;
                        t.position.y = y;
                    }
                    if let Some(saved) = self.anim_saved_transforms.get_mut(&id) {
                        saved.0.x = x;
                        saved.0.y = y;
                    }
                }
                ScriptCmd::Translate { id, dx, dy } => {
                    self.update_entity_facing_from_horizontal(id, dx);
                    if let Some(t) = self.world.get_mut::<Transform>(id) {
                        t.position.x += dx;
                        t.position.y += dy;
                    }
                    if let Some(saved) = self.anim_saved_transforms.get_mut(&id) {
                        saved.0.x += dx;
                        saved.0.y += dy;
                    } else {
                        log::warn!(
                            "[script/translate] entidad {} SIN entrada en anim_saved_transforms — translate no acumulado",
                            id
                        );
                    }
                }
                ScriptCmd::SetScale { id, sx, sy } => {
                    if let Some(t) = self.world.get_mut::<Transform>(id) {
                        t.scale.x = sx;
                        t.scale.y = sy;
                    }
                    if let Some(saved) = self.anim_saved_transforms.get_mut(&id) {
                        saved.1.x = sx;
                        saved.1.y = sy;
                    }
                }
                ScriptCmd::PlayAnimation { id, name } => {
                    let already_active = self
                        .active_animations
                        .get(&id)
                        .map(|a| a.animation_name == name)
                        .unwrap_or(false);
                    if !already_active {
                        self.handle_command(EngineCommand::Common(
                            EngineCommandCommon::PlayAnimation {
                                id,
                                name,
                                loop_: true,
                            },
                        ));
                    }
                }
                ScriptCmd::SetDefaultAnimation { id, name } => {
                    self.handle_command(EngineCommand::Common(
                        EngineCommandCommon::SetDefaultAnimation { id, name },
                    ));
                }
                ScriptCmd::StopAnimation { id } => {
                    self.handle_command(EngineCommand::Common(
                        EngineCommandCommon::StopAnimation { id },
                    ));
                }
                ScriptCmd::SetPhysics {
                    id,
                    enabled,
                    body_type,
                } => {
                    let already_same = if enabled {
                        self.physics.has_physics(id) && self.physics.get_body_type(id) == body_type
                    } else {
                        !self.physics.has_physics(id)
                    };
                    if !already_same {
                        self.handle_command(EngineCommand::Common(
                            EngineCommandCommon::SetPhysics {
                                id,
                                enabled,
                                body_type,
                            },
                        ));
                    }
                }
                ScriptCmd::MoveEntity {
                    id,
                    speed,
                    dir_x,
                    dir_y,
                } => {
                    self.update_entity_facing_from_horizontal(id, speed * dir_x);
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
                ScriptCmd::MoveEntityFacing {
                    id,
                    speed,
                    amount_x,
                    dir_y,
                } => {
                    let facing_right = self.entity_facing_right.get(&id).copied().unwrap_or(true);
                    let facing_sign = if facing_right { 1.0 } else { -1.0 };
                    let dir_x = amount_x.abs() * facing_sign;

                    {
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
                ScriptCmd::Log { .. } => {}
                ScriptCmd::PlayControllerPressKey { key } => {
                    self.play_controller_script_input.insert(key);
                }
                ScriptCmd::PlayControllerJump => {
                    self.queue_play_controller_jump();
                }
                ScriptCmd::PlayControllerSetWalkSpeed(speed) => {
                    self.play_controller_script_walk_speed = Some(speed.max(0.0));
                }
                ScriptCmd::PlayControllerSetSprintMultiplier(mult) => {
                    self.play_controller_script_sprint_multiplier = Some(mult.max(0.0));
                }
                ScriptCmd::PlayControllerSetJumpSpeed(speed) => {
                    self.play_controller_script_jump_speed = Some(speed.max(0.0));
                }
                ScriptCmd::SetVsync { enabled } => {
                    self.set_vsync(enabled);
                }
                ScriptCmd::SetTaa { enabled } => {
                    self.set_taa(enabled, None, None);
                }
                ScriptCmd::SetActivePlayerUiScreen { screen_id } => {
                    if let Err(e) = self.set_active_player_ui_screen(&screen_id) {
                        log::warn!("[script] set_active_player_ui: {e}");
                    }
                }
                ScriptCmd::SetActivePlayerUiScreenByName { name } => {
                    if let Err(e) = self.set_active_player_ui_screen_by_name(&name) {
                        log::warn!("[script] set_active_player_ui_by_name: {e}");
                    }
                }
                ScriptCmd::ClearActivePlayerUiScreen => {
                    self.clear_active_player_ui_screen();
                }
                ScriptCmd::SetGraphicsTextureTier { tier } => {
                    if let Some(t) =
                        crate::config_3d::texture_graphics::TextureGraphicsTier::from_wire(&tier)
                    {
                        self.set_graphics_texture_tier(t);
                    }
                }
                ScriptCmd::SetReflectionTier { tier } => {
                    if let Some(t) =
                        crate::config_3d::reflection_graphics::ReflectionTier::from_wire(&tier)
                    {
                        self.set_reflection_tier(t);
                    }
                }
                ScriptCmd::FireProjectile {
                    template_id,
                    from_id,
                    dir_x,
                    dir_y,
                    dir_z,
                } => {
                    let from = if from_id == 0 { None } else { Some(from_id) };
                    let _ = self.fire_projectile_from_template(
                        template_id,
                        from,
                        glam::Vec3::new(dir_x, dir_y, dir_z),
                    );
                }
                ScriptCmd::ApplyKinematicGravity { .. }
                | ScriptCmd::ApplyKinematicImpulse { .. }
                | ScriptCmd::SlideEntity { .. } => {}
            }
        }
    }
}
