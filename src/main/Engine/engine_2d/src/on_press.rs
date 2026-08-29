// on_press — Fires exactly once per key/button press (no autorepeat).
//
// Responsibilities:
//   1. Log the press event so the developer can see when a key was detected.
//   2. Execute the `on_press` Rhai callback EXACTLY ONE TIME.
//
// This module has ZERO interaction with on_keep logic. It never runs per-frame
// loops and never accesses keyboard_mouse_pressed tracking.

use crate::engine::State;
use crate::ipc::{EngineCommand, EngineCommandCommon};
use crate::scripting::ScriptCmd;

impl State {
    /// Called once when a key/button is pressed (no autorepeat).
    ///
    /// - Logs "[on_press] tecla X detectada".
    /// - Runs the `on_press` Rhai callback exactly one time for every entity
    ///   that has a control binding matching `control_key` on `device`.
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
            if let Some(dir_x) = self.infer_horizontal_input_dir(device, control_key)
                && self.physics_2d.has_physics(id)
                && self.physics_2d.is_horizontal_blocked(id, dir_x)
            {
                self.handle_command(EngineCommand::Common(EngineCommandCommon::StopAnimation {
                    id,
                }));
                continue;
            }

            let snap = self.build_script_snapshot(id);
            match self.script_engine.run_control_script_just_pressed(
                id,
                control_key,
                &path,
                &source,
                snap.as_ref(),
            ) {
                Ok(cmds) => {
                    for cmd in &cmds {
                        match cmd {
                            ScriptCmd::MoveEntity {
                                id, speed, dir_x, ..
                            } => {
                                self.update_entity_facing_from_horizontal(*id, *speed * *dir_x);
                            }
                            ScriptCmd::MoveEntityFacing { id, amount_x, .. } => {
                                let facing_right =
                                    self.entity_facing_right.get(id).copied().unwrap_or(true);
                                let facing_sign = if facing_right { 1.0 } else { -1.0 };
                                self.update_entity_facing_from_horizontal(
                                    *id,
                                    *amount_x * facing_sign,
                                );
                            }
                            ScriptCmd::SlideEntity { id, dx, .. } => {
                                self.update_entity_facing_from_horizontal(*id, *dx);
                            }
                            _ => {}
                        }
                    }

                    self.apply_script_commands(cmds)
                }
                Err(e) => log::error!("[on_press] Error en '{}' ({}): {}", path, control_key, e),
            }
        }
    }
}
