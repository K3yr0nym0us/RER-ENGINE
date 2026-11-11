use super::paths::*;
use super::restore::*;
use super::snapshot::*;
use super::*;

pub(crate) fn ensure_model_cached(state: &mut State, path: &str) -> bool {
    state.poll_and_advance_model_preloads(MODEL_GPU_PARTS_DURING_SAVE_LOAD);
    match state.ensure_static_model_cached(path) {
        Ok(()) => true,
        Err(err) => {
            log::error!("no se pudo cachear modelo '{path}': {err}");
            false
        }
    }
}

/// Modelos 3D que la escena instancia al abrir el `.save` (vía `resources.models`).
pub(crate) fn collect_scene_required_model_ids(
    state: &State,
    view: &ActiveSaveView,
) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    let mut push = |entity: &SavedEntity3D| {
        if !is_3d_model_file_entity(entity) {
            return;
        }
        if let Ok(id) = entity_model_cache_lookup(state, entity)
            && !ids.iter().any(|existing| existing == &id)
        {
            ids.push(id);
        }
    };
    for entity in &view.entities {
        push(entity);
    }
    if let Some(player) = view.player.as_ref() {
        push(player);
    }
    ids
}

pub(crate) fn log_load_step(total: Instant, step: &mut Instant, message: &str) {
    let step_ms = step.elapsed().as_millis() as u64;
    let total_ms = total.elapsed().as_millis() as u64;

    send_load_progress(message, Some(step_ms), Some(total_ms));
    *step = Instant::now();
}

pub(crate) fn load_project_asset_stores(state: &mut State, project: &ProjectSaveData) {
    for sound in &project.sounds {
        state.handle_command(EngineCommand::Common(EngineCommandCommon::LoadSound {
            path: sound.path.clone(),
            name: sound.name.clone(),
        }));
    }
    for font in &project.fonts {
        state.handle_command(EngineCommand::Common(EngineCommandCommon::LoadFont {
            path: font.path.clone(),
            name: font.name.clone(),
        }));
    }
    for bg in &project.backgrounds {
        state.handle_command(EngineCommand::Common(
            EngineCommandCommon::LoadBackgroundAsset {
                path: bg.path.clone(),
                name: bg.name.clone(),
            },
        ));
    }
    for img in &project.hud_images {
        state.handle_command(EngineCommand::Common(EngineCommandCommon::LoadHudImage {
            path: img.path.clone(),
            name: img.name.clone(),
        }));
    }
}

