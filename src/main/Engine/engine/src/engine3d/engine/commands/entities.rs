use super::*;

impl State {
    pub(super) fn cmd_entities(&mut self, cmd: EngineCommand) -> Option<EngineCommand> {
        match cmd {
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
                    true,
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
            EngineCommand::Only3d(EngineCommand3dOnly::PlaceQuickBuildAtCursor {
                pixel_x,
                pixel_y,
            }) => {
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
                    position, yaw, pitch, None, None, None, None, None, None, None, None,
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
                        return None;
                    };
                    self.apply_play_character_view(
                        pos,
                        yaw.unwrap_or(
                            crate::engine3d::config_3d::character_anchor::PLAY_CHARACTER_EDITOR_ORBIT_YAW,
                        ),
                        pitch.unwrap_or(
                            crate::engine3d::config_3d::character_anchor::PLAY_CHARACTER_EDITOR_ORBIT_PITCH,
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
                let in_play_mode = self.preview_playing || self.is_play_controller_active();

                let current_rot = before
                    .as_ref()
                    .map(|t| t.rotation)
                    .unwrap_or(Quat::IDENTITY);
                let current_pos = before.as_ref().map(|t| t.position).unwrap_or(Vec3::ZERO);
                let current_scale = before.as_ref().map(|t| t.scale).unwrap_or(Vec3::ONE);

                let rot_quat = crate::engine3d::config_3d::resolve_set_transform_rotation(
                    current_rot,
                    rotation,
                    rotation_euler_delta,
                    rotation_euler_degrees,
                );
                let position_vec = crate::engine3d::config_3d::resolve_axis_update(
                    current_pos,
                    position,
                    position_axis,
                );
                let scale_vec = crate::engine3d::config_3d::resolve_axis_update(
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
                } else if !is_editor_camera
                    && let Some(transform) = self.world.get_mut::<Transform>(id)
                {
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
                    if let Some(s) = scale_vec
                        && !is_play_character
                    {
                        saved.1 = s;
                    }
                }
                if !is_play_character
                    && !is_editor_camera
                    && (position_vec.is_some() || rotation_changed || scale_vec.is_some())
                {
                    self.sync_entity_physics_collider(id);
                }
                if let Some(prev) = before {
                    let prev_pos = prev.position.to_array();
                    let prev_rot = [
                        prev.rotation.x,
                        prev.rotation.y,
                        prev.rotation.z,
                        prev.rotation.w,
                    ];
                    let prev_scl = prev.scale.to_array();
                    let next_pos = self
                        .world
                        .get::<Transform>(id)
                        .map(|t| t.position.to_array());
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
                    return None;
                }

                if !force && self.is_entity_name_taken(next_name, Some(id)) {
                    send_event(&EngineEvent::Error {
                        message: format!("Ya existe una entidad con el nombre '{}'", next_name),
                    });
                    return None;
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
            EngineCommand::Only3d(EngineCommand3dOnly::LoadCharacter { path, track_undo }) => {
                self.load_character(&path);
                if track_undo.unwrap_or(false)
                    && let Some(&id) = self.character_entities.last()
                {
                    self.push_remove_entity_undo(id);
                    log::info!("[quick_build] personaje {id} registrado en undo");
                }
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
                self.entity_hardness.remove(&id);
                self.entity_projectile_config.remove(&id);
                self.active_projectiles.retain(|p| p.entity_id != id);
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
                if let Some(id) = blueprint
                    .blueprint_id
                    .clone()
                    .filter(|s| !s.trim().is_empty())
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
                let preview_blueprint: Option<crate::engine3d::ipc::BlueprintPlacementMeta> =
                    preview_blueprint.and_then(|v| serde_json::from_value(v).ok());
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
                    self.tool_overlay_buffer =
                        crate::engine3d::gizmo::build_from_vertices(&self.vulkan, &[]);
                    if was_active {
                        send_event(&EngineEvent::ToolCancelled);
                        log::info!("Herramienta cancelada");
                    }
                } else {
                    if let Some(ghost_id) = self.quick_build_ghost_id.take() {
                        self.world.despawn(ghost_id);
                    }
                    let plane_tool_cmd =
                        matches!(tool.as_str(), "draw_collider" | "draw_execution_area");
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
                                crate::engine3d::gizmo::build_from_vertices(&self.vulkan, &[]);
                            if let (Some(path), Some(kind)) =
                                (preview_path.as_deref(), preview_kind.as_deref())
                            {
                                let mut placement_meta =
                                    preview_blueprint.clone().unwrap_or_else(|| {
                                        crate::engine3d::ipc::BlueprintPlacementMeta {
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
                                if placement_meta
                                    .model
                                    .as_deref()
                                    .map(str::trim)
                                    .unwrap_or("")
                                    .is_empty()
                                {
                                    placement_meta.model = Some(path.to_string());
                                }
                                if placement_meta.category.is_none() {
                                    placement_meta.category = preview_entity_category.clone();
                                }
                                crate::engine3d::ipc::enrich_blueprint_placement_meta(
                                    &self.blueprint_registry,
                                    &self.model_store,
                                    &|p| self.model_path_key(p),
                                    &mut placement_meta,
                                    path,
                                );
                                self.quick_build_blueprint = Some(
                                    crate::engine3d::config_3d::quick_build::QuickBuildBlueprint::from_placement_meta(
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
                                self.quick_build_ghost_id =
                                    self.load_quick_build_ghost_3d(path, scale);
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
            EngineCommand::Only3d(EngineCommand3dOnly::MergeEntities { ids }) => {
                self.merge_entities(&ids);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetEntityHardness { id, hardness }) => {
                self.set_entity_hardness(id, hardness);
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
            other => return Some(other),
        }
        None
    }
}
