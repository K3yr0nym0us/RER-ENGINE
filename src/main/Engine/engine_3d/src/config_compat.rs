// ---------------------------------------------------------------------------
// Compatibilidad de contrato IPC/API para el binario 3D
//
// Este módulo mantiene firmas históricas usadas por engine/main para comandos
// compartidos con frontend. Los símbolos con nombre "2d" aquí son stubs de
// compatibilidad y no activan lógica 2D en este binario.
// ---------------------------------------------------------------------------

use std::collections::{HashMap, HashSet};

use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

use crate::ecs::{EntityId, World};
use crate::engine::State;
use crate::gizmo;

#[derive(Debug, Clone)]
pub(crate) struct Camera2D {
    pub x: f32,
    pub y: f32,
    pub half_h: f32,
    pub near: f32,
    pub far: f32,
}

impl Camera2D {
    pub(crate) fn position(&self) -> Vec3 {
        Vec3::new(self.x, self.y, 10.0)
    }

    pub(crate) fn view_proj(&self, aspect: f32) -> Mat4 {
        let half_w = self.half_h * aspect;
        let proj = Mat4::orthographic_rh(-half_w, half_w, -self.half_h, self.half_h, self.near, self.far);
        let view = Mat4::look_at_rh(
            Vec3::new(self.x, self.y, 10.0),
            Vec3::new(self.x, self.y, 0.0),
            Vec3::Y,
        );
        proj * view
    }

    pub(crate) fn pan(&mut self, _dx: f32, _dy: f32, _vw: f32, _vh: f32) {}
}

pub struct GridConfig {
    pub world_width: f32,
    pub world_height: f32,
    pub visible: bool,
    pub cell_size: f32,
}

impl Default for GridConfig {
    fn default() -> Self {
        Self {
            world_width: 100.0,
            world_height: 50.0,
            visible: false,
            cell_size: 1.0,
        }
    }
}

pub struct GridBuffer {
    pub vertex_buffer: wgpu::Buffer,
    pub vertex_count: u32,
}

pub fn build_grid(device: &wgpu::Device, _config: &GridConfig) -> GridBuffer {
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("grid-vbuf-stub"),
        contents: bytemuck::cast_slice(&[gizmo::GizmoVertex {
            position: [0.0, 0.0, 0.0],
            color: [0.0, 0.0, 0.0, 0.0],
        }]),
        usage: wgpu::BufferUsages::VERTEX,
    });
    GridBuffer {
        vertex_buffer,
        vertex_count: 0,
    }
}

pub(crate) struct PhysicsWorld2D {
    active: HashSet<EntityId>,
    body_types: HashMap<EntityId, String>,
}

impl Default for PhysicsWorld2D {
    fn default() -> Self {
        Self {
            active: HashSet::new(),
            body_types: HashMap::new(),
        }
    }
}

impl PhysicsWorld2D {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn body_count(&self) -> u32 {
        self.active.len() as u32
    }

    pub(crate) fn set_gravity(&mut self, _gravity_y: f32) {}

    pub(crate) fn set_entity_physics(
        &mut self,
        entity: EntityId,
        enabled: bool,
        body_type: &str,
        _position: [f32; 3],
        _half_ext: [f32; 3],
    ) {
        if enabled {
            self.active.insert(entity);
            self.body_types.insert(entity, body_type.to_string());
        } else {
            self.active.remove(&entity);
            self.body_types.remove(&entity);
        }
    }

    pub(crate) fn remove_entity_body(&mut self, entity: EntityId) {
        self.active.remove(&entity);
        self.body_types.remove(&entity);
    }

    pub(crate) fn has_physics(&self, entity: EntityId) -> bool {
        self.active.contains(&entity)
    }

    pub(crate) fn get_body_type(&self, entity: EntityId) -> &str {
        self.body_types
            .get(&entity)
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    pub(crate) fn teleport_entity(&mut self, _entity: EntityId, _x: f32, _y: f32) {}

    pub(crate) fn move_physics_entity(
        &mut self,
        entity: EntityId,
        _speed: f32,
        _dir_x: f32,
        _dir_y: f32,
        _dt: f32,
    ) -> bool {
        self.active.contains(&entity)
    }

    pub(crate) fn step(&mut self, _dt: f32, _ecs: &mut World) {}
}

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
        cursor_world: Option<[f32; 2]>,
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

    pub(crate) fn load_character(&mut self, _path: &str) {
        log::warn!("[engine_3d] load_character ignorado (binario 3D)");
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
        _path: &str,
        _kind: &str,
        _scale: [f32; 3],
        _src_rect: Option<[u32; 4]>,
    ) -> Option<u32> {
        None
    }
}
