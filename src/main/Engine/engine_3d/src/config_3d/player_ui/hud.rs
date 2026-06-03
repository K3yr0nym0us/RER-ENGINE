//! Orquestación del overlay HUD (texto, botones, imágenes y objetos poligonales).

use std::collections::HashMap;
use std::sync::Arc;

use ab_glyph::FontArc;

use crate::engine::State;
use crate::gizmo;

use super::button_render;
use super::image_render;
use super::text_render;

impl State {
    /// Clave `scope:screen_id` de la pantalla HUD activa (edición o play).
    pub(crate) fn player_ui_screen_key(&self) -> Option<String> {
        if self.player_ui_edit_active {
            let scope = self.player_ui_edit_scope.as_deref()?;
            let screen_id = self.player_ui_edit_screen_id.as_deref()?;
            return Some(format!("{scope}:{screen_id}"));
        }
        if self.preview_playing {
            let screen_id = self.player_ui_play_screen_id()?;
            return Some(format!("player:{screen_id}"));
        }
        None
    }

    pub(crate) fn player_ui_hud_visible(&self) -> bool {
        self.player_ui_edit_active
            || (self.preview_playing && self.player_ui_play_screen_id().is_some())
    }

    pub(crate) fn apply_player_ui_play_hud(&mut self, entering_play: bool) {
        if entering_play {
            if let Some(id) = self.player_ui_play_screen_id() {
                self.rebuild_player_ui_overlay();
                log::info!("[player-ui] HUD en play: pantalla {id}");
            } else {
                log::info!("[player-ui] play sin pantalla Player UI activa con contenido HUD");
            }
        } else if !self.player_ui_edit_active {
            self.rebuild_player_ui_overlay();
        }
    }

    pub(crate) fn player_ui_font_cached(&self, path: &str) -> Option<Arc<FontArc>> {
        self.player_ui_font_cache.get(path).cloned()
    }

    pub(crate) fn player_ui_font_cached_mut(
        &mut self,
        cache: &mut HashMap<String, Arc<FontArc>>,
        path: &str,
    ) -> Option<Arc<FontArc>> {
        if let Some(font) = cache.get(path) {
            return Some(font.clone());
        }
        let font = super::font::load_font_arc(path)?;
        cache.insert(path.to_string(), font.clone());
        Some(font)
    }

    /// Rebuild completo: resetea atlas y caché de texturas (alta/baja/edición de texto).
    pub(crate) fn rebuild_player_ui_overlay(&mut self) {
        self.rebuild_player_ui_overlay_inner(true);
    }

    /// Preview en vivo durante arrastre: conserva atlas + UV en caché (sin releer PNG cada frame).
    pub(crate) fn rebuild_player_ui_overlay_live(&mut self) {
        self.rebuild_player_ui_overlay_inner(false);
    }

