//! Ruta de render Ash del motor 2D (`RER_2D_VULKAN=1`).
//!
//! Este módulo solo traduce el `State` a datos CPU: uniformes de escena, batches de sprites
//! (los mismos que calcula la ruta wgpu), rejilla, colisiones y gizmo. Vulkan vive entero en
//! `crate::vulkan::renderer`; así la lógica de qué se dibuja (culling, orden, flags, visibilidad
//! de helpers) sigue viviendo en un único sitio y las dos rutas no pueden divergir.

use std::sync::Arc;

use glam::{Mat4, Vec3};
use rer_engine_shared::wgpu_surface::SurfacePresentError;

use crate::vulkan::{FrameInput, GizmoBatch, GridBatch, HudBatch, NdcOverlay, Vulkan2d};

use super::State;
use super::render_helpers::{build_scene_uniforms, build_scene_uniforms_2d};

impl State {
    /// Dibuja y presenta un frame por Vulkan. Los errores de swapchain se mapean al mismo
    /// `SurfacePresentError` que usa la ruta wgpu para que el bucle de eventos los trate igual.
    pub(crate) fn render_vulkan(
        &mut self,
        vulkan: &Arc<Vulkan2d>,
    ) -> Result<(), SurfacePresentError> {
        // El color de fondo ya se guarda en 0..=1 (el IPC manda f64 y se convierte al asignar).
        let clear = self.clear_color;
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

            // ── Overlays de Player UI (fase 5b) ──────────────────────────────
            // La ruta wgpu los dibuja en pases separados; aquí se extrae el mismo dato CPU y el
            // renderer los emite en el único pase del swapchain, en el mismo orden.
            let hud_visible = self.player_ui_hud_visible();
            let has_gizmo = hud_visible && !self.player_ui_text_overlay_buffer.vertices.is_empty();
            let has_glyphs = hud_visible && !self.player_ui_glyph_instances.is_empty();
            let hud_active = has_gizmo || has_glyphs;
            let caret_visible = hud_active
                && self.player_ui_text_editing_id.is_some()
                && self.player_ui_caret_blink_visible();
            // El cursor se reconstruye en el momento de dibujar, igual que `draw_player_ui_hud`.
            // Va antes de tomar los préstamos inmutables de abajo para no solaparlos.
            if caret_visible && let Some(edit_id) = self.player_ui_text_editing_id {
                crate::config_2d::player_ui::text_render::rebuild_caret_buffer(self, edit_id);
            }

            let tool_overlay: &[rer_engine_shared::player_ui::ndc_draw::NdcVertex] =
                if !self.preview_playing && vp_2d.is_some() {
                    &self.tool_overlay_buffer.vertices
                } else {
                    &[]
                };

            let ui = NdcOverlay {
                screen_grid: if self.player_ui_edit_active {
                    &self.ui_work_grid_buffer.vertices
                } else {
                    &[]
                },
                object: if self.player_ui_edit_active {
                    &self.player_ui_object_draw_overlay.vertices
                } else {
                    &[]
                },
                text: if has_gizmo {
                    &self.player_ui_text_overlay_buffer.vertices
                } else {
                    &[]
                },
                caret: if caret_visible {
                    &self.player_ui_caret_buffer.vertices
                } else {
                    &[]
                },
            };

            let snap_hint = self
                .build_snap_hint_instance_2d()
                .map(|instance| (self.canonical_quad_idx, instance));

            let hud = if has_glyphs {
                Some(HudBatch {
                    instances: &self.player_ui_glyph_instances,
                    quad: &self.hud_quad_mesh,
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
                tool_overlay,
                gizmo,
                snap_hint,
                ui,
                hud,
            };

            // Un draw por batch + blit + cada overlay no vacío. Solo es estadística para el IPC
            // (`draw_calls`); no decide nada del frame.
            let mut calls = batches.len() as u32 + 1;
            for non_empty in [
                !grid_vertices.is_empty(),
                !collision.is_empty(),
                !tool_overlay.is_empty(),
                input.snap_hint.is_some(),
                !input.ui.screen_grid.is_empty(),
                !input.ui.object.is_empty(),
                !input.ui.text.is_empty(),
                !input.ui.caret.is_empty(),
                input.hud.is_some(),
                input.gizmo.is_some(),
            ] {
                if non_empty {
                    calls += 1;
                }
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
