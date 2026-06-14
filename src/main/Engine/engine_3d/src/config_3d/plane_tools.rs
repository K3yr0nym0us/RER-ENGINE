use crate::config_3d::plane_tool_rotate_dbg;
use crate::platform::{query_key_e_held_os, query_key_q_held_os};

use glam::{Quat, Vec3};

use crate::config_3d::quick_build::{snap_axis_edges_to_grid, GHOST_OFFSCREEN};
use crate::config_compat::ActiveTool;
use crate::ecs::{EntityId, MeshComponent, NonSelectable, Transform};
use crate::engine::State;
use crate::entity_save_meta::EntitySaveMeta;
use crate::ipc::{send_event, EngineEvent};
use crate::mesh;

const GHOST_ALPHA: f32 = 0.38;
/// Velocidad de giro al mantener Q/E (grados/segundo).
const PLANE_TOOL_ROTATE_SPEED_DEG: f32 = 90.0;
/// Alpha de muros/triggers colocados (semistransparentes en escena).
pub(crate) const PLANE_WALL_VISUAL_ALPHA: f32 = 0.38;
/// Iluminación normal pero sin recibir sombras (shader: 0.25 ≤ render_kind < 0.5).
pub(crate) const PLANE_WALL_RENDER_KIND: f32 = 0.25;
/// Grosor de colisión Rapier (invisible; el mesh es un quad sin volumen).
pub(crate) const PLANE_TOOL_PHYSICS_DEPTH: f32 = 0.05;
pub(crate) const DEFAULT_PLANE_WIDTH: f32 = 4.0;
pub(crate) const DEFAULT_PLANE_HEIGHT: f32 = 3.0;

const COLLIDER_RGBA: [u8; 4] = [60, 220, 200, 120];
const TRIGGER_RGBA: [u8; 4] = [220, 80, 80, 120];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlaneToolKind {
    Collider,
    ExecutionArea,
}

impl PlaneToolKind {
    fn marker_path(self) -> &'static str {
        match self {
            Self::Collider => "[Colisionador]",
            Self::ExecutionArea => "[ExecutionArea]",
        }
    }

    fn save_kind(self) -> &'static str {
        match self {
            Self::Collider => "collider",
            Self::ExecutionArea => "execution_area",
        }
    }

    fn label_base(self) -> &'static str {
        match self {
            Self::Collider => rer_engine_shared::editor_defaults::entity_label::COLLIDER,
            Self::ExecutionArea => rer_engine_shared::editor_defaults::entity_label::EXECUTION_AREA,
        }
    }

    fn color_rgba(self) -> [u8; 4] {
        match self {
            Self::Collider => COLLIDER_RGBA,
            Self::ExecutionArea => TRIGGER_RGBA,
        }
    }
}

pub(crate) fn plane_tool_scale_from_preview(preview_scale: Option<[f32; 3]>) -> [f32; 3] {
    let [w, h, _] = preview_scale.unwrap_or([
        DEFAULT_PLANE_WIDTH,
        DEFAULT_PLANE_HEIGHT,
        PLANE_TOOL_PHYSICS_DEPTH,
    ]);
    plane_tool_visual_scale(w, h)
}

fn plane_tool_visual_scale(width: f32, height: f32) -> [f32; 3] {
    [width.max(0.05), height.max(0.05), 1.0]
}

fn plane_center_on_surface(hit: [f32; 3], height: f32) -> [f32; 3] {
    [hit[0], hit[1] + height * 0.5, hit[2]]
}

impl State {
    pub(crate) fn ensure_plane_tool_wall_mesh(&mut self) -> usize {
        if let Some(idx) = self.plane_tool_wall_mesh_idx {
            return idx;
        }
        let mesh_idx = self.meshes.len();
        self.meshes
            .push(mesh::create_unit_wall_quad_xy(&self.device));
        self.plane_tool_wall_mesh_idx = Some(mesh_idx);
        mesh_idx
    }

    pub(crate) fn is_plane_wall_entity(&self, id: EntityId) -> bool {
        self.save_registry.meta.get(&id).is_some_and(|m| {
            m.path == PlaneToolKind::Collider.marker_path()
                || m.path == PlaneToolKind::ExecutionArea.marker_path()
        })
    }

