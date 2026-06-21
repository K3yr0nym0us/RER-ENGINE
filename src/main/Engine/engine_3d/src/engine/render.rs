use glam::Vec3 as GlamVec3;
use winit::dpi::PhysicalSize;

use rer_engine_shared::wgpu_surface::{acquire_surface_texture, SurfacePresentError};

use crate::config_3d::Camera;
use crate::gizmo;

use glam::Mat4;

use super::{SceneUniforms, State, DEPTH_FORMAT};

struct SceneInstanceBatch {
    mesh_idx: usize,
    texture_layer: u32,
    instances: Vec<crate::mesh::InstanceData>,
}

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
        if self.socket_bone_pick_entity == Some(entity_id) {
            return 0.0;
        }
        if self.bone_physics_pick_entity == Some(entity_id) {
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

    /// Instancias estáticas para escena o captura de probe. Con `for_probe_capture`, omite
    /// todas las sondas (cubemap = estático + jugador; vecinas vía SSR on-screen).
    fn build_scene_instance_batches(
        &self,
        entities: &[(crate::ecs::EntityId, usize, usize, Mat4, i32)],
        probe_index_map: &std::collections::HashMap<crate::ecs::EntityId, usize>,
        for_probe_capture: bool,
    ) -> Vec<SceneInstanceBatch> {
        let mut batches: Vec<SceneInstanceBatch> = Vec::new();
        for (entity_id, mesh_idx, tex_idx, model_matrix, _layer) in entities {
            if for_probe_capture && probe_index_map.contains_key(entity_id) {
                continue;
            }
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
            if let Some(pbr) = self.world.get::<crate::ecs::SurfacePbr>(*entity_id) {
                inst.flag_pad[3] = pbr.roughness;
                inst.tex_layer_pad[1] = pbr.metallic;
                inst.tex_layer_pad[3] = pbr.ior;
            }
            if let Some(&probe_idx) = probe_index_map.get(entity_id) {
                inst.tex_layer_pad[2] = probe_idx as f32;
            }
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
                batches.push(SceneInstanceBatch {
                    mesh_idx: *mesh_idx,
                    texture_layer: layer,
                    instances: vec![inst],
                });
            }
        }
        batches
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
            crate::taa::MRT_LIT_FORMAT,
            self.config.format,
            new_size.width,
            new_size.height,
        );
        self.reflections.resize(
            &self.device,
            new_size.width,
            new_size.height,
        );
        self.reflections.invalidate_temporal();
        if self.player_ui_edit_active {
            self.rebuild_player_ui_screen_grid();
        }
    }

    fn prepare_lit_scene(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        shadows_enabled: bool,
        shadow_darkness: f32,
        zoom_stability: f32,
    ) {
        if shadows_enabled && self.taa.enabled {
            self.taa.resolve_shadow_mask_pub(
                &self.device,
                &self.queue,
                enc,
                zoom_stability,
                self.size.width,
                self.size.height,
            );
        }
        self.taa.run_lit_composite_pub(
            &self.device,
            &self.queue,
            enc,
            if shadows_enabled {
                shadow_darkness
            } else {
                0.35
            },
            shadows_enabled,
        );
    }

    fn present_scene_with_reflections(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        swapchain_view: &wgpu::TextureView,
        _shadows_enabled: bool,
        _shadow_darkness: f32,
        zoom_stability: f32,
        inv_view_proj: [[f32; 4]; 4],
        prev_view_proj: [[f32; 4]; 4],
        ssil_strength: f32,
    ) {
        self.reflections.composite_into(
            &self.device,
            &self.queue,
            enc,
            self.taa.scene_color_view(),
            self.taa.scene_color_texture(),
            1.0,
            ssil_strength,
        );

        let use_scene_taa = self.taa.scene_taa_active();
        if use_scene_taa {
            self.taa.resolve_scene_soft_pub(
                &self.device,
                &self.queue,
                enc,
                zoom_stability,
                self.size.width,
                self.size.height,
                inv_view_proj,
                prev_view_proj,
            );
        }
        self.taa
            .blit_present_pub(enc, swapchain_view, use_scene_taa);
        self.taa.tick_frame_index();
    }

    pub fn render(&mut self) -> Result<(), SurfacePresentError> {
        self.update_animations();
        self.update_skinned_animations();
        self.sync_socket_attached_children();
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

        let output = acquire_surface_texture(&self.surface)?;
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

        let reflection_settings = crate::config_3d::reflection_graphics::ReflectionSettings::from_tier(
            self.reflection_tier,
            self.rt_reflections_available,
        );

        // Reflection probes: lista ordenada (id, centro) y mapa id→índice de capa del cubemap.
        // Solo activo con reflejos encendidos; si no, las instancias quedan con probe_index = -1.
        let probe_list = if reflection_settings.active() {
            self.reflection_probe_render_list()
        } else {
            Vec::new()
        };
        let probe_index_map: std::collections::HashMap<crate::ecs::EntityId, usize> = probe_list
            .iter()
            .enumerate()
            .map(|(i, (id, _))| (*id, i))
            .collect();
        if reflection_settings.active() {
            let mut probe_meta = crate::reflections::probe_env::ProbeMetaUniform::default();
            for (i, &(id, center)) in probe_list.iter().enumerate() {
                let radius = self.reflection_probe_world_radius(id);
                probe_meta.entries[i] = [center.x, center.y, center.z, radius];
            }
            self.probe_env.write_probe_meta(&self.queue, &probe_meta);
        }

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
                let (mesh_center, mesh_half) = if is_sun || is_ground {
                    (t.position, t.scale.abs() * 0.5)
                } else {
                    self.entity_world_pick_aabb(id, t)
                };
                if !is_sun
                    && !is_ground
                    && !self
                        .world_bounds_3d
                        .intersects_world_aabb(mesh_center, mesh_half)
                {
                    return None;
                }
                let visible = if is_sun || is_ground {
                    true
                } else {
                    is_aabb_visible_3d(&frustum_vp, mesh_center, mesh_half)
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
            instances: Vec<crate::mesh::InstanceData>,
        }
        let batches = self.build_scene_instance_batches(
            &entities,
            &probe_index_map,
            false,
        );

        let ghost_overlay = self.build_tool_ghost_overlay();

        let skinned_shadow = self.collect_skinned_draw_instances(&frustum_vp, &probe_index_map);
        let skinned_main = skinned_shadow.clone();
        let skinned_probe = self.collect_skinned_probe_instances(&frustum_vp);

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
                    instances: vec![inst],
                });
            }
        }

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
                multiview_mask: None,
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
                    shadow_pass.set_bind_group(3, &entry.joint_bind_group, &[]);
                    shadow_pass.set_vertex_buffer(0, entry.mesh.vertex_buffer.slice(..));
                    shadow_pass.set_vertex_buffer(1, inst_buf.slice(..));
                    shadow_pass.set_index_buffer(entry.mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    shadow_pass.draw_indexed(0..entry.mesh.index_count, 0, 0..1);
                }
            }
        }

        // ── Captura de reflection probes (un cubemap por esfera) ─────────────────────────
        // Captura el entorno 360° (suelo, vecinas y jugador) alrededor del centro del probe.
        // Round-robin: un probe por frame; burst tras activar reflejos o recrear cubemap.
        if reflection_settings.active() && !probe_list.is_empty() {
            use wgpu::util::DeviceExt;

            let probe_indices: Vec<usize> = if self.probe_capture_burst_all {
                self.probe_capture_burst_all = false;
                log::info!(
                    "[reflexiones] captura burst de {} probes (cubemap)",
                    probe_list.len()
                );
                (0..probe_list.len()).collect()
            } else {
                let idx = self.probe_capture_cursor % probe_list.len();
                self.probe_capture_cursor = self.probe_capture_cursor.wrapping_add(1);
                vec![idx]
            };

            let probe_skinned_bufs: Vec<wgpu::Buffer> = skinned_probe
                .iter()
                .map(|(_, inst)| {
                    self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("probe-skinned-inst"),
                        contents: bytemuck::cast_slice(std::slice::from_ref(inst)),
                        usage: wgpu::BufferUsages::VERTEX,
                    })
                })
                .collect();

            for &probe_idx in &probe_indices {
                let capture_batches = self.build_scene_instance_batches(
                    &entities,
                    &probe_index_map,
                    true,
                );
                let capture_slices: Vec<_> = capture_batches
                    .iter()
                    .map(|b| b.instances.as_slice())
                    .collect();
                let capture_instance_buffers = self.shadow_instance_pool.upload(
                    &self.device,
                    &self.queue,
                    &capture_slices,
                );

                let center = probe_list[probe_idx].1;
                let face_vps =
                    crate::reflections::probe_env::cube_face_view_projs(center, 0.05, 200.0);

                for f in 0..6usize {
                    let su = SceneUniforms {
                        view_proj: face_vps[f],
                        view_proj_stable: face_vps[f],
                        prev_view_proj: face_vps[f],
                        inv_view_proj: face_vps[f],
                        cam_pos: [center.x, center.y, center.z, 0.0],
                        light_dir: scene_uni.light_dir,
                        light_color: scene_uni.light_color,
                        light_view_proj: scene_uni.light_view_proj,
                        light_params: scene_uni.light_params,
                        jitter: [0.0; 4],
                        depth_plane: [0.05, 200.0, 0.0, 0.0],
                        shadow_bias: scene_uni.shadow_bias,
                    };
                    self.probe_env
                        .write_face_uniforms(&self.queue, f, bytemuck::bytes_of(&su));
                }

                for f in 0..6usize {
                    let mut cap_pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("probe-env-capture-pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: self.probe_env.face_view(probe_idx, f),
                            resolve_target: None,
                            depth_slice: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(self.clear_color),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: self.probe_env.capture_depth_view(),
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Clear(1.0),
                                store: wgpu::StoreOp::Store,
                            }),
                            stencil_ops: None,
                        }),
                        occlusion_query_set: None,
                        timestamp_writes: None,
                        multiview_mask: None,
                    });

                    cap_pass.set_pipeline(self.probe_env.capture_pipeline());
                    cap_pass.set_bind_group(0, self.probe_env.face_scene_bind_group(f), &[]);
                    cap_pass.set_bind_group(1, self.texture_array.bind_group.as_ref(), &[]);
                    for (batch, inst_buf) in capture_batches
                        .iter()
                        .zip(capture_instance_buffers.iter())
                    {
                        let Some(mesh) = self.meshes.get(batch.mesh_idx) else {
                            continue;
                        };
                        cap_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                        cap_pass.set_vertex_buffer(1, inst_buf.slice(..));
                        cap_pass
                            .set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                        cap_pass.draw_indexed(0..mesh.index_count, 0, 0..batch.instances.len() as u32);
                    }

                    if !skinned_probe.is_empty() {
                        cap_pass.set_pipeline(self.probe_env.capture_skinned_pipeline());
                        cap_pass.set_bind_group(0, self.probe_env.face_scene_bind_group(f), &[]);
                        cap_pass.set_bind_group(1, self.texture_array.bind_group.as_ref(), &[]);
                        cap_pass.set_bind_group(2, self.probe_env.sample_bind_group(), &[]);
                        for ((gpu_idx, _), inst_buf) in
                            skinned_probe.iter().zip(probe_skinned_bufs.iter())
                        {
                            let Some(entry) = self.skinned_gpu_meshes.get(*gpu_idx) else {
                                continue;
                            };
                            cap_pass.set_bind_group(3, &entry.joint_bind_group, &[]);
                            cap_pass.set_vertex_buffer(0, entry.mesh.vertex_buffer.slice(..));
                            cap_pass.set_vertex_buffer(1, inst_buf.slice(..));
                            cap_pass.set_index_buffer(
                                entry.mesh.index_buffer.slice(..),
                                wgpu::IndexFormat::Uint32,
                            );
                            cap_pass.draw_indexed(0..entry.mesh.index_count, 0, 0..1);
                        }
                    }
                }

                self.probe_env.generate_mips(&self.device, &mut enc, probe_idx);
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
            let ambient_view = self.taa.ambient_view();
            let direct_view = self.taa.direct_view();
            let depth_export_view = self.taa.depth_export_view();
            let velocity_view = self.taa.velocity_view();
            let base_color_view = self.taa.base_color_view();
            let shadow_mask_view = self.taa.shadow_mask_view();

            {
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("render-pass"),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment {
                        view: ambient_view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(self.clear_color),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: shadow_mask_view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            // Rg16Float: .r=shadow (1=sin sombra), .g=roughness (1=máximo
                            // mate → sin reflejos). Píxeles sin geometría quedan fuera de
                            // cualquier traza SSR/RT. (Metallic del cielo es 0 vía direct.a).
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 1.0,
                                g: 1.0,
                                b: 0.0,
                                a: 0.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: direct_view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: depth_export_view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            // R32Float: distancia en metros (0 = cielo/sin geometría).
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: velocity_view,
                        resolve_target: None,
                        depth_slice: None,
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
                multiview_mask: None,
            });

            pass.set_pipeline(&self.render_pipeline);

            pass.set_bind_group(0, &self.scene_bind_group, &[]);
            pass.set_bind_group(1, self.texture_array.bind_group.as_ref(), &[]);
            // Grupo 2: cube array de los probes (el shader principal lo referencia siempre).
            pass.set_bind_group(2, self.probe_env.sample_bind_group(), &[]);

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
                pass.set_bind_group(2, self.probe_env.sample_bind_group(), &[]);
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
                    pass.set_bind_group(3, &entry.joint_bind_group, &[]);
                    pass.set_vertex_buffer(0, entry.mesh.vertex_buffer.slice(..));
                    pass.set_vertex_buffer(1, inst_buf.slice(..));
                    pass.set_index_buffer(entry.mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..entry.mesh.index_count, 0, 0..1);
                    draw_calls += 1;
                }
            }
            }

            // Albedo para SSR/RT: pase aparte (1 MRT) — el principal ya usa 32 B/muestra (límite wgpu).
            if reflection_settings.active() {
                let mut albedo_pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("base-color-export-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: base_color_view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
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
                    multiview_mask: None,
                });
                albedo_pass.set_pipeline(&self.base_color_pipeline);
                albedo_pass.set_bind_group(0, &self.scene_bind_group, &[]);
                albedo_pass.set_bind_group(1, self.texture_array.bind_group.as_ref(), &[]);
                albedo_pass.set_bind_group(2, self.probe_env.sample_bind_group(), &[]);
                for (batch, inst_buf) in batches.iter().zip(instance_buffers.iter()) {
                    let Some(mesh) = self.meshes.get(batch.mesh_idx) else {
                        continue;
                    };
                    albedo_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                    albedo_pass.set_vertex_buffer(1, inst_buf.slice(..));
                    albedo_pass.set_index_buffer(
                        mesh.index_buffer.slice(..),
                        wgpu::IndexFormat::Uint32,
                    );
                    albedo_pass.draw_indexed(0..mesh.index_count, 0, 0..batch.instances.len() as u32);
                }
                if !skinned_main.is_empty() {
                    albedo_pass.set_pipeline(&self.skinned_base_color_pipeline);
                    albedo_pass.set_bind_group(0, &self.scene_bind_group, &[]);
                    albedo_pass.set_bind_group(1, self.texture_array.bind_group.as_ref(), &[]);
                    albedo_pass.set_bind_group(2, self.probe_env.sample_bind_group(), &[]);
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
                        albedo_pass.set_bind_group(3, &entry.joint_bind_group, &[]);
                        albedo_pass.set_vertex_buffer(0, entry.mesh.vertex_buffer.slice(..));
                        albedo_pass.set_vertex_buffer(1, inst_buf.slice(..));
                        albedo_pass.set_index_buffer(
                            entry.mesh.index_buffer.slice(..),
                            wgpu::IndexFormat::Uint32,
                        );
                        albedo_pass.draw_indexed(0..entry.mesh.index_count, 0, 0..1);
                    }
                }
            }
        }

        let inv_vp = scene_uni.inv_view_proj;
        let prev_vp = scene_uni.prev_view_proj;

        let cam_pos = self.camera_world_position();
        // El depth exportado usa la matriz con jitter; reconstrucción (inv_view_proj) y
        // marching deben usar ESA misma matriz para que el test de profundidad coincida.
        let view_proj = glam::Mat4::from_cols_array_2d(&scene_uni.view_proj);
        let inv_view_proj = glam::Mat4::from_cols_array_2d(&inv_vp);
        let rt_sync = reflection_settings.active() && reflection_settings.rt_enabled;

        if rt_sync {
            let include_skinned = !reflection_settings.rt_static_only;
            let instances =
                crate::reflections::tlas::collect_rt_instances(self, include_skinned);
            let rt_materials = crate::reflections::rt_material::build_rt_materials(
                self,
                &instances,
                &probe_index_map,
            );
            let meshes = &self.meshes;
            let skinned_meshes = &self.skinned_gpu_meshes;
            self.rt_accel.sync_scene(
                &rt_materials,
                &instances,
                meshes,
                skinned_meshes,
                &self.device,
                &self.queue,
                &mut enc,
            );
        }

        if reflection_settings.active() {
            self.prepare_lit_scene(
                &mut enc,
                shadows_enabled,
                if shadows_enabled { shadow_darkness } else { 0.35 },
                zoom_stability,
            );
        }

        let lit_scene_view = self.taa.scene_color_view();
        let direct_view = self.taa.direct_view();
        let depth_export_view = self.taa.depth_export_view();
        let velocity_view = self.taa.velocity_view();
        let base_color_view = self.taa.base_color_view();
        let normal_roughness_view = self.taa.normal_roughness_view();
        let shadow_mask_view = self.taa.shadow_mask_view();
        let view_mat = self.camera_view_matrix();
        let shadow_map_view = self._shadow_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("shadow-map-rt"),
            ..Default::default()
        });

        let ran_reflections = if reflection_settings.active() {
            self.reflections.run(
                &self.device,
                &self.queue,
                &mut enc,
                reflection_settings,
                self.reflection_debug_view,
                &depth_export_view,
                normal_roughness_view,
                lit_scene_view,
                direct_view,
                shadow_mask_view,
                base_color_view,
                depth_export_view,
                velocity_view,
                &mut self.rt_accel,
                inv_view_proj,
                view_proj,
                view_mat,
                cam_pos,
                self.camera.near,
                self.camera.far,
                self.clear_color,
                self.rt_reflections_available,
                self.probe_env.sample_bind_group(),
                &shadow_map_view,
                &self.shadow_sampler,
                &scene_uni,
            )
        } else {
            false
        };

        let debug_reflections = ran_reflections
            && self.reflection_debug_view
                != crate::config_3d::reflection_graphics::ReflectionDebugView::Final;

        if debug_reflections {
            self.reflections.run_debug_blit(
                &mut enc,
                &view,
                self.reflection_debug_view,
            );
            self.taa.tick_frame_index();
        } else if ran_reflections {
            let ssil_strength = if crate::reflections::rt_extensions::rt_diffuse_gi_enabled(
                &reflection_settings,
            ) {
                0.65
            } else {
                0.0
            };
            self.present_scene_with_reflections(
                &mut enc,
                &view,
                shadows_enabled,
                shadow_darkness,
                zoom_stability,
                inv_vp,
                prev_vp,
                ssil_strength,
            );
        } else if shadows_enabled {
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
                        depth_slice: None,
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
                    multiview_mask: None,
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
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                    multiview_mask: None,
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
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
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
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                    multiview_mask: None,
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
                    depth_slice: None,
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
                multiview_mask: None,
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
        {
            let skeleton_overlay =
                crate::config_3d::skeleton_debug::build_selected_skeleton_overlay(
                    &self.device,
                    self,
                );
            if skeleton_overlay.vertex_count > 0 {
                let aspect = self.size.width as f32 / self.size.height as f32;
                let vp = self
                    .camera_to_uniform_at_anchor(self.orbit_view_anchor(), aspect)
                    .view_proj;
                let skel_uni: [[f32; 4]; 9] = [
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
                    bytemuck::cast_slice(&skel_uni),
                );
                let mut skel_pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("skeleton-debug-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                    multiview_mask: None,
                });
                skel_pass.set_pipeline(&self.grid_pipeline);
                skel_pass.set_bind_group(0, &self.grid_bind_group, &[]);
                skel_pass.set_vertex_buffer(0, skeleton_overlay.vertex_buffer.slice(..));
                skel_pass.draw(0..skeleton_overlay.vertex_count, 0..1);
                draw_calls += 1;
            }

            let socket_overlay =
                crate::config_3d::socket_debug::build_selected_socket_overlay(
                    &self.device,
                    self,
                );
            if socket_overlay.vertex_count > 0 {
                let aspect = self.size.width as f32 / self.size.height as f32;
                let vp = self
                    .camera_to_uniform_at_anchor(self.orbit_view_anchor(), aspect)
                    .view_proj;
                let skel_uni: [[f32; 4]; 9] = [
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
                    bytemuck::cast_slice(&skel_uni),
                );
                let mut sock_pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("socket-debug-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                    multiview_mask: None,
                });
                sock_pass.set_pipeline(&self.grid_pipeline);
                sock_pass.set_bind_group(0, &self.grid_bind_group, &[]);
                sock_pass.set_vertex_buffer(0, socket_overlay.vertex_buffer.slice(..));
                sock_pass.draw(0..socket_overlay.vertex_count, 0..1);
                draw_calls += 1;
            }
        }

        if !self.preview_playing
            && !self.player_ui_edit_active
            && self.socket_bone_pick_entity.is_none()
            && self.bone_physics_pick_entity.is_none()
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
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                    multiview_mask: None,
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

    fn collect_skinned_draw_instances(
        &self,
        frustum_vp: &Mat4,
        probe_index_map: &std::collections::HashMap<crate::ecs::EntityId, usize>,
    ) -> Vec<(usize, crate::mesh::SkinnedInstanceData)> {
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
            let (mesh_center, mesh_half) = if is_ground {
                (t.position, t.scale.abs() * 0.5)
            } else {
                self.entity_world_pick_aabb(id, t)
            };
            if !is_ground
                && !self
                    .world_bounds_3d
                    .intersects_world_aabb(mesh_center, mesh_half)
            {
                continue;
            }
            if !is_ground && !is_aabb_visible_3d(frustum_vp, mesh_center, mesh_half) {
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
                let mut inst = crate::mesh::InstanceData::new(t.to_matrix(), flag, layer);
                if let Some(pbr) = self.world.get::<crate::ecs::SurfacePbr>(id) {
                    inst.flag_pad[3] = pbr.roughness;
                    inst.tex_layer_pad[1] = pbr.metallic;
                    inst.tex_layer_pad[3] = pbr.ior;
                }
                if let Some(&probe_idx) = probe_index_map.get(&id) {
                    inst.tex_layer_pad[2] = probe_idx as f32;
                }
                out.push((gpu_idx, crate::mesh::SkinnedInstanceData::from_instance(&inst)));
            }
        }
        out
    }

    /// Skinned para captura de cubemap: el jugador local siempre entra (sin cull por
    /// frustum de la cámara FPS). La cámara de juego suele estar dentro del AABB del
    /// cuerpo y el probe se captura desde el centro de cada esfera, no desde el ojo.
    fn collect_skinned_probe_instances(
        &self,
        frustum_vp: &Mat4,
    ) -> Vec<(usize, crate::mesh::SkinnedInstanceData)> {
        let mut out = Vec::new();
        for (&id, binding) in &self.model_animation_bindings {
            let Some(t) = self.world.get::<crate::ecs::Transform>(id) else {
                continue;
            };
            if self.sun_entity == Some(id) {
                continue;
            }
            let is_player = self.play_character_entity == Some(id);
            let is_ground = self
                .world
                .get::<crate::ecs::NameComponent>(id)
                .is_some_and(|n| n.name.eq_ignore_ascii_case("ground"));
            let (mesh_center, mesh_half) = if is_ground {
                (t.position, t.scale.abs() * 0.5)
            } else {
                self.entity_world_pick_aabb(id, t)
            };
            if !is_ground
                && !is_player
                && !self
                    .world_bounds_3d
                    .intersects_world_aabb(mesh_center, mesh_half)
            {
                continue;
            }
            if !is_ground
                && !is_player
                && !is_aabb_visible_3d(frustum_vp, mesh_center, mesh_half)
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
        depth_plane: [camera.near, camera.far, 0.0, 0.0],
        shadow_bias,
    }
}

pub(crate) fn is_aabb_visible_3d(
    view_proj: &glam::Mat4,
    center: GlamVec3,
    half: GlamVec3,
) -> bool {
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
        let extent = half.x * plane[0].abs()
            + half.y * plane[1].abs()
            + half.z * plane[2].abs();
        let dist =
            plane[0] * center.x + plane[1] * center.y + plane[2] * center.z + plane[3];
        if dist < -extent {
            return false;
        }
    }
    true
}
