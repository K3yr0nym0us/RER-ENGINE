use std::sync::Arc;
use std::time::Instant;

use rodio::Source as RodioSource;
use winit::dpi::PhysicalSize;

use crate::engine3d::config_3d::plane_tools::{PlaneToolKind, plane_tool_scale_from_preview};
use crate::engine3d::config_compat::ActiveTool;
use crate::engine3d::ecs::{NameComponent, Transform};
use crate::engine3d::ipc::{
    AnimationFrameData, EngineCommand, EngineCommand3dOnly, EngineCommandCommon, EngineEvent,
    send_event,
};

use super::{
    ActiveAnimation, AnimationState, DecodedAudio, EntityTransformSnapshot, State, UndoAction,
};

mod animation;
mod assets;
mod entities;
mod graphics;
mod player_ui;
mod scenes;
mod scripts;
mod sockets;
mod system;
mod world;

impl State {
    pub fn window(&self) -> &std::sync::Arc<winit::window::Window> {
        &self.window
    }

    pub fn size(&self) -> PhysicalSize<u32> {
        self.size
    }

    pub fn is_preview_playing(&self) -> bool {
        self.preview_playing
    }

    pub fn push_undo_transform(
        &mut self,
        id: u32,
        position: [f32; 3],
        rotation: [f32; 4],
        scale: [f32; 3],
    ) {
        if !self.is_applying_undo {
            self.redo_stack.clear();
        }
        self.undo_stack.push(UndoAction::RestoreTransform {
            id,
            position,
            rotation,
            scale,
        });
        self.sync_editor_scenes_undo_dirty_to_renderer();
    }

    pub fn push_undo_transforms(&mut self, items: Vec<EntityTransformSnapshot>) {
        if items.is_empty() {
            return;
        }
        if !self.is_applying_undo {
            self.redo_stack.clear();
        }
        self.undo_stack
            .push(UndoAction::RestoreTransforms { items });
        self.sync_editor_scenes_undo_dirty_to_renderer();
    }

    /// Vacía historial undo/redo tras carga de escena o guardado (estado limpio del editor).
    pub fn clear_editor_undo_redo(&mut self) {
        let had_history = !self.undo_stack.is_empty() || !self.redo_stack.is_empty();
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.is_applying_undo = false;
        if had_history {
            self.sync_editor_scenes_undo_dirty_to_renderer();
        }
    }

