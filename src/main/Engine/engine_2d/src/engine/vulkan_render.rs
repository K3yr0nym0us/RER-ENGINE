//! Ruta de render Ash del motor 2D (`RER_2D_VULKAN=1`).
//!
//! Este módulo solo traduce el `State` a datos CPU: uniformes de escena, batches de sprites
//! (los mismos que calcula la ruta wgpu), rejilla, colisiones y gizmo. Vulkan vive entero en
//! `crate::vulkan::renderer`; así la lógica de qué se dibuja (culling, orden, flags, visibilidad
//! de helpers) sigue viviendo en un único sitio y las dos rutas no pueden divergir.

use std::sync::Arc;

use glam::{Mat4, Vec3};
use rer_engine_shared::wgpu_surface::SurfacePresentError;

use crate::vulkan::{FrameInput, GizmoBatch, GridBatch, Vulkan2d};

use super::State;
use super::render_helpers::{build_scene_uniforms, build_scene_uniforms_2d};

impl State {
    /// Dibuja y presenta un frame por Vulkan. Los errores de swapchain se mapean al mismo
    /// `SurfacePresentError` que usa la ruta wgpu para que el bucle de eventos los trate igual.
    pub(crate) fn render_vulkan(
        &mut self,
        vulkan: &Arc<Vulkan2d>,
    ) -> Result<(), SurfacePresentError> {
        // `wgpu::Color` guarda r/g/b en 0..=1 (el IPC manda f64 y `SetClearColor` los copia tal
        // cual), así que el clear de Vulkan es una conversión directa.
        let clear = [
            self.clear_color.r as f32,
            self.clear_color.g as f32,
            self.clear_color.b as f32,
            self.clear_color.a as f32,
        ];
        let use_2d_pipeline = self.camera_2d.is_some();

        // Los datos del frame se calculan en un bloque para que sus préstamos a `self` terminen
        // antes de actualizar `last_draw_calls`.
        let draw_calls = {
            let scene_uniforms = if let Some(cam2d) = &self.camera_2d {
                build_scene_uniforms_2d(cam2d, self.size)
            } else {
                build_scene_uniforms(&self.camera, self.size)
            };

            // Misma recolección que la ruta wgpu: culling por viewport, orden (layer, z) y flags.
            let batches = self.collect_sprite_batches();

            let aspect = self.size.width as f32 / self.size.height.max(1) as f32;
            let vp_2d = self
                .camera_2d
                .as_ref()
                .map(|cam| cam.view_proj(aspect).to_cols_array_2d());

            // Los helpers de edición (rejilla y colisiones) siguen la misma condición que el
            // render wgpu: visibles en edición, ocultos en preview salvo debug.
            let show_helpers = !self.preview_playing || self.debug_mode;

            let grid_vertices = if show_helpers && vp_2d.is_some() {
                crate::config_2d::grid_vertices(&self.grid_config)
            } else {
                Vec::new()
            };
            let collision = if show_helpers {
                crate::config_2d::collision_overlay_vertices(self)
            } else {
                Vec::new()
            };
            let grid = match (show_helpers, vp_2d) {
                (true, Some(vp)) => Some(GridBatch {
                    uniforms: line_uniforms(vp, identity_model(), [-1.0, -1.0, 0.0, 0.0]),
                    vertices: &grid_vertices,
                }),
                _ => None,
            };

            let gizmo = if !self.preview_playing && !self.player_ui_edit_active {
                self.selection_center()
                    .filter(|_| self.pivot_edit_mode.is_none())
                    .map(|origin| {
                        let vp = vp_2d.unwrap_or_else(|| self.camera.to_uniform(aspect).view_proj);
                        let scale = self.transform_gizmo_world_scale_2d().unwrap_or(1.0);
                        let model =
                            Mat4::from_translation(origin) * Mat4::from_scale(Vec3::splat(scale));
                        let hovered = self
                            .hovered_gizmo_axis
                            .map(|axis| axis as f32)
                            .unwrap_or(-1.0);
                        let active = self
                            .active_gizmo_axis
                            .map(|axis| axis as f32)
                            .unwrap_or(-1.0);
                        GizmoBatch {
                            uniforms: line_uniforms(
                                vp,
                                model.to_cols_array_2d(),
                                [hovered, active, 0.0, 0.0],
                            ),
                        }
                    })
            } else {
                None
            };

            let input = FrameInput {
                clear,
                scene_uniforms: &scene_uniforms,
                use_2d_pipeline,
                batches: &batches,
                meshes: &self.meshes,
                grid,
                collision: &collision,
                gizmo,
            };

            // Un draw por batch + blit + rejilla/colisiones + gizmo. Solo es estadística para el
            // IPC (`draw_calls`); no decide nada del frame.
            let mut calls = batches.len() as u32 + 1;
            if !grid_vertices.is_empty() || !collision.is_empty() {
                calls += 1;
            }
            if input.gizmo.is_some() {
                calls += 1;
            }

            vulkan.render(input)?;
            calls
        };

        self.last_draw_calls = draw_calls;
        // La reconstrucción del spatial grid (picking) no depende del backend: se hace igual que
        // en la ruta wgpu cuando el frame se presenta por Vulkan.
        self.spatial_grid.clear();
        for &entity in self.world.entities() {
            if let Some(t) = self.world.get::<crate::ecs::Transform>(entity) {
                let sx = t.scale.x.abs() * 0.5;
                let sy = t.scale.y.abs() * 0.5;
                let center = crate::config_2d::transform_visual_center(
                    t.position,
                    self.visual_offsets.get(&entity).copied(),
                );
                self.spatial_grid.insert_entity(
                    entity,
                    [center.x - sx, center.y - sy, center.x + sx, center.y + sy],
                );
            }
        }
        Ok(())
    }
}

/// Uniform block de 9 `vec4` que comparten rejilla, colisiones y gizmo:
/// `view_proj` (4) + `model` (4) + `flags` (1).
fn line_uniforms(view_proj: [[f32; 4]; 4], model: [[f32; 4]; 4], flags: [f32; 4]) -> [[f32; 4]; 9] {
    [
        view_proj[0],
        view_proj[1],
        view_proj[2],
        view_proj[3],
        model[0],
        model[1],
        model[2],
        model[3],
        flags,
    ]
}

fn identity_model() -> [[f32; 4]; 4] {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}
