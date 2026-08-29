/// Snapshot de input de juego inyectado en el contexto Rhai antes de scripts de control.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct PlayScriptInput {
    pub is_play: bool,
    pub mouse_world_2d: Option<(f32, f32)>,
    pub play_aim_dir_3d: Option<(f32, f32, f32)>,
}
