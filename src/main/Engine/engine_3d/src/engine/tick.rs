use std::time::Instant;

use glam::Mat4;

use crate::ecs::Transform;
use crate::ipc::{send_event, EngineEvent};
use crate::mesh;

use super::{ActiveAnimation, AnimationState, State, AUTOSAVE_INTERVAL};

impl State {
    pub(crate) fn update_entity_facing_from_horizontal(&mut self, entity_id: u32, horizontal: f32) {
        const EPS: f32 = 0.0001;
        if horizontal.abs() <= EPS {
            return;
        }
        self.entity_facing_right.insert(entity_id, horizontal > 0.0);
    }

    pub(crate) fn resolve_animation_flip(&self, entity_id: u32, anim: &AnimationState) -> bool {
        if let Some(forced_flip) = self.anim_flip_overrides.get(&entity_id) {
            return *forced_flip;
        }

        let facing_right = self
            .entity_facing_right
            .get(&entity_id)
            .copied()
            .unwrap_or(true);
        let target_is_left = !facing_right;
        anim.flip_horizontal ^ target_is_left
    }

    pub(crate) fn start_animation_deferred(&mut self, entity_id: u32, name: String) {
        let anim_opt = self
            .animations
            .get(&entity_id)
            .and_then(|m| m.get(&name))
            .cloned();
        let Some(anim) = anim_opt else {
            return;
        };

        self.active_animations.remove(&entity_id);

        if let Some(t) = self.world.get::<Transform>(entity_id).cloned() {
            self.anim_saved_transforms
                .entry(entity_id)
                .and_modify(|saved| {
                    saved.0 = t.position;
                })
                .or_insert((t.position, t.scale));
        }

        if let Some(ref audio_decoded) = anim.audio_decoded {
            self.play_audio_internal(std::sync::Arc::clone(audio_decoded), anim.loop_);
        }

        self.script_engine.detach_animation_scripts(entity_id);
        for script in &anim.scripts {
            let anim_path = format!("$anim$::{}::{}", name, script.name);
            let _ = self
                .script_engine
                .attach_script(entity_id, &anim_path, &script.source);
        }

        self.active_animations.insert(
            entity_id,
            ActiveAnimation {
                animation_name: name,
                current_frame: 0,
                last_frame_time: Instant::now(),
                fps: anim.fps,
                finished: false,
            },
        );
    }