    pub(crate) fn ensure_plane_tool_colored_texture(&mut self, rgba: [u8; 4]) -> usize {
        if let Some(&idx) = self.plane_tool_tex_cache.get(&rgba) {
            return idx;
        }
        let tex_idx = self.tex_layers.len();
        let block_layer = self
            .texture_array
            .pack(&self.queue, &rgba, 1, 1);
        self.tex_layers.push(block_layer);
        self.plane_tool_tex_cache.insert(rgba, tex_idx);
        tex_idx
    }

    fn plane_tool_effective_scale(&self) -> [f32; 3] {
        match &self.active_tool {
            ActiveTool::PlacePlaneTool { size, .. } => {
                plane_tool_visual_scale(size[0], size[1])
            }
            _ => plane_tool_scale_from_preview(self.plane_tool_preview_scale),
        }
    }

    fn plane_tool_physics_half_extents(&self, width: f32, height: f32) -> [f32; 3] {
        [
            width.max(0.05) * 0.5,
            height.max(0.05) * 0.5,
            PLANE_TOOL_PHYSICS_DEPTH * 0.5,
        ]
    }

    fn plane_tool_snap_position(&self, pos: [f32; 3]) -> [f32; 3] {
        if !self.ctrl_held {
            return pos;
        }
        let cell = self.grid_config.cell_size.max(0.05);
        let scale = self.plane_tool_effective_scale();
        let half_x = scale[0] * 0.5;
        let half_z = PLANE_TOOL_PHYSICS_DEPTH * 0.5;
        [
            snap_axis_edges_to_grid(pos[0], half_x, cell),
            pos[1],
            snap_axis_edges_to_grid(pos[2], half_z, cell),
        ]
    }

    pub(crate) fn activate_plane_tool(&mut self, kind: PlaneToolKind, size: [f32; 2]) {
        self.clear_plane_tool_rotate_held();
        if let Some(ghost_id) = self.plane_tool_ghost_id.take() {
            self.world.despawn(ghost_id);
        }
        let w = size[0].max(0.05);
        let h = size[1].max(0.05);
        let scale = plane_tool_visual_scale(w, h);
        self.plane_tool_preview_scale = Some(scale);
        self.active_tool = ActiveTool::PlacePlaneTool {
            kind,
            size: [w, h],
            cursor_world: None,
            yaw: 0.0,
        };
        self.plane_tool_ghost_id = self.spawn_plane_tool_ghost(kind, scale);
        self.tool_overlay_buffer = crate::gizmo::build_from_vertices(&self.device, &[]);
        if let Some((px, py)) = self.tool_cursor_pixels {
            self.update_plane_tool_cursor_3d(px, py);
        }
        log::info!(
            "[plane_tool] activada {:?} tamaño {:.2}×{:.2}",
            kind,
            w,
            h
        );
        send_event(&EngineEvent::PlaneToolReady {
            tool: match kind {
                PlaneToolKind::Collider => "draw_collider",
                PlaneToolKind::ExecutionArea => "draw_execution_area",
            }
            .to_string(),
            width: w,
            height: h,
        });
        self.window().focus_window();
        log::info!("[plane_tool] foco transferido a la ventana del motor");
    }

    pub(crate) fn focus_editor_window(&self) {
        rer_engine_shared::platform::focus_overlay_parent_window(self.editor_parent_id);
    }

    pub(crate) fn sync_plane_tool_from_set_active(&mut self, kind: PlaneToolKind, size: [f32; 2]) {
        if let ActiveTool::PlacePlaneTool {
            kind: active_kind,
            size: active_size,
            ..
        } = &self.active_tool
        {
            if *active_kind == kind {
                let old_h = active_size[1];
                self.apply_plane_tool_preview_size(size[0], size[1], old_h);
                return;
            }
        }
        self.activate_plane_tool(kind, size);
    }

    fn plane_tool_yaw(&self) -> f32 {
        match &self.active_tool {
            ActiveTool::PlacePlaneTool { yaw, .. } => *yaw,
            _ => 0.0,
        }
    }

