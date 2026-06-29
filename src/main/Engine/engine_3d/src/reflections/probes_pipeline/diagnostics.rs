//! Logs de diagnóstico para probes.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use glam::Vec3;

use crate::ecs::EntityId;
use crate::engine::SceneInstanceBatch;
use crate::mesh::InstanceData;
use crate::reflections::probe_env::{ProbeMetaUniform, MAX_PROBES};
use crate::reflections::probes_pipeline::capture::ProbeFrameData;

const LOG_SHADER_INTERVAL: u32 = 120;
const LOG_BUFFERS_INTERVAL: u32 = 60;
const LOG_HASH_INTERVAL: u32 = 60;
const LOG_CUBEMAP_INTERVAL: u32 = 120;

pub(crate) struct ProbeDiagState {
    pub epoch: u32,
    frame: u32,
    last_shader_frame: u32,
    last_buffers_frame: u32,
    last_hash_frame: u32,
    last_cubemap_frame: u32,
    /// Frame id del último readback encolado (para el log tras submit).
    pub pending_cubemap_log_frame: Option<u32>,
}

impl ProbeDiagState {
    pub fn new() -> Self {
        Self {
            epoch: 0,
            frame: 0,
            last_shader_frame: 0,
            last_buffers_frame: 0,
            last_hash_frame: 0,
            last_cubemap_frame: 0,
            pending_cubemap_log_frame: None,
        }
    }

    pub fn bump_epoch(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        self.last_shader_frame = 0;
        self.last_buffers_frame = 0;
        self.last_hash_frame = 0;
        self.last_cubemap_frame = 0;
        self.pending_cubemap_log_frame = None;
    }

    pub fn tick_frame(&mut self) -> u32 {
        self.frame = self.frame.wrapping_add(1);
        self.frame
    }
}

fn should_log(frame_id: u32, last: &mut u32, interval: u32) -> bool {
    if *last == 0 || frame_id.saturating_sub(*last) >= interval {
        *last = frame_id;
        return true;
    }
    false
}

