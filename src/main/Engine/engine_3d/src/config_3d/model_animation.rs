//! Binding entidad ↔ asset animado y reproducción de clips (GPU skinning).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use glam::{Mat4, Quat, Vec3};

use crate::config_3d::character_anchor::PLAY_CHARACTER_BODY_HEIGHT;
use crate::config_3d::model_asset::{
    self, compute_gltf_joint_worlds, AnimChannel, AnimKeyframe, AnimProperty, AnimationClip,
    GltfFile, ModelAsset, MAX_JOINTS,
};
use crate::ipc::ModelClipInfoEvent;
use crate::ecs::EntityId;
use crate::engine::State;
use crate::ipc::{send_event, EngineEvent};
use crate::mesh::{upload_skinned, SkinnedMesh};

#[derive(Clone)]
pub(crate) struct ModelAnimationBinding {
    pub asset_path: String,
    /// Índices en `skinned_gpu_meshes` (una entrada por pieza del asset).
    pub part_gpu_indices: Vec<usize>,
    pub uv_rect: [f32; 4],
}

#[derive(Clone)]
pub(crate) struct ActiveModelClip {
    pub clip_name: String,
    pub time_s: f32,
    pub loop_: bool,
    pub playing: bool,
    pub finished: bool,
}

pub(crate) struct GpuSkinnedMeshEntry {
    pub mesh: SkinnedMesh,
    pub joint_buffer: wgpu::Buffer,
    pub joint_bind_group: wgpu::BindGroup,
}

impl State {
    pub(crate) fn try_bind_model_animations(&mut self, id: EntityId, path: &str) {
        self.try_bind_model_animations_with_gltf(id, path, None);
    }

