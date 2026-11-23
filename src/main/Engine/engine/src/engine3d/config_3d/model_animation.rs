//! Binding entidad ↔ asset animado y reproducción de clips (GPU skinning).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use glam::{Mat4, Quat, Vec3};

use crate::engine3d::config_3d::character_anchor::PLAY_CHARACTER_BODY_HEIGHT;
use crate::engine3d::config_3d::model_asset::{
    self, AnimChannel, AnimKeyframe, AnimProperty, AnimationClip, GltfFile, MAX_JOINTS, ModelAsset,
    compute_gltf_joint_worlds, gltf_bind_globals_from_ibm,
};
use crate::engine3d::ecs::{EntityId, Transform};
use crate::engine3d::engine::State;
use crate::engine3d::ipc::ModelClipInfoEvent;
use crate::engine3d::ipc::{EngineEvent, send_event};
use crate::engine3d::mesh::{SkinnedMesh, upload_skinned};

#[derive(Clone)]
pub(crate) struct ModelAnimationBinding {
    pub asset_path: String,
    /// Índices en `skinned_gpu_meshes` (una entrada por pieza del asset).
    pub part_gpu_indices: Vec<usize>,
    /// Capa de textura por pieza (alineada con `part_gpu_indices` / `asset.parts`).
    pub part_tex_layers: Vec<crate::engine3d::texture::TextureLayer>,
    /// Primera pieza; compat con `MeshComponent.tex_idx`.
    pub tex_layer: crate::engine3d::texture::TextureLayer,
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
    /// Palette de matrices de joint (actualizada por `FrameRenderer` desde `FrameInput`).
    pub joint_palette: Vec<[[f32; 4]; 4]>,
}

impl State {
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

        let cache_key = self.model_cache_key(path);
        let asset_key = crate::engine3d::config_3d::static_model_cache::model_asset_cache_key(
            &cache_key, normalize,
        );

