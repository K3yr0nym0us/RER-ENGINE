// ---------------------------------------------------------------------------
// Compatibilidad de contrato IPC/API para el binario 3D
//
// Este módulo mantiene firmas históricas usadas por engine/main para comandos
// compartidos con frontend. Los símbolos con nombre "2d" aquí son stubs de
// compatibilidad y no activan lógica 2D en este binario.
// ---------------------------------------------------------------------------

pub(crate) mod camera;
pub(crate) use camera::Camera2D;

pub(crate) mod mesh;
pub use mesh::{build_grid, GridBuffer, GridConfig};

pub(crate) mod physics;
pub(crate) use physics::PhysicsWorld2D;

use crate::engine::State;

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum ActiveTool {
    None,
    DrawCollider {
        points_world: Vec<[f32; 2]>,
        cursor_world: Option<[f32; 2]>,
    },
    DrawExecutionArea {
        points_world: Vec<[f32; 2]>,
        cursor_world: Option<[f32; 2]>,
    },
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
#[allow(dead_code)]
pub(crate) struct ScenarioMarker {
    pub img_width: u32,
    pub img_height: u32,
    pub base_world_h: f32,
    pub path: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ColliderMarker;

#[derive(Debug, Clone)]
pub(crate) struct ExecutionAreaMarker;

impl State {
    pub(crate) fn setup_2d_platformer(&mut self) {
        log::warn!("[engine_3d] setup_2d_platformer ignorado (binario 3D)");
    }

    pub(crate) fn load_scenario(&mut self, _path: &str) {
        log::warn!("[engine_3d] load_scenario ignorado (binario 3D)");
    }

    // load_character: implementado en config_base.rs

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

    pub(crate) fn handle_pivot_click_2d(&mut self, _pixel_x: f32, _pixel_y: f32) -> bool {
        false
    }

    pub fn pick_entity_2d(&mut self, _pixel_x: f32, _pixel_y: f32) {}

    pub fn pick_gizmo_axis_2d(&self, _pixel_x: f32, _pixel_y: f32) -> Option<usize> {
        None
    }

    pub fn drag_gizmo_2d(
        &mut self,
        _pixel_x: f32,
        _pixel_y: f32,
        _last_x: f32,
        _last_y: f32,
        _axis_idx: usize,
        _snap: bool,
    ) {
    }

    pub fn update_hover_2d(&mut self, _pixel_x: f32, _pixel_y: f32) {
        self.hovered_gizmo_axis = None;
    }

    pub(crate) fn update_tool_overlay_cursor_2d(&mut self, _pixel_x: f32, _pixel_y: f32) {}

    pub(crate) fn undo_last_tool_step_2d(&mut self) -> bool {
        false
    }

    pub(crate) fn handle_tool_click_2d(&mut self, _pixel_x: f32, _pixel_y: f32) -> bool {
        false
    }

    pub(crate) fn create_collision_box_from_points(
        &mut self,
        _pts: &[[f32; 2]; 4],
        _track_undo: bool,
    ) {
    }

    pub(crate) fn create_execution_area_from_points(
        &mut self,
        _pts: &[[f32; 2]; 4],
        _track_undo: bool,
    ) {
    }

    pub(crate) fn update_execution_areas_2d(&mut self) {
        self.execution_overlaps.clear();
    }

    pub(crate) fn load_quick_build_ghost(
        &mut self,
        path: &str,
        _kind: &str,
        scale: [f32; 3],
        _src_rect: Option<[u32; 4]>,
    ) -> Option<u32> {
        if self.camera_2d.is_some() {
            return None;
        }
        self.load_quick_build_ghost_3d(path, scale)
    }
}
