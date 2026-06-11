use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use glam::Vec3 as GlamVec3;
use rodio;
use rodio::Source as RodioSource;
use winit::dpi::PhysicalSize;

use crate::config_2d::ActiveTool;
use crate::ecs::{NameComponent, Transform};
use crate::gizmo;
use crate::ipc::{send_event, AnimationFrameData, EngineCommand, EngineEvent};

use super::audio::DecodedAudio;
use super::types::{ActiveAnimation, AnimationState};
use super::State;

impl State {
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
            }            EngineCommand::SetBounds { x, y, width, height, .. } => {
                // Mover/redimensionar la ventana overlay
                let _ = self.window.set_outer_position(
                    winit::dpi::PhysicalPosition::new(x, y)
                );
                // Redimensionar superficie wgpu
                self.resize(PhysicalSize::new(width, height));
                // Pedir al compositor que aplique el nuevo tamaño
                let _ = self.window.request_inner_size(
                    winit::dpi::PhysicalSize::new(width, height)
                );
            }
            EngineCommand::LoadModel { path } => {
                send_event(&EngineEvent::Error {
                    message: format!("Este binario es 2D: carga de modelos no disponible ({path})"),
                });
            }
            EngineCommand::ReplaceEntityModel { id, path } => {
                send_event(&EngineEvent::Error {
                    message: format!(
                        "Este binario es 2D: reemplazo de modelo no disponible (entidad {id}, {path})"
                    ),
                });
            }
            EngineCommand::SetTransform { id, position, rotation, scale, track_undo } => {
                use glam::{Quat, Vec3};
                let before = self.world.get::<Transform>(id).cloned();
                if let Some(transform) = self.world.get_mut::<Transform>(id) {
                    if let Some(p) = position {
                        transform.position = Vec3::from(p);
                    }
                    if let Some(r) = rotation {
                        transform.rotation = Quat::from_xyzw(r[0], r[1], r[2], r[3]);
                    }
                    if let Some(s) = scale {
                        transform.scale = Vec3::from(s);
                    }
                }
                // Si la entidad está mostrando frames animados, mantener sincronizada
                // la base (orig_pos/orig_scale) para que el siguiente frame respete
                // los cambios hechos desde el panel de Transformaciones.
                if let Some(saved) = self.anim_saved_transforms.get_mut(&id) {
                    if let Some(p) = position {
                        saved.0 = Vec3::from(p);
                    }
                    if let Some(s) = scale {
                        saved.1 = Vec3::from(s);
                    }
                }
                // Ruta de compatibilidad editor -> fisica.
                // El Transform ya fue mutado; aqui solo resincronizamos el body.
                // No usar esto como sustituto del movimiento normal de gameplay.
                if let Some(p) = position {
                    if self.camera_2d.is_some() {
                        self.sync_physics_2d_body_from_xy(id, p[0], p[1]);
                    }
                }
                if let Some(prev) = before {
                    let prev_pos = prev.position.to_array();
                    let prev_rot = [prev.rotation.x, prev.rotation.y, prev.rotation.z, prev.rotation.w];
                    let prev_scl = prev.scale.to_array();
                    let next_pos = self.world.get::<Transform>(id).map(|t| t.position.to_array());
                    let next_rot = self.world.get::<Transform>(id).map(|t| [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w]);
                    let next_scl = self.world.get::<Transform>(id).map(|t| t.scale.to_array());
                    let should_track_undo = track_undo.unwrap_or(true);
                    if !self.is_applying_undo
                        && should_track_undo
                        && (next_pos != Some(prev_pos) || next_rot != Some(prev_rot) || next_scl != Some(prev_scl)) {
                        self.push_undo_transform(id, prev_pos, prev_rot, prev_scl);
                    }
                }
            }
            EngineCommand::SetEntityName { id, name, force } => {
                let next_name = name.trim();
                if next_name.is_empty() {
                    send_event(&EngineEvent::Error { message: "El nombre no puede estar vacio".to_string() });
                    return;
                }

                if !force && self.is_entity_name_taken(next_name, Some(id)) {
                    send_event(&EngineEvent::Error { message: format!("Ya existe una entidad con el nombre '{}'", next_name) });
                    return;
                }

                if let Some(existing) = self.world.get_mut::<NameComponent>(id) {
                    existing.name = next_name.to_string();
                } else {
                    self.world.insert(id, NameComponent { name: next_name.to_string() });
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
                    let physics_enabled = self.physics_2d.has_physics(id);
                    let physics_type = self.physics_2d.get_body_type(id).to_string();

                    send_event(&EngineEvent::EntitySelected {
                        id,
                        name: next_name.to_string(),
                        position,
                        rotation,
                        scale,
                        physics_enabled,
                        physics_type,
                    });
                }
            }
            EngineCommand::SetScene { scene, save_path } => {
                match scene.as_str() {
                    "2D" => {
                        self.setup_2d_platformer();
                        if let Some(path) = save_path.filter(|p| !p.trim().is_empty()) {
                            self.load_proyect_from_save_path(&path);
                        }
                    }
                    "scratch" => self.setup_scratch(),
                    _         => log::info!("SetScene: escena '{}' no reconocida", scene),
                }
            }
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
                let marker = self.world.get::<crate::config_2d::ScenarioMarker>(id).cloned();
                if let Some(m) = marker {
                    let aspect = m.img_width as f32 / m.img_height.max(1) as f32;
                    let new_h  = m.base_world_h * scale.clamp(0.05, 20.0);
                    let new_w  = new_h * aspect;
                    if let Some(t) = self.world.get_mut::<Transform>(id) {
                        // Mantener el comportamiento actual: cambia la escala visual
                        // sin recomponer automaticamente collider/shape en esta fase.
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
            EngineCommand::PlayAnimationFrame { id, path, pivot_x, pivot_y, logical_w, logical_h, src_x, src_y, src_w, src_h } => {
                if self.pivot_edit_mode.is_some() {
                    // Ignorar: el modo edición de pivot tiene prioridad para no interferir con la textura/escala
                    return;
                }
                let (pivot_x, pivot_y) = match (pivot_x, pivot_y) {
                    (Some(x), Some(y)) => (x, y),
                    _ => (logical_w.max(1) as f32 * 0.5, logical_h.max(1) as f32),
                };
                self.play_animation_frame(id, &path, pivot_x, pivot_y, logical_w, logical_h, src_x.zip(src_y).zip(src_w.zip(src_h)).map(|((x, y), (w, h))| (x, y, w, h)), false);
            }
            EngineCommand::RestoreAnimationFrame { id } => {
                self.restore_animation_frame(id);
            }
            EngineCommand::SetPivotEditMode { id, frame_path, pivot_x, pivot_y } => {
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
                // Decodificar a PCM y enviar al Sink persistente.
                let decoded = std::fs::read(&path).ok()
                    .and_then(|b| {
                        let cursor = std::io::Cursor::new(b);
                        rodio::Decoder::new(cursor).ok().map(|dec| {
                            let ch = dec.channels();
                            let sr = dec.sample_rate();
                            let s: Vec<i16> = dec.collect();
                            Arc::new(DecodedAudio { samples: s, channels: ch, sample_rate: sr })
                        })
                    });
                match decoded {
                    Some(audio) => self.play_audio_internal(audio, loop_),
                    None => log::error!("[audio] no se pudo cargar o decodificar: {path}"),
                }
            }
            EngineCommand::StopAudio => {
                self.stop_audio_internal();
            }
            EngineCommand::DeselectEntity => {
                if self.selected_entity.is_some() || !self.selected_entities.is_empty() {
                    self.selected_entity = None;
                    self.selected_entities.clear();
                    send_event(&EngineEvent::EntityDeselected);
                }
            }
            EngineCommand::RemoveEntity { id } => {
                if !self.is_applying_undo {
                    if let Some(snapshot) = self.capture_entity_undo_snapshot(id) {
                        self.undo_stack
                            .push(crate::engine::UndoAction::RestoreEntity { snapshot });
                        self.redo_stack.clear();
                    }
                }
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
                if Some(id) == self.hovered_entity  {
                    self.hovered_entity  = None;
                    send_event(&EngineEvent::EntityUnhovered);
                }
                self.physics_2d.remove_entity_body(id);
                self.scenario_entities.retain(|&e| e != id);
                self.character_entities.retain(|&e| e != id);
                self.collider_entities.retain(|&e| e != id);
                self.execution_area_entities.retain(|&e| e != id);
                self.execution_overlaps.retain(|(trigger_id, actor_id)| *trigger_id != id && *actor_id != id);
                self.anim_flip_overrides.remove(&id);
                self.entity_facing_right.remove(&id);
                self.default_animation_by_entity.remove(&id);
                self.animations.remove(&id);
                self.active_animations.remove(&id);
                self.anim_saved_transforms.remove(&id);
                self.control_bindings_by_entity.remove(&id);
                self.blocked_on_keep_horizontal.remove(&id);
                self.pending_slides.remove(&id);
                let points = if removed_kind == "collider" || removed_kind == "execution_area" {
                    self.save_registry
                        .meta
                        .get(&id)
                        .and_then(|m| m.points)
                        .or_else(|| self.collider_points_from_transform(id))
                } else {
                    None
                };
                self.save_registry.remove_entity(id);
                self.script_engine.detach_entity(id);
                self.world.despawn(id);
                send_event(&EngineEvent::EntityRemoved {
                    id,
                    kind: removed_kind.to_string(),
                    points,
                });
            }
            EngineCommand::SetWorldSize { width, height } => {
                self.grid_config.world_width  = width.max(1.0);
                self.grid_config.world_height = height.max(1.0);
                self.rebuild_grid();
                // Redimensionar el fondo si existe
                if let Some(bg_id) = self.background_entity {
                    if let Some(t) = self.world.get_mut::<Transform>(bg_id) {
                        t.scale = GlamVec3::new(self.grid_config.world_width, self.grid_config.world_height, 1.0);
                    }
                }
            }
            EngineCommand::SetGravity { gravity } => {
                // Contrato legacy actual: el IPC acepta un valor "gravedad" y el
                // runtime lo normaliza siempre hacia abajo para preservar semantica.
                // Revisarlo cambiaria comportamiento de proyectos existentes.
                self.physics_2d.set_gravity(-gravity.abs());
                log::info!("[physics] Gravedad actualizada: -{:.2}", gravity.abs());
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

                if playing && self.player_ui_edit_active {
                    self.apply_player_ui_edit_mode(false, None, None);
                }

                self.preview_playing = playing;
                self.apply_player_ui_play_hud(playing);

                if playing {
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

                    // Forzar un paso de física inicial antes de arrancar
                    // las animaciones default. Esto sincroniza contactos/grounded
                    // después de restaurar transformaciones del editor.
                    self.physics_2d.step(1.0 / 60.0, &mut self.world);

                    // Al entrar en modo juego, reproducir la animación predeterminada
                    // de cada entidad que tenga animaciones registradas.
                    // Limpiar caché de scripts compilados para que ediciones en el editor surtan efecto.
                    self.script_engine.clear_control_script_cache();
                    self.run_scene_script_on_play_start();
                    let entities_with_anims: Vec<u32> = self.animations.keys().copied().collect();
                    let default_count = self.default_animation_by_entity.len();
                    log::info!("[SetPreviewPlaying] entidades con animaciones: {}, con default: {}", entities_with_anims.len(), default_count);
                    for entity_id in entities_with_anims {
                        let default_name = self.default_animation_by_entity.get(&entity_id).cloned();
                        if let Some(name) = default_name {
                            log::info!("[SetPreviewPlaying] iniciando animación '{}' para entidad {}", name, entity_id);
                            self.handle_command(EngineCommand::PlayAnimation { id: entity_id, name });
                        }
                    }
                } else {
                    self.script_engine.reset_scene_play_state();
                    // Al volver al modo editor, detener todas las animaciones activas
                    // y mostrar el primer frame de la animación correspondiente.
                    let active: Vec<(u32, String)> = self.active_animations
                        .iter()
                        .map(|(&id, a)| (id, a.animation_name.clone()))
                        .collect();
                    self.active_animations.clear();
                    for (entity_id, anim_name) in active {
                        self.script_engine.detach_animation_scripts(entity_id);
                        self.show_first_frame_of_animation(entity_id, &anim_name);
                    }
                    self.stop_audio_internal();
                }

                self.execution_overlaps.clear();
                self.blocked_on_keep_horizontal.clear();
                self.pending_slides.clear();

                log::info!(
                    "[preview] modo {}",
                    if playing { "juego" } else { "editor" }
                );
            }
            EngineCommand::SetCtrlHeld { held } => {
                self.ctrl_held = held;
            }
            EngineCommand::SetCamera2d { x, y, half_h } => {
                if let Some(cam2d) = &mut self.camera_2d {
                    cam2d.x      = x;
                    cam2d.y      = y;
                    cam2d.half_h = half_h.clamp(1.0, 50.0);
                    log::debug!("Cámara 2D restaurada: x={x} y={y} half_h={half_h}");
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
                let (pos, half, collider_offset) = if let Some(t) = self.world.get::<Transform>(id) {
                    // Forzar z=0 en la posición física: el Z del Transform es visual
                    // (orden de capas), pero la física usa XYZ interno. Si dos cuerpos
                    // tienen Z distinto no colisionan aunque se solapen en XY.
                    let mut p = t.position.to_array();
                    p[2] = 0.0;
                    if self.character_entities.contains(&id) {
                        if let Some((shape_half, shape_offset)) = crate::config_2d::character_collision_shape(self, id) {
                            (p, shape_half, shape_offset)
                        } else {
                            (p, (t.scale * 0.5).to_array(), [0.0, 0.0, 0.0])
                        }
                    } else {
                        (p, (t.scale * 0.5).to_array(), [0.0, 0.0, 0.0])
                    }
                } else {
                    ([0.0_f32; 3], [0.5_f32; 3], [0.0, 0.0, 0.0])
                };
                self.physics_2d
                    .set_entity_physics(id, enabled, &body_type, pos, half, collider_offset);
                log::debug!("Física {}: entidad {} tipo='{}'",
                    if enabled { "activada" } else { "desactivada" }, id, body_type);
                send_event(&EngineEvent::PhysicsChanged { entity_id: id, enabled, body_type });
            }
            EngineCommand::SetActiveTool { tool, preview_path, preview_kind, preview_scale, preview_src_rect } => {
                if tool.is_empty() {
                    // Solo cancelar si había una herramienta activa (evita eventos espurios al inicio)
                    let was_active = !matches!(self.active_tool, ActiveTool::None);
                    // Limpiar entidad fantasma de quick_build si existía
                    if let Some(ghost_id) = self.quick_build_ghost_id.take() {
                        self.world.despawn(ghost_id);
                    }
                    self.quick_build_preview_path = None;
                    self.quick_build_preview_kind = None;
                    self.quick_build_preview_scale = None;
                    self.active_tool = ActiveTool::None;
                    self.tool_overlay_buffer = gizmo::build_from_vertices(&self.device, &[]);
                    if was_active {
                        send_event(&EngineEvent::ToolCancelled);
                        log::info!("Herramienta cancelada");
                    }
                } else {
                    // Limpiar entidad fantasma previa si la hay
                    if let Some(ghost_id) = self.quick_build_ghost_id.take() {
                        self.world.despawn(ghost_id);
                    }
                    self.quick_build_preview_path = None;
                    self.quick_build_preview_kind = None;
                    self.quick_build_preview_scale = None;
                    match tool.as_str() {
                        "draw_collider" => {
                            self.active_tool = ActiveTool::DrawCollider { points_world: Vec::new(), cursor_world: None };
                            log::info!("Herramienta activa: dibujar colisionador (4 puntos)");
                        }
                        "draw_execution_area" => {
                            self.active_tool = ActiveTool::DrawExecutionArea { points_world: Vec::new(), cursor_world: None };
                            log::info!("Herramienta activa: dibujar área de ejecución (4 puntos)");
                        }
                        "quick_build_place" => {
                            self.active_tool = ActiveTool::QuickBuildPlace { cursor_world: None };
                            self.tool_overlay_buffer = crate::gizmo::build_from_vertices(&self.device, &[]);
                            // Cargar entidad fantasma si se proporcionaron datos del blueprint
                            if let (Some(path), Some(kind), Some(scale)) = (preview_path.as_deref(), preview_kind.as_deref(), preview_scale) {
                                self.quick_build_preview_path = Some(path.to_owned());
                                self.quick_build_preview_kind = Some(kind.to_owned());
                                self.quick_build_preview_scale = Some(scale);
                                self.quick_build_ghost_id = self.load_quick_build_ghost(path, kind, scale, preview_src_rect);
                            }
                            log::info!("Herramienta activa: construcción rápida");
                        }
                        _ => log::warn!("Herramienta desconocida: {}", tool),
                    }
                }
            }
            EngineCommand::CreateColliderFromPoints { points, track_undo } => {
                if self.camera_2d.is_some() {
                    self.create_collision_box_from_points(&points, track_undo.unwrap_or(true));
                } else {
                    log::warn!("CreateColliderFromPoints solo disponible en modo 2D");
                }
            }
            EngineCommand::CreateExecutionAreaFromPoints { points, track_undo } => {
                if self.camera_2d.is_some() {
                    self.create_execution_area_from_points(&points, track_undo.unwrap_or(true));
                } else {
                    log::warn!("CreateExecutionAreaFromPoints solo disponible en modo 2D");
                }
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
            EngineCommand::SetPlayerUiEditMode {
                active,
                scope,
                screen_id,
            } => {
                self.apply_player_ui_edit_mode(
                    active,
                    scope.as_deref(),
                    screen_id.as_deref(),
                );
            }
            EngineCommand::AddPlayerUiTextBox { font_path } => {
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
            EngineCommand::RemovePlayerUiTextBox { id } => {
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
            EngineCommand::AddPlayerUiButton { payload } => {
                if let Err(message) = self.add_player_ui_button(payload) {
                    send_event(&EngineEvent::Error { message });
                }
            }
            EngineCommand::RemovePlayerUiButton { id } => {
                if let Some(button_id) = id {
                    if self.remove_player_ui_button(button_id) {
                        send_event(&EngineEvent::PlayerUiButtonRemoved { id: button_id });
                    }
                }
            }
            EngineCommand::AddPlayerUiImage { image_path } => {
                if let Err(message) = self.add_player_ui_image(&image_path) {
                    send_event(&EngineEvent::Error { message });
                }
            }
            EngineCommand::RemovePlayerUiImage { id } => {
                if let Some(image_id) = id {
                    let _ = self.remove_player_ui_image(image_id);
                } else {
                    let _ = self.remove_selected_player_ui_image();
                }
            }
            EngineCommand::SetPlayerUiObjectDraw { active } => {
                self.set_player_ui_object_draw(active);
            }
            EngineCommand::RemovePlayerUiObject { id } => {
                if let Some(object_id) = id {
                    let _ = self.remove_player_ui_object(object_id);
                } else if let Some(object_id) = self.player_ui_selected_object_id {
                    let _ = self.remove_player_ui_object(object_id);
                }
            }
            EngineCommand::SetPlayerUiHudElementProps {
                element_kind,
                id,
                locked,
                z_index,
            } => {
                if let Err(message) = self.set_player_ui_hud_element_props(
                    &element_kind,
                    id,
                    locked,
                    z_index,
                ) {
                    send_event(&EngineEvent::Error { message });
                }
            }
            EngineCommand::SetPlayerUiObjectStyle {
                id,
                fill_color,
                texture_path,
                clear_texture,
                live,
                skip_undo,
            } => {
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
            EngineCommand::SyncPlayerUiScreens { screens } => {
                self.sync_player_ui_screens(&screens);
            }
            EngineCommand::SetActivePlayerUiScreen { screen_id } => {
                match screen_id.filter(|id| !id.is_empty()) {
                    Some(id) => {
                        if let Err(message) = self.set_active_player_ui_screen(&id) {
                            send_event(&EngineEvent::Error { message });
                        }
                    }
                    None => self.clear_active_player_ui_screen(),
                }
            }
            EngineCommand::SetDebugMode { show } => {
                self.debug_mode = show;
                self.physics_2d.set_debug_mode(show);
                log::info!("[debug] modo debug: {}", show);
            }
            EngineCommand::SetVsync { enabled } => {
                self.set_vsync(enabled);
            }
            EngineCommand::ReloadAsset { path } => {
                log::info!("[IPC] ReloadAsset: {}", path);
                // Buscar UV rect pre-asignado en el atlas (sin re-empacar, sin cambiar ECS).
                if let Some(&uv_rect) = self.static_tex_cache.get(&path) {
                    match std::fs::read(&path) {
                        Ok(bytes) => {
                            use image::ImageReader;
                            match ImageReader::new(std::io::Cursor::new(&bytes))
                                .with_guessed_format()
                                .map_err(|e| e.to_string())
                                .and_then(|r| r.decode().map_err(|e| e.to_string()))
                            {
                                Ok(img) => {
                                    let rgba = img.to_rgba8();
                                    self.atlas.update(&self.queue, rgba.as_raw(), uv_rect);
                                    log::info!("[hot-reload] Textura actualizada en atlas: {}", path);
                                }
                                Err(e) => log::warn!("[hot-reload] Error decodificando PNG '{}': {}", path, e),
                            }
                        }
                        Err(e) => log::warn!("[hot-reload] Error leyendo archivo '{}': {}", path, e),
                    }
                } else if self.background_path.as_deref() == Some(path.as_str()) {
                    // El fondo usa GpuTexture propia, no el atlas — recargarlo completo.
                    self.load_background(&path);
                    log::info!("[hot-reload] Fondo recargado: {}", path);
                } else {
                    log::warn!("[hot-reload] Path no encontrado en static_tex_cache ni como fondo: {}", path);
                }
            }
            EngineCommand::SetAnimation { id, name, frames, fps, loop_, flip_horizontal, audio_path, logical_w, logical_h, scripts, is_cancelable, .. } => {
                log::debug!("[IPC] SetAnimation: entity_id={}, name='{}', frames={}, audio={:?}, scripts={}", id, name, frames.len(), audio_path, scripts.len());

                let fallback_logical_w = logical_w.unwrap_or(64).max(1);
                let fallback_logical_h = logical_h.unwrap_or(64).max(1);

                let measure_bounds = |anim_frames: &Vec<AnimationFrameData>, fallback_w: u32, fallback_h: u32| -> (u32, u32) {
                    let mut max_w = fallback_w.max(1);
                    let mut max_h = fallback_h.max(1);
                    for frame in anim_frames {
                        max_w = max_w.max(frame.src_w.unwrap_or(fallback_w).max(1));
                        max_h = max_h.max(frame.src_h.unwrap_or(fallback_h).max(1));
                    }
                    (max_w, max_h)
                };

                let mut frames = frames;
                let (measured_w, measured_h) = measure_bounds(&frames, fallback_logical_w, fallback_logical_h);

                // Espacio de dibujo: solo lo que envía el editor o el máximo de frames de esta animación.
                let resolved_logical_w = logical_w.unwrap_or(measured_w).max(1);
                let resolved_logical_h = logical_h.unwrap_or(measured_h).max(1);

                for frame in &mut frames {
                    if frame.pivot_x.is_none() || frame.pivot_y.is_none() {
                        frame.pivot_x = Some(resolved_logical_w as f32 * 0.5);
                        frame.pivot_y = Some(resolved_logical_h as f32);
                    }
                }

                // Pre-decodificar audio a muestras PCM durante SetAnimation.
                // En PlayAnimation solo se clona un Vec<i16> — cero I/O, cero decode.
                let audio_decoded: Option<Arc<DecodedAudio>> = audio_path.as_deref().and_then(|p| {
                    let bytes = match std::fs::read(p) {
                        Ok(b) => b,
                        Err(e) => { log::warn!("[SetAnimation] error leyendo audio {}: {}", p, e); return None; }
                    };
                    let cursor = std::io::Cursor::new(bytes);
                    let decoder = match rodio::Decoder::new(cursor) {
                        Ok(d) => d,
                        Err(e) => { log::warn!("[SetAnimation] error decodificando audio {}: {}", p, e); return None; }
                    };
                    let channels    = decoder.channels();
                    let sample_rate = decoder.sample_rate();
                    let samples: Vec<i16> = decoder.collect();
                    log::debug!("[SetAnimation] audio decodificado: {} ({} muestras, {}ch, {}Hz)",
                        p, samples.len(), channels, sample_rate);
                    Some(Arc::new(DecodedAudio { samples, channels, sample_rate }))
                });

                // Pre-cargar todos los frames de la animación en la caché GPU.
                // El primer PlayAnimation ya no tendrá latencia de decode+upload.
                for frame in &frames {
                    self.preload_anim_frame_with_rect(
                        &frame.path,
                        frame.src_x.zip(frame.src_y).zip(frame.src_w.zip(frame.src_h)).map(|((x, y), (w, h))| (x, y, w, h)),
                    );
                }

                // Guardar animación en el almacén por entidad+nombre.
                self.animations
                    .entry(id)
                    .or_insert_with(HashMap::new)
                    .insert(name.clone(), AnimationState {
                        frames,
                        fps,
                        loop_,
                        flip_horizontal,
                        audio_decoded,
                        logical_w: resolved_logical_w,
                        logical_h: resolved_logical_h,
                        scripts,
                        is_cancelable,
                    });
                send_event(&EngineEvent::AnimationLogicalResolved {
                    id,
                    name: name.clone(),
                    logical_w: resolved_logical_w,
                    logical_h: resolved_logical_h,
                });
                log::debug!("[IPC] Animación '{}' guardada y pre-cargada para entidad {}", name, id);
            }
            EngineCommand::RemoveAnimation { id, name } => {
                log::info!("[IPC] RemoveAnimation: entity_id={}, name='{}'", id, name);

                // Detener la animación si está activa
                if let Some(active) = self.active_animations.get(&id) {
                    if active.animation_name == name {
                        self.active_animations.remove(&id);
                        self.restore_animation_frame(id);
                        self.script_engine.detach_animation_scripts(id);
                        send_event(&EngineEvent::AnimationFinished { entity_id: id });
                    }
                }

                // Eliminar del almacén de animaciones
                if let Some(entity_anims) = self.animations.get_mut(&id) {
                    entity_anims.remove(&name);
                    log::debug!("[animation] Eliminada '{}' de entidad {}", name, id);

                    // Si no quedan animaciones, limpiar la entrada
                    if entity_anims.is_empty() {
                        self.animations.remove(&id);
                        self.default_animation_by_entity.remove(&id);
                    }
                }

                if self.default_animation_by_entity.get(&id) == Some(&name) {
                    self.default_animation_by_entity.remove(&id);
                    log::debug!("[animation] predeterminada eliminada al borrar '{}' (entidad {})", name, id);
                }
            }
            EngineCommand::SetDefaultAnimation { id, name } => {
                if name.is_empty() {
                    self.default_animation_by_entity.remove(&id);
                    log::debug!("[animation] sin predeterminada para entidad {}", id);
                    return;
                }
                let exists = self.animations
                    .get(&id)
                    .map(|m| m.contains_key(&name))
                    .unwrap_or(false);
                if exists {
                    self.default_animation_by_entity.insert(id, name.clone());
                    log::debug!("[animation] predeterminada de entidad {} => {}", id, name);
                } else {
                    log::warn!("[animation] set_default_animation ignorado: '{}' no existe en entidad {}", name, id);
                }
            }
EngineCommand::PlayAnimation { id, name } => {
                log::debug!("[handle_command] PlayAnimation: id={}, name='{}'", id, name);

                // Si hay una animación activa con is_cancelable=false que aún no terminó,
                // bloquear la nueva hasta que termine naturalmente.
                if let Some(active) = self.active_animations.get(&id) {
                    if !active.finished {
                        let current_name = active.animation_name.clone();
                        let is_cancelable = self.animations
                            .get(&id)
                            .and_then(|m| m.get(&current_name))
                            .map(|a| a.is_cancelable)
                            .unwrap_or(true); // default: cancelable
                        if !is_cancelable {
                            log::debug!("[animation] PlayAnimation '{}' bloqueado: '{}' no es cancelable en entidad {}", name, current_name, id);
                            return;
                        }
                    }
                }

                let anim_opt = self.animations.get(&id)
                    .and_then(|m| m.get(&name))
                    .cloned();

                match anim_opt {
                    None => log::warn!("[IPC] Animación '{}' no encontrada para entidad {}", name, id),
                    Some(anim) => {
                        // Detener animación previa (el Play de audio incluye clear interno)
                        self.active_animations.remove(&id);

                        // Re-baseline: posición actual + escala base antes del nuevo pivot.
                        if let Some(t) = self.world.get::<Transform>(id).cloned() {
                            self.anim_saved_transforms
                                .entry(id)
                                .and_modify(|saved| {
                                    saved.0 = t.position;
                                })
                                .or_insert((t.position, t.scale));
                        }
                        self.prepare_character_animation_visual(id);

                        // Capturar el tiempo ANTES del I/O de archivos para que
                        // last_frame_time refleje el inicio real del frame 0, no el
                        // tiempo después de cargar texturas/audio (puede ser 50-200ms más tarde).
                        let frame_start = Instant::now();
                        let effective_flip = self.resolve_animation_flip(id, &anim);

                        // Mostrar frame 0 (cache miss solo en el primer play)
                        if let Some(first_frame) = anim.frames.first() {
                            let (pivot_x, pivot_y) =
                                first_frame.resolved_pivot(anim.logical_w, anim.logical_h);
                            self.play_animation_frame(
                                id,
                                &first_frame.path,
                                pivot_x,
                                pivot_y,
                                anim.logical_w,
                                anim.logical_h,
                                first_frame.src_x.zip(first_frame.src_y).zip(first_frame.src_w.zip(first_frame.src_h)).map(|((x, y), (w, h))| (x, y, w, h)),
                                effective_flip,
                            );
                        }

                        // Iniciar audio desde PCM pre-decodificado (cero I/O, cero decode)
                        if let Some(ref audio_decoded) = anim.audio_decoded {
                            self.play_audio_internal(Arc::clone(audio_decoded), anim.loop_);
                        }

                        // Reemplazar los scripts de animación anteriores por los de la nueva.
                        // Los scripts de entidad (LoadScript) se preservan intactos.
                        self.script_engine.detach_animation_scripts(id);
                        for script in &anim.scripts {
                            let anim_path = format!("$anim$::{}::{}", name, script.name);
                            if let Err(e) = self.script_engine.attach_script(id, &anim_path, &script.source) {
                                log::error!("[scripting] Error cargando script de animación '{}': {}", anim_path, e);
                            }
                        }
                        if !anim.scripts.is_empty() {
                            log::debug!("[scripting] {} script(s) de animación '{}' cargados para entidad {}", anim.scripts.len(), name, id);
                        }

                        self.active_animations.insert(id, ActiveAnimation {
                            animation_name: name.clone(),
                            current_frame: 0,
                            last_frame_time: frame_start,
                            fps: anim.fps,
                            finished: false,
                        });
                        log::debug!("[animation] Iniciada '{}' para entidad {} (fps={}, frames={})", name, id, anim.fps, anim.frames.len());
                    }
                }
            }
            EngineCommand::StopAnimation { id } => {
                self.anim_flip_overrides.remove(&id);
                let Some(stopped_animation_name) = self.active_animations.remove(&id).map(|a| a.animation_name) else {
                    // Ya estaba detenida: no repetir fallback, audio stop, detach ni logs.
                    return;
                };
                if self.preview_playing {
                    if let Some(name) = self.default_animation_by_entity.get(&id).cloned() {
                        self.show_first_frame_of_animation(id, &name);
                    } else {
                        self.restore_animation_frame(id);
                    }
                } else {
                    // En modo edición no hay fallback automático a la predeterminada.
                    self.show_first_frame_of_animation(id, &stopped_animation_name);
                }
                self.stop_audio_internal();
                // Descargar scripts de la animación que estaba activa.
                self.script_engine.detach_animation_scripts(id);
                send_event(&EngineEvent::AnimationFinished { entity_id: id });
            }
            EngineCommand::LoadSceneVisualScript { scene_id, source } => {
                log::debug!("[IPC] LoadSceneVisualScript: scene_id={}", scene_id);
                if let Err(e) = self.handle_load_scene_visual_script(scene_id, &source) {
                    log::error!("[scene_script] Error: {e}");
                    send_event(&EngineEvent::Error {
                        message: format!("Error en script de escena: {e}"),
                    });
                }
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
            EngineCommand::RunControlScript { id, control_key, path, source } => {
                self.execute_control_script(id, &control_key, &path, &source);
            }
            EngineCommand::UnloadScript { id } => {
                log::info!("[IPC] UnloadScript: entity_id={}", id);
                self.script_engine.detach_entity(id);
                self.save_registry.script_sources.remove(&id);
            }
            EngineCommand::LoadSprite { path, name } => {
                // Cargar PNG y almacenar sus dimensiones; no se crea entidad ECS.
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
                                send_event(&EngineEvent::SpriteLoaded { path, name, width: w, height: h });
                                log::debug!("[sprite] cargado: {} ({}) ({}x{})", path_for_log, name_for_log, w, h);
                            }
                            Err(e) => {
                                log::error!("[sprite] error decodificando {}: {}", path, e);
                                send_event(&EngineEvent::Error { message: format!("Error al decodificar sprite: {e}") });
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("[sprite] error leyendo {}: {}", path, e);
                        send_event(&EngineEvent::Error { message: format!("No se pudo leer el sprite: {e}") });
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
                let sprites: Vec<crate::ipc::SpriteInfo> = self.sprite_store
                    .iter()
                    .map(|(path, &(ref name, w, h))| crate::ipc::SpriteInfo { path: path.clone(), name: name.clone(), width: w, height: h })
                    .collect();
                let count = sprites.len();
                send_event(&EngineEvent::SpritesList { sprites });
                log::debug!("[sprite] lista enviada: {} sprites", count);
            }
            EngineCommand::LoadSound { path, name } => {
                self.sound_store.insert(path.clone(), name.clone());
                send_event(&EngineEvent::SoundLoaded { path: path.clone(), name: name.clone() });
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
                let sounds: Vec<crate::ipc::SoundInfo> = self.sound_store
                    .iter()
                    .map(|(path, name)| crate::ipc::SoundInfo { path: path.clone(), name: name.clone() })
                    .collect();
                let count = sounds.len();
                send_event(&EngineEvent::SoundsList { sounds });
                log::debug!("[sound] lista enviada: {} sonidos", count);
            }
            EngineCommand::LoadFont { path, name } => {
                match validate_font_file(&path) {
                    Ok(()) => {
                        self.font_store.insert(path.clone(), name.clone());
                        send_event(&EngineEvent::FontLoaded {
                            path: path.clone(),
                            name: name.clone(),
                        });
                        log::debug!("[font] registrada: {} ({})", path, name);
                    }
                    Err(e) => {
                        log::error!("[font] error cargando {}: {}", path, e);
                        send_event(&EngineEvent::Error {
                            message: format!("Error al cargar fuente: {e}"),
                        });
                    }
                }
            }
            EngineCommand::RemoveFont { path } => {
                if self.font_store.remove(&path).is_some() {
                    send_event(&EngineEvent::FontRemoved { path: path.clone() });
                    log::info!("[font] eliminada: {}", path);
                } else {
                    log::warn!("[font] intento de eliminar fuente inexistente: {}", path);
                }
            }
            EngineCommand::GetFontsList => {
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
                log::debug!("[font] lista enviada: {} fuentes", count);
            }
            EngineCommand::LoadHudImage { path, name } => {
                match crate::hud_image_asset::validate_hud_image_file(&path) {
                    Ok((width, height)) => {
                        self.hud_image_store.insert(
                            path.clone(),
                            crate::hud_image_asset::HudImageAssetMeta {
                                name: name.clone(),
                                width_px: width,
                                height_px: height,
                            },
                        );
                        send_event(&EngineEvent::HudImageLoaded {
                            path: path.clone(),
                            name: name.clone(),
                            width,
                            height,
                        });
                        log::debug!("[hud-image] registrada: {} ({})", path, name);
                    }
                    Err(e) => {
                        log::error!("[hud-image] error cargando {}: {}", path, e);
                        send_event(&EngineEvent::Error {
                            message: format!("Error al cargar imagen HUD: {e}"),
                        });
                    }
                }
            }
            EngineCommand::RemoveHudImage { path } => {
                if self.hud_image_store.remove(&path).is_some() {
                    send_event(&EngineEvent::HudImageRemoved { path: path.clone() });
                    log::info!("[hud-image] eliminada: {}", path);
                } else {
                    log::warn!("[hud-image] intento de eliminar imagen inexistente: {}", path);
                }
            }
            EngineCommand::GetHudImagesList => {
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
                send_event(&EngineEvent::HudImagesList { images });
            }
            EngineCommand::LoadBackgroundAsset { path, name } => {
                self.background_store.insert(path.clone(), name.clone());
                send_event(&EngineEvent::BackgroundAssetLoaded { path: path.clone(), name: name.clone() });
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
                let backgrounds: Vec<crate::ipc::BackgroundInfo> = self.background_store
                    .iter()
                    .map(|(path, name)| crate::ipc::BackgroundInfo { path: path.clone(), name: name.clone() })
                    .collect();
                let count = backgrounds.len();
                send_event(&EngineEvent::BackgroundsList { backgrounds });
                log::debug!("[background] lista enviada: {} fondos", count);
            }
            EngineCommand::ListEntityTextures { id: _ }
            | EngineCommand::SetEntityTextureLod {
                id: _,
                material_index: _,
                tier: _,
                image_index: _,
            }
            | EngineCommand::SetEntityTexturePreviewTier { id: _, tier: _ }
            | EngineCommand::SetGraphicsTextureTier { tier: _ }
            | EngineCommand::SetEntityTexturesPreviewFocus { .. }
            | EngineCommand::ResendAllModelClips => {}
            EngineCommand::ApplyEntityRestore {
                id,
                name,
                transform,
                physics,
                animations,
                scripts,
                control_bindings,
                omit_scale,
                skip_transform,
                apply_initial_animation_frame,
            } => {
                self.apply_entity_restore_inner(
                    id,
                    name,
                    &transform,
                    physics.as_ref(),
                    animations.as_deref(),
                    scripts.as_deref(),
                    control_bindings.as_ref(),
                    omit_scale,
                    skip_transform,
                    apply_initial_animation_frame.unwrap_or(true),
                );
            }
            EngineCommand::ImportScene(payload) => {
                self.import_scene(payload);
            }
            EngineCommand::Shutdown => {}
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