        let cached_asset = self
            .model_assets
            .get(&asset_key)
            .or_else(|| {
                if asset_key != cache_key {
                    self.model_assets.get(&cache_key)
                } else {
                    None
                }
            })
            .map(Arc::clone);
        let asset = if let Some(cached) = cached_asset {
            cached
        } else if let Some(file) = gltf_file {
            match model_asset::load_model_asset_from_gltf(file, normalize) {
                Some(loaded) => {
                    self.model_assets
                        .insert(asset_key.clone(), Arc::clone(&loaded));
                    loaded
                }
                None => {
                    let clip_meta: Vec<ModelClipInfoEvent> =
                        model_asset::list_gltf_clip_infos_from_file(file)
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
                            path: cache_key.clone(),
                            clips: clip_meta,
                        });
                    }
                    log::warn!("[model_anim] glTF sin skinning utilizable: {asset_key}");
                    return;
                }
            }
        } else {
            let load_path = self.model_library_path_for(path);

            match model_asset::load_model_asset(Path::new(&load_path), normalize) {
                Some(loaded) => {
                    self.model_assets
                        .insert(asset_key.clone(), Arc::clone(&loaded));
                    loaded
                }
                None => {
                    let warm_play = normalize.is_some();
                    self.ensure_model_anim_assets_from_rerasset(&cache_key, warm_play);
                    let resolved = self
                        .model_assets
                        .get(&asset_key)
                        .or_else(|| {
                            if asset_key != cache_key {
                                self.model_assets.get(&cache_key)
                            } else {
                                None
                            }
                        })
                        .map(Arc::clone);
                    match resolved {
                        Some(loaded) => {
                            if !self.model_assets.contains_key(&asset_key) {
                                self.model_assets
                                    .insert(asset_key.clone(), Arc::clone(&loaded));
                            }
                            loaded
                        }
                        None => {
                            log::warn!("[model_anim] sin model_assets para {asset_key}");
                            return;
                        }
                    }
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

        let fallback_layer = self.fallback_layer;

        let mut part_gpu_indices = Vec::with_capacity(asset.parts.len());
        for (pi, part) in asset.parts.iter().enumerate() {
            let mesh_label = format!("skinned-{id}-p{pi}");
            let gpu_mesh = upload_skinned(
                &self.vulkan,
                &part.mesh.vertices,
                &part.mesh.indices,
                &mesh_label,
            );
            let gpu_idx = self.skinned_gpu_meshes.len();
            self.skinned_gpu_meshes.push(GpuSkinnedMeshEntry {
                mesh: gpu_mesh,
                joint_palette: vec![Mat4::IDENTITY.to_cols_array_2d(); MAX_JOINTS],
            });
            part_gpu_indices.push(gpu_idx);
        }

        let binding = ModelAnimationBinding {
            asset_path: path.to_string(),
            part_gpu_indices,
            part_tex_layers: vec![fallback_layer; asset.parts.len()],
            tex_layer: fallback_layer,
        };
        self.model_animation_bindings.insert(id, binding.clone());
        crate::engine3d::config_3d::entity_textures::bootstrap_entity_model_textures(
            self, id, path,
        );

        send_event(&EngineEvent::ModelClipsReady {
            id,
            path: path.to_string(),
            clips: clip_meta,
        });
        self.write_joint_matrices_all_parts(id, &binding.part_gpu_indices, &asset, None, 0.0);
    }

    /// Re-emite metadatos de clips ya enlazados (p. ej. tras cargar `.save` en el renderer).
    pub(crate) fn resend_model_clips_ready(&self, id: EntityId) {
        let Some(binding) = self.model_animation_bindings.get(&id) else {
            return;
        };
        let path = binding.asset_path.clone();
        let clip_meta: Vec<crate::engine3d::ipc::ModelClipInfoEvent> =
            if let Some(asset) = self.model_assets.get(&path) {
                asset
                    .clips
                    .iter()
                    .map(|c| crate::engine3d::ipc::ModelClipInfoEvent {
                        name: c.name.clone(),
                        duration_s: c.duration_s,
                        fps: c.fps,
                    })
                    .collect()
            } else {
                let path_buf = Path::new(&path);
                let infos = if crate::engine3d::config_3d::is_gltf_model_path(&path) {
                    model_asset::list_gltf_clip_infos(path_buf)
                } else {
                    model_asset::list_model_clip_infos(path_buf)
                };
                infos
                    .into_iter()
                    .map(|c| ModelClipInfoEvent {
                        name: c.name,
                        duration_s: c.duration_s,
                        fps: c.fps,
                    })
                    .collect()
            };
        if clip_meta.is_empty() {
            return;
        }
        send_event(&EngineEvent::ModelClipsReady {
            id,
            path,
            clips: clip_meta,
        });
    }

    pub(crate) fn resend_all_model_clips_ready(&self) {
        let ids: Vec<EntityId> = self.model_animation_bindings.keys().copied().collect();
        for id in ids {
            self.resend_model_clips_ready(id);
        }
    }

    pub(crate) fn unbind_model_animations(&mut self, id: EntityId) {
        self.model_animation_bindings.remove(&id);
        self.active_model_clips.remove(&id);
        self.model_clip_defaults.remove(&id);
        self.root_motion_by_clip.remove(&id);
    }

    pub(crate) fn play_model_clip(&mut self, id: EntityId, name: &str, loop_: bool) {
        let binding = match self.model_animation_bindings.get(&id) {
            Some(b) => b.clone(),
            None => return,
        };
        let asset = match self.get_model_asset_for_entity(&binding.asset_path, id) {
            Some(a) => a,
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
                loop_,
                playing: true,
                finished: false,
            },
        );
        self.emit_entity_animation_play_state(id);
    }

    /// Reproduce la siguiente animación del modelo enlazado a una entidad; al
    /// llegar a la última vuelve a la primera. Sirve para cualquier entidad con
    /// malla skinned, no solo para la demo.
    pub(crate) fn play_next_model_clip(&mut self, id: EntityId) {
        let binding = match self.model_animation_bindings.get(&id) {
            Some(b) => b.clone(),
            None => return,
        };
        let asset = match self.get_model_asset_for_entity(&binding.asset_path, id) {
            Some(a) => a,
            None => return,
        };
        if asset.clips.is_empty() {
            return;
        }
        let current = self
            .active_model_clips
            .get(&id)
            .map(|a| a.clip_name.as_str());
        let next = current
            .and_then(|name| asset.clips.iter().position(|c| c.name == name))
            .map(|i| (i + 1) % asset.clips.len())
            .unwrap_or(0);
        let name = asset.clips[next].name.clone();
        self.play_model_clip(id, &name, false);
    }

    pub(crate) fn stop_model_clip(&mut self, id: EntityId) {
        if let Some(active) = self.active_model_clips.get_mut(&id) {
            active.playing = false;
            active.time_s = 0.0;
            active.finished = true;
        }
        self.emit_entity_animation_play_state(id);
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
            let (asset_path, part_gpu_indices) = match self.model_animation_bindings.get(&id) {
                Some(binding) => (binding.asset_path.clone(), binding.part_gpu_indices.clone()),
                None => continue,
            };
            let asset = match self.get_model_asset_for_entity(&asset_path, id) {
                Some(a) => a,
                None => continue,
            };

            let (clip_index, time_s, loop_) = {
                let active = match self.active_model_clips.get(&id) {
                    Some(a) if a.playing && !a.finished => a,
                    _ => {
                        // Pose bind: tiempo 0, sin clip activo
                        self.write_joint_matrices_all_parts(
                            id,
                            &part_gpu_indices,
                            &asset,
                            None,
                            0.0,
                        );
                        continue;
                    }
                };
                let Some(clip_index) = asset.clips.iter().position(|c| c.name == active.clip_name)
                else {
                    continue;
                };
                (clip_index, active.time_s, active.loop_)
            };
            let clip = &asset.clips[clip_index];

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

            self.write_joint_matrices_all_parts(
                id,
                &part_gpu_indices,
                &asset,
                Some(clip),
                new_time,
            );
        }

        for id in finished_ids {
            if let Some(active) = self.active_model_clips.get_mut(&id) {
                active.playing = false;
                active.finished = true;
            }
            self.emit_entity_animation_play_state(id);
            send_event(&EngineEvent::AnimationFinished { entity_id: id });
        }
    }

    fn write_joint_matrices_all_parts(
        &mut self,
        entity_id: EntityId,
        part_gpu_indices: &[usize],
        asset: &ModelAsset,
        clip: Option<&AnimationClip>,
        time_s: f32,
    ) {
        let joint_count = asset.joint_parents.len().min(MAX_JOINTS);
        let mut local_transforms = asset.bind_local[..joint_count].to_vec();

        if let Some(clip) = clip {
            apply_clip_to_locals(clip, time_s, &mut local_transforms);
        }

        self.apply_bone_physics_to_locals(entity_id, asset, self.delta_time, &mut local_transforms);

        let use_per_part_ibm = asset.bind_pose_from_ibm && !asset.joint_gltf_nodes.is_empty();
        let shared_global = if use_per_part_ibm {
            None
        } else {
            Some(asset_joint_globals_with_locals(asset, &local_transforms))
        };

        let norm = asset.mesh_normalize;
        let inv_norm = norm.inverse();

        for (pi, &gpu_idx) in part_gpu_indices.iter().enumerate() {
            let Some(part) = asset.parts.get(pi) else {
                continue;
            };
            let per_part_global = if use_per_part_ibm {
                Some(gltf_bind_globals_from_ibm(
                    &part.inverse_bind,
                    &asset.bind_local[..joint_count],
                    &local_transforms,
                ))
            } else {
                None
            };
            let global: &[Mat4] = if let Some(ref globals) = per_part_global {
                globals
            } else {
                shared_global.as_ref().expect("shared_global")
            };
            let inv_mesh = part.mesh_bind_world.inverse();
            let gltf_skin = !asset.joint_gltf_nodes.is_empty();
            let mut joint_palette = vec![Mat4::IDENTITY; MAX_JOINTS];
            for ji in 0..joint_count {
                let g2b = Mat4::from_cols_array_2d(&part.inverse_bind[ji]);
                joint_palette[ji] = if gltf_skin && inv_mesh == Mat4::IDENTITY {
                    norm * global[ji] * g2b * inv_norm
                } else {
                    norm * global[ji] * g2b * inv_mesh * inv_norm
                };
            }
            let flat: Vec<[[f32; 4]; 4]> =
                joint_palette.iter().map(|m| m.to_cols_array_2d()).collect();
            if let Some(entry) = self.skinned_gpu_meshes.get_mut(gpu_idx) {
                entry.joint_palette = flat;
            }
        }
    }

    /// Delta del hueso raíz del clip activo del jugador, ya en espacio de mundo.
    ///
    /// Se calcula entre `time_s` (instante renderizado el frame anterior) y
    /// `time_s + dt` (instante que renderizará este frame), así que el render y
    /// el controlador consumen exactamente el mismo tramo de la animación.
    pub(crate) fn root_motion_delta_for_player(&self, dt: f32) -> Option<(Vec3, Quat)> {
        if dt <= 0.0 {
            return None;
        }
        let id = self.play_character_entity?;
        let active = self.active_model_clips.get(&id)?;
        if !active.playing || active.finished {
            return None;
        }
        if !self
            .root_motion_by_clip
            .get(&id)
            .and_then(|by_name| by_name.get(&active.clip_name))
            .copied()
            .unwrap_or(false)
        {
            return None;
        }
        let binding = self.model_animation_bindings.get(&id)?;
        let asset = self.get_model_asset_for_entity(&binding.asset_path, id)?;
        let clip = asset.clips.iter().find(|c| c.name == active.clip_name)?;

        let from = active.time_s.min(clip.duration_s);
        let mut to = from + dt;
        if to > clip.duration_s {
            if active.loop_ {
                // El último keyframe vuelve al primero: no hay salto real. Evitar
                // un delta negativo enorme en el frame de costura del bucle.
                return None;
            }
            to = clip.duration_s;
        }
        let (local_delta, local_rot) = root_motion_delta(&asset, clip, from, to)?;

        let t = self.world.get::<Transform>(id)?;
        let scaled = Vec3::new(
            local_delta.x * t.scale.x,
            local_delta.y * t.scale.y,
            local_delta.z * t.scale.z,
        );
        Some((t.rotation * scaled, (t.rotation * local_rot).normalize()))
    }
}

