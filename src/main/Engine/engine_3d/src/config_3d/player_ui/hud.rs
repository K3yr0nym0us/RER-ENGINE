//! Orquestación del overlay HUD (texto + botones).

use std::sync::Arc;

use ab_glyph::FontArc;

use crate::engine::State;
use crate::gizmo;

use super::button_render;
use super::text_render;

impl State {
    pub(crate) fn player_ui_screen_key(&self) -> Option<String> {
        let scope = self.player_ui_edit_scope.as_ref()?;
        let screen_id = self.player_ui_edit_screen_id.as_ref()?;
        Some(format!("{scope}:{screen_id}"))
    }

    pub(crate) fn player_ui_font_cached(&self, path: &str) -> Option<Arc<FontArc>> {
        self.player_ui_font_cache.get(path).cloned()
    }

    pub(crate) fn player_ui_font_cached_mut(
        &mut self,
        cache: &mut std::collections::HashMap<String, Arc<FontArc>>,
        path: &str,
    ) -> Option<Arc<FontArc>> {
        if let Some(font) = cache.get(path) {
            return Some(font.clone());
        }
        let font = super::font::load_font_arc(path)?;
        cache.insert(path.to_string(), font.clone());
        Some(font)
    }

    pub(crate) fn rebuild_player_ui_overlay(&mut self) {
        let Some(key) = self.player_ui_screen_key() else {
            self.player_ui_text_overlay_buffer = gizmo::build_from_vertices(&self.device, &[]);
            self.player_ui_glyph_instances.clear();
            self.player_ui_glyph_instance_buffer = None;
            return;
        };

        let text_boxes = self
            .player_ui_text_boxes
            .get(&key)
            .cloned()
            .unwrap_or_default();
        let buttons = self
            .player_ui_buttons
            .get(&key)
            .cloned()
            .unwrap_or_default();

        let mut verts = Vec::new();
        text_render::append_text_box_gizmo_verts(
            &mut verts,
            &text_boxes,
            self.player_ui_selected_text_id,
            self.player_ui_text_editing_id,
        );
        button_render::append_button_gizmo_verts(
            &mut verts,
            &buttons,
            self.player_ui_selected_button_id,
        );
        self.player_ui_text_overlay_buffer = gizmo::build_from_vertices(&self.device, &verts);

        self.player_ui_text_atlas.reset(&self.queue);
        self.player_ui_glyph_instances.clear();

        let mut font_cache = self.player_ui_font_cache.clone();
        text_render::append_text_glyphs(self, &text_boxes, &mut font_cache);
        button_render::append_button_hud_glyphs(
            &buttons,
            &mut font_cache,
            &mut self.player_ui_text_atlas,
            &self.queue,
            &mut self.player_ui_glyph_instances,
            self.size.width.max(1) as f32,
            self.size.height.max(1) as f32,
        );
        self.player_ui_font_cache = font_cache;

        self.player_ui_glyph_instance_buffer =
            if self.player_ui_glyph_instances.is_empty() {
                None
            } else {
                use wgpu::util::DeviceExt;
                Some(self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("player-ui-hud-glyphs-inst"),
                    contents: bytemuck::cast_slice(&self.player_ui_glyph_instances),
                    usage: wgpu::BufferUsages::VERTEX,
                }))
            };
    }

    pub(crate) fn draw_player_ui_hud(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
    ) -> u32 {
        if !self.player_ui_edit_active {
            return 0;
        }

        let has_gizmo = self.player_ui_text_overlay_buffer.vertex_count > 0;
        let has_glyphs = self
            .player_ui_glyph_instance_buffer
            .as_ref()
            .is_some_and(|_| !self.player_ui_glyph_instances.is_empty());
        if !has_gizmo && !has_glyphs {
            return 0;
        }

        let mut draw_calls = 0u32;
        const NDC_IDENTITY: [[f32; 4]; 9] = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
            [-1.0, -1.0, 0.0, 0.0],
        ];

        if has_gizmo {
            self.queue.write_buffer(
                &self.gizmo_buffer_uni,
                0,
                bytemuck::cast_slice(&NDC_IDENTITY),
            );
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("player-ui-gizmo-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.gizmo_pipeline);
            pass.set_bind_group(0, &self.gizmo_bind_group, &[]);
            pass.set_vertex_buffer(0, self.player_ui_text_overlay_buffer.vertex_buffer.slice(..));
            pass.draw(0..self.player_ui_text_overlay_buffer.vertex_count, 0..1);
            draw_calls += 1;
        }

        if self.player_ui_text_editing_id.is_some() && self.player_ui_caret_blink_visible() {
            if let Some(edit_id) = self.player_ui_text_editing_id {
                text_render::rebuild_caret_buffer(self, edit_id);
            }
            if self.player_ui_caret_buffer.vertex_count > 0 {
                self.queue.write_buffer(
                    &self.gizmo_buffer_uni,
                    0,
                    bytemuck::cast_slice(&NDC_IDENTITY),
                );
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("player-ui-caret-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.gizmo_pipeline);
                pass.set_bind_group(0, &self.gizmo_bind_group, &[]);
                pass.set_vertex_buffer(0, self.player_ui_caret_buffer.vertex_buffer.slice(..));
                pass.draw(0..self.player_ui_caret_buffer.vertex_count, 0..1);
                draw_calls += 1;
            }
        }

        if has_glyphs {
            if let Some(inst_buf) = &self.player_ui_glyph_instance_buffer {
                let count = self.player_ui_glyph_instances.len() as u32;
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("player-ui-hud-glyphs-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    occlusion_query_set: None,
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.screen_hud_pipeline);
                pass.set_bind_group(0, &self.hud_scene_bind_group, &[]);
                pass.set_bind_group(1, self.player_ui_text_atlas.bind_group.as_ref(), &[]);
                pass.set_vertex_buffer(0, self.hud_quad_mesh.vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, inst_buf.slice(..));
                pass.set_index_buffer(
                    self.hud_quad_mesh.index_buffer.slice(..),
                    wgpu::IndexFormat::Uint32,
                );
                pass.draw_indexed(0..self.hud_quad_mesh.index_count, 0, 0..count);
                draw_calls += 1;
            }
        }

        draw_calls
    }
}