/// Restaura jugador 3D desde manifest: mesh sin auto-escala, transform y cámara del save.
pub(crate) fn restore_player_from_manifest(
    state: &mut State,
    player: &SavedEntity3D,
    cam: &SavedConfigCamera,
) {
    use crate::engine3d::config_3d::character_anchor::{
        PLAY_CHARACTER_EDITOR_ORBIT_PITCH, PLAY_CHARACTER_EDITOR_ORBIT_YAW,
    };

    let pending = build_player_pending_restore_from_entity(player);
    let cache_key = entity_model_cache_lookup(state, player).ok();

    if let Some(ref id) = cache_key {
        if !ensure_model_cached(state, id) {
            log::warn!("[restore] no se pudo precargar modelo jugador: {id}");
        }
        state.poll_and_advance_model_preloads(MODEL_GPU_PARTS_DURING_SAVE_LOAD);
    }

    let id = state.ensure_play_character_shell(
        &player.name,
        "[Player]",
        cache_key.as_deref(),
        Some(player.id),
    );

    if let Some(ref model_id) = cache_key {
        match state.install_play_character_visual_from_path(id, model_id) {
            Ok(asset_key) => {
                state.emit_entity_model_replaced_for_play_character(id, &asset_key);
            }
            Err(e) => log::error!("[restore] mesh jugador: {e}"),
        }
    }

    let feet = player_manifest_position_as_feet(state, player);
    let feet_v = glam::Vec3::from_array(feet);
    let rot_q = player
        .rotation
        .map(|r| glam::Quat::from_xyzw(r[0], r[1], r[2], r[3]));
    let scale = glam::Vec3::from_array(player.scale);
    state.apply_play_character_transform_editor(id, None, rot_q, Some(scale));
    if let Some(ref model_id) = cache_key {
        if let Some(bounds) = state.play_character_visual_local_bounds(model_id) {
            state.place_play_character_at_world_feet_with_bounds(id, bounds, feet_v, false);
        } else {
            state.place_play_character_at_world_feet(id, model_id, feet_v, false);
        }
    } else {
        state.set_play_character_feet_position(feet_v);
    }

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
        true,
        true,
    );
    apply_entity_scripts(state, id, pending.scripts.as_deref());
    apply_entity_animations(state, id, pending.animations.as_deref());
    state.entity_colision.insert(id, pending.colision);
    state.entity_hardness.insert(id, pending.hardness);
    state.ensure_play_character_kinematic_only();
    state.reconcile_entity_physics_with_mesh(id);

    if !state.model_animation_bindings.contains_key(&id) {
        log::warn!(
            "[SHADER_MAT] ent={id} jugador sin animation binding tras restore — personaje puede verse sin texturas correctas"
        );
    }

    let follow_mode = cam.camera_follow_mode.as_deref().map(|m| match m {
        "follow_character" => crate::engine3d::ipc::PlayCameraFollowMode::FollowCharacter,
        _ => crate::engine3d::ipc::PlayCameraFollowMode::MoveWithCharacter,
    });
    let yaw = cam.yaw.unwrap_or(PLAY_CHARACTER_EDITOR_ORBIT_YAW);
    let pitch = cam.pitch.unwrap_or(PLAY_CHARACTER_EDITOR_ORBIT_PITCH);
    state.apply_play_character_view(
        feet,
        yaw,
        pitch,
        cam.fov_y,
        cam.frustum_distance,
        follow_mode,
        player.rotation,
        Some(player.scale),
        cam.camera_eye_position,
        cam.fps_camera_yaw,
        cam.fps_camera_pitch,
    );
    if let Some(eye) = cam.camera_eye_position {
        state.play_camera_eye_position = glam::Vec3::from_array(eye);
        state.capture_play_camera_follow_offset();
    }
    if let Some(rot) = player.rotation {
        let rot_q = glam::Quat::from_xyzw(rot[0], rot[1], rot[2], rot[3]);
        state.apply_play_character_transform_editor(id, None, Some(rot_q), None);
        if state.uses_editor_viewport_camera() {
            state.ensure_editor_camera_entity();
            state.sync_editor_camera_entity_from_viewport();
        }
    }

    let char_path = cache_key
        .as_deref()
        .map(|id| state.model_library_path_for(id))
        .unwrap_or_else(|| "[Player]".to_string());
    send_event(&EngineEvent::CharacterLoaded {
        id,
        path: char_path,
    });
    state.emit_play_character_view_changed(true);
}

/// `player.position` en el manifest = pies en mundo (`build_player_snapshot`).
pub(crate) fn player_manifest_position_as_feet(_state: &State, player: &SavedEntity3D) -> [f32; 3] {
    player.position
}

