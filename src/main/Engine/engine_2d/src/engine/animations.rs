use std::sync::Arc;
use std::time::Instant;

use crate::ecs::Transform;
use crate::ipc::{send_event, EngineEvent};

use super::types::{ActiveAnimation, AnimationState};
use super::State;

impl State {
    pub(super) fn resolve_animation_flip(&self, entity_id: u32, anim: &AnimationState) -> bool {
        if let Some(forced_flip) = self.anim_flip_overrides.get(&entity_id) {
            return *forced_flip;
        }

        // `anim.flip_horizontal` representa la orientación base de autoría:
        // false = dibujada mirando derecha, true = dibujada mirando izquierda.
        let facing_right = self.entity_facing_right.get(&entity_id).copied().unwrap_or(true);
        let target_is_left = !facing_right;
        anim.flip_horizontal ^ target_is_left
    }

    /// Inicia una animación de fallback (p. ej. predeterminada tras una no-loop).
    /// Aplica el frame 0 de inmediato para recalcular pivot/offset; si un script lanza
    /// `PlayAnimation` en el mismo tick, ese comando sobreescribe el visual.
    pub(super) fn start_animation_deferred(&mut self, entity_id: u32, name: String) {
        let anim_opt = self.animations
            .get(&entity_id)
            .and_then(|m| m.get(&name))
            .cloned();
        let Some(anim) = anim_opt else { return; };

        self.active_animations.remove(&entity_id);

        // Re-baseline de posición (igual que en PlayAnimation normal).
        if let Some(t) = self.world.get::<Transform>(entity_id).cloned() {
            self.anim_saved_transforms
                .entry(entity_id)
                .and_modify(|saved| { saved.0 = t.position; })
                .or_insert((t.position, t.scale));
        }

        // Iniciar audio del fallback si tiene (no hay visual, pero el audio sí aplica).
        if let Some(ref audio_decoded) = anim.audio_decoded {
            self.play_audio_internal(Arc::clone(audio_decoded), anim.loop_);
        }

        // Cargar scripts de la animación fallback.
        self.script_engine.detach_animation_scripts(entity_id);
        for script in &anim.scripts {
            let anim_path = format!("$anim$::{}::{}", name, script.name);
            let _ = self.script_engine.attach_script(entity_id, &anim_path, &script.source);
        }

        self.active_animations.insert(entity_id, ActiveAnimation {
            animation_name: name.clone(),
            current_frame:  0,
            last_frame_time: Instant::now(),
            fps:    anim.fps,
            finished: false,
        });

        self.prepare_character_animation_visual(entity_id);
        self.show_first_frame_of_animation(entity_id, &name);
    }

    pub(super) fn show_first_frame_of_animation(&mut self, entity_id: u32, animation_name: &str) {
        self.prepare_character_animation_visual(entity_id);
        let frame_data = self.animations
            .get(&entity_id)
            .and_then(|m| m.get(animation_name))
            .and_then(|anim| {
                anim.frames.first().map(|first| {
                    let flip = self.resolve_animation_flip(entity_id, anim);
                    let (pivot_x, pivot_y) = first.resolved_pivot(anim.logical_w, anim.logical_h);
                    (
                        first.path.clone(),
                        pivot_x,
                        pivot_y,
                        anim.logical_w,
                        anim.logical_h,
                        first.src_x.zip(first.src_y).zip(first.src_w.zip(first.src_h)).map(|((x, y), (w, h))| (x, y, w, h)),
                        flip,
                    )
                })
            });

        if let Some((path, pivot_x, pivot_y, logical_w, logical_h, src_rect, flip_horizontal)) = frame_data {
            self.play_animation_frame(entity_id, &path, pivot_x, pivot_y, logical_w, logical_h, src_rect, flip_horizontal);
        }
    }

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
            // Nota: la lógica de avance de frames se hace abajo con corrección de drift

            let anim_state = match self.animations.get(&entity_id)
                .and_then(|m| m.get(&active.animation_name)) {
                Some(a) => a,
                None => continue,
            };

