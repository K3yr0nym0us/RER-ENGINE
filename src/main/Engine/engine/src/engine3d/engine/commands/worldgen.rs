//! Despacho de comandos del plugin worldgen (`world_gen_*`).
//!
//! Todos los comandos son 3D-only y van tras `cmd_world` en la cadena de
//! `handle_command`. La lógica vive en `config_3d\worldgen`.

use super::*;

impl State {
    pub(super) fn cmd_worldgen(&mut self, cmd: EngineCommand) -> Option<EngineCommand> {
        match cmd {
            EngineCommand::Only3d(EngineCommand3dOnly::WorldGenSetLibraryPath { path }) => {
                self.worldgen_set_library_path(path);
                None
            }
            EngineCommand::Only3d(EngineCommand3dOnly::WorldGenSetConfig { config_json }) => {
                self.worldgen_set_config(config_json);
                None
            }
            EngineCommand::Only3d(EngineCommand3dOnly::WorldGenStart) => {
                self.worldgen_start();
                None
            }
            EngineCommand::Only3d(EngineCommand3dOnly::WorldGenStop) => {
                self.worldgen_stop();
                None
            }
            EngineCommand::Only3d(EngineCommand3dOnly::WorldGenReset) => {
                self.worldgen_reset();
                None
            }
            EngineCommand::Only3d(EngineCommand3dOnly::WorldGenScatter { rules_json }) => {
                self.worldgen_scatter(&rules_json);
                None
            }
            _ => Some(cmd),
        }
    }
}
