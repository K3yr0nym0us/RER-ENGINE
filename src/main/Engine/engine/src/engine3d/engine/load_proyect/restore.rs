use super::paths::*;
use super::*;

pub(crate) fn pick_active_save_view(project: &ProjectSaveData) -> Result<ActiveSaveView, String> {
    if !project.scenes.is_empty() {
        let active = project
            .activeSceneId
            .and_then(|id| project.scenes.iter().find(|s| s.id == id))
            .or_else(|| project.scenes.first())
            .ok_or_else(|| "escenas vacías en manifest".to_string())?;
        return Ok(ActiveSaveView {
            world: active.world.clone(),
            entities: active.entities.clone(),
            player: active.player.clone(),
            config_camera: active.config_camera.clone(),
            sceneId: active.id,
            sceneName: active.name.clone(),
        });
    }

    let world = project
        .world
        .clone()
        .ok_or_else(|| "manifest sin world ni scenes".to_string())?;

    Ok(ActiveSaveView {
        world,
        entities: project.entities.clone(),
        player: project.player.clone(),
        config_camera: project.config_camera.clone(),
        sceneId: 1,
        sceneName: String::new(),
    })
}

pub(crate) fn needs_scene_burst_load(_game_style: &str, view: &ActiveSaveView) -> bool {
    !view.entities.is_empty() || view.player.is_some()
}

pub(crate) fn find_blueprint<'a>(
    id: &str,
    blueprints: &'a [SavedBlueprint],
) -> Option<&'a SavedBlueprint> {
    blueprints.iter().find(|bp| bp.id == id)
}

pub(crate) fn map_control_bindings(
    bindings: Option<&SavedControlBindings>,
) -> Option<ControlBindingsData> {
    bindings.map(|b| ControlBindingsData {
        keyboard_mouse: b
            .keyboard_mouse
            .iter()
            .map(|(k, s)| {
                (
                    k.clone(),
                    ControlScriptData {
                        name: s.name.clone(),
                        source: s.source.clone(),
                    },
                )
            })
            .collect(),
        gamepad: b
            .gamepad
            .iter()
            .map(|(k, s)| {
                (
                    k.clone(),
                    ControlScriptData {
                        name: s.name.clone(),
                        source: s.source.clone(),
                    },
                )
            })
            .collect(),
    })
}

pub(crate) fn resolve_saved_entity_transform(entity: &SavedEntity3D) -> EntityRestoreTransform {
    EntityRestoreTransform {
        position: entity.position,
        rotation: entity.rotation.unwrap_or([0.0, 0.0, 0.0, 1.0]),
        scale: entity.scale,
    }
}

/// Herencia de blueprint: model/physics/scripts/animations/colision; transform solo de la instancia.
pub(crate) fn build_generic_pending_restore(
    entity: &SavedEntity3D,
    transform: EntityRestoreTransform,
    blueprints: &[SavedBlueprint],
) -> PendingRestore {
    let bp = entity
        .blueprint_id
        .as_deref()
        .and_then(|id| find_blueprint(id, blueprints));
    let physics_type = bp
        .and_then(|b| b.physics_type.clone())
        .or_else(|| entity.physics_type.clone())
        .unwrap_or_else(|| "static".to_string());
    let visual = {
        let marker_model = bp
            .map(|b| b.model.as_str())
            .unwrap_or(entity.model.as_str());
        let marker_model_id = bp
            .and_then(|b| b.model_id.as_deref())
            .filter(|s| !s.is_empty());
        if entity_path_marker(marker_model).is_some() || marker_model == "[Player]" {
            None
        } else if let Some(id) = entity.model_id.as_ref().filter(|s| !s.is_empty()) {
            Some(id.clone())
        } else if let Some(id) = marker_model_id {
            Some(id.to_string())
        } else if is_imported_model_id(&entity.model) {
            Some(entity.model.clone())
        } else {
            Some(marker_model.to_string())
        }
    };
    PendingRestore {
        transform,
        name: Some(entity.name.clone()),
        physics_enabled: bp
            .and_then(|b| b.physics_type.as_ref())
            .or(entity.physics_type.as_ref())
            .is_some(),
        physics_type,
        colision: bp.map(|b| b.colision).unwrap_or(entity.colision),
        hardness: entity.hardness,
        animations: bp
            .and_then(|b| b.animations.clone())
            .or_else(|| entity.animations.clone()),
        scripts: bp
            .and_then(|b| b.scripts.clone())
            .or_else(|| entity.scripts.clone()),
        control_bindings: map_control_bindings(entity.controls.as_ref()),
        blueprint_id: entity.blueprint_id.clone(),
        entity_category: entity_library_category(&entity.category),
        visual_model_path: visual,
        saved_entity_id: entity.id,
    }
}