    pub fn apply_undo(&mut self) {
        let Some(action) = self.undo_stack.pop() else {
            return;
        };
        self.is_applying_undo = true;
        match action {
            UndoAction::RestoreTransform {
                id,
                position,
                rotation,
                scale,
            } => {
                if let Some(t) = self.world.get::<Transform>(id) {
                    self.redo_stack.push(UndoAction::RestoreTransform {
                        id,
                        position: t.position.to_array(),
                        rotation: [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w],
                        scale: t.scale.to_array(),
                    });
                }
                self.handle_command(EngineCommand::Common(EngineCommandCommon::SetTransform {
                    id,
                    position: Some(position),
                    position_axis: None,
                    rotation: Some(rotation),
                    scale: Some(scale),
                    scale_axis: None,
                    track_undo: Some(false),
                    body_rotation_only: None,
                    rotation_euler_delta: None,
                    rotation_euler_degrees: None,
                }));
            }
            UndoAction::RestoreTransforms { items } => {
                let mut redo_items: Vec<EntityTransformSnapshot> = Vec::new();
                for (id, _, _, _) in &items {
                    if let Some(t) = self.world.get::<Transform>(*id) {
                        redo_items.push((
                            *id,
                            t.position.to_array(),
                            [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w],
                            t.scale.to_array(),
                        ));
                    }
                }
                if !redo_items.is_empty() {
                    self.redo_stack
                        .push(UndoAction::RestoreTransforms { items: redo_items });
                }
                for (id, position, rotation, scale) in items {
                    self.handle_command(EngineCommand::Common(EngineCommandCommon::SetTransform {
                        id,
                        position: Some(position),
                        position_axis: None,
                        rotation: Some(rotation),
                        scale: Some(scale),
                        scale_axis: None,
                        track_undo: Some(false),
                        body_rotation_only: None,
                        rotation_euler_delta: None,
                        rotation_euler_degrees: None,
                    }));
                }
            }
            UndoAction::RemoveEntity { snapshot } => {
                let id = snapshot.id;
                self.handle_command(EngineCommand::Common(EngineCommandCommon::RemoveEntity {
                    id,
                }));
                self.redo_stack.push(UndoAction::RestoreEntity { snapshot });
            }
            UndoAction::RestoreEntity { .. } => {}
            UndoAction::RestorePlayerUiHud { snapshot } => {
                if let Some(current) =
                    self.capture_player_ui_hud_undo_snapshot_with_key(&snapshot.key)
                {
                    self.redo_stack
                        .push(UndoAction::RestorePlayerUiHud { snapshot: current });
                }
                self.restore_player_ui_hud_undo_snapshot(snapshot);
            }
            UndoAction::RemoveEntitySocket { entity_id, socket } => {
                self.redo_stack.push(UndoAction::RestoreEntitySocket {
                    entity_id,
                    socket: socket.clone(),
                });
                self.apply_undo_remove_entity_socket(entity_id, &socket);
            }
            UndoAction::RestoreEntitySocket { .. } => {}
            UndoAction::RestoreSocketAttachment {
                child_id,
                previous_attachment,
                previous_position,
                previous_rotation,
                previous_scale,
                applied_attachment,
            } => {
                self.redo_stack.push(UndoAction::RestoreSocketAttachment {
                    child_id,
                    previous_attachment: self.entity_attachments.get(&child_id).cloned(),
                    previous_position: self
                        .world
                        .get::<Transform>(child_id)
                        .map(|t| t.position.to_array())
                        .unwrap_or(previous_position),
                    previous_rotation: self
                        .world
                        .get::<Transform>(child_id)
                        .map(|t| [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w])
                        .unwrap_or(previous_rotation),
                    previous_scale: self
                        .world
                        .get::<Transform>(child_id)
                        .map(|t| t.scale.to_array())
                        .unwrap_or(previous_scale),
                    applied_attachment: applied_attachment.clone(),
                });
                self.apply_undo_socket_attachment(
                    child_id,
                    previous_attachment,
                    previous_position,
                    previous_rotation,
                    previous_scale,
                );
            }
            UndoAction::RestoreBonePhysics {
                entity_id,
                bone_name,
                before,
                after,
            } => {
                self.redo_stack.push(UndoAction::RestoreBonePhysics {
                    entity_id,
                    bone_name: bone_name.clone(),
                    before: after,
                    after: before,
                });
                self.apply_undo_bone_physics(entity_id, &bone_name, before);
            }
        }
        self.is_applying_undo = false;
        self.sync_editor_scenes_undo_dirty_to_renderer();
    }

