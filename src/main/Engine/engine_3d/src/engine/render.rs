use glam::Vec3 as GlamVec3;
use winit::dpi::PhysicalSize;

use crate::config_3d::Camera;
use crate::config_compat::Camera2D;
use crate::gizmo;

use super::{SceneUniforms, State, DEPTH_FORMAT};

impl State {
    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        self.size = new_size;
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        self.surface.configure(&self.device, &self.config);
        self.depth_view = create_depth_texture(&self.device, &self.config);
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        self.update_animations();
        let mut draw_calls: u32 = 0;

        self.spatial_grid.clear();
        for &entity in self.world.entities() {
            if let Some(t) = self.world.get::<crate::ecs::Transform>(entity) {
                let sx = t.scale.x.abs() * 0.5;
                let sy = t.scale.y.abs() * 0.5;
                let min_x = t.position.x - sx;
                let min_y = t.position.y - sy;
                let max_x = t.position.x + sx;
                let max_y = t.position.y + sy;
                self.spatial_grid.insert_entity(entity, [min_x, min_y, max_x, max_y]);
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

        {
            let scene_uni = if let Some(cam2d) = &self.camera_2d {
                build_scene_uniforms_2d(cam2d, self.size)
            } else {
                build_scene_uniforms(&self.camera, self.size)
            };
            self.queue
                .write_buffer(&self.scene_buffer, 0, bytemuck::cast_slice(&[scene_uni]));
        }

        let aspect_fc = self.size.width as f32 / self.size.height as f32;
        let frustum_vp_3d: Option<glam::Mat4> = self.camera_2d.is_none().then(|| {
            let raw = self.camera.to_uniform(aspect_fc).view_proj;
            glam::Mat4::from_cols_array_2d(&raw)
        });
        let mut entities: Vec<(crate::ecs::EntityId, usize, usize, glam::Mat4, i32, f32)> = self
            .world
            .query2::<crate::ecs::MeshComponent, crate::ecs::Transform>()
            .filter_map(|(id, mc, t)| {
                if self.preview_playing
                    && self.first_person_player_entity == Some(id)
                {
                    return None;
                }
                let mesh_idx = mc.mesh_idx;
                let tex_idx = mc.tex_idx;
                if self.camera_2d.is_none()
                    && !self.world_bounds_3d.intersects_aabb(t.position, t.scale)
                {
                    return None;
                }
                let visible = if let Some(cam2d) = &self.camera_2d {
                    is_visible_2d(cam2d, t.position, t.scale, aspect_fc)
                } else if let Some(vp) = &frustum_vp_3d {
                    let radius = t.scale.x.abs().max(t.scale.y.abs()).max(t.scale.z.abs()) * 0.87;
                    is_visible_3d(vp, t.position, radius)
                } else {
                    true
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
                let z = t.position.z;
                Some((id, mesh_idx, tex_idx, model_mat, layer, z))
            })
            .collect();
        entities.sort_by(|a, b| {
            let layer_cmp = a.4.cmp(&b.4);
            if layer_cmp != std::cmp::Ordering::Equal {
                layer_cmp
            } else {
                a.5.partial_cmp(&b.5).unwrap_or(std::cmp::Ordering::Equal)
            }
        });

        struct Batch {
            mesh_idx: usize,
            instances: Vec<crate::mesh::InstanceData>,
        }
        let mut batches: Vec<Batch> = Vec::new();
        for (entity_id, mesh_idx, tex_idx, model_matrix, _layer, _z) in &entities {
            if self.preview_playing
                && (self.collider_entities.contains(entity_id)
                    || self.execution_area_entities.contains(entity_id))
            {
                continue;
            }
            let is_selected =
                self.selected_entity == Some(*entity_id) || self.selected_entities.contains(entity_id);
            let flag = if self.preview_playing {
                0.0_f32
            } else if is_selected {
                1.0_f32
            } else if self.hovered_entity == Some(*entity_id) {
                2.0_f32
            } else {
                0.0_f32
            };
            let uv = self
                .anim_overrides
                .get(tex_idx)
                .copied()
                .or_else(|| self.uv_rects.get(*tex_idx).copied())
                .unwrap_or(self.fallback_uv);
            let mut inst = crate::mesh::InstanceData::new(*model_matrix, flag, uv);
            inst.flag_pad[2] = if self
                .world
                .get::<crate::config_compat::ColliderMarker>(*entity_id)
                .is_some()
            {
                1.0_f32
            } else if self
                .world
                .get::<crate::config_compat::ExecutionAreaMarker>(*entity_id)
                .is_some()
            {
                2.0_f32
            } else {
                0.0_f32
            };
            let can_extend = batches.last().map_or(false, |b| b.mesh_idx == *mesh_idx);
            if can_extend {
                batches.last_mut().unwrap().instances.push(inst);
            } else {
                batches.push(Batch {
                    mesh_idx: *mesh_idx,
                    instances: vec![inst],
                });
            }
        }

        let instance_buffers: Vec<wgpu::Buffer> = batches
            .iter()
            .map(|b| {
                use wgpu::util::DeviceExt;
                self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("inst-buf"),
                    contents: bytemuck::cast_slice(&b.instances),
                    usage: wgpu::BufferUsages::VERTEX,
                })
            })
            .collect();

        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("render-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
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

            if self.camera_2d.is_some() {
                pass.set_pipeline(&self.render_pipeline_2d);
            } else {
                pass.set_pipeline(&self.render_pipeline);
            }

            pass.set_bind_group(0, &self.scene_bind_group, &[]);
            pass.set_bind_group(1, self.atlas.bind_group.as_ref(), &[]);

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
        }

        if self.camera_2d.is_none() && self.world_bounds_buffer.vertex_count > 0 {
            let aspect = self.size.width as f32 / self.size.height as f32;
            let vp = self.camera.to_uniform(aspect).view_proj;
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
        if !self.preview_playing && self.camera_2d.is_none() && self.has_first_person_player() {
            if let Some((eye, yaw, pitch)) = self.first_person_camera_gizmo_pose() {
                let aspect = self.size.width as f32 / self.size.height as f32;
                let frustum_buf = gizmo::build_first_person_camera_frustum(
                    &self.device,
                    eye,
                    yaw,
                    pitch,
                    self.camera.fov_y,
                    aspect,
                    2.5,
                );

                let vp = self.camera.to_uniform(aspect).view_proj;
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

        if self.is_first_person_runtime_active() && self.crosshair_buffer.vertex_count > 0 {
            let crosshair_uni: [[f32; 4]; 9] = [
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
            self.queue.write_buffer(
                &self.grid_buffer_uni,
                0,
                bytemuck::cast_slice(&crosshair_uni),
            );

            let mut crosshair_pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("crosshair-pass"),
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
            crosshair_pass.set_pipeline(&self.grid_pipeline);
            crosshair_pass.set_bind_group(0, &self.grid_bind_group, &[]);
            crosshair_pass.set_vertex_buffer(0, self.crosshair_buffer.vertex_buffer.slice(..));
            crosshair_pass.draw(0..self.crosshair_buffer.vertex_count, 0..1);
            draw_calls += 1;
        }

        if !self.preview_playing {

            if let Some(cam2d) = &self.camera_2d {
                let aspect = self.size.width as f32 / self.size.height as f32;
                let vp = cam2d.view_proj(aspect).to_cols_array_2d();
                let grid_uni: [[f32; 4]; 9] = [
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
                    .write_buffer(&self.grid_buffer_uni, 0, bytemuck::cast_slice(&grid_uni));

                let mut grd_pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("grid-pass"),
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
                grd_pass.set_pipeline(&self.grid_pipeline);
                grd_pass.set_bind_group(0, &self.grid_bind_group, &[]);
                grd_pass.set_vertex_buffer(0, self.grid_buffer.vertex_buffer.slice(..));
                grd_pass.draw(0..self.grid_buffer.vertex_count, 0..1);
                draw_calls += 1;
            }
        }

        if !self.preview_playing && self.camera_2d.is_some() && self.tool_overlay_buffer.vertex_count > 0
        {
            let mut tool_pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("tool-overlay-pass"),
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
            tool_pass.set_pipeline(&self.grid_pipeline);
            tool_pass.set_bind_group(0, &self.grid_bind_group, &[]);
            tool_pass.set_vertex_buffer(0, self.tool_overlay_buffer.vertex_buffer.slice(..));
            tool_pass.draw(0..self.tool_overlay_buffer.vertex_count, 0..1);
            draw_calls += 1;
        }

        if let Some(hint_inst) = self.build_snap_hint_instance_2d() {
            use wgpu::util::DeviceExt;
            let hint_inst_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("snap-hint-inst-buf"),
                contents: bytemuck::cast_slice(&[hint_inst]),
                usage: wgpu::BufferUsages::VERTEX,
            });

            if let Some(mesh) = self.meshes.get(self.canonical_quad_idx) {
                let mut hint_pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("snap-hint-pass"),
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

        if !self.preview_playing {
            if let Some(origin) = self.selection_center().filter(|_| self.pivot_edit_mode.is_none())
            {
                let aspect = self.size.width as f32 / self.size.height as f32;
                let vp = if let Some(cam2d) = &self.camera_2d {
                    cam2d.view_proj(aspect).to_cols_array_2d()
                } else {
                    self.camera.to_uniform(aspect).view_proj
                };

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

pub(crate) fn build_scene_uniforms(camera: &Camera, size: PhysicalSize<u32>) -> SceneUniforms {
    let aspect = size.width as f32 / size.height as f32;
    let view_proj = camera.to_uniform(aspect).view_proj;
    let p = camera.position();
    SceneUniforms {
        view_proj,
        cam_pos: [p.x, p.y, p.z, 0.0],
    }
}

pub(crate) fn build_scene_uniforms_2d(cam: &Camera2D, size: PhysicalSize<u32>) -> SceneUniforms {
    let aspect = size.width as f32 / size.height as f32;
    let view_proj = cam.view_proj(aspect).to_cols_array_2d();
    let p = cam.position();
    SceneUniforms {
        view_proj,
        cam_pos: [p.x, p.y, p.z, 0.0],
    }
}

pub(crate) fn is_visible_2d(cam: &Camera2D, pos: GlamVec3, scale: GlamVec3, aspect: f32) -> bool {
    let half_w = cam.half_h * aspect;
    let margin = scale.x.abs().max(scale.y.abs()) * 0.5;
    let min_x = cam.x - half_w - margin;
    let max_x = cam.x + half_w + margin;
    let min_y = cam.y - cam.half_h - margin;
    let max_y = cam.y + cam.half_h + margin;

    let e_min_x = pos.x - scale.x.abs() * 0.5;
    let e_max_x = pos.x + scale.x.abs() * 0.5;
    let e_min_y = pos.y - scale.y.abs() * 0.5;
    let e_max_y = pos.y + scale.y.abs() * 0.5;

    e_max_x >= min_x && e_min_x <= max_x && e_max_y >= min_y && e_min_y <= max_y
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
