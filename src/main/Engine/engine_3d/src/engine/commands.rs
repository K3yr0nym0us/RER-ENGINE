use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use rodio;
use rodio::Source as RodioSource;
use winit::dpi::PhysicalSize;

use crate::config_compat::ActiveTool;
use crate::config_3d::plane_tools::{PlaneToolKind, plane_tool_scale_from_preview};
use crate::ecs::{NameComponent, Transform};
use crate::gizmo;
use crate::ipc::{send_event, AnimationFrameData, EngineCommand, EngineCommand3dOnly, EngineCommandCommon, EngineEvent};

use super::{ActiveAnimation, AnimationState, DecodedAudio, State, UndoAction};

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
        self.undo_stack
            .push(UndoAction::RestoreTransform { id, position, rotation, scale });
        self.sync_editor_scenes_undo_dirty_to_renderer();
    }

    pub fn push_undo_transforms(&mut self, items: Vec<(u32, [f32; 3], [f32; 4], [f32; 3])>) {
        if items.is_empty() {
            return;
        }
        if !self.is_applying_undo {
            self.redo_stack.clear();
        }
        self.undo_stack.push(UndoAction::RestoreTransforms { items });
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
                let mut redo_items: Vec<(u32, [f32; 3], [f32; 4], [f32; 3])> = Vec::new();
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
                    self.redo_stack.push(UndoAction::RestoreTransforms { items: redo_items });
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
                self.handle_command(EngineCommand::Common(EngineCommandCommon::RemoveEntity { id }));
                self.redo_stack
                    .push(UndoAction::RestoreEntity { snapshot });
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
                self.redo_stack
                    .push(UndoAction::RestoreEntitySocket { entity_id, socket: socket.clone() });
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
                let mut undo_items: Vec<(u32, [f32; 3], [f32; 4], [f32; 3])> = Vec::new();
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
                    self.undo_stack.push(UndoAction::RestoreTransforms { items: undo_items });
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
                self.undo_stack
                    .push(UndoAction::RemoveEntity { snapshot });
            }
            UndoAction::RemoveEntity { snapshot } => {
                let id = snapshot.id;
                self.handle_command(EngineCommand::Common(EngineCommandCommon::RemoveEntity { id }));
                self.undo_stack
                    .push(UndoAction::RestoreEntity { snapshot });
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
                self.undo_stack
                    .push(UndoAction::RemoveEntitySocket { entity_id, socket: socket.clone() });
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
                        previous_rotation: [
                            t.rotation.x,
                            t.rotation.y,
                            t.rotation.z,
                            t.rotation.w,
                        ],
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
        match cmd {
            EngineCommand::Common(EngineCommandCommon::Ping) => {
                send_event(&EngineEvent::Pong);
            }
            EngineCommand::Common(EngineCommandCommon::SetClearColor { r, g, b }) => {
                self.clear_color = wgpu::Color { r, g, b, a: 1.0 };
            }
            EngineCommand::Common(EngineCommandCommon::Resize { width, height }) => {
                self.resize(PhysicalSize::new(width, height));
            }
            EngineCommand::Common(EngineCommandCommon::SetBounds {
                x,
                y,
                width,
                height,
                ..
            }) => {
                let _ = self.window.set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
                self.resize(PhysicalSize::new(width, height));
                let _ = self
                    .window
                    .request_inner_size(winit::dpi::PhysicalSize::new(width, height));
            }
            EngineCommand::Common(EngineCommandCommon::LoadModel {
                path,
                single_instance,
                entity_category,
                kind,
            }) => {
                let category = entity_category.as_deref();
                let kind = kind.as_deref().unwrap_or("model");
                if single_instance.unwrap_or(false) {
                    self.load_model_single(&path, category, kind);
                } else {
                    self.load_model(&path, category, kind);
                }
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SpawnCachedModel {
                path,
                name,
                position,
                rotation,
                scale,
                entity_category,
                blueprint_id,
                physics_enabled,
                physics_type,
            }) => {
                if let Err(message) = self.spawn_cached_model_from_save(
                    &path,
                    position,
                    rotation,
                    scale,
                    name.as_deref(),
                    entity_category,
                    blueprint_id,
                    physics_enabled,
                    &physics_type,
                    None,
                ) {
                    send_event(&EngineEvent::Error { message });
                }
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SpawnQuickBuildInstance {
                position,
                rotation,
                scale,
            }) => {
                let _ = self.spawn_quick_build_instance_at(position, rotation, scale);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::PlaceQuickBuildAtCursor { pixel_x, pixel_y }) => {
                let pixels = match (pixel_x, pixel_y) {
                    (Some(x), Some(y)) => Some((x, y)),
                    _ => None,
                };
                if !self.place_plane_tool_at_cursor(pixels) {
                    self.place_quick_build_at_cursor(pixels);
                }
            }
            EngineCommand::Common(EngineCommandCommon::ReplaceEntityModel { id, path }) => {
                self.replace_entity_model(id, &path);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::LoadModelAsset { path, name, category }) => {
                self.register_model_asset(&path, &name, category.as_deref());
            }
            EngineCommand::Only3d(EngineCommand3dOnly::RemoveModelAsset { path }) => {
                if let Some(removed) = self.remove_model_from_library(&path) {
                    send_event(&EngineEvent::ModelAssetRemoved { path: removed.clone() });
                    log::info!("[model] eliminado de recursos: {}", removed);
                } else {
                    log::warn!("[model] intento de eliminar modelo inexistente: {}", path);
                }
            }
            EngineCommand::Only3d(EngineCommand3dOnly::GetModelsList) => {
                let models: Vec<crate::ipc::ModelInfo> = self
                    .model_store
                    .iter()
                    .map(|(path, entry)| {
                        let state = entry.model_id.as_ref().and_then(|id| {
                            self.imported_model_registry.get(id).map(|e| {
                                match e.state {
                                    rer_engine_shared::assets::AssetState::Importing => {
                                        "importing"
                                    }
                                    rer_engine_shared::assets::AssetState::Ready => "ready",
                                    rer_engine_shared::assets::AssetState::Failed => "failed",
                                }
                                .to_string()
                            })
                        });
                        crate::ipc::ModelInfo {
                            path: path.clone(),
                            name: entry.name.clone(),
                            category: entry.category.clone(),
                            model_id: entry.model_id.clone(),
                            asset: entry.rerasset_path.clone(),
                            state,
                        }
                    })
                    .collect();
                let count = models.len();
                send_event(&EngineEvent::ModelsList { models });
                log::info!("[model] lista enviada: {} modelos", count);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SpawnEditorBox {
                name,
                position,
                scale,
            }) => {
                self.spawn_editor_box(&name, position, scale);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SpawnSun {
                name,
                position,
                scale,
            }) => {
                self.spawn_sun(&name, position, scale);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SpawnGround { position, scale }) => {
                self.spawn_ground_plane(position, scale);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetDirectionalLight {
                ambient,
                intensity,
                shadow_darkness,
            }) => {
                self.apply_directional_light_settings(ambient, intensity, shadow_darkness);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetPlayCharacterSpawn {
                position,
                yaw,
                pitch,
            }) => {
                self.apply_play_character_view(
                    position,
                    yaw,
                    pitch,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                );
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetPlayCharacterView {
                position,
                position_axis,
                yaw,
                pitch,
                fov_y,
                frustum_distance,
                camera_only,
                camera_follow_mode,
                body_rotation,
                body_scale,
                camera_eye_position,
                fps_camera_yaw,
                fps_camera_pitch,
            }) => {
                if camera_only.unwrap_or(false) {
                    self.apply_play_camera_view_patch(
                        position_axis,
                        yaw,
                        pitch,
                        fov_y,
                        frustum_distance,
                        camera_follow_mode,
                    );
                } else {
                    let Some(pos) = position else {
                        log::warn!("set_play_character_view: falta position (carga/restauración)");
                        return;
                    };
                    self.apply_play_character_view(
                        pos,
                        yaw.unwrap_or(
                            crate::config_3d::character_anchor::PLAY_CHARACTER_EDITOR_ORBIT_YAW,
                        ),
                        pitch.unwrap_or(
                            crate::config_3d::character_anchor::PLAY_CHARACTER_EDITOR_ORBIT_PITCH,
                        ),
                        fov_y,
                        frustum_distance,
                        camera_follow_mode,
                        body_rotation,
                        body_scale,
                        camera_eye_position,
                        fps_camera_yaw,
                        fps_camera_pitch,
                    );
                }
            }
            EngineCommand::Common(EngineCommandCommon::SetTransform {
                id,
                position,
                position_axis,
                rotation,
                scale,
                scale_axis,
                track_undo,
                body_rotation_only,
                rotation_euler_delta,
                rotation_euler_degrees,
            }) => {
                use glam::{Quat, Vec3};
                let before = self.world.get::<Transform>(id).cloned();
                let is_play_character = self.play_character_entity == Some(id);
                let is_editor_camera = self.editor_camera_entity == Some(id);
                let in_play_mode =
                    self.preview_playing || self.is_play_controller_active();

                let current_rot = before
                    .as_ref()
                    .map(|t| t.rotation)
                    .unwrap_or(Quat::IDENTITY);
                let current_pos = before
                    .as_ref()
                    .map(|t| t.position)
                    .unwrap_or(Vec3::ZERO);
                let current_scale = before
                    .as_ref()
                    .map(|t| t.scale)
                    .unwrap_or(Vec3::ONE);

                let rot_quat = crate::config_3d::resolve_set_transform_rotation(
                    current_rot,
                    rotation,
                    rotation_euler_delta,
                    rotation_euler_degrees,
                );
                let position_vec = crate::config_3d::resolve_axis_update(
                    current_pos,
                    position,
                    position_axis,
                );
                let scale_vec = crate::config_3d::resolve_axis_update(
                    current_scale,
                    scale,
                    scale_axis,
                );
                let position_arr = position_vec.map(|v| v.to_array());
                let rotation_changed = rot_quat.is_some();

                if is_editor_camera && !in_play_mode {
                    self.apply_editor_camera_transform(id, position_arr);
                } else if is_play_character {
                    if body_rotation_only.unwrap_or(false) && !in_play_mode {
                        log::trace!(
                            "set_transform: jugador FP en editor (sin mover viewport orbital)"
                        );
                    }
                    let rot_apply = if in_play_mode && rot_quat.is_some() {
                        
                        None
                    } else {
                        rot_quat
                    };
                    // Rotación en editor: solo cuerpo; posición del front desincronizada rompe el anclaje de pies.
                    let position_apply = if rot_apply.is_some() && !in_play_mode {
                        None
                    } else {
                        position_arr
                    };
                    self.apply_play_character_transform_editor(
                        id,
                        position_apply,
                        rot_apply,
                        scale_vec,
                    );
                    // No sync_player_rotation_from_look: panel Transform = solo cuerpo en editor.
                } else if !is_editor_camera {
                    if let Some(transform) = self.world.get_mut::<Transform>(id) {
                        if let Some(r) = rot_quat {
                            transform.rotation = r;
                        }
                        if let Some(p) = position_vec {
                            transform.position = p;
                        }
                        if let Some(s) = scale_vec {
                            transform.scale = s;
                        }
                    }
                }

                if self.sun_entity == Some(id) {
                    self.sync_directional_light_from_sun();
                }

                if let Some(saved) = self.anim_saved_transforms.get_mut(&id) {
                    if let Some(p) = position_vec {
                        saved.0 = if is_play_character {
                            self.world
                                .get::<Transform>(id)
                                .map(|t| t.position)
                                .unwrap_or(p)
                        } else {
                            p
                        };
                    }
                    if let Some(s) = scale_vec {
                        if !is_play_character {
                            saved.1 = s;
                        }
                    }
                }
                if !is_play_character && !is_editor_camera {
                    if position_vec.is_some() || rotation_changed || scale_vec.is_some() {
                        self.sync_entity_physics_collider(id);
                    }
                }
                if let Some(prev) = before {
                    let prev_pos = prev.position.to_array();
                    let prev_rot = [prev.rotation.x, prev.rotation.y, prev.rotation.z, prev.rotation.w];
                    let prev_scl = prev.scale.to_array();
                    let next_pos = self.world.get::<Transform>(id).map(|t| t.position.to_array());
                    let next_rot = self
                        .world
                        .get::<Transform>(id)
                        .map(|t| [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w]);
                    let next_scl = self.world.get::<Transform>(id).map(|t| t.scale.to_array());
                    let should_track_undo = track_undo.unwrap_or(true);
                    if !self.is_applying_undo
                        && should_track_undo
                        && (next_pos != Some(prev_pos)
                            || next_rot != Some(prev_rot)
                            || next_scl != Some(prev_scl))
                    {
                        self.push_undo_transform(id, prev_pos, prev_rot, prev_scl);
                    }
                }
                if is_play_character {
                    self.emit_play_character_view_changed(false);
                }
                self.handle_entity_attachment_after_transform(&[id]);
                self.notify_reflection_probe_transform_changed(&[id]);
            }
            EngineCommand::Common(EngineCommandCommon::SetEntityName { id, name, force }) => {
                let next_name = name.trim();
                if next_name.is_empty() {
                    send_event(&EngineEvent::Error {
                        message: "El nombre no puede estar vacio".to_string(),
                    });
                    return;
                }

                if !force && self.is_entity_name_taken(next_name, Some(id)) {
                    send_event(&EngineEvent::Error {
                        message: format!("Ya existe una entidad con el nombre '{}'", next_name),
                    });
                    return;
                }

                if let Some(existing) = self.world.get_mut::<NameComponent>(id) {
                    existing.name = next_name.to_string();
                } else {
                    self.world.insert(
                        id,
                        NameComponent {
                            name: next_name.to_string(),
                        },
                    );
                }

                if self.selected_entity == Some(id) {
                    let transform = self.world.get::<Transform>(id).cloned().unwrap_or_default();
                    let position = transform.position.to_array();
                    let rotation = [
                        transform.rotation.x,
                        transform.rotation.y,
                        transform.rotation.z,
                        transform.rotation.w,
                    ];
                    let scale = transform.scale.to_array();
                    let physics_enabled = self.physics.has_physics(id);
                    let physics_type = self.physics.get_body_type(id).to_string();

                    send_event(&EngineEvent::EntitySelected {
                        id,
                        name: next_name.to_string(),
                        position,
                        rotation,
                        scale,
                        physics_enabled,
                        physics_type,
                        blueprint_id: self.entity_blueprint_ids.get(&id).cloned(),
                    });
                }
            }
            EngineCommand::Common(EngineCommandCommon::SetScene { scene, save_path }) => match scene.as_str() {
                "3D" | "first-person" | "second-person" | "third-person" => {
                    if let Some(path) = save_path.filter(|p| !p.trim().is_empty()) {
                        if std::path::Path::new(&path).is_dir() {
                            self.load_proyect_from_save_path(&path);
                        } else {
                            log::warn!(
                                "[SetScene] se esperaba directorio extraído, no archivo: {path}"
                            );
                            self.setup_default_3d_scene();
                        }
                    } else {
                        self.setup_default_3d_scene();
                    }
                }
                "2D" => {
                    let _ = save_path;
                    log::warn!("SetScene '2D' ignorado en rer_engine_3d");
                }
                _ => log::info!("SetScene: escena '{}' no reconocida", scene),
            },
            EngineCommand::Only3d(EngineCommand3dOnly::LoadCharacter { path, track_undo }) => {
                self.load_character(&path);
                if track_undo.unwrap_or(false) {
                    if let Some(&id) = self.character_entities.last() {
                        self.push_remove_entity_undo(id);
                        log::info!("[quick_build] personaje {id} registrado en undo");
                    }
                }
            }
            EngineCommand::Common(EngineCommandCommon::PlayAudio { path, loop_ }) => {
                let decoded = std::fs::read(&path).ok().and_then(|b| {
                    let cursor = std::io::Cursor::new(b);
                    rodio::Decoder::new(cursor).ok().map(|dec| {
                        let ch = dec.channels().get();
                        let sr = dec.sample_rate().get();
                        let s: Vec<f32> = dec.collect();
                        Arc::new(DecodedAudio {
                            samples: s,
                            channels: ch,
                            sample_rate: sr,
                        })
                    })
                });
                match decoded {
                    Some(audio) => self.play_audio_internal(audio, loop_),
                    None => log::error!("[audio] no se pudo cargar o decodificar: {path}"),
                }
            }
            EngineCommand::Common(EngineCommandCommon::StopAudio) => {
                self.stop_audio_internal();
                log::info!("[audio] detenido por comando externo");
            }
            EngineCommand::Common(EngineCommandCommon::DeselectEntity) => {
                if self.selected_entity.is_some() || !self.selected_entities.is_empty() {
                    self.selected_entity = None;
                    self.selected_entities.clear();
                    send_event(&EngineEvent::EntityDeselected);
                }
            }
            EngineCommand::Common(EngineCommandCommon::RemoveEntity { id }) => {
                let removed_kind = if self.collider_entities.contains(&id) {
                    "collider"
                } else if self.execution_area_entities.contains(&id) {
                    "execution_area"
                } else if self.scenario_entities.contains(&id) {
                    "scenario"
                } else if self.character_entities.contains(&id) {
                    "character"
                } else {
                    "model"
                };

                self.selected_entities.retain(|&e| e != id);
                if Some(id) == self.selected_entity {
                    self.selected_entity = self.selected_entities.last().copied();
                }
                if self.selected_entities.is_empty() && self.selected_entity.is_none() {
                    send_event(&EngineEvent::EntityDeselected);
                }
                if Some(id) == self.hovered_entity {
                    self.hovered_entity = None;
                    send_event(&EngineEvent::EntityUnhovered);
                }
                self.physics.remove_entity_body(id);
                self.scenario_entities.retain(|&e| e != id);
                self.character_entities.retain(|&e| e != id);
                self.collider_entities.retain(|&e| e != id);
                self.execution_area_entities.retain(|&e| e != id);
                self.execution_overlaps
                    .retain(|(trigger_id, actor_id)| *trigger_id != id && *actor_id != id);
                self.entity_facing_right.remove(&id);
                self.default_animation_by_entity.remove(&id);
                self.unbind_model_animations(id);
                self.animations.remove(&id);
                self.active_animations.remove(&id);
                self.anim_saved_transforms.remove(&id);
                self.control_bindings_by_entity.remove(&id);
                self.script_engine.detach_entity(id);
                let was_probe = self.is_reflection_probe_entity(id);
                self.save_registry.remove_entity(id);
                if was_probe {
                    self.release_probe_entity_slot(id);
                }
                self.entity_blueprint_ids.remove(&id);
                self.entity_colision.remove(&id);
                self.clear_entity_attachments_for_removed(id);
                if self.sun_entity == Some(id) {
                    self.sun_entity = None;
                }
                self.world.despawn(id);
                send_event(&EngineEvent::EntityRemoved {
                    id,
                    kind: removed_kind.to_string(),
                });
            }
            EngineCommand::Common(EngineCommandCommon::SetWorldSize {
                width,
                height,
                depth,
            }) => {
                self.set_world_bounds_3d_size(width, height, depth);
                self.clamp_play_character_camera_to_bounds();
            }
            EngineCommand::Common(EngineCommandCommon::SetGravity { gravity }) => {
                let magnitude = gravity.abs();
                self.physics.set_gravity(-magnitude);
                log::info!("[physics] Gravedad actualizada: {:.2} m/s²", magnitude);
            }
            EngineCommand::Common(EngineCommandCommon::SetGridVisible { visible }) => {
                self.grid_config.visible = visible;
            }
            EngineCommand::Common(EngineCommandCommon::SetGridCellSize { size }) => {
                self.grid_config.cell_size = size.clamp(0.05, 100.0);
                self.refresh_ground_checker_uv();
            }
            EngineCommand::Common(EngineCommandCommon::SetTargetFps { fps }) => {
                self.target_fps = fps.clamp(1, 1000);
                log::info!("[render] Límite de FPS actualizado: {}", self.target_fps);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetGraphicsTextureTier { tier }) => {
                if let Some(t) =
                    crate::config_3d::texture_graphics::TextureGraphicsTier::from_wire(&tier)
                {
                    self.set_graphics_texture_tier(t);
                }
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetTextureDetailDistance { distance_m }) => {
                self.set_texture_detail_near_m(distance_m);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetReflectionTier { tier }) => {
                log::info!("[reflexiones] IPC set_reflection_tier: {tier}");
                if let Some(t) =
                    crate::config_3d::reflection_graphics::ReflectionTier::from_wire(&tier)
                {
                    self.set_reflection_tier(t);
                } else {
                    log::warn!("[reflexiones] tier IPC no reconocido: {tier}");
                }
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetReflectionRaytracing { .. }) => {} // RT disabled
            EngineCommand::Only3d(EngineCommand3dOnly::SetReflectionProbes { enabled }) => {
                log::info!("[reflexiones] IPC set_reflection_probes: {enabled}");
                self.set_reflection_probes(enabled);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SpawnReflectionProbe { position }) => {
                log::info!("[reflexiones] IPC spawn_reflection_probe");
                let _ = self.spawn_reflection_probe(position);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetReflectionDebugView { view }) => {
                if let Some(v) =
                    crate::config_3d::reflection_graphics::ReflectionDebugView::from_wire(&view)
                {
                    self.set_reflection_debug_view(v);
                } else {
                    log::warn!("[reflexiones] vista debug IPC no reconocida: {view}");
                }
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetSsrDebugMode { enabled }) => {
                self.set_reflection_debug_view(if enabled {
                    crate::config_3d::reflection_graphics::ReflectionDebugView::SsrDebug
                } else {
                    crate::config_3d::reflection_graphics::ReflectionDebugView::Final
                });
                send_event(&EngineEvent::SsrDebugModeChanged { enabled });
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetShadowTier { tier }) => {
                log::info!("[sombras] IPC set_shadow_tier: {tier}");
                if let Some(t) = crate::config_3d::shadow_graphics::ShadowTier::from_wire(&tier) {
                    self.set_shadow_tier(t);
                } else {
                    log::warn!("[sombras] tier IPC no reconocido: {tier}");
                }
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetWorldRadius { radius }) => {
                self.set_world_bounds_3d_radius(radius);
                self.clamp_play_character_camera_to_bounds();
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetTaa { enabled, blend, jitter_scale }) => {
                // TAA IPC log suppressed
                self.set_taa(enabled, blend, jitter_scale);
                send_event(&EngineEvent::TaaChanged { enabled, blend, jitter_scale });
            }
            EngineCommand::Common(EngineCommandCommon::SetDebugMode { show }) => {
                self.debug_mode = show;
                log::info!("[debug] modo debug (colisiones): {}", show);
            }
            EngineCommand::Common(EngineCommandCommon::SetPlayerUiEditMode {
                active,
                scope,
                screen_id,
            }) => {
                self.apply_player_ui_edit_mode(
                    active,
                    scope.as_deref(),
                    screen_id.as_deref(),
                );
            }
            EngineCommand::Common(EngineCommandCommon::AddPlayerUiTextBox { font_path }) => {
                match self.add_player_ui_text_box(&font_path) {
                    Ok(id) => {
                        if let Some(key) = self.player_ui_text_key() {
                            if let Some(entry) = self
                                .player_ui_text_boxes
                                .get(&key)
                                .and_then(|list| list.iter().find(|b| b.id == id))
                            {
                                send_event(&EngineEvent::PlayerUiTextBoxAdded {
                                    id: entry.id,
                                    font_path: entry.font_path.clone(),
                                    font_name: entry.font_name.clone(),
                                    text: entry.text.clone(),
                                    center_x: entry.center_x,
                                    center_y: entry.center_y,
                                    width: entry.width,
                                    height: entry.height,
                                });
                            }
                        }
                    }
                    Err(message) => {
                        send_event(&EngineEvent::Error { message });
                    }
                }
            }
            EngineCommand::Common(EngineCommandCommon::RemovePlayerUiTextBox { id }) => {
                let removed_id = if let Some(box_id) = id {
                    self.remove_player_ui_text_box(box_id)
                        .then_some(box_id)
                } else {
                    self.remove_selected_player_ui_text_box()
                };
                if let Some(box_id) = removed_id {
                    send_event(&EngineEvent::PlayerUiTextBoxRemoved { id: box_id });
                }
            }
            EngineCommand::Common(EngineCommandCommon::AddPlayerUiButton { payload }) => {
                match self.add_player_ui_button(payload) {
                    Ok(_) => {}
                    Err(message) => {
                        send_event(&EngineEvent::Error { message });
                    }
                }
            }
            EngineCommand::Common(EngineCommandCommon::RemovePlayerUiButton { id }) => {
                if let Some(button_id) = id {
                    if self.remove_player_ui_button(button_id) {
                        send_event(&EngineEvent::PlayerUiButtonRemoved { id: button_id });
                    }
                }
            }
            EngineCommand::Common(EngineCommandCommon::AddPlayerUiImage { image_path }) => {
                match self.add_player_ui_image(&image_path) {
                    Ok(_) => {}
                    Err(message) => {
                        send_event(&EngineEvent::Error { message });
                    }
                }
            }
            EngineCommand::Common(EngineCommandCommon::RemovePlayerUiImage { id }) => {
                if let Some(image_id) = id {
                    let _ = self.remove_player_ui_image(image_id);
                } else {
                    let _ = self.remove_selected_player_ui_image();
                }
            }
            EngineCommand::Common(EngineCommandCommon::SetPlayerUiObjectDraw { active }) => {
                self.set_player_ui_object_draw(active);
            }
            EngineCommand::Common(EngineCommandCommon::RemovePlayerUiObject { id }) => {
                if let Some(object_id) = id {
                    let _ = self.remove_player_ui_object(object_id);
                } else if let Some(object_id) = self.player_ui_selected_object_id {
                    let _ = self.remove_player_ui_object(object_id);
                }
            }
            EngineCommand::Common(EngineCommandCommon::SetPlayerUiHudElementProps {
                element_kind,
                id,
                locked,
                z_index,
            }) => {
                if let Err(message) = self.set_player_ui_hud_element_props(
                    &element_kind,
                    id,
                    locked,
                    z_index,
                ) {
                    send_event(&EngineEvent::Error { message });
                }
            }
            EngineCommand::Common(EngineCommandCommon::SetPlayerUiObjectStyle {
                id,
                fill_color,
                texture_path,
                clear_texture,
                live,
                skip_undo,
            }) => {
                if let Err(message) = self.set_player_ui_object_style(
                    id,
                    fill_color,
                    texture_path,
                    clear_texture,
                    live,
                    skip_undo,
                ) {
                    send_event(&EngineEvent::Error { message });
                }
            }
            EngineCommand::Common(EngineCommandCommon::SyncPlayerUiScreens { screens }) => {
                self.sync_player_ui_screens(&screens);
            }
            EngineCommand::Common(EngineCommandCommon::SetActivePlayerUiScreen { screen_id }) => {
                match screen_id.filter(|id| !id.is_empty()) {
                    Some(id) => {
                        if let Err(message) = self.set_active_player_ui_screen(&id) {
                            send_event(&EngineEvent::Error { message });
                        }
                    }
                    None => self.clear_active_player_ui_screen(),
                }
            }
            EngineCommand::Common(EngineCommandCommon::SetPreviewPlaying { playing }) => {
                if self.preview_playing == playing {
                    return;
                }

                if playing && self.player_ui_edit_active {
                    self.apply_player_ui_edit_mode(false, None, None);
                }

                self.preview_playing = playing;
                self.apply_player_ui_play_hud(playing);

                if playing {
                    self.capture_preview_editor_snapshots();
                    self.reset_play_controller_motion();
                    self.ensure_play_character_kinematic_only();
                    self.capture_play_camera_follow_offset();
                    self.active_tool = ActiveTool::None;
                    self.tool_overlay_buffer = gizmo::build_from_vertices(&self.device, &[]);
                    if self.pivot_edit_mode.is_some() {
                        self.pivot_edit_mode = None;
                    }
                    if self.logical_area_mode.is_some() {
                        self.logical_area_mode = None;
                    }

                    if self.selected_entity.take().is_some() || !self.selected_entities.is_empty() {
                        self.selected_entities.clear();
                        send_event(&EngineEvent::EntityDeselected);
                    }
                    if self.hovered_entity.take().is_some() {
                        send_event(&EngineEvent::EntityUnhovered);
                    }
                    self.hovered_gizmo_axis = None;
                    self.active_gizmo_axis = None;

                    self.script_engine.clear_control_script_cache();
                    self.restore_preview_editor_snapshots_on_enter();
                    self.capture_play_session_rotation_baselines();
                    self.run_scene_script_on_play_start();
                } else {
                    self.script_engine.reset_scene_play_state();
                    for id in self.active_model_clips.keys().copied().collect::<Vec<_>>() {
                        self.stop_model_clip(id);
                    }
                    let active: Vec<(u32, String)> = self
                        .active_animations
                        .iter()
                        .map(|(&id, a)| (id, a.animation_name.clone()))
                        .collect();
                    self.active_animations.clear();
                    for (entity_id, anim_name) in active {
                        self.script_engine.detach_animation_scripts(entity_id);
                        self.show_first_frame_of_animation(entity_id, &anim_name);
                    }
                    self.stop_audio_internal();
                    self.commit_play_session_to_editor();
                    self.clear_preview_editor_snapshots();
                    self.reset_play_controller_motion();
                    self.sync_fps_camera_mode();
                    self.emit_play_character_view_changed(false);
                }

                self.execution_overlaps.clear();

                log::info!("[preview] modo {}", if playing { "juego" } else { "editor" });
            }
            EngineCommand::Common(EngineCommandCommon::SetCtrlHeld { held }) => {
                self.ctrl_held = held;
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetCameraFov { fov_y }) => {
                self.camera.fov_y = fov_y.clamp(0.1, std::f32::consts::FRAC_PI_2 - 0.01);
                if self.has_play_character() {
                    self.emit_play_character_view_changed(false);
                }
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetPlayEditorFrustumDistance { distance }) => {
                self.fps_editor_frustum_distance = distance.clamp(0.5, 50.0);
                if self.has_play_character() {
                    self.emit_play_character_view_changed(false);
                }
            }
            EngineCommand::Common(EngineCommandCommon::SetPhysics { id, enabled, body_type }) => {
                if self.play_character_entity == Some(id)
                    || self.editor_camera_entity == Some(id)
                {
                    self.physics.remove_entity_body(id);
                } else if enabled {
                    self.set_entity_physics_from_mesh_aabb(id, &body_type);
                } else {
                    self.physics.remove_entity_body(id);
                }
                
                send_event(&EngineEvent::PhysicsChanged {
                    entity_id: id,
                    enabled,
                    body_type,
                });
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetEntityColision { id, colision }) => {
                self.entity_colision.insert(id, colision);
                if colision {
                    if self.physics.has_physics(id) {
                        let body_type = self.physics.get_body_type(id).to_string();
                        self.set_entity_physics_from_mesh_aabb(id, &body_type);
                    } else {
                        self.reconcile_entity_physics_with_mesh(id);
                    }
                } else if self.play_character_entity != Some(id) {
                    self.physics.remove_entity_body(id);
                }
                
            }
            EngineCommand::Only3d(EngineCommand3dOnly::RegisterBlueprint { blueprint }) => {
                if let Some(id) = blueprint.blueprint_id.clone().filter(|s| !s.trim().is_empty())
                {
                    log::info!(
                        "[blueprint] registrada id={id} categoría={:?}",
                        blueprint.category
                    );
                    self.blueprint_registry.insert(id, blueprint);
                } else {
                    log::warn!("[blueprint] register_blueprint sin blueprint_id");
                }
            }
            EngineCommand::Common(EngineCommandCommon::SetActiveTool {
                tool,
                preview_path,
                preview_kind,
                preview_scale,
                preview_src_rect: _,
                preview_rotation,
                preview_name,
                preview_physics_enabled,
                preview_physics_type,
                preview_entity_category,
                preview_blueprint_id,
                preview_blueprint,
            }) => {
                let preview_blueprint: Option<crate::ipc::BlueprintPlacementMeta> = preview_blueprint
                    .and_then(|v| serde_json::from_value(v).ok());
                if tool.is_empty() {
                    let was_active = !matches!(self.active_tool, ActiveTool::None);
                    if let Some(ghost_id) = self.quick_build_ghost_id.take() {
                        self.world.despawn(ghost_id);
                    }
                    self.deactivate_plane_tool();
                    self.quick_build_preview_path = None;
                    self.quick_build_preview_kind = None;
                    self.quick_build_preview_scale = None;
                    self.quick_build_blueprint = None;
                    self.active_tool = ActiveTool::None;
                    self.tool_overlay_buffer = gizmo::build_from_vertices(&self.device, &[]);
                    if was_active {
                        send_event(&EngineEvent::ToolCancelled);
                        log::info!("Herramienta cancelada");
                    }
                } else {
                    if let Some(ghost_id) = self.quick_build_ghost_id.take() {
                        self.world.despawn(ghost_id);
                    }
                    let plane_tool_cmd = matches!(tool.as_str(), "draw_collider" | "draw_execution_area");
                    if !plane_tool_cmd {
                        self.deactivate_plane_tool();
                    }
                    self.quick_build_preview_path = None;
                    self.quick_build_preview_kind = None;
                    self.quick_build_preview_scale = None;
                    self.quick_build_blueprint = None;
                    match tool.as_str() {
                        "draw_collider" => {
                            let scale = plane_tool_scale_from_preview(preview_scale);
                            self.sync_plane_tool_from_set_active(
                                PlaneToolKind::Collider,
                                [scale[0], scale[1]],
                            );
                        }
                        "draw_execution_area" => {
                            let scale = plane_tool_scale_from_preview(preview_scale);
                            self.sync_plane_tool_from_set_active(
                                PlaneToolKind::ExecutionArea,
                                [scale[0], scale[1]],
                            );
                        }
                        "quick_build_place" => {
                            self.active_tool = ActiveTool::QuickBuildPlace { cursor_world: None };
                            self.tool_overlay_buffer =
                                crate::gizmo::build_from_vertices(&self.device, &[]);
                            if let (Some(path), Some(kind)) = (
                                preview_path.as_deref(),
                                preview_kind.as_deref(),
                            ) {
                                let mut placement_meta =
                                    preview_blueprint.clone().unwrap_or_else(|| {
                                        crate::ipc::BlueprintPlacementMeta {
                                            category: preview_entity_category.clone(),
                                            model: Some(path.to_string()),
                                            colision: None,
                                            physics_type: preview_physics_type.clone(),
                                            physics_enabled: preview_physics_enabled,
                                            rotation: preview_rotation,
                                            scale: preview_scale,
                                            blueprint_id: preview_blueprint_id.clone(),
                                            template_name: preview_name.clone(),
                                            scripts: None,
                                            animations: None,
                                        }
                                    });
                                let scale = preview_scale
                                    .or(placement_meta.scale)
                                    .unwrap_or([1.0, 1.0, 1.0]);
                                placement_meta.scale = Some(scale);
                                if placement_meta.blueprint_id.is_none() {
                                    placement_meta.blueprint_id = preview_blueprint_id.clone();
                                }
                                if placement_meta.model.as_deref().map(str::trim).unwrap_or("").is_empty() {
                                    placement_meta.model = Some(path.to_string());
                                }
                                if placement_meta.category.is_none() {
                                    placement_meta.category = preview_entity_category.clone();
                                }
                                crate::ipc::enrich_blueprint_placement_meta(
                                    &self.blueprint_registry,
                                    &self.model_store,
                                    &|p| self.model_path_key(p),
                                    &mut placement_meta,
                                    path,
                                );
                                self.quick_build_blueprint = Some(
                                    crate::config_3d::quick_build::QuickBuildBlueprint::from_placement_meta(
                                        &placement_meta,
                                        path,
                                    ),
                                );
                                if let Some(ref bp) = self.quick_build_blueprint {
                                    log::info!(
                                        "[quick_build] blueprint activa categoría={:?} plantilla={}",
                                        bp.entity_category,
                                        bp.template_name
                                    );
                                }
                                self.quick_build_preview_path = Some(path.to_owned());
                                self.quick_build_preview_kind = Some(kind.to_owned());
                                self.quick_build_preview_scale = Some(scale);
                                self.quick_build_ghost_id = self.load_quick_build_ghost_3d(path, scale);
                                if self.quick_build_ghost_id.is_none() {
                                    log::warn!(
                                        "[quick_build] no se pudo crear ghost para preview: {path}"
                                    );
                                }
                                {
                                    let ghost_name = self
                                        .quick_build_blueprint
                                        .as_ref()
                                        .map(|b| b.template_name.clone());
                                    log::info!(
                                        "[quick_build] ghost listo id={:?} path={path}",
                                        self.quick_build_ghost_id
                                    );
                                    send_event(&EngineEvent::QuickBuildGhostReady {
                                        path: path.to_owned(),
                                        name: ghost_name,
                                    });
                                }
                            }
                            log::info!("Herramienta activa: construcción rápida");
                        }
                        _ => log::warn!("Herramienta desconocida: {}", tool),
                    }
                }
            }
            EngineCommand::Common(EngineCommandCommon::Undo) => {
                self.apply_undo();
            }
            EngineCommand::Common(EngineCommandCommon::Redo) => {
                self.apply_redo();
            }
            EngineCommand::Common(EngineCommandCommon::SetLocale { locale }) => {
                log::info!("[IPC] SetLocale: {}", locale);
                self.snap_locale = locale;
            }
            EngineCommand::Common(EngineCommandCommon::SetAutosave { enabled }) => {
                self.autosave_enabled = enabled;
                self.autosave_last_tick = Instant::now();
                log::info!("[autosave] {}", if enabled { "activado" } else { "desactivado" });
            }
            EngineCommand::Common(EngineCommandCommon::ExportSaveSnapshot) => {
                self.export_save_snapshot();
            }
            EngineCommand::Common(EngineCommandCommon::GetDefaultSceneName { id }) => {
                let name = rer_engine_shared::editor_defaults::default_scene_name(id);
                send_event(&EngineEvent::DefaultSceneNameReady { id, name });
            }
            EngineCommand::Only3d(EngineCommand3dOnly::CreateEditorScene { name }) => {
                self.handle_create_editor_scene(&name);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SwitchEditorScene { scene_id }) => {
                self.handle_switch_editor_scene(scene_id);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::DeleteEditorScene { scene_id }) => {
                self.handle_delete_editor_scene(scene_id);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::NotifyProjectSaved { extract_dir }) => {
                self.handle_notify_project_saved(&extract_dir);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::ClearEditorUndoRedo) => {
                self.clear_editor_undo_redo();
            }
            EngineCommand::Only3d(EngineCommand3dOnly::MergeEntities { ids }) => {
                self.merge_entities(&ids);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::ListEntityBones { entity_id }) => {
                let bones = self.list_entity_bone_names(entity_id);
                crate::ipc::send_event_silent(&crate::ipc::EngineEvent::EntityBonesList {
                    entity_id,
                    bones,
                });
            }
            EngineCommand::Only3d(EngineCommand3dOnly::ListEntitySockets { entity_id }) => {
                let sockets = self.list_entity_sockets(entity_id);
                send_event(&crate::ipc::EngineEvent::EntitySocketsList {
                    entity_id,
                    sockets,
                });
            }
            EngineCommand::Only3d(EngineCommand3dOnly::UpsertEntitySocket {
                entity_id,
                name,
                bone_name,
                local_position,
                local_rotation,
            }) => {
                use glam::{Quat, Vec3};
                let socket = crate::config_3d::entity_sockets::EntitySocket {
                    name,
                    bone_name,
                    local_position: Vec3::from_array(local_position),
                    local_rotation: Quat::from_xyzw(
                        local_rotation[0],
                        local_rotation[1],
                        local_rotation[2],
                        local_rotation[3],
                    ),
                };
                match self.upsert_entity_socket(entity_id, socket) {
                    Ok(()) => {}
                    Err(message) => {
                        send_event(&crate::ipc::EngineEvent::Error { message });
                    }
                }
            }
            EngineCommand::Only3d(EngineCommand3dOnly::RemoveEntitySocket { entity_id, name }) => {
                self.remove_entity_socket(entity_id, &name);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::AttachToSocket {
                child_ids,
                host_id,
                socket_name,
            }) => {
                self.attach_to_socket(&child_ids, host_id, &socket_name);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::DetachFromSocket { child_id }) => {
                self.detach_from_socket(child_id);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetSocketBonePickMode {
                entity_id,
                active,
            }) => {
                self.set_socket_bone_pick_mode(entity_id, active);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetBonePhysicsEditorEntity {
                entity_id,
                active,
            }) => {
                self.set_bone_physics_editor_entity(entity_id, active);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetBonePhysicsPickMode {
                entity_id,
                active,
            }) => {
                self.set_bone_physics_pick_mode(entity_id, active);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetBonePhysics {
                entity_id,
                bone_name,
                mode,
            }) => {
                use crate::config_3d::bone_physics::parse_bone_physics_mode;
                match parse_bone_physics_mode(&mode) {
                    Some(parsed) => {
                        if let Err(message) = self.set_bone_physics(entity_id, &bone_name, parsed) {
                            send_event(&crate::ipc::EngineEvent::Error { message });
                        }
                    }
                    None => {
                        send_event(&crate::ipc::EngineEvent::Error {
                            message: format!("Modo de física por hueso inválido: {mode}"),
                        });
                    }
                }
            }
            EngineCommand::Only3d(EngineCommand3dOnly::RemoveBonePhysics {
                entity_id,
                bone_name,
            }) => {
                self.remove_bone_physics(entity_id, &bone_name);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::ListEntityBonePhysics { entity_id }) => {
                let entries = self.list_entity_bone_physics(entity_id);
                crate::ipc::send_event_silent(&crate::ipc::EngineEvent::EntityBonePhysicsList {
                    entity_id,
                    entries,
                });
            }
            EngineCommand::Common(EngineCommandCommon::ResendAllModelClips) => {
                self.resend_all_model_clips_ready();
            }
            EngineCommand::Common(EngineCommandCommon::ApplyEntityRestore {
                id,
                name,
                transform,
                physics,
                control_bindings,
                omit_scale,
                skip_transform,
                ..
            }) => {
                self.apply_entity_restore_inner(
                    id,
                    name,
                    &transform,
                    physics.as_ref(),
                    control_bindings.as_ref(),
                    omit_scale,
                    skip_transform,
                );
                self.reconcile_entity_physics_with_mesh(id);
            }
            EngineCommand::Common(EngineCommandCommon::ReloadAsset { path }) => {
                log::info!("[IPC] ReloadAsset: {}", path);
                let key = self.model_path_key(&path);
                if self.static_model_cache.remove(&key).is_some() {
                    self.model_assets.remove(&key);
                    crate::config_3d::model_asset::invalidate_gltf_import_cache(
                        std::path::Path::new(&key),
                    );
                    log::info!(
                        "[hot-reload] Caché de modelo invalidada; recarga al volver a instanciar: {}",
                        path
                    );
                } else {
                    log::warn!(
                        "[hot-reload] Path no está en caché de modelos 3D: {}",
                        path
                    );
                }
            }
            EngineCommand::Common(EngineCommandCommon::SetAnimation {
                id,
                name,
                frames,
                fps,
                loop_,
                flip_horizontal,
                audio_path,
                logical_w,
                logical_h,
                scripts,
                is_cancelable,
                ..
            }) => {
                

                let fallback_logical_w = logical_w.unwrap_or(64).max(1);
                let fallback_logical_h = logical_h.unwrap_or(64).max(1);

                let measure_bounds =
                    |anim_frames: &Vec<AnimationFrameData>, fallback_w: u32, fallback_h: u32| -> (u32, u32) {
                        let mut max_w = fallback_w.max(1);
                        let mut max_h = fallback_h.max(1);
                        for frame in anim_frames {
                            max_w = max_w.max(frame.src_w.unwrap_or(fallback_w).max(1));
                            max_h = max_h.max(frame.src_h.unwrap_or(fallback_h).max(1));
                        }
                        (max_w, max_h)
                    };

                let (measured_w, measured_h) =
                    measure_bounds(&frames, fallback_logical_w, fallback_logical_h);
                let mut resolved_logical_w = measured_w.max(1);
                let mut resolved_logical_h = measured_h.max(1);

                if let Some(reference_anim) = self
                    .animations
                    .get(&id)
                    .and_then(|by_name| by_name.values().next())
                {
                    let (ref_bounds_w, ref_bounds_h) = measure_bounds(
                        &reference_anim.frames,
                        reference_anim.logical_w.max(1),
                        reference_anim.logical_h.max(1),
                    );

                    let ratio_w = (ref_bounds_w as f32) / (reference_anim.logical_w.max(1) as f32);
                    let ratio_h = (ref_bounds_h as f32) / (reference_anim.logical_h.max(1) as f32);

                    resolved_logical_w =
                        ((measured_w as f32) / ratio_w.max(0.0001)).round().max(1.0) as u32;
                    resolved_logical_h =
                        ((measured_h as f32) / ratio_h.max(0.0001)).round().max(1.0) as u32;
                }

                let audio_decoded: Option<Arc<DecodedAudio>> = audio_path.as_deref().and_then(|p| {
                    let bytes = match std::fs::read(p) {
                        Ok(b) => b,
                        Err(e) => {
                            log::warn!("[SetAnimation] error leyendo audio {}: {}", p, e);
                            return None;
                        }
                    };
                    let cursor = std::io::Cursor::new(bytes);
                    let decoder = match rodio::Decoder::new(cursor) {
                        Ok(d) => d,
                        Err(e) => {
                            log::warn!("[SetAnimation] error decodificando audio {}: {}", p, e);
                            return None;
                        }
                    };
                    let channels = decoder.channels().get();
                    let sample_rate = decoder.sample_rate().get();
                    let samples: Vec<f32> = decoder.collect();

                    Some(Arc::new(DecodedAudio {
                        samples,
                        channels,
                        sample_rate,
                    }))
                });

                for frame in &frames {
                    self.preload_anim_frame_with_rect(
                        &frame.path,
                        frame.src_x
                            .zip(frame.src_y)
                            .zip(frame.src_w.zip(frame.src_h))
                            .map(|((x, y), (w, h))| (x, y, w, h)),
                    );
                }

                self.animations
                    .entry(id)
                    .or_insert_with(HashMap::new)
                    .insert(
                        name.clone(),
                        AnimationState {
                            frames,
                            fps,
                            loop_,
                            flip_horizontal,
                            audio_decoded,
                            logical_w: resolved_logical_w,
                            logical_h: resolved_logical_h,
                            scripts,
                            is_cancelable,
                        },
                    );
                self.default_animation_by_entity
                    .entry(id)
                    .or_insert(name.clone());
                send_event(&EngineEvent::AnimationLogicalResolved {
                    id,
                    name: name.clone(),
                    logical_w: resolved_logical_w,
                    logical_h: resolved_logical_h,
                });
                
            }
            EngineCommand::Common(EngineCommandCommon::RemoveAnimation { id, name }) => {
                log::info!("[IPC] RemoveAnimation: entity_id={}, name='{}'", id, name);

                if let Some(active) = self.active_animations.get(&id) {
                    if active.animation_name == name {
                        self.active_animations.remove(&id);
                        self.restore_animation_frame(id);
                        self.script_engine.detach_animation_scripts(id);
                        send_event(&EngineEvent::AnimationFinished { entity_id: id });
                    }
                }

                if let Some(entity_anims) = self.animations.get_mut(&id) {
                    entity_anims.remove(&name);
                    

                    if entity_anims.is_empty() {
                        self.animations.remove(&id);
                        self.default_animation_by_entity.remove(&id);
                    }
                }

                if let Some(default) = self.default_animation_by_entity.get(&id) {
                    if default == &name {
                        let new_default = self
                            .animations
                            .get(&id)
                            .and_then(|m| m.keys().next().cloned());
                        match new_default {
                            Some(new_name) => {
                                self.default_animation_by_entity.insert(id, new_name.clone());
                                
                            }
                            None => {
                                self.default_animation_by_entity.remove(&id);
                                
                            }
                        }
                    }
                }
            }
            EngineCommand::Common(EngineCommandCommon::SetDefaultAnimation { id, name }) => {
                if self.model_animation_bindings.contains_key(&id) {
                    self.set_default_model_clip(id, &name);
                    
                    return;
                }
                let exists = self
                    .animations
                    .get(&id)
                    .map(|m| m.contains_key(&name))
                    .unwrap_or(false);
                if exists {
                    self.default_animation_by_entity.insert(id, name.clone());
                    
                } else {
                    log::warn!(
                        "[animation] set_default_animation ignorado: '{}' no existe en entidad {}",
                        name,
                        id
                    );
                }
            }
            EngineCommand::Common(EngineCommandCommon::PlayAnimation { id, name, loop_ }) => {
                

                if self.model_animation_bindings.contains_key(&id) {
                    self.play_model_clip(id, &name, loop_);
                    return;
                }

                if let Some(active) = self.active_animations.get(&id) {
                    if !active.finished {
                        let current_name = active.animation_name.clone();
                        let is_cancelable = self
                            .animations
                            .get(&id)
                            .and_then(|m| m.get(&current_name))
                            .map(|a| a.is_cancelable)
                            .unwrap_or(true);
                        if !is_cancelable {
                            
                            return;
                        }
                    }
                }

                let anim_opt = self.animations.get(&id).and_then(|m| m.get(&name)).cloned();

                match anim_opt {
                    None => log::warn!("[IPC] Animación '{}' no encontrada para entidad {}", name, id),
                    Some(anim) => {
                        self.active_animations.remove(&id);

                        if let Some(t) = self.world.get::<Transform>(id).cloned() {
                            self.anim_saved_transforms
                                .entry(id)
                                .and_modify(|saved| {
                                    saved.0 = t.position;
                                })
                                .or_insert((t.position, t.scale));
                        }

                        let frame_start = Instant::now();
                        let effective_flip = self.resolve_animation_flip(id, &anim);

                        if let Some(first_frame) = anim.frames.first() {
                            let (px, py) =
                                first_frame.resolved_pivot(anim.logical_w, anim.logical_h);
                            self.play_animation_frame(
                                id,
                                &first_frame.path,
                                px,
                                py,
                                anim.logical_w,
                                anim.logical_h,
                                first_frame
                                    .src_x
                                    .zip(first_frame.src_y)
                                    .zip(first_frame.src_w.zip(first_frame.src_h))
                                    .map(|((x, y), (w, h))| (x, y, w, h)),
                                effective_flip,
                            );
                        }

                        if let Some(ref audio_decoded) = anim.audio_decoded {
                            self.play_audio_internal(Arc::clone(audio_decoded), anim.loop_);
                        }

                        self.script_engine.detach_animation_scripts(id);
                        for script in &anim.scripts {
                            let anim_path = format!("$anim$::{}::{}", name, script.name);
                            if let Err(e) =
                                self.script_engine.attach_script(id, &anim_path, &script.source)
                            {
                                log::error!(
                                    "[scripting] Error cargando script de animación '{}': {}",
                                    anim_path,
                                    e
                                );
                            }
                        }

                        self.active_animations.insert(
                            id,
                            ActiveAnimation {
                                animation_name: name.clone(),
                                current_frame: 0,
                                last_frame_time: frame_start,
                                fps: anim.fps,
                                finished: false,
                            },
                        );
                        self.emit_entity_animation_play_state(id);
                    }
                }
            }
            EngineCommand::Common(EngineCommandCommon::StopAnimation { id }) => {
                log::info!("[IPC] StopAnimation: entity_id={}", id);
                if self.model_animation_bindings.contains_key(&id) {
                    self.stop_model_clip(id);
                    return;
                }
                let stopped_animation_name =
                    self.active_animations.remove(&id).map(|a| a.animation_name);
                if self.preview_playing {
                    let fallback_name = self
                        .default_animation_by_entity
                        .get(&id)
                        .cloned()
                        .or_else(|| {
                            self.animations
                                .get(&id)
                                .and_then(|m| m.keys().next().cloned())
                        });
                    if let Some(name) = fallback_name {
                        self.show_first_frame_of_animation(id, &name);
                    } else {
                        self.restore_animation_frame(id);
                    }
                } else if let Some(name) = stopped_animation_name {
                    self.show_first_frame_of_animation(id, &name);
                } else {
                    self.restore_animation_frame(id);
                }
                self.stop_audio_internal();
                self.script_engine.detach_animation_scripts(id);
                self.emit_entity_animation_play_state(id);
                send_event(&EngineEvent::AnimationFinished { entity_id: id });
                log::info!("[animation] Stopped for entity {}", id);
            }
            EngineCommand::Common(EngineCommandCommon::QueryEntityAnimationPlayState {
                entity_id,
            }) => {
                self.emit_entity_animation_play_state(entity_id);
            }
            EngineCommand::Common(EngineCommandCommon::LoadSceneVisualScript { scene_id, source }) => {
                log::info!("[IPC] LoadSceneVisualScript: scene_id={}", scene_id);
                if let Err(e) = self.handle_load_scene_visual_script(scene_id, &source) {
                    log::error!("[scene_script] Error: {e}");
                    send_event(&EngineEvent::Error {
                        message: format!("Error en script de escena: {e}"),
                    });
                }
            }
            EngineCommand::Common(EngineCommandCommon::LoadScript { id, path, source }) => {
                log::info!("[IPC] LoadScript: entity_id={} path={}", id, path);
                if let Err(e) = self.script_engine.attach_script(id, &path, &source) {
                    log::error!("[scripting] Error cargando script '{}': {}", path, e);
                    send_event(&EngineEvent::Error {
                        message: format!("Error en script '{path}': {e}"),
                    });
                } else {
                    use crate::entity_save_meta::ScriptSourceRecord;
                    let list = self
                        .save_registry
                        .script_sources
                        .entry(id)
                        .or_insert_with(Vec::new);
                    if let Some(existing) = list.iter_mut().find(|s| s.name == path) {
                        existing.source = source;
                    } else {
                        list.push(ScriptSourceRecord {
                            name: path,
                            source,
                        });
                    }
                }
            }
            EngineCommand::Common(EngineCommandCommon::SetControlBindings { id, bindings }) => {
                if bindings.keyboard_mouse.is_empty() && bindings.gamepad.is_empty() {
                    self.control_bindings_by_entity.remove(&id);
                } else {
                    self.control_bindings_by_entity.insert(id, bindings);
                }
                self.script_engine.clear_control_script_cache();
            }
            EngineCommand::Common(EngineCommandCommon::RunControlScript {
                id,
                control_key,
                path,
                source,
            }) => {
                self.execute_control_script(id, &control_key, &path, &source);
            }
            EngineCommand::Common(EngineCommandCommon::UnloadScript { id }) => {
                log::info!("[IPC] UnloadScript: entity_id={}", id);
                self.script_engine.detach_entity(id);
                self.save_registry.script_sources.remove(&id);
            }
            EngineCommand::Common(EngineCommandCommon::LoadSprite { path, name }) => {
                match std::fs::read(&path) {
                    Ok(bytes) => {
                        use image::ImageReader;
                        match ImageReader::new(std::io::Cursor::new(&bytes))
                            .with_guessed_format()
                            .map_err(|e| e.to_string())
                            .and_then(|r| r.decode().map_err(|e| e.to_string()))
                        {
                            Ok(img) => {
                                let img = img.to_rgba8();
                                let (w, h) = img.dimensions();
                                self.sprite_store.insert(path.clone(), (name.clone(), w, h));
                                send_event(&EngineEvent::SpriteLoaded {
                                    path,
                                    name,
                                    width: w,
                                    height: h,
                                });
                                
                            }
                            Err(e) => {
                                log::error!("[sprite] error decodificando {}: {}", path, e);
                                send_event(&EngineEvent::Error {
                                    message: format!("Error al decodificar sprite: {e}"),
                                });
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("[sprite] error leyendo {}: {}", path, e);
                        send_event(&EngineEvent::Error {
                            message: format!("No se pudo leer el sprite: {e}"),
                        });
                    }
                }
            }
            EngineCommand::Common(EngineCommandCommon::RemoveSprite { path }) => {
                if self.sprite_store.remove(&path).is_some() {
                    send_event(&EngineEvent::SpriteRemoved { path: path.clone() });
                    log::info!("[sprite] eliminado: {}", path);
                } else {
                    log::warn!("[sprite] intento de eliminar sprite inexistente: {}", path);
                }
            }
            EngineCommand::Common(EngineCommandCommon::GetSpritesList) => {
                let sprites: Vec<crate::ipc::SpriteInfo> = self
                    .sprite_store
                    .iter()
                    .map(|(path, &(ref name, w, h))| crate::ipc::SpriteInfo {
                        path: path.clone(),
                        name: name.clone(),
                        width: w,
                        height: h,
                    })
                    .collect();
                let count = sprites.len();
                send_event(&EngineEvent::SpritesList { sprites });
                log::info!("[sprite] lista enviada: {} sprites", count);
            }
            EngineCommand::Common(EngineCommandCommon::LoadSound { path, name }) => {
                self.sound_store.insert(path.clone(), name.clone());
                send_event(&EngineEvent::SoundLoaded {
                    path: path.clone(),
                    name: name.clone(),
                });
                
            }
            EngineCommand::Common(EngineCommandCommon::RemoveSound { path }) => {
                if self.sound_store.remove(&path).is_some() {
                    send_event(&EngineEvent::SoundRemoved { path: path.clone() });
                    log::info!("[sound] eliminado: {}", path);
                } else {
                    log::warn!("[sound] intento de eliminar sonido inexistente: {}", path);
                }
            }
            EngineCommand::Common(EngineCommandCommon::GetSoundsList) => {
                let sounds: Vec<crate::ipc::SoundInfo> = self
                    .sound_store
                    .iter()
                    .map(|(path, name)| crate::ipc::SoundInfo {
                        path: path.clone(),
                        name: name.clone(),
                    })
                    .collect();
                let count = sounds.len();
                send_event(&EngineEvent::SoundsList { sounds });
                log::info!("[sound] lista enviada: {} sonidos", count);
            }
            EngineCommand::Common(EngineCommandCommon::LoadFont { path, name }) => {
                match validate_font_file(&path) {
                    Ok(()) => {
                        self.font_store.insert(path.clone(), name.clone());
                        send_event(&EngineEvent::FontLoaded {
                            path: path.clone(),
                            name: name.clone(),
                        });
                        
                    }
                    Err(e) => {
                        log::error!("[font] error cargando {}: {}", path, e);
                        send_event(&EngineEvent::Error {
                            message: format!("Error al cargar fuente: {e}"),
                        });
                    }
                }
            }
            EngineCommand::Common(EngineCommandCommon::RemoveFont { path }) => {
                if self.font_store.remove(&path).is_some() {
                    send_event(&EngineEvent::FontRemoved { path: path.clone() });
                    log::info!("[font] eliminada: {}", path);
                } else {
                    log::warn!("[font] intento de eliminar fuente inexistente: {}", path);
                }
            }
            EngineCommand::Common(EngineCommandCommon::GetFontsList) => {
                let fonts: Vec<crate::ipc::FontInfo> = self
                    .font_store
                    .iter()
                    .map(|(path, name)| crate::ipc::FontInfo {
                        path: path.clone(),
                        name: name.clone(),
                    })
                    .collect();
                let count = fonts.len();
                send_event(&EngineEvent::FontsList { fonts });
                log::info!("[font] lista enviada: {} fuentes", count);
            }
            EngineCommand::Common(EngineCommandCommon::LoadHudImage { path, name }) => {
                match crate::hud_image_asset::validate_hud_image_file(&path) {
                    Ok((width_px, height_px)) => {
                        self.hud_image_store.insert(
                            path.clone(),
                            crate::hud_image_asset::HudImageAssetMeta {
                                name: name.clone(),
                                width_px,
                                height_px,
                            },
                        );
                        send_event(&EngineEvent::HudImageLoaded {
                            path: path.clone(),
                            name: name.clone(),
                            width: width_px,
                            height: height_px,
                        });
                        
                    }
                    Err(e) => {
                        log::error!("[hud-image] error cargando {}: {}", path, e);
                        send_event(&EngineEvent::Error {
                            message: format!("Error al cargar imagen HUD: {e}"),
                        });
                    }
                }
            }
            EngineCommand::Common(EngineCommandCommon::RemoveHudImage { path }) => {
                if self.hud_image_store.remove(&path).is_some() {
                    send_event(&EngineEvent::HudImageRemoved { path: path.clone() });
                    log::info!("[hud-image] eliminada: {}", path);
                } else {
                    log::warn!("[hud-image] intento de eliminar imagen inexistente: {}", path);
                }
            }
            EngineCommand::Common(EngineCommandCommon::GetHudImagesList) => {
                let images: Vec<crate::ipc::HudImageInfo> = self
                    .hud_image_store
                    .iter()
                    .map(|(path, meta)| crate::ipc::HudImageInfo {
                        path: path.clone(),
                        name: meta.name.clone(),
                        width: meta.width_px,
                        height: meta.height_px,
                    })
                    .collect();
                let count = images.len();
                send_event(&EngineEvent::HudImagesList { images });
                log::info!("[hud-image] lista enviada: {} imágenes", count);
            }
            EngineCommand::Common(EngineCommandCommon::LoadBackgroundAsset { path, name }) => {
                self.background_store.insert(path.clone(), name.clone());
                send_event(&EngineEvent::BackgroundAssetLoaded {
                    path: path.clone(),
                    name: name.clone(),
                });
                
            }
            EngineCommand::Common(EngineCommandCommon::RemoveBackgroundAsset { path }) => {
                if self.background_store.remove(&path).is_some() {
                    send_event(&EngineEvent::BackgroundAssetRemoved { path: path.clone() });
                    log::info!("[background] eliminado: {}", path);
                } else {
                    log::warn!("[background] intento de eliminar fondo inexistente: {}", path);
                }
            }
            EngineCommand::Common(EngineCommandCommon::GetBackgroundsList) => {
                let backgrounds: Vec<crate::ipc::BackgroundInfo> = self
                    .background_store
                    .iter()
                    .map(|(path, name)| crate::ipc::BackgroundInfo {
                        path: path.clone(),
                        name: name.clone(),
                    })
                    .collect();
                let count = backgrounds.len();
                send_event(&EngineEvent::BackgroundsList { backgrounds });
                log::info!("[background] lista enviada: {} fondos", count);
            }
            EngineCommand::Common(EngineCommandCommon::Shutdown) => {}
        }
    }
}

/// Valida extensión y contenido de un archivo de fuente (.ttf / .otf).
fn validate_font_file(path: &str) -> Result<(), String> {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if ext != "ttf" && ext != "otf" {
        return Err("Se esperaba un archivo .ttf u .otf".to_string());
    }
    let bytes =
        std::fs::read(path).map_err(|e| format!("No se pudo leer la fuente: {e}"))?;
    ttf_parser::Face::parse(&bytes, 0).map_err(|e| format!("Archivo de fuente inválido: {e}"))?;
    Ok(())
}