    pub fn apply_redo(&mut self) {
        let Some(action) = self.redo_stack.pop() else {
            return;
        };
        self.is_applying_undo = true;
        match action {
            UndoAction::RestoreTransform {
                id,
                position,
                rotation,
                scale,
            } => {
                if let Some(t) = self.world.get::<Transform>(id) {
                    self.undo_stack.push(UndoAction::RestoreTransform {
                        id,
                        position: t.position.to_array(),
                        rotation: [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w],
                        scale: t.scale.to_array(),
                    });
                }
                self.handle_command(EngineCommand::Common(EngineCommandCommon::SetTransform {
                    id,
                    position: Some(position),
                    position_axis: None,
                    rotation: Some(rotation),
                    scale: Some(scale),
                    scale_axis: None,
                    track_undo: Some(false),
                    body_rotation_only: None,
                    rotation_euler_delta: None,
                    rotation_euler_degrees: None,
                }));
            }
            UndoAction::RestoreTransforms { items } => {
                let mut undo_items: Vec<EntityTransformSnapshot> = Vec::new();
                for (id, _, _, _) in &items {
                    if let Some(t) = self.world.get::<Transform>(*id) {
                        undo_items.push((
                            *id,
                            t.position.to_array(),
                            [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w],
                            t.scale.to_array(),
                        ));
                    }
                }
                if !undo_items.is_empty() {
                    self.undo_stack
                        .push(UndoAction::RestoreTransforms { items: undo_items });
                }
                for (id, position, rotation, scale) in items {
                    self.handle_command(EngineCommand::Common(EngineCommandCommon::SetTransform {
                        id,
                        position: Some(position),
                        position_axis: None,
                        rotation: Some(rotation),
                        scale: Some(scale),
                        scale_axis: None,
                        track_undo: Some(false),
                        body_rotation_only: None,
                        rotation_euler_delta: None,
                        rotation_euler_degrees: None,
                    }));
                }
            }
            UndoAction::RestoreEntity { snapshot } => {
                self.restore_entity_from_undo_snapshot(&snapshot);
                self.undo_stack.push(UndoAction::RemoveEntity { snapshot });
            }
            UndoAction::RemoveEntity { snapshot } => {
                let id = snapshot.id;
                self.handle_command(EngineCommand::Common(EngineCommandCommon::RemoveEntity {
                    id,
                }));
                self.undo_stack.push(UndoAction::RestoreEntity { snapshot });
            }
            UndoAction::RestorePlayerUiHud { snapshot } => {
                if let Some(current) =
                    self.capture_player_ui_hud_undo_snapshot_with_key(&snapshot.key)
                {
                    self.undo_stack
                        .push(UndoAction::RestorePlayerUiHud { snapshot: current });
                }
                self.restore_player_ui_hud_undo_snapshot(snapshot);
            }
            UndoAction::RemoveEntitySocket { .. } => {}
            UndoAction::RestoreEntitySocket { entity_id, socket } => {
                self.undo_stack.push(UndoAction::RemoveEntitySocket {
                    entity_id,
                    socket: socket.clone(),
                });
                self.apply_redo_restore_entity_socket(entity_id, &socket);
            }
            UndoAction::RestoreSocketAttachment {
                child_id,
                previous_attachment: _,
                previous_position: _,
                previous_rotation: _,
                previous_scale: _,
                applied_attachment,
            } => {
                if let Some(t) = self.world.get::<Transform>(child_id) {
                    self.undo_stack.push(UndoAction::RestoreSocketAttachment {
                        child_id,
                        previous_attachment: self.entity_attachments.get(&child_id).cloned(),
                        previous_position: t.position.to_array(),
                        previous_rotation: [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w],
                        previous_scale: t.scale.to_array(),
                        applied_attachment: applied_attachment.clone(),
                    });
                }
                self.apply_redo_socket_attachment(child_id, &applied_attachment);
            }
            UndoAction::RestoreBonePhysics {
                entity_id,
                bone_name,
                before,
                after,
            } => {
                self.undo_stack.push(UndoAction::RestoreBonePhysics {
                    entity_id,
                    bone_name: bone_name.clone(),
                    before: after,
                    after: before,
                });
                self.apply_undo_bone_physics(entity_id, &bone_name, after);
            }
        }
        self.is_applying_undo = false;
        self.sync_editor_scenes_undo_dirty_to_renderer();
    }

    pub fn handle_command(&mut self, cmd: EngineCommand) {
        let mut cmd = Some(cmd);
        if let Some(c) = cmd.take() {
            cmd = self.cmd_system(c);
        }
        if let Some(c) = cmd.take() {
            cmd = self.cmd_scenes(c);
        }
        if let Some(c) = cmd.take() {
            cmd = self.cmd_entities(c);
        }
        if let Some(c) = cmd.take() {
            cmd = self.cmd_assets(c);
        }
        if let Some(c) = cmd.take() {
            cmd = self.cmd_graphics(c);
        }
        if let Some(c) = cmd.take() {
            cmd = self.cmd_world(c);
        }
        if let Some(c) = cmd.take() {
            cmd = self.cmd_player_ui(c);
        }
        if let Some(c) = cmd.take() {
            cmd = self.cmd_animation(c);
        }
        if let Some(c) = cmd.take() {
            cmd = self.cmd_scripts(c);
        }
        if let Some(c) = cmd.take() {
            cmd = self.cmd_sockets(c);
        }
        if let Some(cmd) = cmd {
            log::warn!("[commands] comando sin manejar: {cmd:?}");
        }
    }
}

/// Valida extensión y contenido de un archivo de fuente (.ttf / .otf).
pub(super) fn validate_font_file(path: &str) -> Result<(), String> {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if ext != "ttf" && ext != "otf" {
        return Err("Se esperaba un archivo .ttf u .otf".to_string());
    }
    let bytes = std::fs::read(path).map_err(|e| format!("No se pudo leer la fuente: {e}"))?;
    ttf_parser::Face::parse(&bytes, 0).map_err(|e| format!("Archivo de fuente inválido: {e}"))?;
    Ok(())
}
