//! Preparación y codificación GPU de capturas de cubemap por probe.

use std::collections::HashMap;

use glam::Vec3;
use wgpu::util::DeviceExt;

use crate::config_3d::reflection_graphics::ReflectionSettings;
use crate::ecs::EntityId;
use crate::engine::{SceneUniforms, State};
use crate::reflections::probes::registry;
use crate::reflections::probe_env;

/// Datos de probes preparados al inicio del frame (antes del main pass).
pub struct ProbeFrameData {
    pub probe_list: Vec<(EntityId, Vec3, usize)>,
    pub probe_index_map: HashMap<EntityId, usize>,
}

pub fn prepare_probe_frame(state: &mut State, settings: &ReflectionSettings) -> ProbeFrameData {
    state.ensure_probe_slots_allocated();
    let mut probe_list = if settings.active() {
        state.reflection_probe_render_list()
    } else {
        Vec::new()
    };
    if settings.active() && probe_list.is_empty() {
        let min = state.world_bounds_3d.min_corner();
        let max = state.world_bounds_3d.max_corner();
        probe_list.push((
            registry::FALLBACK_SCENE_PROBE_ID,
            registry::fallback_probe_center_from_bounds(min, max),
            0,
        ));
    }
    if settings.active() {
        let probe_ids: Vec<_> = probe_list.iter().map(|(id, _, _)| *id).collect();
        state.sync_probe_capture_burst_for_entity_set(&probe_ids);
        let bounds_min = state.world_bounds_3d.min_corner();
        let bounds_max = state.world_bounds_3d.max_corner();
        let meta = registry::build_probe_meta(&probe_list, |id| {
            if registry::is_fallback_scene_probe(id) {
                registry::fallback_probe_radius_from_bounds(bounds_min, bounds_max)
            } else {
                state.reflection_probe_world_radius(id)
            }
        });
        state
            .probe_env
            .write_probe_meta(&state.queue, &meta);
    } else {
        state.probe_env.write_probe_meta(
            &state.queue,
            &crate::reflections::probe_env::ProbeMetaUniform::default(),
        );
    }
    let probe_index_map = registry::probe_index_map_from_list(&probe_list);
    ProbeFrameData {
        probe_list,
        probe_index_map,
    }
}

/// Índices en `probe_list` a capturar este frame (burst, todas si ≤5, o round-robin).
pub fn probe_capture_indices(
    state: &mut State,
    probe_list: &[(EntityId, Vec3, usize)],
) -> Vec<usize> {
    let probe_list_len = probe_list.len();
    if probe_list_len == 0 {
        return Vec::new();
    }
    if state.probe_capture_burst_all {
        state.probe_capture_burst_all = false;
        log::info!(
            "[reflexiones] captura burst de {probe_list_len} probes (cubemap)"
        );
        for &(id, _, slot) in probe_list {
            log::info!("[reflexiones] probe entidad {id} → ranura cubemap {slot}");
        }
        (0..probe_list_len).collect()
    } else if probe_list_len <= 5 {
        (0..probe_list_len).collect()
    } else {
        let idx = state.probe_capture_cursor % probe_list_len;
        state.probe_capture_cursor = state.probe_capture_cursor.wrapping_add(1);
        vec![idx]
    }
}

/// Codifica capturas 360° de los probes indicados (6 caras + mips por probe).
pub fn encode_probe_captures(
    state: &mut State,
    enc: &mut wgpu::CommandEncoder,
    settings: &ReflectionSettings,
    probe_frame: &ProbeFrameData,
    scene_uni: &SceneUniforms,
    skinned_probe: &[(usize, crate::mesh::SkinnedInstanceData)],
) {
    if !settings.active() || probe_frame.probe_list.is_empty() {
        return;
    }

    let probe_indices = probe_capture_indices(state, &probe_frame.probe_list);

    let probe_skinned_bufs: Vec<wgpu::Buffer> = skinned_probe
        .iter()
        .map(|(_, inst)| {
            state.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("probe-skinned-inst"),
                contents: bytemuck::cast_slice(std::slice::from_ref(inst)),
                usage: wgpu::BufferUsages::VERTEX,
            })
        })
        .collect();

    let probe_capture_static = state.collect_probe_capture_static_entities();
    let probe_index_map = &probe_frame.probe_index_map;

    for &list_idx in &probe_indices {
        let capture_batches = state.build_scene_instance_batches(
            &probe_capture_static,
            probe_index_map,
            true,
        );
        let capture_slices: Vec<_> = capture_batches
            .iter()
            .map(|b| b.instances.as_slice())
            .collect();
        let capture_instance_buffers = state.capture_instance_pool.upload(
            &state.device,
            &state.queue,
            &capture_slices,
        );

        let (_, center, cubemap_slot) = probe_frame.probe_list[list_idx];
        let face_vps = probe_env::cube_face_view_projs(center, 0.05, 200.0);

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
            state
                .probe_env
                .write_face_uniforms(&state.queue, f, bytemuck::bytes_of(&su));
        }

        for f in 0..6usize {
            let mut cap_pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("probe-env-capture-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: state.probe_env.face_view(cubemap_slot, f),
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(state.clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: state.probe_env.capture_depth_view(),
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

            cap_pass.set_pipeline(state.probe_env.capture_pipeline());
            cap_pass.set_bind_group(0, state.probe_env.face_scene_bind_group(f), &[]);
            cap_pass.set_bind_group(1, state.texture_array.bind_group.as_ref(), &[]);
            for (batch, inst_buf) in capture_batches
                .iter()
                .zip(capture_instance_buffers.iter())
            {
                let Some(mesh) = state.meshes.get(batch.mesh_idx) else {
                    continue;
                };
                cap_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                cap_pass.set_vertex_buffer(1, inst_buf.slice(..));
                cap_pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                cap_pass.draw_indexed(0..mesh.index_count, 0, 0..batch.instances.len() as u32);
            }

            if !skinned_probe.is_empty() {
                cap_pass.set_pipeline(state.probe_env.capture_skinned_pipeline());
                cap_pass.set_bind_group(0, state.probe_env.face_scene_bind_group(f), &[]);
                cap_pass.set_bind_group(1, state.texture_array.bind_group.as_ref(), &[]);
                cap_pass.set_bind_group(2, state.probe_env.capture_sample_bind_group(), &[]);
                for ((gpu_idx, _), inst_buf) in skinned_probe.iter().zip(probe_skinned_bufs.iter()) {
                    let Some(entry) = state.skinned_gpu_meshes.get(*gpu_idx) else {
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

        state
            .probe_env
            .generate_mips(&state.device, enc, cubemap_slot);
    }
}
