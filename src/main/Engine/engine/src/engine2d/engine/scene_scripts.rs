use super::State;

impl State {
    pub(crate) fn handle_load_scene_visual_script(
        &mut self,
        scene_id: u32,
        source: &str,
    ) -> Result<(), String> {
        self.script_engine.load_scene_script(scene_id, source)
    }

    pub(crate) fn run_scene_script_on_play_start(&mut self) {
        let cmds = self.script_engine.on_scene_play_start();
        if !cmds.is_empty() {
            self.apply_script_commands(cmds);
        }
    }

    pub(crate) fn update_scene_scripts(&mut self) {
        if !self.preview_playing {
            return;
        }
        let cmds = self.script_engine.tick_scene_script(self.delta_time);
        if !cmds.is_empty() {
            self.apply_script_commands(cmds);
        }
    }
}
