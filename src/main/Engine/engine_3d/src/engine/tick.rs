use std::time::Instant;

use crate::ecs::Transform;
use crate::ipc::{EngineEvent, send_event};
use crate::mesh;

use super::{AUTOSAVE_INTERVAL, ActiveAnimation, AnimationState, State};

impl State {
    pub(crate) fn update_entity_facing_from_horizontal(&mut self, entity_id: u32, horizontal: f32) {
        const EPS: f32 = 0.0001;
        if horizontal.abs() <= EPS {
            return;
        }
        self.entity_facing_right.insert(entity_id, horizontal > 0.0);
    }

    pub(crate) fn resolve_animation_flip(&self, entity_id: u32, anim: &AnimationState) -> bool {
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
                    let (px, py) = first.resolved_pivot(anim.logical_w, anim.logical_h);
                    (
                        first.path.clone(),
                        px,
                        py,
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

    /// Compatibilidad IPC 2D: en 3D el hint Ctrl no se dibuja (solo HUD vía `screen_hud_image`).
    pub fn set_snap_hint_visible(&mut self, _visible: bool) {}

    /// Recarga PNG de HUD de pantalla en `screen_hud_atlas` (no tocar `texture_array`).
    pub(crate) fn reload_screen_hud_images(&mut self) {
        self.screen_hud_atlas.reset(&self.queue);
        self.fps_exit_hint_es = self
            .screen_hud_atlas
            .pack_png_from_engine_assets(&self.queue, "tooltip-btn-esc-salir.png");
        self.fps_exit_hint_en = self
            .screen_hud_atlas
            .pack_png_from_engine_assets(&self.queue, "tooltip-btn-esc-exit.png");
    }

    pub(crate) fn update_fps_exit_hint_alpha(&mut self) {
        let target = if self.preview_playing && self.play_character_entity.is_some() {
            1.0_f32
        } else {
            0.0_f32
        };
        let k = if target > self.fps_exit_hint_alpha {
            3.6_f32
        } else {
            2.8_f32
        };
        let blend = 1.0 - (-k * self.delta_time.max(0.0)).exp();
        self.fps_exit_hint_alpha += (target - self.fps_exit_hint_alpha) * blend;
        if (self.fps_exit_hint_alpha - target).abs() < 0.001 {
            self.fps_exit_hint_alpha = target;
        }
    }

    /// Tooltip «Esc para salir del play» (`screen_hud_image`, esquina inferior izquierda).
    pub(crate) fn build_fps_exit_hint_instance(&self) -> Option<mesh::InstanceData> {
        if self.fps_exit_hint_alpha <= 0.003 {
            return None;
        }
        let packed = crate::screen_hud_image::pick_localized_screen_hud(
            &self.snap_locale,
            self.fps_exit_hint_en,
            self.fps_exit_hint_es,
        )?;
        let vw = self.size.width as f32;
        let vh = self.size.height as f32;
        let a = self.fps_exit_hint_alpha.clamp(0.0, 1.0);
        let eased_alpha = a * a * (3.0 - 2.0 * a);
        let model = crate::screen_hud_image::ndc_transform_bottom_left(
            vw,
            vh,
            packed,
            crate::screen_hud_image::ScreenHudBottomLeftLayout::default(),
            eased_alpha,
        )?;
        Some(crate::screen_hud_image::build_screen_hud_instance(
            packed,
            model,
            eased_alpha,
        ))
    }

    fn selection_entity_ids(&self) -> Vec<crate::ecs::EntityId> {
        if !self.selected_entities.is_empty() {
            self.selected_entities.clone()
        } else {
            self.selected_entity.into_iter().collect()
        }
    }

    /// AABB mundial de la selección: centro + semiejes (misma fuente para gizmo y rotación).
    pub(crate) fn selection_world_bounds(&self) -> Option<(glam::Vec3, glam::Vec3)> {
        let ids = self.selection_entity_ids();
        if ids.is_empty() {
            return None;
        }

        let mut bounds_min = glam::Vec3::splat(f32::INFINITY);
        let mut bounds_max = glam::Vec3::splat(f32::NEG_INFINITY);
        for id in ids {
            let t = self.world.get::<crate::ecs::Transform>(id)?;
            let (center, half) = self.entity_world_pick_aabb(id, t);
            bounds_min = bounds_min.min(center - half);
            bounds_max = bounds_max.max(center + half);
        }
        if !bounds_min.is_finite() || !bounds_max.is_finite() {
            return None;
        }
        let half = (bounds_max - bounds_min) * 0.5;
        let center = (bounds_max + bounds_min) * 0.5;
        Some((center, half))
    }

    /// Centro de selección para gizmo/grupo (centro del AABB mundial de la malla).
    pub(crate) fn selection_center(&self) -> Option<glam::Vec3> {
        self.selection_world_bounds().map(|(center, _)| center)
    }

    /// Mayor dimensión (m) del AABB de selección para acotar el gizmo en objetos pequeños.
    pub(crate) fn selection_max_extent(&self) -> Option<f32> {
        let ids: Vec<u32> = if !self.selected_entities.is_empty() {
            self.selected_entities.clone()
        } else {
            self.selected_entity.into_iter().collect()
        };
        if ids.is_empty() {
            return None;
        }
        let mut max_extent = 0.0f32;
        for id in ids {
            let t = self.world.get::<Transform>(id)?;
            let (_, half) = self.entity_world_pick_aabb(id, t);
            let extent = half[0].max(half[1]).max(half[2]) * 2.0;
            max_extent = max_extent.max(extent);
        }
        Some(max_extent.max(1e-4))
    }

    /// Semiejes del AABB mundial de la selección (eje X/Y/Z alineados al mundo).
    pub(crate) fn selection_world_half_extents(&self) -> Option<glam::Vec3> {
        self.selection_world_bounds().map(|(_, half)| half)
    }

    /// Inicio de cada flecha del gizmo en unidades del mesh (antes de escalar).
    pub(crate) fn transform_gizmo_axis_start_mesh(&self) -> Option<[f32; 3]> {
        let half = self.selection_world_half_extents()?;
        let scale = self.transform_gizmo_world_scale()?;
        let anchor = self.selection_center()?;
        let gap_world = rer_engine_shared::gizmo::axis_gap_world(
            self.camera_world_position(),
            anchor,
            self.camera.fov_y,
            self.size.height,
        );
        Some([
            rer_engine_shared::gizmo::axis_start_mesh_units(half.x, gap_world, scale),
            rer_engine_shared::gizmo::axis_start_mesh_units(half.y, gap_world, scale),
            rer_engine_shared::gizmo::axis_start_mesh_units(half.z, gap_world, scale),
        ])
    }

    /// Escala del gizmo de transformación (pantalla + tope por tamaño de selección).
    pub(crate) fn transform_gizmo_world_scale(&self) -> Option<f32> {
        let anchor = self.selection_center()?;
        let screen = rer_engine_shared::gizmo::world_scale_perspective(
            self.camera_world_position(),
            anchor,
            self.camera.fov_y,
            self.size.height,
        );
        Some(rer_engine_shared::gizmo::clamp_scale_for_selection(
            screen,
            self.selection_max_extent(),
        ))
    }

    /// Escala del gizmo de rotación: al menos un poco más grande que el AABB de la selección.
    pub(crate) fn transform_gizmo_rotation_world_scale(&self) -> Option<f32> {
        let base = self.transform_gizmo_world_scale()?;
        let half = self.selection_world_half_extents()?;
        let anchor = self.selection_center()?;
        let gap_world = rer_engine_shared::gizmo::axis_gap_world(
            self.camera_world_position(),
            anchor,
            self.camera.fov_y,
            self.size.height,
        );
        const MARGIN: f32 = 1.18;
        let min_radius_world = half.x.max(half.y).max(half.z) + gap_world;
        let min_scale = min_radius_world / crate::gizmo::GIZMO_ROTATION_RING_RADIUS;
        Some(base.max(min_scale * MARGIN))
    }

    pub fn update(&mut self) {
        let now = Instant::now();
        self.delta_time = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;
        self.tick_fp_baseline_defer();
        self.update_fps_exit_hint_alpha();

        self.metrics_frame_count += 1;
        if now.duration_since(self.metrics_last_emit) >= std::time::Duration::from_secs(1) {
            let elapsed_secs = now.duration_since(self.metrics_last_emit).as_secs_f32();
            let fps = self.metrics_frame_count as f32 / elapsed_secs;
            let physics_bodies = self.physics.body_count();
            let (play_character_position, play_character_yaw, play_character_pitch) =
                if self.play_character_entity.is_some() {
                    (
                        Some(self.play_character_feet_position().to_array()),
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
                cpu_percent: self.process_metrics_sampler.sample_cpu_percent(),
                gpu_percent: self.process_metrics_sampler.sample_gpu_percent(),
                play_character_position,
                play_character_yaw,
                play_character_pitch,
            });
            self.metrics_last_emit = now;
            self.metrics_frame_count = 0;
        }
        if self.autosave_enabled && now.duration_since(self.autosave_last_tick) >= AUTOSAVE_INTERVAL
        {
            send_event(&EngineEvent::AutosaveTick);
            self.autosave_last_tick = now;
        }
        self.update_scripts();
        self.apply_plane_tool_held_rotation();
        self.update_scene_scripts();
        self.update_texture_distance_lod();
        if self.preview_playing {
            let skip_sync = self
                .play_character_entity
                .map(|id| vec![id])
                .unwrap_or_default();
            self.physics
                .step(self.delta_time, &mut self.world, &skip_sync);
            self.update_execution_areas_3d();
        }
        self.tick_projectiles(self.delta_time);
    }
}