    pub(crate) fn show_first_frame_of_animation(&mut self, entity_id: u32, animation_name: &str) {
        let frame_data = self
            .animations
            .get(&entity_id)
            .and_then(|m| m.get(animation_name))
            .and_then(|anim| {
                anim.frames.first().map(|first| {
                    let flip = self.resolve_animation_flip(entity_id, anim);
                    (
                        first.path.clone(),
                        first.pivot_x,
                        first.pivot_y,
                        anim.logical_w,
                        anim.logical_h,
                        first
                            .src_x
                            .zip(first.src_y)
                            .zip(first.src_w.zip(first.src_h))
                            .map(|((x, y), (w, h))| (x, y, w, h)),
                        flip,
                    )
                })
            });

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
        }
    }

    /// Notifica al `State` qué eje del gizmo está siendo arrastrado.
    pub fn set_active_gizmo_axis(&mut self, axis: Option<usize>) {
        self.active_gizmo_axis = axis;
    }

    /// Muestra/oculta el hint visual de snap a cuadrícula en el viewport 2D.
    pub fn set_snap_hint_visible(&mut self, visible: bool) {
        self.show_snap_hint = visible;
    }

    fn load_snap_hint_uv(&mut self, filename: &str) -> (Option<[f32; 4]>, (f32, f32)) {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../assets")
            .join(filename);
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
                        let uv = self.atlas.pack(&self.queue, img.as_raw(), w, h);
                        (Some(uv), (w as f32, h as f32))
                    }
                    Err(e) => {
                        log::warn!("[snap-hint] Error decodificando '{}': {}", path.display(), e);
                        (None, (0.0, 0.0))
                    }
                }
            }
            Err(e) => {
                log::warn!("[snap-hint] No se pudo leer '{}': {}", path.display(), e);
                (None, (0.0, 0.0))
            }
        }
    }

    pub(crate) fn reload_snap_hint_assets(&mut self) {
        let (uv_es, size_es) = self.load_snap_hint_uv("tooltip-btn-ctrl-to-auto-adjust.png");
        let (uv_en, size_en) =
            self.load_snap_hint_uv("tooltip-btn-ctrl-to-auto-adjust-english.png");
        self.snap_hint_uv = uv_es;
        self.snap_hint_size = size_es;
        self.snap_hint_uv_en = uv_en;
        self.snap_hint_size_en = size_en;

        let (fp_es, fp_size_es) = self.load_snap_hint_uv("tooltip-btn-esc-salir.png");
        let (fp_en, fp_size_en) = self.load_snap_hint_uv("tooltip-btn-esc-exit.png");
        self.fp_exit_hint_uv = fp_es;
        self.fp_exit_hint_size = fp_size_es;
        self.fp_exit_hint_uv_en = fp_en;
        self.fp_exit_hint_size_en = fp_size_en;
    }

    pub(crate) fn update_snap_hint_alpha(&mut self) {
        let target = if self.show_snap_hint && !self.preview_playing && self.camera_2d.is_some() {
            1.0_f32
        } else {
            0.0_f32
        };
        let k = if target > self.snap_hint_alpha {
            4.2_f32
        } else {
            3.4_f32
        };
        let blend = 1.0 - (-k * self.delta_time.max(0.0)).exp();
        self.snap_hint_alpha += (target - self.snap_hint_alpha) * blend;
        if (self.snap_hint_alpha - target).abs() < 0.001 {
            self.snap_hint_alpha = target;
        }
    }

    pub(crate) fn update_fp_exit_hint_alpha(&mut self) {
        let target = if self.preview_playing
            && self.camera_2d.is_none()
            && self.first_person_player_entity.is_some()
        {
            1.0_f32
        } else {
            0.0_f32
        };
        let k = if target > self.fp_exit_hint_alpha {
            3.6_f32
        } else {
            2.8_f32
        };
        let blend = 1.0 - (-k * self.delta_time.max(0.0)).exp();
        self.fp_exit_hint_alpha += (target - self.fp_exit_hint_alpha) * blend;
        if (self.fp_exit_hint_alpha - target).abs() < 0.001 {
            self.fp_exit_hint_alpha = target;
        }
    }

    pub(crate) fn build_snap_hint_instance_2d(&self) -> Option<mesh::InstanceData> {
        if self.snap_hint_alpha <= 0.003 || self.preview_playing {
            return None;
        }
        let (uv, img_w, img_h) = if self.snap_locale == "en" {
            let uv = self.snap_hint_uv_en.or(self.snap_hint_uv)?;
            let (w, h) = if self.snap_hint_uv_en.is_some() {
                self.snap_hint_size_en
            } else {
                self.snap_hint_size
            };
            (uv, w, h)
        } else {
            let uv = self.snap_hint_uv.or(self.snap_hint_uv_en)?;
            let (w, h) = if self.snap_hint_uv.is_some() {
                self.snap_hint_size
            } else {
                self.snap_hint_size_en
            };
            (uv, w, h)
        };
        let Some(cam) = &self.camera_2d else {
            return None;
        };
        if self.size.width == 0 || self.size.height == 0 || img_w <= 0.0 || img_h <= 0.0 {
            return None;
        }

        let aspect = self.size.width as f32 / self.size.height as f32;
        let half_w = cam.half_h * aspect;
        let world_per_px_x = (half_w * 2.0) / self.size.width as f32;
        let world_per_px_y = (cam.half_h * 2.0) / self.size.height as f32;

        let margin_px = 18.0_f32;
        let max_width_px = (self.size.width as f32 * 0.22).clamp(120.0, 320.0);
        let scale_px = (max_width_px / img_w).min(1.0);
        let draw_w_px = img_w * scale_px;
        let draw_h_px = img_h * scale_px;

        let draw_w_world = draw_w_px * world_per_px_x;
        let draw_h_world = draw_h_px * world_per_px_y;
        let margin_x_world = margin_px * world_per_px_x;
        let margin_y_world = margin_px * world_per_px_y;

        let a = self.snap_hint_alpha.clamp(0.0, 1.0);
        let eased_alpha = a * a * (3.0 - 2.0 * a);
        let scale_in = 0.92 + 0.08 * eased_alpha;
        let slide_px = (1.0 - eased_alpha) * 14.0;

        let center_x = cam.x - half_w + margin_x_world + draw_w_world * 0.5;
        let center_y =
            cam.y + cam.half_h - margin_y_world - draw_h_world * 0.5 - slide_px * world_per_px_y;
        let model = Mat4::from_translation(glam::vec3(center_x, center_y, 0.9))
            * Mat4::from_scale(glam::vec3(draw_w_world * scale_in, draw_h_world * scale_in, 1.0));
        let mut inst = mesh::InstanceData::new(model, 0.0, uv);
        inst.flag_pad[1] = eased_alpha;
        Some(inst)
    }

    /// Tooltip «Esc para salir del play» en primera persona 3D (esquina inferior izquierda).
    pub(crate) fn build_fp_exit_hint_instance(&self) -> Option<mesh::InstanceData> {
        if self.fp_exit_hint_alpha <= 0.003 {
            return None;
        }
        let (uv, img_w, img_h) = if self.snap_locale == "en" {
            let uv = self.fp_exit_hint_uv_en.or(self.fp_exit_hint_uv)?;
            let (w, h) = if self.fp_exit_hint_uv_en.is_some() {
                self.fp_exit_hint_size_en
            } else {
                self.fp_exit_hint_size
            };
            (uv, w, h)
        } else {
            let uv = self.fp_exit_hint_uv.or(self.fp_exit_hint_uv_en)?;
            let (w, h) = if self.fp_exit_hint_uv.is_some() {
                self.fp_exit_hint_size
            } else {
                self.fp_exit_hint_size_en
            };
            (uv, w, h)
        };
        let w = self.size.width as f32;
        let h = self.size.height as f32;
        if w <= 0.0 || h <= 0.0 || img_w <= 0.0 || img_h <= 0.0 {
            return None;
        }

        let margin_px = 18.0_f32;
        let max_width_px = (w * 0.28).clamp(120.0, 360.0);
        let scale_px = (max_width_px / img_w).min(1.0);
        const DISPLAY_SCALE: f32 = 0.9;
        let draw_w_px = img_w * scale_px * DISPLAY_SCALE;
        let draw_h_px = img_h * scale_px * DISPLAY_SCALE;

        let a = self.fp_exit_hint_alpha.clamp(0.0, 1.0);
        let eased_alpha = a * a * (3.0 - 2.0 * a);
        let scale_in = 0.92 + 0.08 * eased_alpha;
        let slide_px = (1.0 - eased_alpha) * 14.0;

        // Espacio NDC fijo en pantalla (como el crosshair): view_proj = identidad en el pass.
        let ndc_w = 2.0 * (draw_w_px / w) * scale_in;
        let ndc_h = 2.0 * (draw_h_px / h) * scale_in;
        let margin_x_ndc = 2.0 * margin_px / w;
        let margin_y_ndc = 2.0 * margin_px / h;
        let slide_ndc = 2.0 * slide_px / h;
        let cx = -1.0 + margin_x_ndc + ndc_w * 0.5;
        let cy = -1.0 + margin_y_ndc + ndc_h * 0.5 + slide_ndc;
        let model = Mat4::from_translation(glam::vec3(cx, cy, 0.0))
            * Mat4::from_scale(glam::vec3(ndc_w, ndc_h, 1.0));
        let mut inst = mesh::InstanceData::new(model, 0.0, uv);
        inst.flag_pad[1] = eased_alpha;
        Some(inst)
    }

    /// Centro de selección para gizmo/grupo. Si no hay grupo, usa `selected_entity`.
    pub(crate) fn selection_center(&self) -> Option<glam::Vec3> {
        if !self.selected_entities.is_empty() {
            let mut sum = glam::Vec3::ZERO;
            let mut count = 0usize;
            for &id in &self.selected_entities {
                if let Some(t) = self.world.get::<Transform>(id) {
                    sum += t.position;
                    count += 1;
                }
            }
            if count > 0 {
                return Some(sum / count as f32);
            }
        }
        self.selected_entity
            .and_then(|id| self.world.get::<Transform>(id).map(|t| t.position))
    }

    /// Sincroniza `anim_saved_transforms` desde la posición actual del `Transform`.
    pub(crate) fn sync_physics_anim_origins(&mut self) {
        let ids: Vec<u32> = self.anim_saved_transforms.keys().copied().collect();
        for id in ids {
            if self.physics_2d.has_physics(id) {
                if let Some(t) = self.world.get::<Transform>(id) {
                    let (px, py) = (t.position.x, t.position.y);
                    if let Some(saved) = self.anim_saved_transforms.get_mut(&id) {
                        saved.0.x = px;
                        saved.0.y = py;
                    }
                }
            }
        }
    }

    pub fn update(&mut self) {
        let now = Instant::now();
        self.delta_time = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;
        self.update_snap_hint_alpha();
        self.update_fp_exit_hint_alpha();

        self.metrics_frame_count += 1;
        if now.duration_since(self.metrics_last_emit) >= std::time::Duration::from_secs(1) {
            let elapsed_secs = now.duration_since(self.metrics_last_emit).as_secs_f32();
            let fps = self.metrics_frame_count as f32 / elapsed_secs;
            let physics_bodies = if self.camera_2d.is_some() {
                self.physics_2d.body_count()
            } else {
                self.physics.body_count()
            };
            let (first_person_position, first_person_yaw, first_person_pitch) =
                if self.camera_2d.is_none() && self.first_person_player_entity.is_some() {
                    (
                        Some(self.first_person_feet_position().to_array()),
                        Some(self.camera.yaw),
                        Some(self.camera.pitch),
                    )
                } else {
                    (None, None, None)
                };
            send_event(&EngineEvent::DebugMetrics {
                fps,
                frame_time_ms: self.delta_time * 1000.0,
                draw_calls: self.last_draw_calls,
                physics_bodies,
                first_person_position,
                first_person_yaw,
                first_person_pitch,
            });
            self.metrics_last_emit = now;
            self.metrics_frame_count = 0;
        }
        if self.autosave_enabled && now.duration_since(self.autosave_last_tick) >= AUTOSAVE_INTERVAL
        {
            send_event(&EngineEvent::AutosaveTick);
            self.autosave_last_tick = now;
        }
        if self.camera_2d.is_some() {
            self.update_scripts();
            if self.preview_playing {
                self.physics_2d.step(self.delta_time, &mut self.world);
                self.sync_physics_anim_origins();
                self.update_execution_areas_2d();
            }
        } else {
            self.update_scripts();
            if self.preview_playing {
                let skip_sync = self
                    .first_person_player_entity
                    .map(|id| vec![id])
                    .unwrap_or_default();
                self.physics
                    .step(self.delta_time, &mut self.world, &skip_sync);
            }
        }
    }
}
