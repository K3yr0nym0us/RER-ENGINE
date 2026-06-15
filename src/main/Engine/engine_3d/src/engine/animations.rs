use std::time::Instant;

use crate::ipc::{send_event, EngineEvent};

use super::State;

impl State {
    pub(crate) fn update_animations(&mut self) {
        let now = Instant::now();
        let mut to_play: Vec<(u32, usize)> = Vec::new();
        let mut to_restore: Vec<(u32, String)> = Vec::new();

        let entity_ids: Vec<u32> = self.active_animations.keys().copied().collect();
        for entity_id in entity_ids {
            let active = match self.active_animations.get_mut(&entity_id) {
                Some(a) => a,
                None => continue,
            };

            if active.finished {
                continue;
            }

            let anim_state = match self
                .animations
                .get(&entity_id)
                .and_then(|m| m.get(&active.animation_name))
            {
                Some(a) => a,
                None => continue,
            };

            let frame_duration_ms = 1000u64 / active.fps.max(1) as u64;
            let frame_duration = std::time::Duration::from_millis(frame_duration_ms);
            let elapsed = now.duration_since(active.last_frame_time);

            if elapsed < frame_duration {
                to_play.push((entity_id, active.current_frame));
                continue;
            }

            let frames_to_advance =
                (elapsed.as_millis() / frame_duration_ms as u128).max(1) as usize;
            let total_frames = anim_state.frames.len();

            active.last_frame_time += frame_duration * frames_to_advance as u32;
            if now.duration_since(active.last_frame_time) > frame_duration * 3 {
                active.last_frame_time = now - frame_duration;
            }

            let next_frame_idx = active.current_frame + frames_to_advance;

            if next_frame_idx >= total_frames {
                if anim_state.loop_ {
                    active.current_frame = next_frame_idx % total_frames;
                    to_play.push((entity_id, active.current_frame));
                } else {
                    active.finished = true;
                    to_restore.push((entity_id, active.animation_name.clone()));
                }
            } else {
                active.current_frame = next_frame_idx;
                to_play.push((entity_id, next_frame_idx));
            }
        }

        for (entity_id, frame_idx) in to_play {
            let frame_data = {
                let anim_name = self
                    .active_animations
                    .get(&entity_id)
                    .map(|a| a.animation_name.clone())
                    .unwrap_or_default();
                let anim_opt = self.animations.get(&entity_id).and_then(|m| m.get(&anim_name));
                match anim_opt {
                    None => {
                        log::warn!(
                            "[animation] entidad {} tiene active_animation '{}' pero ya no existe en el almacén — limpiando",
                            entity_id,
                            anim_name
                        );
                        None
                    }
                    Some(anim) => {
                        let frame_idx_clamped = frame_idx.min(anim.frames.len().saturating_sub(1));
                        anim.frames.get(frame_idx_clamped).map(|f| {
                            let flip = self.resolve_animation_flip(entity_id, anim);
                            (
                                f.path.clone(),
                                f.resolved_pivot(anim.logical_w, anim.logical_h).0,
                                f.resolved_pivot(anim.logical_w, anim.logical_h).1,
                                anim.logical_w,
                                anim.logical_h,
                                f.src_x
                                    .zip(f.src_y)
                                    .zip(f.src_w.zip(f.src_h))
                                    .map(|((x, y), (w, h))| (x, y, w, h)),
                                flip,
                            )
                        })
                    }
                }
            };
            if let Some((path, pivot_x, pivot_y, logical_w, logical_h, src_rect, flip_horizontal)) =
                frame_data
            {
                self.play_animation_frame(
                    entity_id,
                    &path,
                    pivot_x,
                    pivot_y,
                    logical_w,
                    logical_h,
                    src_rect,
                    flip_horizontal,
                );
            } else {
                self.active_animations.remove(&entity_id);
            }
        }

        for (entity_id, animation_name) in to_restore {
            self.script_engine.detach_animation_scripts(entity_id);
            if self.preview_playing {
                let fallback_name = self
                    .default_animation_by_entity
                    .get(&entity_id)
                    .cloned()
                    .or_else(|| {
                        self.animations
                            .get(&entity_id)
                            .and_then(|m| m.keys().next().cloned())
                    });
                if let Some(fname) = fallback_name {
                    self.start_animation_deferred(entity_id, fname);
                } else {
                    self.show_first_frame_of_animation(entity_id, &animation_name);
                }
            } else {
                self.show_first_frame_of_animation(entity_id, &animation_name);
            }
            
            send_event(&EngineEvent::AnimationFinished { entity_id });
        }

        self.active_animations.retain(|_, a| !a.finished);
    }
}