    /// `degrees` > 0 rota a la derecha (E); < 0 a la izquierda (Q). Magnitud en grados.
    pub(crate) fn rotate_plane_tool(&mut self, degrees: f32) {
        if degrees.abs() < f32::EPSILON {
            return;
        }
        let delta = degrees.to_radians();
        let new_yaw = if let ActiveTool::PlacePlaneTool { yaw, .. } = &mut self.active_tool {
            *yaw += delta;
            *yaw
        } else {
            return;
        };
        if let Some(ghost_id) = self.plane_tool_ghost_id {
            if let Some(t) = self.world.get_mut::<Transform>(ghost_id) {
                t.rotation = Quat::from_rotation_y(new_yaw);
            }
        }
        
    }

    pub(crate) fn clear_plane_tool_rotate_held(&mut self) {
        self.plane_tool_rotate_left = false;
        self.plane_tool_rotate_right = false;
        plane_tool_rotate_dbg::log_clear("clear_plane_tool_rotate_held()");
    }

    pub(crate) fn apply_plane_tool_held_rotation(&mut self) {
        if !matches!(self.active_tool, ActiveTool::PlacePlaneTool { .. }) {
            return;
        }
        // Q/E: solo polling OS en el motor (sin IPC ni flags desde Electron).
        let os_q = query_key_q_held_os();
        let os_e = query_key_e_held_os();
        let left = os_q;
        let right = os_e;
        if !left && !right {
            plane_tool_rotate_dbg::log_apply_rotation(
                self.engine_window_focused,
                os_q,
                os_e,
                left,
                right,
                0.0,
            );
            return;
        }
        let mut degrees = 0.0f32;
        if left {
            degrees -= PLANE_TOOL_ROTATE_SPEED_DEG * self.delta_time;
        }
        if right {
            degrees += PLANE_TOOL_ROTATE_SPEED_DEG * self.delta_time;
        }
        plane_tool_rotate_dbg::log_apply_rotation(
            self.engine_window_focused,
            os_q,
            os_e,
            left,
            right,
            degrees,
        );
        if degrees.abs() >= f32::EPSILON {
            self.rotate_plane_tool(degrees);
        }
    }

    pub(crate) fn apply_plane_tool_preview_size(
        &mut self,
        width: f32,
        height: f32,
        previous_height: f32,
    ) {
        if !matches!(self.active_tool, ActiveTool::PlacePlaneTool { .. }) {
            return;
        }
        let w = width.max(0.05);
        let h = height.max(0.05);
        let scale = plane_tool_visual_scale(w, h);
        self.plane_tool_preview_scale = Some(scale);

        if let ActiveTool::PlacePlaneTool {
            size,
            cursor_world,
            ..
        } = &mut self.active_tool
        {
            *size = [w, h];
            if let Some(pos) = cursor_world {
                let ground_y = pos[1] - previous_height * 0.5;
                *pos = [pos[0], ground_y + h * 0.5, pos[2]];
            }
        }

        if let Some(ghost_id) = self.plane_tool_ghost_id {
            if let Some(t) = self.world.get_mut::<Transform>(ghost_id) {
                t.scale = Vec3::from_array(scale);
                if let ActiveTool::PlacePlaneTool { cursor_world, .. } = &self.active_tool {
                    if let Some(pos) = cursor_world {
                        t.position = Vec3::from_array(*pos);
                    }
                }
            }
        }
    }

    fn spawn_plane_tool_ghost(&mut self, kind: PlaneToolKind, scale: [f32; 3]) -> Option<EntityId> {
        let mesh_idx = self.ensure_plane_tool_wall_mesh();
        let tex_idx = self.ensure_plane_tool_colored_texture(kind.color_rgba());
        let ghost_id = self.world.spawn(Some("__plane_ghost__"));
        self.world.insert(
            ghost_id,
            MeshComponent {
                mesh_idx,
                tex_idx,
            },
        );
        self.world.insert(
            ghost_id,
            Transform {
                position: Vec3::new(GHOST_OFFSCREEN, GHOST_OFFSCREEN, GHOST_OFFSCREEN),
                scale: Vec3::from_array(scale),
                ..Default::default()
            },
        );
        self.world.insert(ghost_id, NonSelectable);
        Some(ghost_id)
    }

