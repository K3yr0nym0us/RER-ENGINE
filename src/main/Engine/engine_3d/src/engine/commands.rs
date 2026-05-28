use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use glam::Vec3 as GlamVec3;
use rodio;
use rodio::Source as RodioSource;
use winit::dpi::PhysicalSize;

use crate::config_compat::ActiveTool;
use crate::ecs::{NameComponent, Transform};
use crate::gizmo;
use crate::ipc::{send_event, AnimationFrameData, EngineCommand, EngineEvent};

use super::{ActiveAnimation, AnimationState, DecodedAudio, State, UndoAction};

impl State {
    /// Reconstruye el vertex buffer de la cuadrícula con la configuración actual.
    pub(crate) fn rebuild_grid(&mut self) {
        // La cuadrícula 2D no se dibuja en el binario 3D; `grid_config` sigue usándose para snap/quick-build.
    }

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
    }

    pub fn push_undo_transforms(&mut self, items: Vec<(u32, [f32; 3], [f32; 4], [f32; 3])>) {
        if items.is_empty() {
            return;
        }
        if !self.is_applying_undo {
            self.redo_stack.clear();
        }
        self.undo_stack.push(UndoAction::RestoreTransforms { items });
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
                self.handle_command(EngineCommand::SetTransform {
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
                });
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
                    self.handle_command(EngineCommand::SetTransform {
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
                    });
                }
            }
            UndoAction::RemoveEntity { snapshot } => {
                let id = snapshot.id;
                self.handle_command(EngineCommand::RemoveEntity { id });
                self.redo_stack
                    .push(UndoAction::RestoreEntity { snapshot });
            }
            UndoAction::RestoreEntity { .. } => {}
        }
        self.is_applying_undo = false;
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
                self.handle_command(EngineCommand::SetTransform {
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
                });
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
                    self.handle_command(EngineCommand::SetTransform {
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
                    });
                }
            }
            UndoAction::RestoreEntity { snapshot } => {
                self.restore_entity_from_undo_snapshot(&snapshot);
                self.undo_stack
                    .push(UndoAction::RemoveEntity { snapshot });
            }
            UndoAction::RemoveEntity { snapshot } => {
                let id = snapshot.id;
                self.handle_command(EngineCommand::RemoveEntity { id });
                self.undo_stack
                    .push(UndoAction::RestoreEntity { snapshot });
            }
        }
        self.is_applying_undo = false;
    }

    pub fn handle_command(&mut self, cmd: EngineCommand) {
        match cmd {
            EngineCommand::Ping => {
                send_event(&EngineEvent::Pong);
            }
            EngineCommand::SetClearColor { r, g, b } => {
                self.clear_color = wgpu::Color { r, g, b, a: 1.0 };
            }
            EngineCommand::Resize { width, height } => {
                self.resize(PhysicalSize::new(width, height));
            }
            EngineCommand::SetBounds {
                x,
                y,
                width,
                height,
                ..
            } => {
                let _ = self.window.set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
                self.resize(PhysicalSize::new(width, height));
                let _ = self
                    .window
                    .request_inner_size(winit::dpi::PhysicalSize::new(width, height));
            }
            EngineCommand::LoadModel {
                path,
                single_instance,
                entity_category,
            } => {
                let category = entity_category.as_deref();
                if single_instance.unwrap_or(false) {
                    self.load_model_single(&path, category);
                } else {
                    self.load_model(&path, category);
                }
            }
            EngineCommand::SpawnCachedModel {
                path,
                name,
                position,
                rotation,
                scale,
                entity_category,
                blueprint_id,
                physics_enabled,
                physics_type,
            } => {
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
                ) {
                    send_event(&EngineEvent::Error { message });
                }
            }
            EngineCommand::SpawnQuickBuildInstance {
                position,
                rotation,
                scale,
            } => {
                let _ = self.spawn_quick_build_instance_at(position, rotation, scale);
            }
            EngineCommand::PlaceQuickBuildAtCursor { pixel_x, pixel_y } => {
                let pixels = match (pixel_x, pixel_y) {
                    (Some(x), Some(y)) => Some((x, y)),
                    _ => None,
                };
                self.place_quick_build_at_cursor(pixels);
            }
            EngineCommand::ReplaceEntityModel { id, path } => {
                self.replace_entity_model(id, &path);
            }
            EngineCommand::LoadModelAsset { path, name } => {
                self.register_model_asset(&path, &name);
            }
            EngineCommand::RemoveModelAsset { path } => {
                if self.model_store.remove(&path).is_some() {
                    self.invalidate_static_model_cache(&path);
                    send_event(&EngineEvent::ModelAssetRemoved { path: path.clone() });
                    log::info!("[model] eliminado de recursos: {}", path);
                } else {
                    log::warn!("[model] intento de eliminar modelo inexistente: {}", path);
                }
            }
            EngineCommand::GetModelsList => {
                let models: Vec<crate::ipc::ModelInfo> = self
                    .model_store
                    .iter()
                    .map(|(path, name)| crate::ipc::ModelInfo {
                        path: path.clone(),
                        name: name.clone(),
                    })
                    .collect();
                let count = models.len();
                send_event(&EngineEvent::ModelsList { models });
                log::info!("[model] lista enviada: {} modelos", count);
            }
            EngineCommand::SpawnEditorBox {
                name,
                position,
                scale,
            } => {
                self.spawn_editor_box(&name, position, scale);
            }
            EngineCommand::SpawnSun {
                name,
                position,
                scale,
            } => {
                self.spawn_sun(&name, position, scale);
            }
            EngineCommand::SpawnGround { position, scale } => {
                self.spawn_ground_plane(position, scale);
            }
            EngineCommand::SetDirectionalLight {
                ambient,
                intensity,
                shadow_darkness,
            } => {
                self.apply_directional_light_settings(ambient, intensity, shadow_darkness);
            }
            EngineCommand::SetPlayCharacterSpawn {
                position,
                yaw,
                pitch,
            } => {
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
            EngineCommand::SetPlayCharacterView {
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
            } => {
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
            EngineCommand::SetTransform {
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
            } => {
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
                        log::debug!(
                            "set_transform: rotación del jugador ignorada en preview/play"
                        );
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
                    if let Some(t) = self.world.get::<Transform>(id).cloned() {
                        if self.physics.has_physics(id)
                            && (position_vec.is_some() || rotation_changed || scale_vec.is_some())
                        {
                            let half = [
                                (t.scale.x * 0.5).max(0.01),
                                (t.scale.y * 0.5).max(0.01),
                                (t.scale.z * 0.5).max(0.01),
                            ];
                            let pos = t.position.to_array();
                            let model_path = self
                                .save_registry
                                .meta
                                .get(&id)
                                .map(|m| m.path.as_str())
                                .unwrap_or("");
                            let body_pos = crate::config_3d::physics_body_position_for_model_path(
                                model_path,
                                pos,
                                half,
                            );
                            self.physics
                                .sync_entity_physics_from_transform(id, body_pos, half);
                        }
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
            }
            EngineCommand::SetEntityName { id, name, force } => {
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
            EngineCommand::SetScene { scene, save_path } => match scene.as_str() {
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
                    self.setup_2d_platformer();
                }
                _ => log::info!("SetScene: escena '{}' no reconocida", scene),
            },
            EngineCommand::LoadScenario { path, track_undo } => {
                self.load_scenario(&path);
                if track_undo.unwrap_or(false) {
                    if let Some(&id) = self.scenario_entities.last() {
                        self.push_remove_entity_undo(id);
                        log::info!("[quick_build] escenario {id} registrado en undo");
                    }
                }
            }
            EngineCommand::SetScenarioScale { id, scale } => {
                let marker = self.world.get::<crate::config_compat::ScenarioMarker>(id).cloned();
                if let Some(m) = marker {
                    let aspect = m.img_width as f32 / m.img_height.max(1) as f32;
                    let new_h = m.base_world_h * scale.clamp(0.05, 20.0);
                    let new_w = new_h * aspect;
                    if let Some(t) = self.world.get_mut::<Transform>(id) {
                        t.scale = GlamVec3::new(new_w, new_h, 1.0);
                    }
                }
            }
            EngineCommand::LoadCharacter { path, track_undo } => {
                self.load_character(&path);
                if track_undo.unwrap_or(false) {
                    if let Some(&id) = self.character_entities.last() {
                        self.push_remove_entity_undo(id);
                        log::info!("[quick_build] personaje {id} registrado en undo");
                    }
                }
            }
            EngineCommand::SetCharacterScale { id, scale } => {
                self.set_character_scale(id, scale);
            }
            EngineCommand::PlayAnimationFrame {
                id,
                path,
                pivot_x,
                pivot_y,
                logical_w,
                logical_h,
                src_x,
                src_y,
                src_w,
                src_h,
            } => {
                if self.pivot_edit_mode.is_some() {
                    return;
                }
                self.play_animation_frame(
                    id,
                    &path,
                    pivot_x,
                    pivot_y,
                    logical_w,
                    logical_h,
                    src_x.zip(src_y)
                        .zip(src_w.zip(src_h))
                        .map(|((x, y), (w, h))| (x, y, w, h)),
                    false,
                );
            }
            EngineCommand::RestoreAnimationFrame { id } => {
                self.restore_animation_frame(id);
            }
            EngineCommand::SetPivotEditMode {
                id,
                frame_path,
                pivot_x,
                pivot_y,
            } => {
                self.enter_pivot_edit_mode(id, &frame_path, pivot_x, pivot_y);
            }
            EngineCommand::CancelPivotEditMode => {
                self.cancel_pivot_edit_mode();
            }
            EngineCommand::SetLogicalAreaMode { id, w, h } => {
                self.enter_logical_area_mode(id, w, h);
            }
            EngineCommand::CancelLogicalAreaMode => {
                self.cancel_logical_area_mode();
            }
            EngineCommand::PlayAudio { path, loop_ } => {
                let decoded = std::fs::read(&path).ok().and_then(|b| {
                    let cursor = std::io::Cursor::new(b);
                    rodio::Decoder::new(cursor).ok().map(|dec| {
                        let ch = dec.channels();
                        let sr = dec.sample_rate();
                        let s: Vec<i16> = dec.collect();
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
            EngineCommand::StopAudio => {
                self.stop_audio_internal();
                log::info!("[audio] detenido por comando externo");
            }
            EngineCommand::RemoveEntity { id } => {
                let removed_kind = if self.scenario_entities.contains(&id) {
                    "scenario"
                } else if self.character_entities.contains(&id) {
                    "character"
                } else if self.collider_entities.contains(&id) {
                    "collider"
                } else if self.execution_area_entities.contains(&id) {
                    "execution_area"
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
                self.save_registry.remove_entity(id);
                self.entity_blueprint_ids.remove(&id);
                if self.sun_entity == Some(id) {
                    self.sun_entity = None;
                }
                self.world.despawn(id);
                send_event(&EngineEvent::EntityRemoved {
                    id,
                    kind: removed_kind.to_string(),
                });
            }
            EngineCommand::SetWorldSize {
                width,
                height,
                depth,
            } => {
                self.set_world_bounds_3d_size(width, height, depth);
                self.clamp_play_character_camera_to_bounds();
            }
            EngineCommand::SetGravity { gravity } => {
                let magnitude = gravity.abs();
                self.physics.set_gravity(-magnitude);
                log::info!("[physics] Gravedad actualizada: {:.2} m/s²", magnitude);
            }
            EngineCommand::SetGridVisible { visible } => {
                self.grid_config.visible = visible;
                self.rebuild_grid();
            }
            EngineCommand::SetGridCellSize { size } => {
                self.grid_config.cell_size = size.clamp(0.05, 100.0);
                self.rebuild_grid();
            }
            EngineCommand::SetTargetFps { fps } => {
                self.target_fps = fps.clamp(1, 1000);
                log::info!("[render] Límite de FPS actualizado: {}", self.target_fps);
            }
            EngineCommand::SetPreviewPlaying { playing } => {
                if self.preview_playing == playing {
                    return;
                }

                self.preview_playing = playing;

                if playing {
                    self.capture_preview_editor_snapshots();
                    self.reset_play_controller_motion();
                    self.ensure_play_character_kinematic_only();
                    self.capture_play_camera_follow_offset();
                    self.active_tool = ActiveTool::None;
                    self.tool_overlay_buffer = gizmo::build_from_vertices(&self.device, &[]);
                    if self.pivot_edit_mode.is_some() {
                        self.cancel_pivot_edit_mode();
                    }
                    if self.logical_area_mode.is_some() {
                        self.cancel_logical_area_mode();
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
                } else {
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
            EngineCommand::SetCtrlHeld { held } => {
                self.ctrl_held = held;
            }
            EngineCommand::SetCamera2d { .. } => {
                log::warn!("SetCamera2d ignorado en rer_engine_3d (solo editor 3D)");
            }
            EngineCommand::SetCameraFov { fov_y } => {
                self.camera.fov_y = fov_y.clamp(0.1, std::f32::consts::FRAC_PI_2 - 0.01);
                if self.has_play_character() {
                    self.emit_play_character_view_changed(false);
                }
            }
            EngineCommand::SetFpsEditorFrustumDistance { distance } => {
                self.fps_editor_frustum_distance = distance.clamp(0.5, 50.0);
                if self.has_play_character() {
                    self.emit_play_character_view_changed(false);
                }
            }
            EngineCommand::LoadBackground { path } => {
                self.background_path = Some(path.clone());
                self.load_background(&path);
            }
            EngineCommand::ClearBackground => {
                self.background_path = None;
                self.clear_background();
            }
            EngineCommand::SetPhysics { id, enabled, body_type } => {
                let (pos, half) = if let Some(t) = self.world.get::<Transform>(id) {
                    (t.position.to_array(), (t.scale.abs() * 0.5).to_array())
                } else {
                    ([0.0_f32; 3], [0.5_f32; 3])
                };
                if self.play_character_entity == Some(id) {
                    self.physics.remove_entity_body(id);
                } else {
                    self.physics.set_entity_physics(id, enabled, &body_type, pos, half);
                }
                log::debug!(
                    "Física {}: entidad {} tipo='{}'",
                    if enabled { "activada" } else { "desactivada" },
                    id,
                    body_type
                );
                send_event(&EngineEvent::PhysicsChanged {
                    entity_id: id,
                    enabled,
                    body_type,
                });
            }
            EngineCommand::SetActiveTool {
                tool,
                preview_path,
                preview_kind,
                preview_scale,
                preview_src_rect,
                preview_rotation,
                preview_name,
                preview_physics_enabled,
                preview_physics_type,
                preview_entity_category,
                preview_blueprint_id,
            } => {
                if tool.is_empty() {
                    let was_active = !matches!(self.active_tool, ActiveTool::None);
                    if let Some(ghost_id) = self.quick_build_ghost_id.take() {
                        self.world.despawn(ghost_id);
                    }
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
                    self.quick_build_preview_path = None;
                    self.quick_build_preview_kind = None;
                    self.quick_build_preview_scale = None;
                    self.quick_build_blueprint = None;
                    match tool.as_str() {
                        "draw_collider" => {
                            self.active_tool = ActiveTool::DrawCollider {
                                points_world: Vec::new(),
                                cursor_world: None,
                            };
                            log::info!("Herramienta activa: dibujar colisionador (4 puntos)");
                        }
                        "draw_execution_area" => {
                            self.active_tool = ActiveTool::DrawExecutionArea {
                                points_world: Vec::new(),
                                cursor_world: None,
                            };
                            log::info!("Herramienta activa: dibujar área de ejecución (4 puntos)");
                        }
                        "quick_build_place" => {
                            self.active_tool = ActiveTool::QuickBuildPlace { cursor_world: None };
                            self.tool_overlay_buffer =
                                crate::gizmo::build_from_vertices(&self.device, &[]);
                            if let (Some(path), Some(kind), Some(scale)) = (
                                preview_path.as_deref(),
                                preview_kind.as_deref(),
                                preview_scale,
                            ) {
                                let is_environment =
                                    preview_entity_category.as_deref() == Some("environment");
                                let physics_enabled = if is_environment {
                                    true
                                } else {
                                    preview_physics_enabled.unwrap_or(false)
                                };
                                let physics_type = if is_environment {
                                    "static".to_string()
                                } else {
                                    preview_physics_type
                                        .unwrap_or_else(|| "static".to_string())
                                };
                                self.quick_build_blueprint =
                                    Some(crate::config_3d::quick_build::QuickBuildBlueprint {
                                        name: preview_name
                                            .unwrap_or_else(|| "Objeto".to_string()),
                                        rotation: preview_rotation
                                            .unwrap_or([0.0, 0.0, 0.0, 1.0]),
                                        physics_enabled,
                                        physics_type,
                                        entity_category: preview_entity_category,
                                        blueprint_id: preview_blueprint_id,
                                    });
                                self.quick_build_preview_path = Some(path.to_owned());
                                self.quick_build_preview_kind = Some(kind.to_owned());
                                self.quick_build_preview_scale = Some(scale);
                                self.quick_build_ghost_id = self.load_quick_build_ghost(
                                    path,
                                    kind,
                                    scale,
                                    preview_src_rect,
                                );
                                if self.quick_build_ghost_id.is_none() {
                                    log::warn!(
                                        "[quick_build] no se pudo crear ghost para preview: {path}"
                                    );
                                }
                                {
                                    let ghost_name = self
                                        .quick_build_blueprint
                                        .as_ref()
                                        .map(|b| b.name.clone());
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
            EngineCommand::CreateColliderFromPoints { .. } => {
                log::warn!("CreateColliderFromPoints no disponible en rer_engine_3d");
            }
            EngineCommand::CreateExecutionAreaFromPoints { .. } => {
                log::warn!("CreateExecutionAreaFromPoints no disponible en rer_engine_3d");
            }
            EngineCommand::Undo => {
                if self.undo_last_tool_step_2d() {
                    return;
                }
                self.apply_undo();
            }
            EngineCommand::Redo => {
                self.apply_redo();
            }
            EngineCommand::SetLocale { locale } => {
                log::info!("[IPC] SetLocale: {}", locale);
                self.snap_locale = locale;
            }
            EngineCommand::SetAutosave { enabled } => {
                self.autosave_enabled = enabled;
                self.autosave_last_tick = Instant::now();
                log::info!("[autosave] {}", if enabled { "activado" } else { "desactivado" });
            }
            EngineCommand::ExportSaveSnapshot => {
                self.export_save_snapshot();
            }
            EngineCommand::GetDefaultSceneName { id } => {
                let name = rer_engine_shared::editor_defaults::default_scene_name(id);
                send_event(&EngineEvent::DefaultSceneNameReady { id, name });
            }
            EngineCommand::ApplyEntityRestore {
                id,
                name,
                transform,
                physics,
                control_bindings,
                omit_scale,
                skip_transform,
            } => {
                self.apply_entity_restore_inner(
                    id,
                    name,
                    &transform,
                    physics.as_ref(),
                    control_bindings.as_ref(),
                    omit_scale,
                    skip_transform,
                );
            }
            EngineCommand::ReloadAsset { path } => {
                log::info!("[IPC] ReloadAsset: {}", path);
                let key = self.model_path_key(&path);
                if self.static_model_cache.remove(&key).is_some() {
                    self.model_assets.remove(&key);
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
            EngineCommand::SetAnimation {
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
            } => {
                log::debug!(
                    "[IPC] SetAnimation: entity_id={}, name='{}', frames={}, audio={:?}, scripts={}",
                    id,
                    name,
                    frames.len(),
                    audio_path,
                    scripts.len()
                );

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
                    let channels = decoder.channels();
                    let sample_rate = decoder.sample_rate();
                    let samples: Vec<i16> = decoder.collect();
                    log::debug!(
                        "[SetAnimation] audio decodificado: {} ({} muestras, {}ch, {}Hz)",
                        p,
                        samples.len(),
                        channels,
                        sample_rate
                    );
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
                log::debug!(
                    "[IPC] Animación '{}' guardada y pre-cargada para entidad {}",
                    name,
                    id
                );
            }
            EngineCommand::RemoveAnimation { id, name } => {
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
                    log::debug!("[animation] Eliminada '{}' de entidad {}", name, id);

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
                                log::debug!(
                                    "[animation] Predeterminada cambiada a '{}' para entidad {}",
                                    new_name,
                                    id
                                );
                            }
                            None => {
                                self.default_animation_by_entity.remove(&id);
                                log::debug!("[animation] Sin animaciones restantes para entidad {}", id);
                            }
                        }
                    }
                }
            }
            EngineCommand::SetDefaultAnimation { id, name } => {
                if self.model_animation_bindings.contains_key(&id) {
                    self.set_default_model_clip(id, &name);
                    log::debug!("[model_anim] predeterminada entidad {} => {}", id, name);
                    return;
                }
                let exists = self
                    .animations
                    .get(&id)
                    .map(|m| m.contains_key(&name))
                    .unwrap_or(false);
                if exists {
                    self.default_animation_by_entity.insert(id, name.clone());
                    log::debug!("[animation] predeterminada de entidad {} => {}", id, name);
                } else {
                    log::warn!(
                        "[animation] set_default_animation ignorado: '{}' no existe en entidad {}",
                        name,
                        id
                    );
                }
            }
            EngineCommand::PlayAnimation { id, name, loop_ } => {
                log::debug!("[IPC] PlayAnimation: entity_id={}, name='{}'", id, name);

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
                            log::debug!(
                                "[animation] PlayAnimation '{}' bloqueado: '{}' no es cancelable en entidad {}",
                                name,
                                current_name,
                                id
                            );
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
                            self.play_animation_frame(
                                id,
                                &first_frame.path,
                                first_frame.pivot_x,
                                first_frame.pivot_y,
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
                        if !anim.scripts.is_empty() {
                            log::debug!(
                                "[scripting] {} script(s) de animación '{}' cargados para entidad {}",
                                anim.scripts.len(),
                                name,
                                id
                            );
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
                        log::debug!(
                            "[animation] Iniciada '{}' para entidad {} (fps={}, frames={})",
                            name,
                            id,
                            anim.fps,
                            anim.frames.len()
                        );
                    }
                }
            }
            EngineCommand::StopAnimation { id } => {
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
                send_event(&EngineEvent::AnimationFinished { entity_id: id });
                log::info!("[animation] Stopped for entity {}", id);
            }
            EngineCommand::LoadScript { id, path, source } => {
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
            EngineCommand::SetControlBindings { id, bindings } => {
                if bindings.keyboard_mouse.is_empty() && bindings.gamepad.is_empty() {
                    self.control_bindings_by_entity.remove(&id);
                } else {
                    self.control_bindings_by_entity.insert(id, bindings);
                }
            }
            EngineCommand::RunControlScript {
                id,
                control_key,
                path,
                source,
            } => {
                self.execute_control_script(id, &control_key, &path, &source);
            }
            EngineCommand::UnloadScript { id } => {
                log::info!("[IPC] UnloadScript: entity_id={}", id);
                self.script_engine.detach_entity(id);
                self.save_registry.script_sources.remove(&id);
            }
            EngineCommand::LoadSprite { path, name } => {
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
                                let path_for_log = path.clone();
                                let name_for_log = name.clone();
                                send_event(&EngineEvent::SpriteLoaded {
                                    path,
                                    name,
                                    width: w,
                                    height: h,
                                });
                                log::debug!(
                                    "[sprite] cargado: {} ({}) ({}x{})",
                                    path_for_log,
                                    name_for_log,
                                    w,
                                    h
                                );
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
            EngineCommand::RemoveSprite { path } => {
                if self.sprite_store.remove(&path).is_some() {
                    send_event(&EngineEvent::SpriteRemoved { path: path.clone() });
                    log::info!("[sprite] eliminado: {}", path);
                } else {
                    log::warn!("[sprite] intento de eliminar sprite inexistente: {}", path);
                }
            }
            EngineCommand::GetSpritesList => {
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
            EngineCommand::LoadSound { path, name } => {
                self.sound_store.insert(path.clone(), name.clone());
                send_event(&EngineEvent::SoundLoaded {
                    path: path.clone(),
                    name: name.clone(),
                });
                log::debug!("[sound] registrado: {} ({})", path, name);
            }
            EngineCommand::RemoveSound { path } => {
                if self.sound_store.remove(&path).is_some() {
                    send_event(&EngineEvent::SoundRemoved { path: path.clone() });
                    log::info!("[sound] eliminado: {}", path);
                } else {
                    log::warn!("[sound] intento de eliminar sonido inexistente: {}", path);
                }
            }
            EngineCommand::GetSoundsList => {
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
            EngineCommand::LoadBackgroundAsset { path, name } => {
                self.background_store.insert(path.clone(), name.clone());
                send_event(&EngineEvent::BackgroundAssetLoaded {
                    path: path.clone(),
                    name: name.clone(),
                });
                log::debug!("[background] registrado: {} ({})", path, name);
            }
            EngineCommand::RemoveBackgroundAsset { path } => {
                if self.background_store.remove(&path).is_some() {
                    send_event(&EngineEvent::BackgroundAssetRemoved { path: path.clone() });
                    log::info!("[background] eliminado: {}", path);
                } else {
                    log::warn!("[background] intento de eliminar fondo inexistente: {}", path);
                }
            }
            EngineCommand::GetBackgroundsList => {
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
            EngineCommand::Shutdown => {}
        }
    }
}
