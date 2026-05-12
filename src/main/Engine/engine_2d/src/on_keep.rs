// on_keep — Handles continuous input while a key/button is held.
//
// Responsibilities:
//   1. Log when a key/button transitions DOWN (first press, no repeat).
//   2. Log when a key/button is released (UP).
//   3. Execute the `on_keep` Lua callback every frame while held (movement, etc.).
//
// This module has ZERO interaction with on_press logic. It never fires the
// `on_press` Lua callback and never uses `just_pressed` guards.

use crate::engine::State;
use crate::scripting::ScriptCmd;

impl State {
    /// Called once when a key/button first transitions to the held state.
    /// Logs "[on_keep] tecla X bajó".
    pub fn dispatch_on_keep_key_down(&self, control_key: &str) {
        if !self.preview_playing {
            return;
        }
        log::info!("[on_keep] tecla {} bajó", control_key);
    }

    /// Called once when a key/button is released.
    /// Logs "[on_keep] tecla X subió".
    pub fn dispatch_on_keep_key_up(&mut self, device: &str, control_key: &str) {
        if !self.preview_playing {
            return;
        }
        self.clear_on_keep_horizontal_block_for_input(device, control_key);
        log::info!("[on_keep] tecla {} subió", control_key);
    }

    /// Called every frame while a key/button is held (from `RedrawRequested`).
    /// Executes the `on_keep` Lua callback for continuous movement/actions.
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
                    "gamepad"        => b.gamepad.get(control_key),
                    _                => None,
                }?;
                Some((id, script.name.clone(), script.source.clone()))
            })
            .collect();

        for (id, path, source) in bindings {
            if self.should_block_on_keep_script(id, device, control_key) {
                continue;
            }

            let snap = self.build_script_snapshot(id);
            match self.script_engine.run_control_script(
                id,
                control_key,
                &path,
                &source,
                snap.as_ref(),
            ) {
                Ok(mut cmds) => {
                    // on_keep corre cada frame; suprimir ScriptCmd::Log evita spam en consola
                    // y deja en on_keep solo los logs de bajada/subida.
                    cmds.retain(|c| !matches!(c, ScriptCmd::Log { .. }));
                    self.apply_script_commands(cmds);
                }
                Err(e)   => log::error!("[on_keep] Error en '{}' ({}): {}", path, control_key, e),
            }
        }
    }
}