pub(crate) fn build_player_pending_restore_from_entity(entity: &SavedEntity3D) -> PendingRestore {
    let visual = if entity_path_marker(&entity.model).is_some() || entity.model == "[Player]" {
        None
    } else if let Some(id) = entity.model_id.as_ref().filter(|s| !s.is_empty()) {
        Some(id.clone())
    } else if !entity.model.is_empty() {
        Some(entity.model.clone())
    } else {
        None
    };
    PendingRestore {
        transform: EntityRestoreTransform {
            position: entity.position,
            rotation: entity.rotation.unwrap_or([0.0, 0.0, 0.0, 1.0]),
            scale: entity.scale,
        },
        name: Some(entity.name.clone()),
        physics_enabled: entity.physics_type.is_some(),
        physics_type: entity
            .physics_type
            .clone()
            .unwrap_or_else(|| "dynamic".to_string()),
        colision: entity.colision,
        hardness: entity.hardness,
        animations: entity.animations.clone(),
        scripts: entity.scripts.clone(),
        control_bindings: map_control_bindings(entity.controls.as_ref()),
        blueprint_id: entity.blueprint_id.clone(),
        entity_category: None,
        visual_model_path: visual,
        saved_entity_id: entity.id,
    }
}

pub(crate) fn apply_entity_scripts(state: &mut State, id: u32, scripts: Option<&[SavedScript]>) {
    let Some(list) = scripts else { return };
    for script in list {
        state.handle_command(EngineCommand::Common(EngineCommandCommon::LoadScript {
            id,
            path: script.name.clone(),
            source: script.source.clone(),
        }));
    }
}