/// ¿Toca encolar readback de cubemap este frame?
pub(crate) fn should_log_cubemap(frame_id: u32, diag: &mut ProbeDiagState) -> bool {
    should_log(frame_id, &mut diag.last_cubemap_frame, LOG_CUBEMAP_INTERVAL)
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

fn hash_instances(batches: &[SceneInstanceBatch]) -> u64 {
    let mut h = DefaultHasher::new();
    for batch in batches {
        batch.mesh_idx.hash(&mut h);
        batch.probe_layer.hash(&mut h);
        for inst in &batch.instances {
            for v in &inst.tex_layer_pad {
                v.to_bits().hash(&mut h);
            }
            inst.flag_pad[3].to_bits().hash(&mut h);
            inst.tex_layer_pad[1].to_bits().hash(&mut h);
        }
    }
    h.finish()
}

fn hash_probe_meta(meta: &ProbeMetaUniform) -> u64 {
    hash_bytes(bytemuck::bytes_of(meta))
}

/// Réplica CPU de `refl_nearest_probe_layer_entries` (lo que usa `fs_main` hoy).
fn nearest_probe_layer(world_pos: Vec3, entries: &[[f32; 4]; MAX_PROBES]) -> i32 {
    let mut best_i = -1i32;
    let mut best_d = f32::MAX;
    for (i, e) in entries.iter().enumerate() {
        if e[3] <= 0.0 {
            continue;
        }
        let center = Vec3::new(e[0], e[1], e[2]);
        let d = world_pos.distance(center);
        if d < best_d {
            best_d = d;
            best_i = i32::try_from(i).unwrap_or(-1);
        }
    }
    best_i
}

/// Capa si el shader usara `refl_resolve_probe_layer` (own-slot cuando inst ≥ 0).
fn resolve_probe_layer_policy(world_pos: Vec3, inst_probe: i32, entries: &[[f32; 4]; MAX_PROBES]) -> i32 {
    if inst_probe >= 0 {
        return inst_probe;
    }
    nearest_probe_layer(world_pos, entries)
}

fn world_pos_from_instance(inst: &InstanceData) -> Vec3 {
    let m = glam::Mat4::from_cols_array_2d(&inst.model);
    m.transform_point3(Vec3::ZERO)
}

fn find_instance_for_entity(
    entity_id: EntityId,
    batches: &[SceneInstanceBatch],
) -> Option<(usize, usize, &InstanceData)> {
    let mut global = 0usize;
    for (batch_i, batch) in batches.iter().enumerate() {
        for (local_i, (&eid, inst)) in batch
            .entity_ids
            .iter()
            .zip(batch.instances.iter())
            .enumerate()
        {
            if eid == entity_id {
                return Some((global, batch_i * 1000 + local_i, inst));
            }
            global += 1;
        }
    }
    None
}

/// Log 1: inputs que recibe / computa el fragment (réplica CPU + buffer de instancia).
fn log_shader_inputs(
    frame_id: u32,
    probe_frame: &ProbeFrameData,
    batches: &[SceneInstanceBatch],
    probe_meta: &ProbeMetaUniform,
) {
    log::info!(
        "[reflexiones][shader-input] frame={frame_id} — réplica CPU de fs_main (capa_nearest = política actual)"
    );
    for &(entity_id, center, slot) in &probe_frame.probe_list {
        let inst_probe = probe_frame
            .probe_index_map
            .get(&entity_id)
            .copied()
            .map(|s| s as i32)
            .unwrap_or(-1);
        let layer_nearest = nearest_probe_layer(center, &probe_meta.entries);
        let layer_policy = resolve_probe_layer_policy(center, inst_probe, &probe_meta.entries);
        let (instance_id, gpu_probe_index, gpu_world) =
            if let Some((global_id, _, inst)) = find_instance_for_entity(entity_id, batches) {
                (
                    global_id,
                    inst.tex_layer_pad[2] as i32,
                    world_pos_from_instance(inst),
                )
            } else {
                (usize::MAX, -1, center)
            };
        let mismatch_gpu = (gpu_probe_index != inst_probe).then_some("GPU≠mapa");
        let mismatch_nearest = (layer_nearest != layer_policy).then_some("nearest≠own-slot");
        log::info!(
            "[reflexiones][shader-input] entidad={entity_id} instancia={instance_id} \
             ranura_asignada={slot} probe_index_map={inst_probe} probe_index_gpu={gpu_probe_index} \
             capa_cubemap_fs_main={layer_nearest} capa_own_slot={layer_policy} \
             world_centro=({:.2},{:.2},{:.2}) world_gpu=({:.2},{:.2},{:.2}) \
             meta_radio={:.3} {}",
            center.x,
            center.y,
            center.z,
            gpu_world.x,
            gpu_world.y,
            gpu_world.z,
            probe_meta.entries[slot][3],
            [
                mismatch_gpu.unwrap_or(""),
                mismatch_nearest.unwrap_or(""),
            ]
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(" ")
        );
    }
    if probe_frame.probe_list.is_empty() {
        log::info!("[reflexiones][shader-input] sin probes activos en probe_list (tier Off o sin [ReflectionProbe])");
    }
}

/// Log 2: snapshot del buffer de instancias antes del draw (solo batches con probes).
fn log_draw_buffer_snapshots(frame_id: u32, probe_frame: &ProbeFrameData, batches: &[SceneInstanceBatch]) {
    let probe_ids: std::collections::HashSet<EntityId> =
        probe_frame.probe_list.iter().map(|(id, _, _)| *id).collect();
    log::info!(
        "[reflexiones][draw-buffer] frame={frame_id} batches_totales={}",
        batches.len()
    );
    let mut lines = 0u32;
    const MAX_LINES: u32 = 8;
    let mut global_base = 0u32;
    for (batch_i, batch) in batches.iter().enumerate() {
        let has_probe = batch.entity_ids.iter().any(|id| probe_ids.contains(id));
        if !has_probe {
            global_base += batch.instances.len() as u32;
            continue;
        }
        if lines >= MAX_LINES {
            break;
        }
        let count = batch.instances.len();
        let probe_indices: Vec<i32> = batch
            .instances
            .iter()
            .take(5)
            .map(|i| i.tex_layer_pad[2] as i32)
            .collect();
        let entity_sample: Vec<EntityId> = batch.entity_ids.iter().take(5).copied().collect();
        log::info!(
            "[reflexiones][draw-buffer] batch={batch_i} mesh={} instancias={global_base}..{} \
             probe_layer_batch={} probe_index[0..5]={probe_indices:?} entidades[0..5]={entity_sample:?}",
            batch.mesh_idx,
            global_base + count as u32 - 1,
            batch.probe_layer,
        );
        global_base += count as u32;
        lines += 1;
    }
    if lines == 0 {
        log::info!("[reflexiones][draw-buffer] ningún batch contiene entidades [ReflectionProbe] este frame");
    }
}

/// Log 3: hash de buffers por frame (detectar si el GPU recibe datos distintos).
fn log_frame_buffer_hashes(
    frame_id: u32,
    batches: &[SceneInstanceBatch],
    probe_meta: &ProbeMetaUniform,
) {
    let inst_hash = hash_instances(batches);
    let meta_hash = hash_probe_meta(probe_meta);
    let inst_count: usize = batches.iter().map(|b| b.instances.len()).sum();
    log::info!(
        "[reflexiones][buffer-hash] frame={frame_id} instancias={inst_count} \
         instance_buffer_hash=0x{inst_hash:016x} probe_meta_hash=0x{meta_hash:016x}"
    );
    for (i, e) in probe_meta.entries.iter().enumerate().take(MAX_PROBES) {
        if e[3] > 0.0 {
            log::info!(
                "[reflexiones][buffer-hash] meta[{i}] centro=({:.2},{:.2},{:.2}) radio={:.3}",
                e[0],
                e[1],
                e[2],
                e[3]
            );
        }
    }
}