pub(crate) fn asset_joint_globals_with_clip(
    asset: &ModelAsset,
    clip: Option<&AnimationClip>,
    time_s: f32,
) -> Vec<Mat4> {
    let joint_count = asset.joint_parents.len().min(MAX_JOINTS);
    let mut locals = asset.bind_local[..joint_count].to_vec();
    if let Some(clip) = clip {
        apply_clip_to_locals(clip, time_s, &mut locals);
    }
    asset_joint_globals_with_locals(asset, &locals)
}

fn asset_joint_globals_with_locals(asset: &ModelAsset, locals: &[Mat4]) -> Vec<Mat4> {
    if asset.bind_pose_from_ibm && !asset.joint_gltf_nodes.is_empty() {
        return gltf_bind_globals_from_ibm(&asset.inverse_bind, &asset.bind_local, locals);
    }
    if !asset.joint_gltf_nodes.is_empty() {
        compute_gltf_joint_worlds(
            &asset.joint_gltf_nodes[..locals.len()],
            locals,
            &asset.gltf_scene_parents,
            &asset.gltf_bind_node_local,
        )
    } else {
        compute_joint_globals(
            &asset.joint_parents[..locals.len()],
            locals,
            &asset.joint_prefix_world[..locals.len()],
        )
    }
}

