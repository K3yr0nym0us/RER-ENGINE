use super::*;

impl State {
    pub(super) fn cmd_player_ui(&mut self, cmd: EngineCommand) -> Option<EngineCommand> {
        match cmd {
            EngineCommand::Common(EngineCommandCommon::SetPlayerUiEditMode {
                active,
                scope,
                screen_id,
            }) => {
                self.apply_player_ui_edit_mode(active, scope.as_deref(), screen_id.as_deref());
            }
            EngineCommand::Common(EngineCommandCommon::AddPlayerUiTextBox { font_path }) => {
                match self.add_player_ui_text_box(&font_path) {
                    Ok(id) => {
                        if let Some(key) = self.player_ui_text_key()
                            && let Some(entry) = self
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
                    Err(message) => {
                        send_event(&EngineEvent::Error { message });
                    }
                }
            }
            EngineCommand::Common(EngineCommandCommon::RemovePlayerUiTextBox { id }) => {
                let removed_id = if let Some(box_id) = id {
                    self.remove_player_ui_text_box(box_id).then_some(box_id)
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
                if let Some(button_id) = id
                    && self.remove_player_ui_button(button_id)
                {
                    send_event(&EngineEvent::PlayerUiButtonRemoved { id: button_id });
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
                if let Err(message) =
                    self.set_player_ui_hud_element_props(&element_kind, id, locked, z_index)
                {
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
                    return None;
                }

                if playing && self.player_ui_edit_active {
                    self.apply_player_ui_edit_mode(false, None, None);
                }

                self.preview_playing = playing;
                self.apply_player_ui_play_hud(playing);

                if playing {
                    self.clear_active_projectiles();
                    self.capture_preview_editor_snapshots();
                    self.reset_play_controller_motion();
                    self.ensure_play_character_kinematic_only();
                    self.capture_play_camera_follow_offset();
                    self.active_tool = ActiveTool::None;
                    self.tool_overlay_buffer =
                        crate::engine3d::gizmo::build_from_vertices(&self.vulkan, &[]);
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
                    self.clear_active_projectiles();
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

                log::info!(
                    "[preview] modo {}",
                    if playing { "juego" } else { "editor" }
                );
            }
            other => return Some(other),
        }
        None
    }
}