pub(crate) fn apply_entity_animations(
    state: &mut State,
    id: u32,
    animations: Option<&[SavedAnimation]>,
) {
    let Some(list) = animations else { return };
    for anim in list {
        let frames: Vec<AnimationFrameData> = anim
            .frames
            .iter()
            .map(|f| AnimationFrameData {
                path: f.path.clone(),
                pivot_x: Some(f.pivot_x),
                pivot_y: Some(f.pivot_y),
                src_x: f.src_x,
                src_y: f.src_y,
                src_w: f.src_w,
                src_h: f.src_h,
            })
            .collect();
        let anim_scripts: Vec<AnimScriptData> = anim
            .scripts
            .as_ref()
            .map(|scripts| {
                scripts
                    .iter()
                    .map(|s| AnimScriptData {
                        name: s.name.clone(),
                        source: s.source.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        state.handle_command(EngineCommand::Common(EngineCommandCommon::SetAnimation {
            id,
            name: anim.name.clone(),
            frames,
            fps: anim.fps,
            loop_: anim.r#loop,
            flip_horizontal: !(anim.facing_right.unwrap_or(true)),
            audio_path: anim.audio_path.clone(),
            logical_w: Some(anim.logical_w),
            logical_h: Some(anim.logical_h),
            scripts: anim_scripts,
            is_cancelable: anim.is_cancelable.unwrap_or(true),
        }));
        if anim.is_default.unwrap_or(false) {
            state.handle_command(EngineCommand::Common(
                EngineCommandCommon::SetDefaultAnimation {
                    id,
                    name: anim.name.clone(),
                },
            ));
        }
    }
}

pub(crate) fn apply_full_entity_restore(
    state: &mut State,
    id: u32,
    pending: &PendingRestore,
    model_path: &str,
    skip_transform: bool,
    omit_scale: bool,
) {
    let physics = if pending.physics_enabled {
        Some(EntityRestorePhysics {
            enabled: true,
            body_type: pending.physics_type.clone(),
        })
    } else {
        None
    };
    state.apply_entity_restore_inner(
        id,
        pending.name.clone(),
        &pending.transform,
        physics.as_ref(),
        pending.control_bindings.as_ref(),
        omit_scale,
        skip_transform,
    );

    apply_entity_scripts(state, id, pending.scripts.as_deref());
    apply_entity_animations(state, id, pending.animations.as_deref());

    if let Some(visual) = pending
        .visual_model_path
        .as_ref()
        .filter(|p| !p.is_empty() && !models_refer_to_same_asset(state, p, model_path))
    {
        state.replace_entity_model(id, visual);
    }

    state.entity_colision.insert(id, pending.colision);
    state.entity_hardness.insert(id, pending.hardness);
    state.reconcile_entity_physics_with_mesh(id);
}

pub(crate) fn collect_saved_entity_attachments(
    view: &ActiveSaveView,
) -> Vec<crate::engine3d::config_3d::entity_attachments::SavedEntityAttachment> {
    let mut out = Vec::new();
    let mut push = |entity: &SavedEntity3D| {
        let is_socket = entity.attach_socket_host_id.is_some()
            && entity
                .attach_socket_name
                .as_ref()
                .is_some_and(|n| !n.trim().is_empty());
        let is_entity = entity.attach_parent_id.is_some();

        if !is_socket && !is_entity {
            return;
        }

        // Locals pueden faltar en saves antiguos: se recalculan al restaurar desde poses mundiales.
        let local_position = entity.attach_local_position.unwrap_or([0.0, 0.0, 0.0]);
        let local_rotation = entity.attach_local_rotation.unwrap_or([0.0, 0.0, 0.0, 1.0]);
        let child_world_scale = entity.attach_local_scale.unwrap_or(entity.scale);
        let locals_from_save = entity.attach_local_position.is_some()
            && entity.attach_local_rotation.is_some()
            && entity.attach_local_scale.is_some();

        out.push(
            crate::engine3d::config_3d::entity_attachments::SavedEntityAttachment {
                entity_id: entity.id,
                parent_id: if is_socket {
                    None
                } else {
                    entity.attach_parent_id
                },
                attach_socket_host_id: if is_socket {
                    entity.attach_socket_host_id
                } else {
                    None
                },
                attach_socket_name: if is_socket {
                    entity.attach_socket_name.clone()
                } else {
                    None
                },
                local_position,
                local_rotation,
                child_world_scale,
                recompute_locals: !locals_from_save,
            },
        );
    };
    for entity in &view.entities {
        push(entity);
    }
    if let Some(player) = &view.player {
        push(player);
    }
    out
}

pub(crate) fn restore_entity_sockets_after_scene_load(state: &mut State, view: &ActiveSaveView) {
    let mut restore = |entity: &SavedEntity3D| {
        if !entity.sockets.is_empty() {
            state.restore_entity_sockets_from_saved(entity.id, &entity.sockets);
            state.emit_entity_sockets_if_any(entity.id);
        }
    };
    for entity in &view.entities {
        restore(entity);
    }
    if let Some(player) = &view.player {
        restore(player);
    }
}

pub(crate) fn restore_entity_bone_physics_after_scene_load(
    state: &mut State,
    view: &ActiveSaveView,
) {
    let mut restore = |entity: &SavedEntity3D| {
        if !entity.bone_physics.is_empty() {
            state.restore_entity_bone_physics_from_saved(entity.id, &entity.bone_physics);
            state.emit_entity_bone_physics_if_any(entity.id);
        }
    };
    for entity in &view.entities {
        restore(entity);
    }
    if let Some(player) = &view.player {
        restore(player);
    }
}

pub(crate) fn restore_entity_projectile_config_after_scene_load(
    state: &mut State,
    view: &ActiveSaveView,
) {
    let mut restore = |entity: &SavedEntity3D| {
        if entity.category == "projectile" {
            let config = entity.projectile.clone().unwrap_or_default();
            state.restore_projectile_config_from_saved(entity.id, &config);
            state.emit_projectile_config_changed(entity.id);
        }
    };
    for entity in &view.entities {
        restore(entity);
    }
    if let Some(player) = &view.player {
        restore(player);
    }
}

pub(crate) fn restore_entity_attachments_after_scene_load(
    state: &mut State,
    view: &ActiveSaveView,
) {
    let mut saved = collect_saved_entity_attachments(view);

    // Si el id runtime del jugador ≠ id del manifest, remapear anclas de fusión/socket.
    if let (Some(saved_player), Some(actual_player)) =
        (view.player.as_ref(), state.play_character_entity)
        && saved_player.id != actual_player
        && saved_player.id != 0
    {
        let from = saved_player.id;
        let to = actual_player;
        let mut remapped = 0usize;
        for entry in &mut saved {
            if entry.parent_id == Some(from) {
                entry.parent_id = Some(to);
                remapped += 1;
            }
            if entry.attach_socket_host_id == Some(from) {
                entry.attach_socket_host_id = Some(to);
                remapped += 1;
            }
            if entry.entity_id == from {
                entry.entity_id = to;
                remapped += 1;
            }
        }
        if remapped > 0 {
            log::warn!("[Fusión] remap jugador {from} → {to} en {remapped} campo(s) de vínculo");
        }
    }

    // Siempre restaurar (aunque vacío) para limpiar estado stale tras clear de escena.
    state.restore_entity_attachments_from_saved(&saved);

    let excluded = state.play_character_movement_excluded_colliders().len();
    if !saved.is_empty() {
        log::info!(
            "[Fusión] exclusión de movimiento tras restore: {excluded} collider(s) (jugador + accesorios)"
        );
    }
}
