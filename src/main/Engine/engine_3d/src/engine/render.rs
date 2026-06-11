use glam::Vec3 as GlamVec3;
use winit::dpi::PhysicalSize;

use crate::config_3d::Camera;
use crate::gizmo;

use glam::Mat4;

use super::{SceneUniforms, State, DEPTH_FORMAT};

impl State {
    fn editor_selection_flag(
        &self,
        entity_id: crate::ecs::EntityId,
        is_selected: bool,
        is_hovered: bool,
    ) -> f32 {
        if self.preview_playing {
            return 0.0;
        }
        if self.entity_textures_preview_entity == Some(entity_id) && is_selected {
            return 0.0;
        }
        if is_selected {
            1.0
        } else if is_hovered {
            2.0
        } else {
            0.0
        }
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        self.size = new_size;
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        self.surface.configure(&self.device, &self.config);
        self.depth_view = create_depth_texture(&self.device, &self.config);
        self.taa.resize(
            &self.device,
            self.config.format,
            new_size.width,
            new_size.height,
        );
        if self.player_ui_edit_active {
            self.rebuild_player_ui_screen_grid();
        }
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        self.update_animations();
        self.update_skinned_animations();
        let mut draw_calls: u32 = 0;

        self.spatial_grid.clear();
        for &entity in self.world.entities() {
            if let Some(t) = self.world.get::<crate::ecs::Transform>(entity) {
                let sx = t.scale.x.abs() * 0.5;
                let sz = t.scale.z.abs() * 0.5;
                let min_x = t.position.x - sx;
                let min_z = t.position.z - sz;
                let max_x = t.position.x + sx;
                let max_z = t.position.z + sz;
                self.spatial_grid.insert_entity(entity, [min_x, min_z, max_x, max_z]);
            }
        }

        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render-encoder"),
            });

        self.sync_directional_light_from_sun();
        if self.taa.enabled {
            self.taa.begin_frame(false);
        }
        let scene_uni = if self.uses_player_fps_viewport() {
            build_scene_uniforms_from_view(
                &self.camera,
                self.camera_view_matrix(),
                self.camera_world_position(),
                self.size,
                self.prev_view_proj,
                self.taa.current_jitter,
                self.scene_light_dir(),
                self.scene_light_color(),
                self.build_light_view_proj(),
                self.scene_light_params(),
                self.scene_shadow_bias(),
            )
        } else {
            build_scene_uniforms(
                &self.camera,
                self.orbit_view_anchor(),
                self.viewport_orbit_angles(),
                self.size,
                self.prev_view_proj,
                self.taa.current_jitter,
                self.scene_light_dir(),
                self.scene_light_color(),
                self.build_light_view_proj(),
                self.scene_light_params(),
                self.scene_shadow_bias(),
            )
        };
        self.queue
            .write_buffer(&self.scene_buffer, 0, bytemuck::cast_slice(&[scene_uni]));
        let zoom_stability = if self.uses_player_fps_viewport() {
            crate::taa::zoom_stability_distance(0.01)
        } else {
            crate::taa::zoom_stability_distance(self.viewport_orbit_angles().2)
        };
        let shadows_enabled = scene_uni.light_color[3] > 0.5;
        let shadow_darkness = self.shadow_darkness;

        let ambient_view = self.taa.ambient_view();
        let direct_view = self.taa.direct_view();
        let depth_export_view = self.taa.depth_export_view();
        let velocity_view = self.taa.velocity_view();
        let shadow_mask_view = self.taa.shadow_mask_view();

        let aspect_fc = self.size.width as f32 / self.size.height as f32;
        let frustum_vp = {
            let raw = self
                .camera_to_uniform_at_anchor(self.orbit_view_anchor(), aspect_fc)
                .view_proj;
            glam::Mat4::from_cols_array_2d(&raw)
        };
        let mut entities: Vec<(crate::ecs::EntityId, usize, usize, glam::Mat4, i32)> = self
            .world
            .query2::<crate::ecs::MeshComponent, crate::ecs::Transform>()
            .filter_map(|(id, mc, t)| {
                if self.model_animation_bindings.contains_key(&id) {
                    return None;
                }
                let mesh_idx = mc.mesh_idx;
                let tex_idx = mc.tex_idx;
                let is_sun = self.sun_entity == Some(id);
                let is_ground = self
                    .world
                    .get::<crate::ecs::NameComponent>(id)
                    .is_some_and(|n| n.name.eq_ignore_ascii_case("ground"));
                // El sol vive lejos del origen (luz direccional); no recortar por caja del mundo ni frustum.
                if !is_sun
                    && !is_ground
                    && !self.world_bounds_3d.intersects_aabb(t.position, t.scale)
                {
                    return None;
                }
                let visible = if is_sun || is_ground {
                    true
                } else {
                    let radius = t.scale.x.abs().max(t.scale.y.abs()).max(t.scale.z.abs()) * 0.87;
                    is_visible_3d(&frustum_vp, t.position, radius)
                };
                if !visible {
                    return None;
                }
                let model_mat = t.to_matrix();
                let layer = self
                    .world
                    .get::<crate::ecs::RenderLayer>(id)
                    .map(|rl| rl.value)
                    .unwrap_or(0);
                Some((id, mesh_idx, tex_idx, model_mat, layer))
            })
            .collect();
        entities.sort_by(|a, b| a.4.cmp(&b.4));

        struct Batch {
            mesh_idx: usize,
            texture_layer: u32,
            instances: Vec<crate::mesh::InstanceData>,
        }
        let mut batches: Vec<Batch> = Vec::new();
        for (entity_id, mesh_idx, tex_idx, model_matrix, _layer) in &entities {
            if self.quick_build_ghost_id == Some(*entity_id)
                || self.plane_tool_ghost_id == Some(*entity_id)
            {
                continue;
            }
            if self.preview_playing
                && !self.debug_mode
                && (self.collider_entities.contains(entity_id)
                    || self.execution_area_entities.contains(entity_id))
            {
                continue;
            }
            let is_selected =
                self.selected_entity == Some(*entity_id) || self.selected_entities.contains(entity_id);
            let is_hovered = self.hovered_entity == Some(*entity_id);
            let flag = self.editor_selection_flag(*entity_id, is_selected, is_hovered);
            let layer = self.texture_layer_for(*tex_idx);
            let mut inst = crate::mesh::InstanceData::new(*model_matrix, flag, layer);
            if self.is_plane_wall_entity(*entity_id) {
                inst.flag_pad[1] = crate::config_3d::plane_tools::PLANE_WALL_VISUAL_ALPHA;
                inst.flag_pad[2] = crate::config_3d::plane_tools::PLANE_WALL_RENDER_KIND;
            }
            let can_extend = batches.last().map_or(false, |b| {
                b.mesh_idx == *mesh_idx && b.texture_layer == layer
            });
            if can_extend {
                batches.last_mut().unwrap().instances.push(inst);
            } else {
                batches.push(Batch {
                    mesh_idx: *mesh_idx,
                    texture_layer: layer,
                    instances: vec![inst],
                });
            }
        }

        let ghost_overlay = self.build_tool_ghost_overlay();

        let skinned_shadow = self.collect_skinned_draw_instances();
        let skinned_main = skinned_shadow.clone();

        let mut shadow_batches: Vec<Batch> = Vec::new();
        for (entity_id, mesh_idx, _tex_idx, model_matrix, _layer) in &entities {
            if self.quick_build_ghost_id == Some(*entity_id)
                || self.plane_tool_ghost_id == Some(*entity_id)
            {
                continue;
            }
            if self.is_plane_wall_entity(*entity_id) {
                continue;
            }
            if self.sun_entity == Some(*entity_id) {
                continue;
            }
            if *mesh_idx == 0 {
                continue;
            }
            let inst = crate::mesh::InstanceData::new(*model_matrix, 0.0, self.fallback_layer);
            let can_extend = shadow_batches
                .last()
                .map_or(false, |b| b.mesh_idx == *mesh_idx);
            if can_extend {
                shadow_batches.last_mut().unwrap().instances.push(inst);
            } else {
                shadow_batches.push(Batch {
                    mesh_idx: *mesh_idx,
                    texture_layer: self.fallback_layer,
                    instances: vec![inst],
                });
            }
        }

        let mut instance_slices = Vec::with_capacity(batches.len());
        for b in &batches {
            instance_slices.push(b.instances.as_slice());
        }
        let instance_buffers =
            self.scene_instance_pool
                .upload(&self.device, &self.queue, &instance_slices);

        {
            let mut shadow_slices = Vec::with_capacity(shadow_batches.len());
            for b in &shadow_batches {
                shadow_slices.push(b.instances.as_slice());
            }
            let shadow_instance_buffers = self.shadow_instance_pool.upload(
                &self.device,
                &self.queue,
                &shadow_slices,
            );

            let shadow_map_view = self._shadow_texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("shadow-map-pass"),
                ..Default::default()
            });
            let mut shadow_pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow-pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &shadow_map_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            shadow_pass.set_pipeline(&self.shadow_pipeline);
            shadow_pass.set_bind_group(0, &self.shadow_pass_bind_group, &[]);
            for (batch, inst_buf) in shadow_batches
                .iter()
                .zip(shadow_instance_buffers.iter())
            {
                let Some(mesh) = self.meshes.get(batch.mesh_idx) else {
                    continue;
                };
                shadow_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                shadow_pass.set_vertex_buffer(1, inst_buf.slice(..));
                shadow_pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                shadow_pass.draw_indexed(0..mesh.index_count, 0, 0..batch.instances.len() as u32);
            }

            if !skinned_shadow.is_empty() {
                shadow_pass.set_pipeline(&self.skinned_shadow_pipeline);
                let skinned_slices: Vec<&[crate::mesh::SkinnedInstanceData]> = skinned_shadow
                    .iter()
                    .map(|(_, inst)| std::slice::from_ref(inst))
                    .collect();
                let skinned_bufs = self.skinned_instance_pool.upload_skinned(
                    &self.device,
                    &self.queue,
                    &skinned_slices,
                );
                for ((gpu_idx, _), inst_buf) in skinned_shadow.iter().zip(skinned_bufs.iter()) {
                    let Some(entry) = self.skinned_gpu_meshes.get(*gpu_idx) else {
                        continue;
                    };
                    shadow_pass.set_bind_group(0, &self.shadow_pass_bind_group, &[]);
                    shadow_pass.set_bind_group(1, &entry.joint_bind_group, &[]);
                    shadow_pass.set_vertex_buffer(0, entry.mesh.vertex_buffer.slice(..));
                    shadow_pass.set_vertex_buffer(1, inst_buf.slice(..));
                    shadow_pass.set_index_buffer(entry.mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    shadow_pass.draw_indexed(0..entry.mesh.index_count, 0, 0..1);
                }
            }
        }

        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("render-pass"),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment {
                        view: ambient_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(self.clear_color),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: shadow_mask_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: direct_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: depth_export_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: velocity_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                ],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            pass.set_pipeline(&self.render_pipeline);

            pass.set_bind_group(0, &self.scene_bind_group, &[]);
            pass.set_bind_group(1, self.texture_array.bind_group.as_ref(), &[]);

            for (batch, inst_buf) in batches.iter().zip(instance_buffers.iter()) {
                let Some(mesh) = self.meshes.get(batch.mesh_idx) else {
                    continue;
                };
                pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, inst_buf.slice(..));
                pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.index_count, 0, 0..batch.instances.len() as u32);
                draw_calls += 1;
            }

            if !skinned_main.is_empty() {
                pass.set_pipeline(&self.skinned_render_pipeline);
                pass.set_bind_group(0, &self.scene_bind_group, &[]);
                pass.set_bind_group(1, self.texture_array.bind_group.as_ref(), &[]);
                let skinned_slices: Vec<&[crate::mesh::SkinnedInstanceData]> = skinned_main
                    .iter()
                    .map(|(_, inst)| std::slice::from_ref(inst))
                    .collect();
                let skinned_bufs = self.skinned_instance_pool.upload_skinned(
                    &self.device,
                    &self.queue,
                    &skinned_slices,
                );
                for ((gpu_idx, _), inst_buf) in skinned_main.iter().zip(skinned_bufs.iter()) {
                    let Some(entry) = self.skinned_gpu_meshes.get(*gpu_idx) else {
                        continue;
                    };
                    pass.set_bind_group(2, &entry.joint_bind_group, &[]);
                    pass.set_vertex_buffer(0, entry.mesh.vertex_buffer.slice(..));
                    pass.set_vertex_buffer(1, inst_buf.slice(..));
                    pass.set_index_buffer(entry.mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..entry.mesh.index_count, 0, 0..1);
                    draw_calls += 1;
                }
            }
        }

        let inv_vp = scene_uni.inv_view_proj;
        let prev_vp = scene_uni.prev_view_proj;
        if shadows_enabled {
            self.taa.resolve_shadow_and_present(
                &self.device,
                &self.queue,
                &mut enc,
                &view,
                shadow_darkness,
                true,
                zoom_stability,
                self.size.width,
                self.size.height,
                inv_vp,
                prev_vp,
            );
        } else {
            self.taa.present_scene(
                &self.device,
                &self.queue,
                &mut enc,
                &view,
                true,
                zoom_stability,
                self.size.width,
                self.size.height,
                inv_vp,
                prev_vp,
            );
        }

        self.prev_view_proj = scene_uni.view_proj_stable;

        if let Some((ghost_mesh_idx, ghost_inst)) = ghost_overlay {
            use wgpu::util::DeviceExt;
            let ghost_inst_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("qb-ghost-inst-buf"),
                contents: bytemuck::cast_slice(&[ghost_inst]),
                usage: wgpu::BufferUsages::VERTEX,
            });
            if let Some(mesh) = self.meshes.get(ghost_mesh_idx) {
                let mut ghost_pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("qb-ghost-overlay-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
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
                ghost_pass.set_pipeline(&self.render_pipeline_overlay);
                ghost_pass.set_bind_group(0, &self.scene_bind_group, &[]);
                ghost_pass.set_bind_group(1, self.texture_array.bind_group.as_ref(), &[]);
                ghost_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                ghost_pass.set_vertex_buffer(1, ghost_inst_buf.slice(..));
                ghost_pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                ghost_pass.draw_indexed(0..mesh.index_count, 0, 0..1);
                draw_calls += 1;
            }
        }

        if self.debug_mode && !self.preview_playing {
            let collision_overlay =
                crate::config_3d::collision_overlay::build_editor_collision_overlay(
                    &self.device,
                    self,
                );
            if collision_overlay.vertex_count > 0 {
                let aspect = self.size.width as f32 / self.size.height as f32;
                let vp = self
                    .camera_to_uniform_at_anchor(self.orbit_view_anchor(), aspect)
                    .view_proj;
                let col_uni: [[f32; 4]; 9] = [
                    vp[0],
                    vp[1],
                    vp[2],
                    vp[3],
                    [1.0, 0.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                    [-1.0, -1.0, 0.0, 0.0],
                ];
                self.queue.write_buffer(
                    &self.grid_buffer_uni,
                    0,
                    bytemuck::cast_slice(&col_uni),
                );

                let mut col_pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("collision-debug-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
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
                col_pass.set_pipeline(&self.grid_pipeline);
                col_pass.set_bind_group(0, &self.grid_bind_group, &[]);
                col_pass.set_vertex_buffer(0, collision_overlay.vertex_buffer.slice(..));
                col_pass.draw(0..collision_overlay.vertex_count, 0..1);
                draw_calls += 1;
            }
        }

        if self.world_bounds_buffer.vertex_count > 0 {
            let aspect = self.size.width as f32 / self.size.height as f32;
            let vp = self
                .camera_to_uniform_at_anchor(self.orbit_view_anchor(), aspect)
                .view_proj;
            let bounds_uni: [[f32; 4]; 9] = [
                vp[0],
                vp[1],
                vp[2],
                vp[3],
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
                [-1.0, -1.0, 0.0, 0.0],
            ];
            self.queue
                .write_buffer(&self.grid_buffer_uni, 0, bytemuck::cast_slice(&bounds_uni));

            let mut bounds_pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("world-bounds-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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
            bounds_pass.set_pipeline(&self.grid_pipeline);
            bounds_pass.set_bind_group(0, &self.grid_bind_group, &[]);
            bounds_pass.set_vertex_buffer(0, self.world_bounds_buffer.vertex_buffer.slice(..));
            bounds_pass.draw(0..self.world_bounds_buffer.vertex_count, 0..1);
            draw_calls += 1;
        }

        // Gizmo de cámara FP en modo editor: cubito en el ojo + frustum hasta el
        // rectángulo lejano, para visualizar a dónde mirará la cámara al pulsar
        // Play (estilo Godot/Unity al seleccionar una `Camera3D`).
        if !self.preview_playing && !self.player_ui_edit_active && self.has_play_character() {
            if let Some((eye, yaw, pitch)) = self.play_character_camera_gizmo_pose() {
                let aspect = self.size.width as f32 / self.size.height as f32;
                let frustum_buf = gizmo::build_fps_camera_frustum(
                    &self.device,
                    eye,
                    yaw,
                    pitch,
                    self.camera.fov_y,
                    aspect,
                    self.fps_editor_frustum_distance,
                );

                let vp = self
                .camera_to_uniform_at_anchor(self.orbit_view_anchor(), aspect)
                .view_proj;
                let frustum_uni: [[f32; 4]; 9] = [
                    vp[0],
                    vp[1],
                    vp[2],
                    vp[3],
                    [1.0, 0.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                    [-1.0, -1.0, 0.0, 0.0],
                ];
                self.queue.write_buffer(
                    &self.grid_buffer_uni,
                    0,
                    bytemuck::cast_slice(&frustum_uni),
                );

                let mut frustum_pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("fp-camera-frustum-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
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
                frustum_pass.set_pipeline(&self.grid_pipeline);
                frustum_pass.set_bind_group(0, &self.grid_bind_group, &[]);
                frustum_pass.set_vertex_buffer(0, frustum_buf.vertex_buffer.slice(..));
                frustum_pass.draw(0..frustum_buf.vertex_count, 0..1);
                draw_calls += 1;
            }
        }

        if let Some(hint_inst) = self.build_fps_exit_hint_instance() {
            use wgpu::util::DeviceExt;
            let hint_inst_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("fp-exit-hint-inst-buf"),
                contents: bytemuck::cast_slice(&[hint_inst]),
                usage: wgpu::BufferUsages::VERTEX,
            });

            let mut hint_pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("fp-exit-hint-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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
            hint_pass.set_pipeline(&self.screen_hud_pipeline);
            hint_pass.set_bind_group(0, &self.hud_scene_bind_group, &[]);
            hint_pass.set_bind_group(1, self.screen_hud_atlas.bind_group.as_ref(), &[]);
            hint_pass.set_vertex_buffer(0, self.hud_quad_mesh.vertex_buffer.slice(..));
            hint_pass.set_vertex_buffer(1, hint_inst_buf.slice(..));
            hint_pass.set_index_buffer(
                self.hud_quad_mesh.index_buffer.slice(..),
                wgpu::IndexFormat::Uint32,
            );
            hint_pass.draw_indexed(0..self.hud_quad_mesh.index_count, 0, 0..1);
            draw_calls += 1;
        }

        draw_calls += self.draw_player_ui_screen_grid(&mut enc, &view);
        draw_calls += self.draw_player_ui_object_draw_overlay(&mut enc, &view);
        draw_calls += self.draw_player_ui_text_boxes(&mut enc, &view);

        if !self.preview_playing
            && !self.player_ui_edit_active
            && self.entity_textures_preview_entity.is_none()
        {
            if let Some(origin) = self.selection_center().filter(|_| self.pivot_edit_mode.is_none())
            {
                let aspect = self.size.width as f32 / self.size.height as f32;
                let vp = self
                    .camera_to_uniform_at_anchor(self.orbit_view_anchor(), aspect)
                    .view_proj;

                let gizmo_model = glam::Mat4::from_translation(origin);

                let gm = gizmo_model.to_cols_array_2d();
                let h_ax = self.hovered_gizmo_axis.map(|a| a as f32).unwrap_or(-1.0);
                let a_ax = self.active_gizmo_axis.map(|a| a as f32).unwrap_or(-1.0);
                let gizmo_uni: [[f32; 4]; 9] = [
                    vp[0],
                    vp[1],
                    vp[2],
                    vp[3],
                    gm[0],
                    gm[1],
                    gm[2],
                    gm[3],
                    [h_ax, a_ax, 0.0, 0.0],
                ];
                self.queue.write_buffer(
                    &self.gizmo_buffer_uni,
                    0,
                    bytemuck::cast_slice(&gizmo_uni),
                );

                let mut gpass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("gizmo-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
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

    fn collect_skinned_draw_instances(&self) -> Vec<(usize, crate::mesh::SkinnedInstanceData)> {
        // Clonable lightweight list for shadow + main pass.
        let mut out = Vec::new();
        for (&id, binding) in &self.model_animation_bindings {
            let Some(t) = self.world.get::<crate::ecs::Transform>(id) else {
                continue;
            };
            if self.sun_entity == Some(id) {
                continue;
            }
            let is_ground = self
                .world
                .get::<crate::ecs::NameComponent>(id)
                .is_some_and(|n| n.name.eq_ignore_ascii_case("ground"));
            if !is_ground
                && !self
                    .world_bounds_3d
                    .intersects_aabb(t.position, t.scale)
            {
                continue;
            }
            let is_selected = self.selected_entity == Some(id)
                || self.selected_entities.contains(&id);
            let is_hovered = self.hovered_entity == Some(id);
            let flag = self.editor_selection_flag(id, is_selected, is_hovered);
            for (pi, &gpu_idx) in binding.part_gpu_indices.iter().enumerate() {
                let layer = binding
                    .part_tex_layers
                    .get(pi)
                    .copied()
                    .unwrap_or(binding.tex_layer);
                let inst = crate::mesh::InstanceData::new(t.to_matrix(), flag, layer);
                out.push((gpu_idx, crate::mesh::SkinnedInstanceData::from_instance(&inst)));
            }
        }
        out
    }
}

pub(crate) fn create_depth_texture(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
) -> wgpu::TextureView {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth-texture"),
        size: wgpu::Extent3d {
            width: config.width.max(1),
            height: config.height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

pub(crate) fn build_scene_uniforms(
    camera: &Camera,
    orbit_anchor: glam::Vec3,
    orbit_angles: (f32, f32, f32),
    size: PhysicalSize<u32>,
    prev_view_proj: [[f32; 4]; 4],
    jitter: [f32; 2],
    light_dir: [f32; 4],
    light_color: [f32; 4],
    light_view_proj: [[f32; 4]; 4],
    light_params: [f32; 4],
    shadow_bias: [f32; 4],
) -> SceneUniforms {
    let (yaw, pitch, distance) = orbit_angles;
    let view = camera.view_matrix_at_angles(orbit_anchor, yaw, pitch, distance);
    let eye = camera.position_at_angles(orbit_anchor, yaw, pitch, distance);
    build_scene_uniforms_from_view(
        camera,
        view,
        eye,
        size,
        prev_view_proj,
        jitter,
        light_dir,
        light_color,
        light_view_proj,
        light_params,
        shadow_bias,
    )
}

pub(crate) fn build_scene_uniforms_from_view(
    camera: &Camera,
    view: Mat4,
    eye: glam::Vec3,
    size: PhysicalSize<u32>,
    prev_view_proj: [[f32; 4]; 4],
    jitter: [f32; 2],
    light_dir: [f32; 4],
    light_color: [f32; 4],
    light_view_proj: [[f32; 4]; 4],
    light_params: [f32; 4],
    shadow_bias: [f32; 4],
) -> SceneUniforms {
    let aspect = size.width as f32 / size.height as f32;
    let w = size.width.max(1) as f32;
    let h = size.height.max(1) as f32;
    let proj = camera.proj_matrix(aspect);
    let vp_stable = (proj * view).to_cols_array_2d();
    let mut proj_j = proj;
    proj_j.x_axis.z += jitter[0] * 2.0 / w;
    proj_j.y_axis.z += jitter[1] * 2.0 / h;
    let vp = proj_j * view;
    let view_proj = vp.to_cols_array_2d();
    let inv_view_proj = vp.inverse().to_cols_array_2d();
    SceneUniforms {
        view_proj,
        view_proj_stable: vp_stable,
        prev_view_proj,
        inv_view_proj,
        cam_pos: [eye.x, eye.y, eye.z, 0.0],
        light_dir,
        light_color,
        light_view_proj,
        light_params,
        jitter: [jitter[0], jitter[1], 0.0, 0.0],
        shadow_bias,
    }
}

pub(crate) fn is_visible_3d(view_proj: &glam::Mat4, center: GlamVec3, radius: f32) -> bool {
    let m = view_proj.to_cols_array_2d();
    let r0 = [m[0][0], m[1][0], m[2][0], m[3][0]];
    let r1 = [m[0][1], m[1][1], m[2][1], m[3][1]];
    let r2 = [m[0][2], m[1][2], m[2][2], m[3][2]];
    let r3 = [m[0][3], m[1][3], m[2][3], m[3][3]];

    let planes: [[f32; 4]; 6] = [
        [r3[0] + r0[0], r3[1] + r0[1], r3[2] + r0[2], r3[3] + r0[3]],
        [r3[0] - r0[0], r3[1] - r0[1], r3[2] - r0[2], r3[3] - r0[3]],
        [r3[0] + r1[0], r3[1] + r1[1], r3[2] + r1[2], r3[3] + r1[3]],
        [r3[0] - r1[0], r3[1] - r1[1], r3[2] - r1[2], r3[3] - r1[3]],
        [r3[0] + r2[0], r3[1] + r2[1], r3[2] + r2[2], r3[3] + r2[3]],
        [r3[0] - r2[0], r3[1] - r2[1], r3[2] - r2[2], r3[3] - r2[3]],
    ];

    for plane in &planes {
        let len = (plane[0] * plane[0] + plane[1] * plane[1] + plane[2] * plane[2]).sqrt();
        if len < 1e-6 {
            continue;
        }
        let dist = (plane[0] * center.x + plane[1] * center.y + plane[2] * center.z + plane[3]) / len;
        if dist < -radius {
            return false;
        }
    }
    true
}