    pub(crate) fn try_bind_model_animations_with_gltf(
        &mut self,
        id: EntityId,
        path: &str,
        gltf_file: Option<&GltfFile>,
    ) {
        self.unbind_model_animations(id);
        // Clips embebidos 3D sustituyen animaciones 2D por hoja de sprites en esta entidad.
        self.animations.remove(&id);
        self.active_animations.remove(&id);
        self.default_animation_by_entity.remove(&id);

        let normalize = if self.play_character_entity == Some(id) {
            Some(PLAY_CHARACTER_BODY_HEIGHT)
        } else {
            None
        };

        let path_buf = Path::new(path);

        let asset = if let Some(cached) = self.model_assets.get(path) {
            Arc::clone(cached)
        } else if let Some(file) = gltf_file {
            match model_asset::load_model_asset_from_gltf(file, normalize) {
                Some(loaded) => {
                    self.model_assets
                        .insert(path.to_string(), Arc::clone(&loaded));
                    loaded
                }
                None => {
                    let clip_meta: Vec<ModelClipInfoEvent> = model_asset::list_gltf_clip_infos_from_file(
                        file,
                    )
                    .into_iter()
                    .map(|c| ModelClipInfoEvent {
                        name: c.name,
                        duration_s: c.duration_s,
                        fps: c.fps,
                    })
                    .collect();
                    if !clip_meta.is_empty() {
                        send_event(&EngineEvent::ModelClipsReady {
                            id,
                            path: path.to_string(),
                            clips: clip_meta,
                        });
                    }
                    log::warn!("[model_anim] glTF sin skinning utilizable: {path}");
                    return;
                }
            }
        } else {
            match model_asset::load_model_asset(path_buf, normalize) {
                Some(loaded) => {
                    self.model_assets
                        .insert(path.to_string(), Arc::clone(&loaded));
                    loaded
                }
                None => {
                    let clip_meta: Vec<ModelClipInfoEvent> = model_asset::list_model_clip_infos(
                        path_buf,
                    )
                    .into_iter()
                    .map(|c| ModelClipInfoEvent {
                        name: c.name,
                        duration_s: c.duration_s,
                        fps: c.fps,
                    })
                    .collect();
                    if clip_meta.is_empty() {
                        log::warn!("[model_anim] modelo sin clips embebidos: {path}");
                        return;
                    }
                    log::warn!(
                        "[model_anim] skinning no disponible; solo metadatos de {} clip(s): {path}",
                        clip_meta.len()
                    );
                    send_event(&EngineEvent::ModelClipsReady {
                        id,
                        path: path.to_string(),
                        clips: clip_meta,
                    });
                    return;
                }
            }
        };

        let clip_meta: Vec<ModelClipInfoEvent> = asset
            .clips
            .iter()
            .map(|c| ModelClipInfoEvent {
                name: c.name.clone(),
                duration_s: c.duration_s,
                fps: c.fps,
            })
            .collect();

        if clip_meta.is_empty() {
            log::warn!("[model_anim] modelo sin clips embebidos (solo bind pose): {path}");
        }

        let tex_idx = self
            .world
            .get::<crate::ecs::MeshComponent>(id)
            .map(|mc| mc.tex_idx)
            .unwrap_or(0);
        let uv_rect = self
            .uv_rects
            .get(tex_idx)
            .copied()
            .unwrap_or(self.fallback_uv);

        let joint_layout = self
            .joint_bind_group_layout
            .as_ref()
            .expect("joint_bind_group_layout");
        let mut part_gpu_indices = Vec::with_capacity(asset.parts.len());
        for (pi, part) in asset.parts.iter().enumerate() {
            let mesh_label = format!("skinned-{id}-p{pi}");
            let gpu_mesh = upload_skinned(
                &self.device,
                &part.mesh.vertices,
                &part.mesh.indices,
                &mesh_label,
            );
            let joint_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("joint-matrices"),
                size: (MAX_JOINTS * 64) as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let joint_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("joint-bind-group"),
                layout: joint_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: joint_buffer.as_entire_binding(),
                }],
            });
            let gpu_idx = self.skinned_gpu_meshes.len();
            self.skinned_gpu_meshes.push(GpuSkinnedMeshEntry {
                mesh: gpu_mesh,
                joint_buffer,
                joint_bind_group,
            });
            part_gpu_indices.push(gpu_idx);
        }

        // La textura ya está en el atlas vía MeshComponent / uv_rect; no reempacar aquí.

        let binding = ModelAnimationBinding {
            asset_path: path.to_string(),
            part_gpu_indices,
            uv_rect,
        };
        self.model_animation_bindings.insert(id, binding.clone());

        send_event(&EngineEvent::ModelClipsReady {
            id,
            path: path.to_string(),
            clips: clip_meta,
        });
        self.write_joint_matrices_all_parts(&binding, &asset, None, 0.0);

        log::info!(
            "[model_anim] clips enlazados para entidad {id}: {} clip(s), {} pieza(s) desde {path}",
            asset.clips.len(),
            asset.parts.len()
        );
    }

    pub(crate) fn unbind_model_animations(&mut self, id: EntityId) {
        self.model_animation_bindings.remove(&id);
        self.active_model_clips.remove(&id);
        self.model_clip_defaults.remove(&id);
    }

    pub(crate) fn play_model_clip(&mut self, id: EntityId, name: &str, loop_: bool) {
        let binding = match self.model_animation_bindings.get(&id) {
            Some(b) => b.clone(),
            None => return,
        };
        let asset = match self.model_assets.get(&binding.asset_path) {
            Some(a) => Arc::clone(a),
            None => return,
        };
        let _clip = match asset.clips.iter().find(|c| c.name == name) {
            Some(c) => c.clone(),
            None => {
                log::warn!("[model_anim] clip '{name}' no encontrado para entidad {id}");
                return;
            }
        };

        self.active_model_clips.insert(
            id,
            ActiveModelClip {
                clip_name: name.to_string(),
                time_s: 0.0,
                loop_: loop_,
                playing: true,
                finished: false,
            },
        );
        log::debug!("[model_anim] play '{name}' en entidad {id} (loop={loop_})");
    }

    pub(crate) fn stop_model_clip(&mut self, id: EntityId) {
        if let Some(active) = self.active_model_clips.get_mut(&id) {
            active.playing = false;
            active.time_s = 0.0;
            active.finished = true;
        }
        send_event(&EngineEvent::AnimationFinished { entity_id: id });
    }

    pub(crate) fn set_default_model_clip(&mut self, id: EntityId, name: &str) {
        if self.model_animation_bindings.contains_key(&id) {
            self.model_clip_defaults.insert(id, name.to_string());
        }
    }

    pub(crate) fn update_skinned_animations(&mut self) {
        let dt = self.delta_time;
        let mut finished_ids: Vec<EntityId> = Vec::new();

        let ids: Vec<EntityId> = self.model_animation_bindings.keys().copied().collect();
        for id in ids {
            let binding = match self.model_animation_bindings.get(&id) {
                Some(b) => b.clone(),
                None => continue,
            };
            let asset = match self.model_assets.get(&binding.asset_path) {
                Some(a) => Arc::clone(a),
                None => continue,
            };

            let (clip_name, time_s, loop_, _playing) = {
                let active = match self.active_model_clips.get(&id) {
                    Some(a) if a.playing && !a.finished => a,
                    _ => {
                        // Pose bind: tiempo 0, sin clip activo
                        self.write_joint_matrices_all_parts(&binding, &asset, None, 0.0);
                        continue;
                    }
                };
                (
                    active.clip_name.clone(),
                    active.time_s,
                    active.loop_,
                    active.playing,
                )
            };

            let clip = match asset.clips.iter().find(|c| c.name == clip_name) {
                Some(c) => c,
                None => continue,
            };

            let mut new_time = time_s + dt;
            if new_time >= clip.duration_s {
                if loop_ {
                    new_time %= clip.duration_s.max(1e-6);
                } else {
                    new_time = clip.duration_s;
                    finished_ids.push(id);
                }
            }

            if let Some(active) = self.active_model_clips.get_mut(&id) {
                active.time_s = new_time;
            }

            self.write_joint_matrices_all_parts(&binding, &asset, Some(clip), new_time);
        }

        for id in finished_ids {
            if let Some(active) = self.active_model_clips.get_mut(&id) {
                active.playing = false;
                active.finished = true;
            }
            send_event(&EngineEvent::AnimationFinished { entity_id: id });
        }
    }

    fn write_joint_matrices_all_parts(
        &mut self,
        binding: &ModelAnimationBinding,
        asset: &ModelAsset,
        clip: Option<&AnimationClip>,
        time_s: f32,
    ) {
        let joint_count = asset.joint_parents.len().min(MAX_JOINTS);
        let mut local_transforms = asset.bind_local[..joint_count].to_vec();

        if let Some(clip) = clip {
            apply_clip_to_locals(clip, time_s, &mut local_transforms);
        }

        let global = if !asset.joint_gltf_nodes.is_empty() {
            compute_gltf_joint_worlds(
                &asset.joint_gltf_nodes[..joint_count],
                &local_transforms,
                &asset.gltf_scene_parents,
                &asset.gltf_bind_node_local,
            )
        } else {
            compute_joint_globals(
                &asset.joint_parents[..joint_count],
                &local_transforms,
                &asset.joint_prefix_world[..joint_count],
            )
        };

        let norm = asset.mesh_normalize;
        let inv_norm = norm.inverse();

        for (pi, &gpu_idx) in binding.part_gpu_indices.iter().enumerate() {
            let Some(part) = asset.parts.get(pi) else {
                continue;
            };
            let inv_mesh = part.mesh_bind_world.inverse();
            let gltf_skin = !asset.joint_gltf_nodes.is_empty();
            let mut joint_palette = vec![Mat4::IDENTITY; MAX_JOINTS];
            for ji in 0..joint_count {
                let g2b = Mat4::from_cols_array_2d(&part.inverse_bind[ji]);
                joint_palette[ji] = if gltf_skin {
                    // Khronos / Godot: inv(meshGlobal) * jointGlobal * IBM; vértices en espacio de malla
                    norm * inv_mesh * global[ji] * g2b * inv_norm
                } else {
                    // FBX: vértices ya en mundo del nodo de malla
                    norm * global[ji] * g2b * inv_mesh * inv_norm
                };
            }
            let flat: Vec<[[f32; 4]; 4]> =
                joint_palette.iter().map(|m| m.to_cols_array_2d()).collect();
            if let Some(entry) = self.skinned_gpu_meshes.get(gpu_idx) {
                self.queue.write_buffer(
                    &entry.joint_buffer,
                    0,
                    bytemuck::cast_slice(&flat),
                );
            }
        }
    }
}

