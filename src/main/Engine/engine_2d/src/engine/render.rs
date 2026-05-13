use glam::Mat4;
use wgpu::util::DeviceExt;

use crate::config_2d::build_scenario_collision_overlay;
use crate::ecs::MeshComponent;
use crate::mesh;

use super::render_helpers::{build_scene_uniforms, build_scene_uniforms_2d, is_visible_2d};
use super::State;

impl State {
    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        self.update_animations();
        let mut draw_calls: u32 = 0;

        // ── Paso 0: reconstruir spatial grid para picking ──────────────────────────
        self.spatial_grid.clear();
        for &entity in self.world.entities() {
            if let Some(t) = self.world.get::<crate::ecs::Transform>(entity) {
                let sx = t.scale.x.abs() * 0.5;
                let sy = t.scale.y.abs() * 0.5;
                let center = if let Some(vo) = self.visual_offsets.get(&entity) {
                    t.position + *vo
                } else {
                    t.position
                };
                let min_x = center.x - sx;
                let min_y = center.y - sy;
                let max_x = center.x + sx;
                let max_y = center.y + sy;
                self.spatial_grid.insert_entity(entity, [min_x, min_y, max_x, max_y]);
            }
        }

        let output  = self.surface.get_current_texture()?;
        let view    = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut enc = self.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("render-encoder") },
        );

        // ── Paso 1: escribir uniforms de escena compartidos (view_proj + cam_pos) ──
        {
            let scene_uni = if let Some(cam2d) = &self.camera_2d {
                build_scene_uniforms_2d(cam2d, self.size)
            } else {
                build_scene_uniforms(&self.camera, self.size)
            };
            self.queue.write_buffer(&self.scene_buffer, 0, bytemuck::cast_slice(&[scene_uni]));
        }

        // ── Paso 2: recopilar entidades visibles (culling 2D + sort layer+Z) ──
        let aspect_fc = self.size.width as f32 / self.size.height as f32;
        // query2<MeshComponent, Transform> itera solo entidades con ambos componentes,
        // evitando el scan de todas las entidades + doble lookup de hash por entidad.
        let visual_offsets = &self.visual_offsets;
        let mut entities: Vec<(crate::ecs::EntityId, usize, usize, Mat4, i32, f32)> =
            self.world.query2::<MeshComponent, crate::ecs::Transform>()
            .filter_map(|(id, mc, t)| {
                let mesh_idx = mc.mesh_idx;
                let tex_idx  = mc.tex_idx;
                // ── Culling por viewport 2D ──────────────────────────────────
                // Para culling usamos la posición visual (con offset de pivot)
                let visual_pos = if let Some(vo) = visual_offsets.get(&id) {
                    t.position + *vo
                } else {
                    t.position
                };
                let visible = self.camera_2d
                    .as_ref()
                    .map(|cam2d| is_visible_2d(cam2d, visual_pos, t.scale, aspect_fc))
                    .unwrap_or(true);
                if !visible { return None; }
                let model_mat = if let Some(vo) = visual_offsets.get(&id) {
                    Mat4::from_scale_rotation_translation(t.scale, t.rotation, t.position + *vo)
                } else {
                    t.to_matrix()
                };
                let layer     = self.world.get::<crate::ecs::RenderLayer>(id).map(|rl| rl.value).unwrap_or(0);
                let z         = t.position.z;
                Some((id, mesh_idx, tex_idx, model_mat, layer, z))
            }).collect();
        // Sort by (layer ASC, z ASC) — lower layer first, within layer sort by z (back-to-front)
        entities.sort_by(|a, b| {
            let layer_cmp = a.4.cmp(&b.4);
            if layer_cmp != std::cmp::Ordering::Equal {
                layer_cmp
            } else {
                a.5.partial_cmp(&b.5).unwrap_or(std::cmp::Ordering::Equal)
            }
        });

        // ── Paso 3: agrupar en batches por mesh_idx ────────────────────────
        // Con el atlas todas las entidades comparten el mismo bind group,
        // así que solo agrupamos por geometría (mesh_idx).
        // El UV rect viaja en cada instancia — no hay cambio de bind group entre batches.
        struct Batch {
            mesh_idx:  usize,
            instances: Vec<mesh::InstanceData>,
        }
        let mut batches: Vec<Batch> = Vec::new();
        for (entity_id, mesh_idx, tex_idx, model_matrix, _layer, _z) in &entities {
            if self.preview_playing && !self.debug_mode
                && (self.collider_entities.contains(entity_id)
                    || self.execution_area_entities.contains(entity_id))
            {
                continue;
            }
            let is_selected = self.selected_entity == Some(*entity_id)
                || self.selected_entities.contains(entity_id);
            let flag = if self.preview_playing {
                0.0_f32
            } else if is_selected {
                1.0_f32   // dorado
            } else if self.hovered_entity == Some(*entity_id) {
                2.0_f32   // cian
            } else {
                0.0_f32
            };
            // anim_overrides tiene prioridad sobre uv_rects[]:
            // durante una animación activa evita mutar la UV base de la entidad.
            let uv = self.anim_overrides.get(tex_idx)
                .copied()
                .or_else(|| self.uv_rects.get(*tex_idx).copied())
                .unwrap_or(self.fallback_uv);
            let mut inst = mesh::InstanceData::new(*model_matrix, flag, uv);
            inst.flag_pad[2] = if self.world.get::<crate::config_2d::ColliderMarker>(*entity_id).is_some() {
                1.0_f32
            } else if self.world.get::<crate::config_2d::ExecutionAreaMarker>(*entity_id).is_some() {
                2.0_f32
            } else {
                0.0_f32
            };
            // Extender el último batch si coincide mesh (mismo UV rect viaja por instancia)
            let can_extend = batches.last().map_or(false, |b| b.mesh_idx == *mesh_idx);
            if can_extend {
                batches.last_mut().unwrap().instances.push(inst);
            } else {
                batches.push(Batch { mesh_idx: *mesh_idx, instances: vec![inst] });
            }
        }

        // ── Paso 4: crear buffers de instancias en GPU ──────────────────────
        let instance_buffers: Vec<wgpu::Buffer> = batches.iter().map(|b| {
            self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label:    Some("inst-buf"),
                contents: bytemuck::cast_slice(&b.instances),
                usage:    wgpu::BufferUsages::VERTEX,
            })
        }).collect();

        // ── Paso 5: render pass principal ──────────────────────────────────
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("render-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view:           &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load:  wgpu::LoadOp::Clear(self.clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load:  wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes:    None,
            });

            // En 2D usamos el pipeline sin depth-write: el sort back-to-front
            // más el alpha blending se encargan del orden correcto, y no hay
            // bloqueo de píxeles transparentes por profundidad.
            if self.camera_2d.is_some() {
                pass.set_pipeline(&self.render_pipeline_2d);
            } else {
                pass.set_pipeline(&self.render_pipeline);
            }

            // El bind group 0 (view_proj + cam_pos) es compartido por todos los batches.
            pass.set_bind_group(0, &self.scene_bind_group, &[]);
            // El bind group 1 (atlas) es compartido por TODOS los sprites — se setea UNA vez.
            pass.set_bind_group(1, self.atlas.bind_group.as_ref(), &[]);

            for (batch, inst_buf) in batches.iter().zip(instance_buffers.iter()) {
                let Some(mesh) = self.meshes.get(batch.mesh_idx) else { continue };
                pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, inst_buf.slice(..));
                pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.index_count, 0, 0..batch.instances.len() as u32);
                draw_calls += 1;
            }
        }

        // ── Grid pass (solo modo 2D; borde siempre visible, líneas según config) ──
        if !self.preview_playing || self.debug_mode {
            if let Some(cam2d) = &self.camera_2d {
                let aspect   = self.size.width as f32 / self.size.height as f32;
                let vp       = cam2d.view_proj(aspect).to_cols_array_2d();
                // Uniforms: view_proj + model identity + flags -1
                let grid_uni: [[f32; 4]; 9] = [
                    vp[0], vp[1], vp[2], vp[3],
                    [1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0],
                    [-1.0, -1.0, 0.0, 0.0],
                ];
                self.queue.write_buffer(&self.grid_buffer_uni, 0, bytemuck::cast_slice(&grid_uni));

                let mut grd_pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("grid-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view:           &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load:  wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set:      None,
                    timestamp_writes:         None,
                });
                grd_pass.set_pipeline(&self.grid_pipeline);
                grd_pass.set_bind_group(0, &self.grid_bind_group, &[]);
                grd_pass.set_vertex_buffer(0, self.grid_buffer.vertex_buffer.slice(..));
                grd_pass.draw(0..self.grid_buffer.vertex_count, 0..1);
                draw_calls += 1;
            }

            let collision_overlay_buffer = build_scenario_collision_overlay(&self.device, self);
            if collision_overlay_buffer.vertex_count > 0 {
                let mut collision_pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("collision-overlay-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view:           &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load:  wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set:      None,
                    timestamp_writes:         None,
                });
                collision_pass.set_pipeline(&self.grid_pipeline);
                collision_pass.set_bind_group(0, &self.grid_bind_group, &[]);
                collision_pass.set_vertex_buffer(0, collision_overlay_buffer.vertex_buffer.slice(..));
                collision_pass.draw(0..collision_overlay_buffer.vertex_count, 0..1);
                draw_calls += 1;
            }
        }

        // ── Tool overlay pass (solo modo 2D; cruces + líneas de construcción) ──
        if !self.preview_playing && self.camera_2d.is_some() && self.tool_overlay_buffer.vertex_count > 0 {
            let mut tool_pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("tool-overlay-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view:           &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load:  wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set:      None,
                timestamp_writes:         None,
            });
            tool_pass.set_pipeline(&self.grid_pipeline);          // LineList, sin depth
            tool_pass.set_bind_group(0, &self.grid_bind_group, &[]); // view_proj actualizado
            tool_pass.set_vertex_buffer(0, self.tool_overlay_buffer.vertex_buffer.slice(..));
            tool_pass.draw(0..self.tool_overlay_buffer.vertex_count, 0..1);
            draw_calls += 1;
        }

        // ── Snap hint pass (PNG en viewport 2D durante drag de gizmo) ──────
        if let Some(hint_inst) = self.build_snap_hint_instance_2d() {
            let hint_inst_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label:    Some("snap-hint-inst-buf"),
                contents: bytemuck::cast_slice(&[hint_inst]),
                usage:    wgpu::BufferUsages::VERTEX,
            });

            if let Some(mesh) = self.meshes.get(self.canonical_quad_idx) {
                let mut hint_pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("snap-hint-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view:           &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load:  wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load:  wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    occlusion_query_set:      None,
                    timestamp_writes:         None,
                });
                hint_pass.set_pipeline(&self.render_pipeline_2d);
                hint_pass.set_bind_group(0, &self.scene_bind_group, &[]);
                hint_pass.set_bind_group(1, self.atlas.bind_group.as_ref(), &[]);
                hint_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                hint_pass.set_vertex_buffer(1, hint_inst_buf.slice(..));
                hint_pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                hint_pass.draw_indexed(0..mesh.index_count, 0, 0..1);
                draw_calls += 1;
            }
        }

        // ── Gizmos (segundo pass, sin depth) ─────────────────────────────────
        // Ocultar gizmo durante el modo edición de pivot: las flechas de movimiento
        // robarían el foco e impedirían hacer click libremente sobre el asset.
        if !self.preview_playing {
            if let Some(origin) = self.selection_center().filter(|_| self.pivot_edit_mode.is_none()) {
            let aspect   = self.size.width as f32 / self.size.height as f32;
            let vp = if let Some(cam2d) = &self.camera_2d {
                cam2d.view_proj(aspect).to_cols_array_2d()
            } else {
                self.camera.to_uniform(aspect).view_proj
            };

            // Situar el gizmo en el centro de selección (single o multi-select)
            let gizmo_model = glam::Mat4::from_translation(origin);

            let gm = gizmo_model.to_cols_array_2d();
            let h_ax = self.hovered_gizmo_axis.map(|a| a as f32).unwrap_or(-1.0);
            let a_ax = self.active_gizmo_axis.map(|a| a as f32).unwrap_or(-1.0);
            let gizmo_uni: [[f32; 4]; 9] = [
                vp[0], vp[1], vp[2], vp[3],
                gm[0], gm[1], gm[2], gm[3],
                [h_ax, a_ax, 0.0, 0.0],
            ];
            self.queue.write_buffer(
                &self.gizmo_buffer_uni, 0, bytemuck::cast_slice(&gizmo_uni),
            );

            let mut gpass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("gizmo-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view:           &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load:  wgpu::LoadOp::Load,   // preservar frame anterior
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set:      None,
                timestamp_writes:         None,
            });
            gpass.set_pipeline(&self.gizmo_pipeline);
            gpass.set_bind_group(0, &self.gizmo_bind_group, &[]);
            gpass.set_vertex_buffer(0, self.gizmo_buffer.vertex_buffer.slice(..));
            gpass.draw(0..self.gizmo_buffer.vertex_count, 0..1);
            draw_calls += 1;
            }
        }

        self.queue.submit(std::iter::once(enc.finish()));
        self.last_draw_calls = draw_calls;
        output.present();
        Ok(())
    }
}