pub(crate) fn apply_loaded_proyect_3d_with_scene(
    state: &mut State,
    project: &ProjectSaveData,
    forced_scene: Option<&SavedScene>,
) -> Result<ActiveSaveView, String> {
    let load_started_at = Instant::now();
    let mut step_started = Instant::now();
    state.restoring_save_manifest = true;
    let view = match forced_scene {
        Some(scene) => active_view_from_saved_scene(scene),
        None => match pick_active_save_view(project) {
            Ok(v) => v,
            Err(e) => {
                state.restoring_save_manifest = false;
                return Err(e);
            }
        },
    };
    let open_msg = format!(
        "Abriendo escena «{}» ({} entidades)…",
        view.sceneName,
        view.entities.len()
    );

    send_load_progress(&open_msg, None, None);
    if forced_scene.is_none() && state.mount_save_on_empty_world {
        state.mount_save_on_empty_world = false;
        log_load_step(
            load_started_at,
            &mut step_started,
            "Montando escena desde .save",
        );
    } else {
        state.clear_scene_entities_for_save_load();
        state.apply_empty_3d_editor_defaults();
        log_load_step(
            load_started_at,
            &mut step_started,
            "Escena anterior vaciada, leyendo mundo",
        );
    }
    let game_style = project.gameStyle.as_str();
    let blueprints = &project.blueprints;
    let burst_load = needs_scene_burst_load(game_style, &view);
    let saved_player = view.player.clone();
    let saved_config_camera = view.config_camera.clone();

    let light_ambient = view.world.lightAmbient.unwrap_or(DEFAULT_LIGHT_AMBIENT);
    let light_intensity = view.world.lightIntensity.unwrap_or(DEFAULT_LIGHT_INTENSITY);
    let shadow_darkness = view.world.shadowDarkness.unwrap_or(DEFAULT_SHADOW_DARKNESS);

    let depth = view.world.worldDepth.unwrap_or(50.0);
    let radius = view
        .world
        .worldRadius
        .unwrap_or_else(|| view.world.worldWidth.min(view.world.worldHeight).min(depth) * 0.5);
    state.handle_command(EngineCommand::Only3d(EngineCommand3dOnly::SetWorldRadius {
        radius,
    }));
    state.handle_command(EngineCommand::Common(EngineCommandCommon::SetGridVisible {
        visible: view.world.gridVisible,
    }));
    state.handle_command(EngineCommand::Common(
        EngineCommandCommon::SetGridCellSize {
            size: view.world.gridCellSize,
        },
    ));
    let target_fps = if view.world.targetFps.is_finite() && view.world.targetFps > 0.0 {
        view.world.targetFps as u64
    } else {
        60
    };
    state.handle_command(EngineCommand::Common(EngineCommandCommon::SetTargetFps {
        fps: target_fps,
    }));
    if let Some(gravity) = view.world.gravity {
        state.handle_command(EngineCommand::Common(EngineCommandCommon::SetGravity {
            gravity,
        }));
    }
    state.handle_command(EngineCommand::Only3d(
        EngineCommand3dOnly::SetDirectionalLight {
            ambient: Some(light_ambient),
            intensity: Some(light_intensity),
            shadow_darkness: Some(shadow_darkness),
        },
    ));
    crate::engine3d::config_3d::entity_textures::apply_graphics_settings_from_world_wire(
        state,
        view.world.graphicsTextureTier.as_deref(),
        view.world.textureDetailDistance,
    );
    crate::engine3d::config_3d::reflection_settings::apply_reflection_settings_from_world_wire(
        state,
        view.world.reflectionTier.as_deref(),
        view.world.reflectionRaytracing,
        view.world.reflectionProbes,
    );
    crate::engine3d::config_3d::shadow_settings::apply_shadow_settings_from_world_wire(
        state,
        view.world.shadowTier.as_deref(),
    );
    crate::engine3d::config_3d::msaa_settings::apply_msaa_settings_from_world_wire(
        state,
        view.world.msaaTier.as_deref(),
    );
    let taa_enabled = view.world.taaEnabled.unwrap_or(false);
    let taa_blend = view.world.taaBlend.unwrap_or(0.2);
    let taa_jitter_scale = view.world.taaJitterScale.unwrap_or(1.0);
    state.set_taa(taa_enabled, Some(taa_blend), Some(taa_jitter_scale));
    state.ensure_material_validation_demo();
    send_event(&EngineEvent::GraphicsTextureTierChanged {
        tier: state.graphics_texture_tier.wire().to_string(),
    });
    send_event(&EngineEvent::TextureDetailDistanceChanged {
        distance_m: state.texture_detail_near_m,
    });
    send_event(&EngineEvent::ReflectionTierChanged {
        tier: state.reflection_tier.wire().to_string(),
    });
    send_event(&EngineEvent::ReflectionProbesChanged {
        enabled: state.reflection_probes_enabled,
    });
    send_event(&EngineEvent::ReflectionRaytracingChanged {
        enabled: state.reflection_raytracing_enabled,
    });
    send_event(&EngineEvent::ShadowTierChanged {
        tier: state.shadow_tier.wire().to_string(),
    });
    send_event(&EngineEvent::MsaaTierChanged {
        tier: state.msaa_tier.wire().to_string(),
        sample_count: state.msaa_sample_count,
    });
    send_event(&EngineEvent::TaaChanged {
        enabled: state.taa_enabled,
        blend: Some(state.taa_blend),
        jitter_scale: Some(state.taa_jitter_scale),
    });
    load_project_asset_stores(state, project);
    log_load_step(
        load_started_at,
        &mut step_started,
        "Sonidos y fondos registrados",
    );
    if let Some(resources) = &project.resources {
        let extract_dir = std::env::var("RER_PROJECT_EXTRACT_DIR").unwrap_or_default();
        let extract_path = std::path::Path::new(extract_dir.as_str());
        for res in &resources.models {
            let asset_disk = resolve_manifest_asset_path(&res.asset, extract_path, &res.id);
            if !asset_disk.is_file() {
                log::error!(
                    "[load] .rerasset no encontrado: {} (model_id={})",
                    asset_disk.display(),
                    res.id
                );
                send_event(&EngineEvent::Error {
                    message: format!(
                        "Modelo importado faltante en .save: {} ({})",
                        res.name,
                        asset_disk.display()
                    ),
                });
                continue;
            }
            log::info!(
                "[load-pack] GPU «{}» desde {}",
                res.name,
                asset_disk.display()
            );
            let source_path = source_path_for_imported_model(project, &res.id, extract_path);
            state.cache_rerasset_material_tex_map(&res.id, &asset_disk);
            state
                .imported_model_registry
                .insert(crate::engine3d::assets::ImportedModelEntry {
                    model_id: res.id.clone(),
                    name: res.name.clone(),
                    category: crate::engine3d::ipc::normalize_model_library_category(Some(
                        res.model_type.as_str(),
                    )),
                    state: rer_engine_core::assets::AssetState::Ready,
                    rerasset_path: asset_disk.clone(),
                    source_path: source_path.clone(),
                    source_size: 0,
                    source_mtime_secs: 0,
                    importer_version: res.importer_version,
                });
            state
                .imported_model_registry
                .link_imported_model_aliases(&res.id, &source_path);
            let asset_rel = relative_rerasset_manifest_path(&res.id);
            let key = if is_imported_model_id(&source_path) {
                source_path.clone()
            } else {
                state.model_path_key(&source_path)
            };
            state.model_store.insert(
                key,
                crate::engine3d::ipc::ModelStoreEntry {
                    name: res.name.clone(),
                    category: crate::engine3d::ipc::normalize_model_library_category(Some(
                        res.model_type.as_str(),
                    )),
                    model_id: Some(res.id.clone()),
                    rerasset_path: Some(asset_rel),
                },
            );
        }
        for entity in view.entities.iter().chain(view.player.iter()) {
            if let Ok(model_id) = entity_model_cache_lookup(state, entity) {
                state
                    .imported_model_registry
                    .link_imported_model_aliases(&model_id, &entity.model);
            }
        }
    } else if !collect_scene_required_model_ids(state, &view).is_empty() {
        state.restoring_save_manifest = false;
        return Err(
            "Manifest sin resources.models pero la escena referencia modelos 3D importados."
                .to_string(),
        );
    }
    let burst_load_planned = burst_load;
    log_load_step(
        load_started_at,
        &mut step_started,
        "Modelos importados: carga lazy al instanciar entidades",
    );

    let mut model_load_queue: Vec<(String, PendingRestore)> = Vec::new();

    for entity in &view.entities {
        state.poll_and_advance_model_preloads(MODEL_GPU_PARTS_DURING_SAVE_LOAD);
        let transform = resolve_saved_entity_transform(entity);
        let pending = build_generic_pending_restore(entity, transform, blueprints);

        match entity.category.as_str() {
            "player" => {
                log::warn!(
                    "manifest: entidad player en entities ignorada; usar objeto player del manifest"
                );
            }
            _ if is_collider_path(&entity.model) => {
                if let Some(id) = state.restore_collider_plane_from_save(
                    &entity.name,
                    entity.position,
                    entity.scale,
                    entity.rotation,
                    None,
                ) {
                    apply_full_entity_restore(state, id, &pending, "[Colisionador]", false, false);
                }
            }
            _ if is_execution_area_path(&entity.model) => {
                if let Some(id) = state.restore_trigger_plane_from_save(
                    &entity.name,
                    entity.position,
                    entity.scale,
                    entity.rotation,
                    None,
                ) {
                    apply_full_entity_restore(state, id, &pending, "[ExecutionArea]", false, false);
                }
            }
            _ if is_editor_box_path(&entity.model) => {
                state.spawn_editor_box(&entity.name, entity.position, entity.scale);
                if let Some(id) = state.scenario_entities.last().copied() {
                    apply_full_entity_restore(state, id, &pending, "[EditorBox]", true, false);
                }
            }
            "sun" => {
                state.spawn_sun(&entity.name, entity.position, entity.scale);
                if let Some(id) = state.sun_entity {
                    apply_full_entity_restore(state, id, &pending, "[Sun]", true, false);
                }
            }
            "ground" => {
                state.spawn_ground_plane(entity.position, entity.scale);
                if let Some(id) = state.ground_entity_id() {
                    apply_full_entity_restore(state, id, &pending, "[Ground]", true, false);
                }
            }
            _ if is_ball_path(&entity.model) => {
                let diameter = entity
                    .scale
                    .into_iter()
                    .fold(f32::INFINITY, f32::min)
                    .max(0.15);
                let physics_type = entity.physics_type.as_deref().unwrap_or("dynamic");
                let id = state.spawn_physics_ball(
                    &entity.name,
                    entity.position,
                    [diameter, diameter, diameter],
                    physics_type,
                );
                apply_full_entity_restore(state, id, &pending, "[Ball]", true, false);
            }
            "environment" | "object" | "character" | "weapon" | "projectile"
                if is_3d_model_file_entity(entity) =>
            {
                let model_key = match entity_model_cache_lookup(state, entity) {
                    Ok(id) => id,
                    Err(e) => {
                        log::error!("{e}");
                        continue;
                    }
                };
                model_load_queue.push((model_key.clone(), pending.clone()));

                if !burst_load_planned {
                    if !ensure_model_cached(state, &model_key) {
                        continue;
                    }
                    if let Ok(id) = state.spawn_cached_model_from_save(
                        &model_key,
                        pending.transform.position,
                        pending.transform.rotation,
                        pending.transform.scale,
                        pending.name.as_deref(),
                        pending.entity_category.clone(),
                        pending.blueprint_id.clone(),
                        pending.physics_enabled,
                        &pending.physics_type,
                        Some(pending.saved_entity_id),
                        true,
                    ) {
                        apply_full_entity_restore(state, id, &pending, &model_key, false, false);
                    }
                }
            }
            other => {
                log::warn!(
                    "entidad category='{other}' model='{}' ignorada en carga 3D",
                    entity.model
                );
            }
        }
    }

    log_load_step(
        load_started_at,
        &mut step_started,
        &format!(
            "Entidades del manifest procesadas ({})",
            view.entities.len()
        ),
    );

    if burst_load_planned {
        state.poll_and_advance_model_preloads(MODEL_GPU_PARTS_DURING_SAVE_LOAD);

        let queued_ids: Vec<String> = model_load_queue.iter().map(|(p, _)| p.clone()).collect();
        for model_id in &queued_ids {
            if !ensure_model_cached(state, model_id) {
                log::error!("no se pudo cachear modelo '{model_id}' para burst load");
            }
        }

        for (path, pending) in model_load_queue {
            state.poll_and_advance_model_preloads(MODEL_GPU_PARTS_DURING_SAVE_LOAD);
            if !ensure_model_cached(state, &path) {
                continue;
            }
            if let Ok(id) = state.spawn_cached_model_from_save(
                &path,
                pending.transform.position,
                pending.transform.rotation,
                pending.transform.scale,
                pending.name.as_deref(),
                pending.entity_category.clone(),
                pending.blueprint_id.clone(),
                pending.physics_enabled,
                &pending.physics_type,
                Some(pending.saved_entity_id),
                true,
            ) {
                apply_full_entity_restore(state, id, &pending, &path, false, false);
            }
        }
    }

    match (saved_player.as_ref(), saved_config_camera.as_ref()) {
        (Some(player_entity), Some(cam)) => {
            restore_player_from_manifest(state, player_entity, cam);
        }
        (Some(_), None) => {
            log::error!("[load_proyect] proyecto 3D requiere config_camera en manifest");
        }
        (None, _) => {
            if view.entities.is_empty() {
                state.apply_3d_placeholder_sun_and_player();
            } else {
                log::error!("[load_proyect] proyecto 3D requiere player en manifest");
            }
        }
    }

    if burst_load_planned {
        log_load_step(
            load_started_at,
            &mut step_started,
            "Modelos 3D instanciados",
        );
    }

    if let (Some(player_entity), Some(id)) = (saved_player.as_ref(), state.play_character_entity)
        && let Some(bindings) = map_control_bindings(player_entity.controls.as_ref())
    {
        state.handle_command(crate::engine3d::ipc::EngineCommand::Common(
            EngineCommandCommon::SetControlBindings { id, bindings },
        ));
    }

    for id in state.world.entities().to_vec() {
        state.reconcile_entity_physics_with_mesh(id);
    }

    let done_msg = format!(
        "Carga terminada — escena «{}» ({} entidades, {} ms)",
        view.sceneName,
        view.entities.len(),
        load_started_at.elapsed().as_millis()
    );
    log::info!("[load-pack] {done_msg}");
    send_load_progress(
        &done_msg,
        None,
        Some(load_started_at.elapsed().as_millis() as u64),
    );
    state.import_player_ui_text_boxes_from_save(&project.player_ui_text_boxes);
    state.import_player_ui_buttons_from_save(&project.player_ui_buttons);
    state.import_player_ui_images_from_save(&project.player_ui_images);
    state.import_player_ui_objects_from_save(&project.player_ui_objects);
    state.ensure_default_3d_player_ui();
    let player_screens: Vec<crate::engine3d::ipc::PlayerUiScreenInfo> = project
        .playerUiScreens
        .iter()
        .map(|s| crate::engine3d::ipc::PlayerUiScreenInfo {
            id: s.id.clone(),
            name: s.name.clone(),
            active: s.active,
        })
        .collect();
    state.sync_player_ui_screens(&player_screens);
    if let Some(scene) = forced_scene
        && is_3d_placeholder_saved_scene(scene)
    {
        state.finalize_3d_placeholder_editor_scene();
    }
    restore_entity_sockets_after_scene_load(state, &view);
    restore_entity_bone_physics_after_scene_load(state, &view);
    restore_entity_projectile_config_after_scene_load(state, &view);
    restore_entity_attachments_after_scene_load(state, &view);
    state.sanitize_reflection_probe_entities();
    state.restoring_save_manifest = false;
    Ok(view)
}