            let frame_duration_ms = 1000u64 / active.fps.max(1) as u64;
            let frame_duration = std::time::Duration::from_millis(frame_duration_ms);
            let elapsed = now.duration_since(active.last_frame_time);

            if elapsed < frame_duration {
                // Mismo frame — no tocar collider ni visuales, ya están en el estado correcto.
                continue;
            }

            // Cuántos frames debieron haberse mostrado (recuperación de lag/stutter).
            // Con `= now` el error se acumula; con `+= frame_duration` el reloj es exacto.
            let frames_to_advance = (elapsed.as_millis() / frame_duration_ms as u128) as usize;
            let total_frames = anim_state.frames.len();

            // Avanzar el reloj de animación por el número exacto de frames,
            // no resincronizar a `now` (eso causaría deriva acumulada).
            active.last_frame_time += frame_duration * frames_to_advance as u32;
            // Salvaguarda: si el motor estuvo suspendido/bloqueado demasiado tiempo,
            // resincronizar para evitar una ráfaga de frames al retomar.
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
                if self.debug_mode {
                    log::info!("[anim] entidad {entity_id} avanza a frame {next_frame_idx} (anim '{}')", active.animation_name);
                }
                to_play.push((entity_id, next_frame_idx));
            }
        }

        for (entity_id, frame_idx) in to_play {
            let anim_name = self.active_animations.get(&entity_id)
                .map(|a| a.animation_name.clone())
                .unwrap_or_default();
            let (frame_data, flip, logical_w, logical_h) = if let Some(anim_map) = self.animations.get(&entity_id) {
                if let Some(anim) = anim_map.get(&anim_name) {
                    let frame_idx_clamped = frame_idx.min(anim.frames.len().saturating_sub(1));
                    if let Some(f) = anim.frames.get(frame_idx_clamped) {
                        let flip = self.resolve_animation_flip(entity_id, anim);
                        (Some(f.clone()), flip, anim.logical_w, anim.logical_h)
                    } else {
                        log::warn!("[animation] Frame {} no existe para entidad {} animación '{}' — se mantiene en active_animations", frame_idx_clamped, entity_id, anim_name);
                        (None, false, 0, 0)
                    }
                } else {
                    log::warn!("[animation] animación '{}' no existe para entidad {} — se mantiene en active_animations", anim_name, entity_id);
                    (None, false, 0, 0)
                }
            } else {
                log::warn!("[animation] entidad {} tiene active_animation '{}' pero ya no existe en el almacén — se mantiene en active_animations", entity_id, anim_name);
                (None, false, 0, 0)
            };
            if let Some(f) = frame_data {
                let (pivot_x, pivot_y) = f.resolved_pivot(logical_w, logical_h);
                self.play_animation_frame(
                    entity_id,
                    &f.path,
                    pivot_x,
                    pivot_y,
                    logical_w,
                    logical_h,
                    f.src_x.zip(f.src_y).zip(f.src_w.zip(f.src_h)).map(|((x, y), (w, h))| (x, y, w, h)),
                    flip,
                );
            }
        }

        for (entity_id, animation_name) in to_restore {
            // Desenganche de scripts de animación cuando una animación no-loop termina.
            self.script_engine.detach_animation_scripts(entity_id);
            if self.preview_playing {
                if let Some(fname) = self.default_animation_by_entity.get(&entity_id).cloned() {
                    // Iniciar el fallback de forma diferida (sin renderizar frame 0) para
                    // evitar el flash de 1 frame cuando un control script va a reiniciar
                    // la animación correcta en el siguiente tick del event loop.
                    self.start_animation_deferred(entity_id, fname);
                } else {
                    self.show_first_frame_of_animation(entity_id, &animation_name);
                }
            } else {
                // En modo edición no ejecutar fallback a animación predeterminada.
                self.show_first_frame_of_animation(entity_id, &animation_name);
            }
            // El audio no-looping se agota solo cuando las muestras PCM terminan.
            // No enviamos Stop aquí para evitar que sobrescriba un Play ya encolado
            // si el usuario dispara la siguiente animación justo al terminar esta.
            send_event(&EngineEvent::AnimationFinished { entity_id });
        }

        self.active_animations.retain(|_, a| !a.finished);
    }
}