    pub(crate) fn deactivate_plane_tool(&mut self) {
        self.clear_plane_tool_rotate_held();
        if let Some(ghost_id) = self.plane_tool_ghost_id.take() {
            self.world.despawn(ghost_id);
        }
        self.plane_tool_preview_scale = None;
        if matches!(self.active_tool, ActiveTool::PlacePlaneTool { .. }) {
            self.active_tool = ActiveTool::None;
        }
    }

    pub(crate) fn update_plane_tool_cursor_3d(&mut self, pixel_x: f32, pixel_y: f32) {
        if !matches!(self.active_tool, ActiveTool::PlacePlaneTool { .. }) {
            return;
        }

        let Some(raw) = self.raycast_plane_tool_point(pixel_x, pixel_y) else {
            return;
        };
        let scale = self.plane_tool_effective_scale();
        let snapped = self.plane_tool_snap_position(plane_center_on_surface(raw, scale[1]));
        let yaw = self.plane_tool_yaw();

        if let ActiveTool::PlacePlaneTool { cursor_world, .. } = &mut self.active_tool {
            *cursor_world = Some(snapped);
        }

        if let Some(ghost_id) = self.plane_tool_ghost_id {
            if let Some(t) = self.world.get_mut::<Transform>(ghost_id) {
                t.scale = Vec3::from_array(scale);
                t.position = Vec3::from_array(snapped);
                t.rotation = Quat::from_rotation_y(yaw);
            }
        }
    }

    fn raycast_plane_tool_point(&mut self, pixel_x: f32, pixel_y: f32) -> Option<[f32; 3]> {
        self.raycast_placement_point(pixel_x, pixel_y)
    }

    pub(crate) fn place_plane_tool_at_cursor(
        &mut self,
        pixels: Option<(f32, f32)>,
    ) -> bool {
        let kind = match &self.active_tool {
            ActiveTool::PlacePlaneTool { kind, .. } => *kind,
            _ => return false,
        };

        let fit_to_grid = self.ctrl_held;
        let stored = match &self.active_tool {
            ActiveTool::PlacePlaneTool { cursor_world, .. } => *cursor_world,
            _ => None,
        };

        let position = if let Some((px, py)) = pixels {
            if fit_to_grid {
                self.raycast_plane_tool_point(px, py)
                    .map(|p| {
                        let scale = self.plane_tool_effective_scale();
                        self.plane_tool_snap_position(plane_center_on_surface(p, scale[1]))
                    })
                    .or(stored)
            } else {
                stored.or_else(|| {
                    self.raycast_plane_tool_point(px, py).map(|p| {
                        let scale = self.plane_tool_effective_scale();
                        plane_center_on_surface(p, scale[1])
                    })
                })
            }
        } else {
            stored
        };

        let Some(pos) = position else {
            send_event(&EngineEvent::Error {
                message: "[plane_tool] sin posición (apunta al suelo o una superficie)".into(),
            });
            return false;
        };

        let scale = self.plane_tool_effective_scale();
        let yaw = self.plane_tool_yaw();
        self.spawn_plane_entity_at(kind, pos, scale, None, None, true, Some(yaw), None);
        self.deactivate_plane_tool();
        self.focus_editor_window();
        true
    }

    pub(crate) fn build_plane_tool_ghost_overlay(&self) -> Option<(usize, mesh::InstanceData)> {
        let ActiveTool::PlacePlaneTool { .. } = self.active_tool else {
            return None;
        };
        let ghost_id = self.plane_tool_ghost_id?;
        let mc = self.world.get::<MeshComponent>(ghost_id)?;
        let t = self.world.get::<Transform>(ghost_id)?;
        if t.position.x < GHOST_OFFSCREEN + 1.0 {
            return None;
        }
        let layer = self.texture_layer_for(mc.tex_idx);
        let mut inst = mesh::InstanceData::new(t.to_matrix(), 0.0, layer);
        inst.flag_pad[1] = GHOST_ALPHA;
        Some((mc.mesh_idx, inst))
    }

