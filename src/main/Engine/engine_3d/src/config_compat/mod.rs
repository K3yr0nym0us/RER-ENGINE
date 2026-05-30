// ---------------------------------------------------------------------------
// Compatibilidad de contrato IPC/API para el binario 3D
//
// Stubs para comandos compartidos con el frontend 2D; no activan runtime 2D aquí.
// ---------------------------------------------------------------------------

pub(crate) mod mesh;
pub use mesh::GridConfig;

use crate::engine::State;

#[derive(Debug)]
pub(crate) enum ActiveTool {
    None,
    QuickBuildPlace {
        cursor_world: Option<[f32; 3]>,
    },
}

impl Default for ActiveTool {
    fn default() -> Self {
        ActiveTool::None
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ScenarioMarker {
    pub img_width: u32,
    pub img_height: u32,
    pub base_world_h: f32,
    #[allow(dead_code)]
    pub path: String,
}

impl State {
    pub(crate) fn setup_2d_platformer(&mut self) {
        log::warn!("[engine_3d] setup_2d_platformer ignorado (binario 3D)");
    }

    pub(crate) fn load_scenario(&mut self, _path: &str) {
        log::warn!("[engine_3d] load_scenario ignorado (binario 3D)");
    }

    pub(crate) fn set_character_scale(&mut self, _id: u32, _scale: f32) {}

    pub(crate) fn load_background(&mut self, _path: &str) {
        log::warn!("[engine_3d] load_background ignorado (binario 3D)");
    }

    pub(crate) fn clear_background(&mut self) {
        self.background_entity = None;
    }

    pub(crate) fn play_animation_frame(
        &mut self,
        _id: u32,
        _path: &str,
        _pivot_x: f32,
        _pivot_y: f32,
        _logical_w: u32,
        _logical_h: u32,
        _src_rect: Option<(u32, u32, u32, u32)>,
        _flip_horizontal: bool,
    ) {
    }

    pub(crate) fn preload_anim_frame_with_rect(
        &mut self,
        _path: &str,
        _src_rect: Option<(u32, u32, u32, u32)>,
    ) {
    }

    pub(crate) fn restore_animation_frame(&mut self, _id: u32) {}

    pub(crate) fn enter_pivot_edit_mode(
        &mut self,
        _id: u32,
        _frame_path: &str,
        _pivot_x: f32,
        _pivot_y: f32,
    ) {
    }

    pub(crate) fn cancel_pivot_edit_mode(&mut self) {
        self.pivot_edit_mode = None;
    }

    pub(crate) fn enter_logical_area_mode(&mut self, id: u32, _w: u32, _h: u32) {
        self.logical_area_mode = Some(id);
    }

    pub(crate) fn cancel_logical_area_mode(&mut self) {
        self.logical_area_mode = None;
    }

    pub(crate) fn undo_last_tool_step_2d(&mut self) -> bool {
        false
    }

    pub(crate) fn load_quick_build_ghost(
        &mut self,
        path: &str,
        _kind: &str,
        scale: [f32; 3],
        _src_rect: Option<[u32; 4]>,
    ) -> Option<u32> {
        self.load_quick_build_ghost_3d(path, scale)
    }
}
