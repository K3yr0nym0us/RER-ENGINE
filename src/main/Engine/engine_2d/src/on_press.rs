// on_press — Fires exactly once per key/button press (no autorepeat).
//
// Responsibilities:
//   1. Log the press event so the developer can see when a key was detected.
//   2. Execute the `on_press` Lua callback EXACTLY ONE TIME.
//
// This module has ZERO interaction with on_keep logic. It never runs per-frame
// loops and never accesses keyboard_mouse_pressed tracking.

use crate::engine::State;
use crate::ipc::EngineCommand;
use crate::scripting::ScriptCmd;

impl State {
    /// Called once when a key/button is pressed (no autorepeat).
    ///
    /// - Logs "[on_press] tecla X detectada".
    /// - Runs the `on_press` Lua callback exactly one time for every entity
    ///   that has a control binding matching `control_key` on `device`.
    pub fn dispatch_on_press(&mut self, device: &str, control_key: &str) {
        if !self.preview_playing {
            return;
        }

        log::info!("[on_press] tecla {} detectada", control_key);

        let bindings: Vec<(u32, String, String)> = self
            .control_bindings_by_entity
            .iter()
            .filter_map(|(&id, b)| {
                let script = match device {
                    "keyboard_mouse" => b.keyboard_mouse.get(control_key),
                    "gamepad"        => b.gamepad.get(control_key),
                    _                => None,
                }?;
                Some((id, script.name.clone(), script.source.clone()))
            })
            .collect();

        for (id, path, source) in bindings {
            if let Some(dir_x) = self.infer_horizontal_input_dir(device, control_key) {
                if self.physics_2d.has_physics(id) && self.physics_2d.is_horizontal_blocked(id, dir_x) {
                    self.handle_command(EngineCommand::StopAnimation { id });
                    continue;
                }
            }

            let snap = self.build_script_snapshot(id);
            match self.script_engine.run_control_script_just_pressed(
                id,
                control_key,
                &path,
                &source,
                snap.as_ref(),
            ) {
                Ok(mut cmds) => {
                    let mut extra_cmds: Vec<ScriptCmd> = Vec::new();

                    // Compatibilidad para kinematic en on_press:
                    // si un script usa move_entity (pensado para movimiento continuo),
                    // convertirlo a un slide corto para que la pulsación única sea visible.
                    for cmd in &mut cmds {
                        if let ScriptCmd::MoveEntity { id, speed, dir_x, dir_y } = *cmd {
                            if self.physics_2d.get_body_type(id) == "kinematic" {
                                let len = (dir_x * dir_x + dir_y * dir_y).sqrt();
                                if len > 1e-6 {
                                    let nx = dir_x / len;
                                    let ny = dir_y / len;
                                    // Distancia base visible por pulsación única.
                                    // Mantiene comportamiento estable sin teletransportar.
                                    let press_distance = speed.max(0.1) * 0.18;
                                    *cmd = ScriptCmd::SlideEntity {
                                        id,
                                        dx: nx * press_distance,
                                        dy: ny * press_distance,
                                        speed: speed.max(0.1),
                                    };
                                }
                            }
                        }

                        // move_entity_facing: usa el facing actual del personaje.
                        // En kinematic, si no hay componente vertical, lo convertimos a slide
                        // horizontal corto; si hay componente vertical, añadimos deriva lateral
                        // separada respetando el facing actual.
                        if let ScriptCmd::MoveEntityFacing { id, speed, amount_x, dir_y } = *cmd {
                            if self.physics_2d.get_body_type(id) == "kinematic" {
                                let facing_right = self.entity_facing_right.get(&id).copied().unwrap_or(true);
                                let facing_sign = if facing_right { 1.0 } else { -1.0 };
                                let horizontal_distance = amount_x.abs() * speed.max(0.1) * 0.18;

                                if dir_y.abs() <= 1e-6 {
                                    *cmd = ScriptCmd::SlideEntity {
                                        id,
                                        dx: horizontal_distance * facing_sign,
                                        dy: 0.0,
                                        speed: speed.max(0.1),
                                    };
                                } else if horizontal_distance > 1e-6 {
                                    extra_cmds.push(ScriptCmd::SlideEntity {
                                        id,
                                        dx: horizontal_distance * facing_sign,
                                        dy: 0.0,
                                        speed: speed.max(0.1),
                                    });
                                }
                            }
                        }
                    }

                    cmds.extend(extra_cmds);

                    // Asegurar orientación correcta ANTES de procesar PlayAnimation.
                    // Algunos scripts llaman play_animation antes de move_entity en el mismo
                    // on_press; si no adelantamos el facing, el flip puede quedar invertido.
                    for cmd in &cmds {
                        match cmd {
                            ScriptCmd::MoveEntity { id, speed, dir_x, .. } => {
                                self.update_entity_facing_from_horizontal(*id, *speed * *dir_x);
                            }
                            ScriptCmd::MoveEntityFacing { id, amount_x, .. } => {
                                let facing_right = self.entity_facing_right.get(id).copied().unwrap_or(true);
                                let facing_sign = if facing_right { 1.0 } else { -1.0 };
                                self.update_entity_facing_from_horizontal(*id, *amount_x * facing_sign);
                            }
                            ScriptCmd::SlideEntity { id, dx, .. } => {
                                self.update_entity_facing_from_horizontal(*id, *dx);
                            }
                            _ => {}
                        }
                    }

                    self.apply_script_commands(cmds)
                }
                Err(e)   => log::error!("[on_press] Error en '{}' ({}): {}", path, control_key, e),
            }
        }
    }
}