/// Resuelve globals aunque los padres estén después en el array (ancestros añadidos al final).
fn compute_joint_globals(
    joint_parents: &[Option<usize>],
    locals: &[Mat4],
    prefix_world: &[Mat4],
) -> Vec<Mat4> {
    let n = locals.len().min(joint_parents.len()).min(prefix_world.len());
    let mut global = vec![Mat4::IDENTITY; n];
    let mut done = vec![false; n];
    let mut remaining = n;
    while remaining > 0 {
        let mut progressed = false;
        for ji in 0..n {
            if done[ji] {
                continue;
            }
            match joint_parents[ji] {
                None => {
                    global[ji] = prefix_world[ji] * locals[ji];
                    done[ji] = true;
                    progressed = true;
                    remaining -= 1;
                }
                Some(p) if p < n && done[p] => {
                    global[ji] = global[p] * locals[ji];
                    done[ji] = true;
                    progressed = true;
                    remaining -= 1;
                }
                _ => {}
            }
        }
        if !progressed {
            for ji in 0..n {
                if !done[ji] {
                    global[ji] = prefix_world[ji] * locals[ji];
                    done[ji] = true;
                }
            }
            break;
        }
    }
    global
}

fn apply_clip_to_locals(clip: &AnimationClip, time_s: f32, locals: &mut [Mat4]) {
    let channels_by_joint: HashMap<usize, Vec<&AnimChannel>> = clip
        .channels
        .iter()
        .fold(HashMap::new(), |mut m, ch| {
            m.entry(ch.joint_index).or_default().push(ch);
            m
        });

    for (ji, channels) in channels_by_joint {
        if ji >= locals.len() {
            continue;
        }
        let mut translation = Vec3::ZERO;
        let mut rotation = Quat::IDENTITY;
        let mut scale = Vec3::ONE;
        let mut has_t = false;
        let mut has_r = false;
        let mut has_s = false;

        for ch in channels {
            match ch.property {
                AnimProperty::Translation => {
                    if let Some(v) = sample_translation(&ch.keyframes, time_s) {
                        translation = v;
                        has_t = true;
                    }
                }
                AnimProperty::Rotation => {
                    if let Some(q) = sample_rotation(&ch.keyframes, time_s) {
                        rotation = q;
                        has_r = true;
                    }
                }
                AnimProperty::Scale => {
                    if let Some(v) = sample_scale(&ch.keyframes, time_s) {
                        scale = v;
                        has_s = true;
                    }
                }
            }
        }

        let (base_s, base_r, base_t) = locals[ji].to_scale_rotation_translation();
        let t = if has_t { translation } else { base_t };
        let r = if has_r { rotation } else { base_r };
        let s = if has_s { scale } else { base_s };
        locals[ji] = Mat4::from_scale_rotation_translation(s, r, t);
    }
}