/// Resuelve globals aunque los padres estén después en el array (ancestros añadidos al final).
fn compute_joint_globals(
    joint_parents: &[Option<usize>],
    locals: &[Mat4],
    prefix_world: &[Mat4],
) -> Vec<Mat4> {
    let n = locals
        .len()
        .min(joint_parents.len())
        .min(prefix_world.len());
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
    let channels_by_joint: HashMap<usize, Vec<&AnimChannel>> =
        clip.channels.iter().fold(HashMap::new(), |mut m, ch| {
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

fn find_keyframe_pair(
    keys: &[AnimKeyframe],
    t: f32,
) -> Option<(&AnimKeyframe, &AnimKeyframe, f32)> {
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

/// Joint que gobierna el desplazamiento del clip: el más alto en la jerarquía
/// que tenga canales de traslación/rotación. En exportaciones de Blender suele
/// ser `Hips` (hijo de `Armature`), no el nodo raíz sin padre.
fn root_joint_index(asset: &ModelAsset, clip: &AnimationClip) -> Option<usize> {
    let depth = |mut ji: usize| -> usize {
        let mut d = 0;
        while let Some(parent) = asset.joint_parents.get(ji).copied().flatten() {
            if parent == ji {
                break;
            }
            ji = parent;
            d += 1;
        }
        d
    };

    let mut best: Option<(usize, usize, usize)> = None;
    for ji in 0..asset.joint_parents.len().min(MAX_JOINTS) {
        let has_motion = clip.channels.iter().any(|ch| {
            ch.joint_index == ji
                && matches!(
                    ch.property,
                    AnimProperty::Translation | AnimProperty::Rotation
                )
        });
        if !has_motion {
            continue;
        }
        let name = asset.joint_names.get(ji).map(String::as_str).unwrap_or("");
        let lower = name.to_ascii_lowercase();
        let preference = if lower.contains("root") {
            0
        } else if lower.contains("hips") {
            1
        } else {
            2
        };
        let score = (depth(ji), preference, ji);
        if best.is_none_or(|b| score < b) {
            best = Some(score);
        }
    }
    best.map(|(_, _, ji)| ji)
}

/// Matriz del joint raíz en el espacio normalizado del modelo (mismo `norm`
/// que usa el skinning), para que el delta sea comparable con el render.
fn root_joint_matrix(
    asset: &ModelAsset,
    clip: &AnimationClip,
    ji: usize,
    time_s: f32,
) -> Option<Mat4> {
    let joint_count = asset.joint_parents.len().min(MAX_JOINTS);
    let mut locals = asset.bind_local[..joint_count].to_vec();
    apply_clip_to_locals(clip, time_s, &mut locals);
    let globals = asset_joint_globals_with_locals(asset, &locals);
    let global = *globals.get(ji)?;
    let norm = asset.mesh_normalize;
    Some(norm * global * norm.inverse())
}

/// Delta de posición y rotación del hueso raíz entre dos instantes del clip.
pub(crate) fn root_motion_delta(
    asset: &ModelAsset,
    clip: &AnimationClip,
    from_t: f32,
    to_t: f32,
) -> Option<(Vec3, Quat)> {
    let ji = root_joint_index(asset, clip)?;
    let before = root_joint_matrix(asset, clip, ji, from_t)?;
    let after = root_joint_matrix(asset, clip, ji, to_t)?;
    let delta = after * before.inverse();
    let (_, rotation, translation) = delta.to_scale_rotation_translation();
    Some((translation, rotation))
}

#[cfg(test)]
mod root_motion_tests {
    use super::*;
    use std::collections::HashMap;

    fn test_asset() -> ModelAsset {
        ModelAsset {
            path: String::new(),
            joint_parents: vec![None],
            inverse_bind: vec![Mat4::IDENTITY.to_cols_array_2d()],
            bind_local: vec![Mat4::IDENTITY],
            joint_prefix_world: vec![Mat4::IDENTITY],
            joint_names: vec!["root".to_string()],
            parts: Vec::new(),
            clips: Vec::new(),
            mesh_normalize: Mat4::IDENTITY,
            facing_forward_xz: glam::Vec2::X,
            joint_gltf_nodes: Vec::new(),
            gltf_scene_parents: HashMap::new(),
            gltf_bind_node_local: HashMap::new(),
            bind_pose_from_ibm: false,
        }
    }

    fn translation_clip() -> AnimationClip {
        AnimationClip {
            name: "walk".to_string(),
            duration_s: 1.0,
            fps: 30.0,
            channels: vec![AnimChannel {
                joint_index: 0,
                property: AnimProperty::Translation,
                keyframes: vec![
                    AnimKeyframe {
                        time: 0.0,
                        translation: Some([0.0, 0.0, 0.0]),
                        rotation: None,
                        scale: None,
                    },
                    AnimKeyframe {
                        time: 1.0,
                        translation: Some([2.0, 0.0, 0.0]),
                        rotation: None,
                        scale: None,
                    },
                ],
            }],
        }
    }

    #[test]
    fn root_motion_devuelve_desplazamiento_interpolado() {
        let asset = test_asset();
        let clip = translation_clip();
        let (delta, rotation) = root_motion_delta(&asset, &clip, 0.0, 0.5).unwrap();
        assert!((delta.x - 1.0).abs() < 1e-5, "delta = {delta:?}");
        assert!(delta.y.abs() < 1e-5 && delta.z.abs() < 1e-5);
        assert!((rotation.w - 1.0).abs() < 1e-5);
    }

    #[test]
    fn root_motion_sin_canal_de_traslacion_es_neutro() {
        let asset = test_asset();
        let clip = AnimationClip {
            channels: Vec::new(),
            ..translation_clip()
        };
        assert!(root_motion_delta(&asset, &clip, 0.0, 0.5).is_none());
    }

    #[test]
    fn root_joint_ignora_joints_sin_canales_y_prefiere_root_en_empate() {
        let mut asset = test_asset();
        asset.joint_parents = vec![None, None, None];
        asset.inverse_bind = vec![Mat4::IDENTITY.to_cols_array_2d(); 3];
        asset.bind_local = vec![Mat4::IDENTITY; 3];
        asset.joint_prefix_world = vec![Mat4::IDENTITY; 3];
        asset.joint_names = vec!["Hips".into(), "root".into(), "otro".into()];
        let mut clip = translation_clip();
        clip.channels[0].joint_index = 1;
        assert_eq!(root_joint_index(&asset, &clip), Some(1));

        clip.channels.push(AnimChannel {
            joint_index: 0,
            property: AnimProperty::Rotation,
            keyframes: vec![AnimKeyframe {
                time: 0.0,
                translation: None,
                rotation: Some([0.0, 0.0, 0.0, 1.0]),
                scale: None,
            }],
        });
        assert_eq!(root_joint_index(&asset, &clip), Some(1));
    }
}