    pub(crate) fn sync_plane_wall_physics(&mut self, id: EntityId) {
        if !self.collider_entities.contains(&id) {
            return;
        }
        let Some(t) = self.world.get::<Transform>(id).cloned() else {
            return;
        };
        let half = self.plane_tool_physics_half_extents(t.scale.x.abs(), t.scale.y.abs());
        let body_type = if self.physics.has_physics(id) {
            self.physics.get_body_type(id).to_string()
        } else {
            "static".to_string()
        };
        self.physics.set_entity_physics_oriented(
            id,
            true,
            &body_type,
            t.position.to_array(),
            t.rotation,
            half,
        );
    }

    pub(crate) fn spawn_plane_entity_at(
        &mut self,
        kind: PlaneToolKind,
        position: [f32; 3],
        scale: [f32; 3],
        forced_id: Option<EntityId>,
        display_name: Option<&str>,
        track_undo: bool,
        yaw: Option<f32>,
        saved_rotation: Option<[f32; 4]>,
    ) -> Option<EntityId> {
        let label = display_name
            .filter(|n| !n.trim().is_empty())
            .map(|n| n.to_owned())
            .unwrap_or_else(|| self.next_numbered_entity_name(kind.label_base()));

        let visual_scale = plane_tool_visual_scale(scale[0], scale[1]);
        let mesh_idx = self.ensure_plane_tool_wall_mesh();
        let tex_idx = self.ensure_plane_tool_colored_texture(kind.color_rgba());

        let id = if let Some(forced) = forced_id {
            if self.world.get::<Transform>(forced).is_some() {
                forced
            } else {
                self.world.spawn(Some(&label))
            }
        } else {
            self.world.spawn(Some(&label))
        };

        let rotation = if let Some([x, y, z, w]) = saved_rotation {
            Quat::from_xyzw(x, y, z, w).normalize()
        } else {
            Quat::from_rotation_y(yaw.unwrap_or(0.0))
        };

        self.world.insert(
            id,
            MeshComponent {
                mesh_idx,
                tex_idx,
            },
        );
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.position = Vec3::from_array(position);
            t.scale = Vec3::from_array(visual_scale);
            t.rotation = rotation;
        } else {
            self.world.insert(
                id,
                Transform {
                    position: Vec3::from_array(position),
                    scale: Vec3::from_array(visual_scale),
                    rotation,
                    ..Default::default()
                },
            );
        }

        match kind {
            PlaneToolKind::Collider => {
                self.entity_colision.insert(id, true);
                self.collider_entities.push(id);
                self.sync_plane_wall_physics(id);
            }
            PlaneToolKind::ExecutionArea => {
                self.execution_area_entities.push(id);
            }
        }

        self.save_registry.register_meta(
            id,
            EntitySaveMeta {
                kind: kind.save_kind().to_string(),
                path: kind.marker_path().to_string(),
                visual_model_path: None,
                entity_category: None,
            },
        );

        if track_undo {
            self.push_remove_entity_undo(id);
        }

        match kind {
            PlaneToolKind::Collider => {
                send_event(&EngineEvent::ColliderCreated {
                    id,
                    points: None,
                    position: Some(position),
                    scale: Some(visual_scale),
                });
            }
            PlaneToolKind::ExecutionArea => {
                send_event(&EngineEvent::ExecutionAreaCreated {
                    id,
                    points: None,
                    position: Some(position),
                    scale: Some(visual_scale),
                });
            }
        }

        log::info!(
            "[plane_tool] {:?} creado id={id} pos={position:?} scale={scale:?}",
            kind
        );
        Some(id)
    }

    pub(crate) fn restore_collider_plane_from_save(
        &mut self,
        name: &str,
        position: [f32; 3],
        scale: [f32; 3],
        rotation: Option<[f32; 4]>,
        forced_id: Option<EntityId>,
    ) -> Option<EntityId> {
        self.spawn_plane_entity_at(
            PlaneToolKind::Collider,
            position,
            scale,
            forced_id,
            Some(name),
            false,
            None,
            rotation,
        )
    }

    pub(crate) fn restore_trigger_plane_from_save(
        &mut self,
        name: &str,
        position: [f32; 3],
        scale: [f32; 3],
        rotation: Option<[f32; 4]>,
        forced_id: Option<EntityId>,
    ) -> Option<EntityId> {
        self.spawn_plane_entity_at(
            PlaneToolKind::ExecutionArea,
            position,
            scale,
            forced_id,
            Some(name),
            false,
            None,
            rotation,
        )
    }
}
