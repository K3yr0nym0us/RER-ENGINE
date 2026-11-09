use super::*;

impl State {
    pub(super) fn cmd_animation(&mut self, cmd: EngineCommand) -> Option<EngineCommand> {
        match cmd {
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

                let measure_bounds = |anim_frames: &Vec<AnimationFrameData>,
                                      fallback_w: u32,
                                      fallback_h: u32|
                 -> (u32, u32) {
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

                let audio_decoded: Option<Arc<DecodedAudio>> =
                    audio_path.as_deref().and_then(|p| {
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
                        frame
                            .src_x
                            .zip(frame.src_y)
                            .zip(frame.src_w.zip(frame.src_h))
                            .map(|((x, y), (w, h))| (x, y, w, h)),
                    );
                }

                self.animations.entry(id).or_default().insert(
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

                if let Some(active) = self.active_animations.get(&id)
                    && active.animation_name == name
                {
                    self.active_animations.remove(&id);
                    self.restore_animation_frame(id);
                    self.script_engine.detach_animation_scripts(id);
                    send_event(&EngineEvent::AnimationFinished { entity_id: id });
                }

                if let Some(entity_anims) = self.animations.get_mut(&id) {
                    entity_anims.remove(&name);

                    if entity_anims.is_empty() {
                        self.animations.remove(&id);
                        self.default_animation_by_entity.remove(&id);
                    }
                }

                if let Some(default) = self.default_animation_by_entity.get(&id)
                    && default == &name
                {
                    let new_default = self
                        .animations
                        .get(&id)
                        .and_then(|m| m.keys().next().cloned());
                    match new_default {
                        Some(new_name) => {
                            self.default_animation_by_entity
                                .insert(id, new_name.clone());
                        }
                        None => {
                            self.default_animation_by_entity.remove(&id);
                        }
                    }
                }
            }
            EngineCommand::Common(EngineCommandCommon::SetDefaultAnimation { id, name }) => {
                if self.model_animation_bindings.contains_key(&id) {
                    self.set_default_model_clip(id, &name);

                    return None;
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
                    return None;
                }

                if let Some(active) = self.active_animations.get(&id)
                    && !active.finished
                {
                    let current_name = active.animation_name.clone();
                    let is_cancelable = self
                        .animations
                        .get(&id)
                        .and_then(|m| m.get(&current_name))
                        .map(|a| a.is_cancelable)
                        .unwrap_or(true);
                    if !is_cancelable {
                        return None;
                    }
                }

                let anim_opt = self.animations.get(&id).and_then(|m| m.get(&name)).cloned();

                match anim_opt {
                    None => log::warn!(
                        "[IPC] Animación '{}' no encontrada para entidad {}",
                        name,
                        id
                    ),
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
                                self.script_engine
                                    .attach_script(id, &anim_path, &script.source)
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
                    return None;
                }
                let stopped_animation_name =
                    self.active_animations.remove(&id).map(|a| a.animation_name);
                if self.preview_playing {
                    let fallback_name =
                        self.default_animation_by_entity
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
            other => return Some(other),
        }
        None
    }
}