    fn rebuild_player_ui_overlay_inner(&mut self, reset_atlas: bool) {
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
        let vw = self.size.width.max(1) as f32;
        let vh = self.size.height.max(1) as f32;

        // Solo normalizar proporción en rebuild completo; en vivo respetar width/height del drag
        // (p. ej. resize libre con Shift).
        if reset_atlas {
            if let Some(list) = self.player_ui_images.get_mut(&key) {
                for img in list.iter_mut() {
                    img.sync_height_for_viewport(vw, vh);
                }
            }
            if let Some(list) = self.player_ui_buttons.get_mut(&key) {
                for btn in list.iter_mut() {
                    btn.sync_height_for_viewport(vw, vh);
                }
            }
        }

        let images = self
            .player_ui_images
            .get(&key)
            .cloned()
            .unwrap_or_default();
        let buttons = self
            .player_ui_buttons
            .get(&key)
            .cloned()
            .unwrap_or_default();
        let objects = self
            .player_ui_objects
            .get(&key)
            .cloned()
            .unwrap_or_default();

        let mut verts = Vec::new();
        let draw_order =
            super::hud_layers::hud_draw_order(&text_boxes, &buttons, &images, &objects);
        for layer in &draw_order {
            match layer.kind {
                super::hud_layers::HudLayerKind::Object => {
                    let obj = &objects[layer.index];
                    let selected = if self.player_ui_edit_active {
                        self.player_ui_selected_object_id
                    } else {
                        None
                    };
                    super::object::append_object_gizmo_verts(
                        &mut verts,
                        std::slice::from_ref(obj),
                        selected,
                    );
                }
                super::hud_layers::HudLayerKind::Text if self.player_ui_edit_active => {
                    let b = &text_boxes[layer.index];
                    text_render::append_text_box_gizmo_verts(
                        &mut verts,
                        std::slice::from_ref(b),
                        self.player_ui_selected_text_id,
                        self.player_ui_text_editing_id,
                    );
                }
                super::hud_layers::HudLayerKind::Button if self.player_ui_edit_active => {
                    let btn = &buttons[layer.index];
                    button_render::append_button_gizmo_verts(
                        &mut verts,
                        std::slice::from_ref(btn),
                        self.player_ui_selected_button_id,
                    );
                }
                super::hud_layers::HudLayerKind::Image if self.player_ui_edit_active => {
                    let img = &images[layer.index];
                    image_render::append_image_gizmo_verts(
                        &mut verts,
                        std::slice::from_ref(img),
                        self.player_ui_selected_image_id,
                    );
                }
                _ => {}
            }
        }
        self.player_ui_text_overlay_buffer = gizmo::build_from_vertices(&self.device, &verts);

        if reset_atlas {
            self.player_ui_text_atlas.reset(&self.queue);
            self.player_ui_hud_texture_cache.clear();
        }

        self.player_ui_glyph_instances.clear();

        let mut font_cache = self.player_ui_font_cache.clone();
        for layer in &draw_order {
            match layer.kind {
                super::hud_layers::HudLayerKind::Text => {
                    let b = &text_boxes[layer.index];
                    text_render::append_text_glyphs(self, std::slice::from_ref(b), &mut font_cache);
                }
                super::hud_layers::HudLayerKind::Button => {
                    let btn = &buttons[layer.index];
                    button_render::append_button_hud_glyphs(
                        std::slice::from_ref(btn),
                        &mut font_cache,
                        &mut self.player_ui_text_atlas,
                        &self.queue,
                        &mut self.player_ui_glyph_instances,
                        vw,
                        vh,
                        &mut self.player_ui_hud_texture_cache,
                    );
                }
                super::hud_layers::HudLayerKind::Image => {
                    let img = &images[layer.index];
                    image_render::append_image_hud_glyphs(
                        std::slice::from_ref(img),
                        &mut self.player_ui_text_atlas,
                        &self.queue,
                        &mut self.player_ui_glyph_instances,
                        vw,
                        vh,
                        &mut self.player_ui_hud_texture_cache,
                    );
                }
                super::hud_layers::HudLayerKind::Object => {}
            }
        }
        self.player_ui_font_cache = font_cache;

        self.upload_player_ui_glyph_instances();
    }

    fn upload_player_ui_glyph_instances(&mut self) {
        if self.player_ui_glyph_instances.is_empty() {
            self.player_ui_glyph_instance_buffer = None;
            return;
        }
        let bytes = bytemuck::cast_slice(&self.player_ui_glyph_instances);
        let size = bytes.len() as u64;
        if let Some(buf) = &self.player_ui_glyph_instance_buffer {
            if buf.size() == size {
                self.queue.write_buffer(buf, 0, bytes);
                return;
            }
        }
        use wgpu::util::DeviceExt;
        self.player_ui_glyph_instance_buffer =
            Some(self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("player-ui-hud-glyphs-inst"),
                contents: bytes,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            }));
    }

    pub(crate) fn draw_player_ui_hud(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
    ) -> u32 {
        if !self.player_ui_hud_visible() {
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