fn sample_translation(keys: &[AnimKeyframe], t: f32) -> Option<Vec3> {
    let (a, b, alpha) = find_keyframe_pair(keys, t)?;
    let va = Vec3::from(a.translation?);
    let vb = Vec3::from(b.translation?);
    Some(va.lerp(vb, alpha))
}

fn sample_scale(keys: &[AnimKeyframe], t: f32) -> Option<Vec3> {
    let (a, b, alpha) = find_keyframe_pair(keys, t)?;
    let va = Vec3::from(a.scale?);
    let vb = Vec3::from(b.scale?);
    Some(va.lerp(vb, alpha))
}

fn sample_rotation(keys: &[AnimKeyframe], t: f32) -> Option<Quat> {
    let (a, b, alpha) = find_keyframe_pair(keys, t)?;
    let qa = Quat::from_array(a.rotation?);
    let qb = Quat::from_array(b.rotation?);
    Some(qa.slerp(qb, alpha))
}

fn find_keyframe_pair<'a>(
    keys: &'a [AnimKeyframe],
    t: f32,
) -> Option<(&'a AnimKeyframe, &'a AnimKeyframe, f32)> {
    if keys.is_empty() {
        return None;
    }
    if keys.len() == 1 {
        return Some((&keys[0], &keys[0], 0.0));
    }
    if t <= keys[0].time {
        return Some((&keys[0], &keys[0], 0.0));
    }
    for w in keys.windows(2) {
        if t >= w[0].time && t <= w[1].time {
            let span = (w[1].time - w[0].time).max(1e-6);
            let alpha = (t - w[0].time) / span;
            return Some((&w[0], &w[1], alpha));
        }
    }
    let last = keys.len() - 1;
    Some((&keys[last], &keys[last], 0.0))
}
