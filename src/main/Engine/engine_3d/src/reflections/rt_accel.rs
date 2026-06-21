//! Orquestador RT v2: BVH CPU + BLAS/TLAS hardware opcional.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use wgpu::util::DeviceExt;
use wgpu::{CommandEncoder, Device, Queue, Tlas, TlasInstance};

use crate::mesh::Mesh;
use crate::config_3d::model_animation::GpuSkinnedMeshEntry;
use crate::reflections::blas::BlasCache;
use crate::reflections::bvh::{build_bvh, BvhNodeGpu, RtTriangleGpu};
use crate::reflections::skinned_blas::SkinnedBlasCache;
use crate::reflections::tlas::{
    instance_triangles_world, mat4_to_tlas_transform, RtInstanceDesc, MAX_SKINNED_RT_INSTANCES,
    MAX_STATIC_RT_INSTANCES,
};

const ASYNC_BVH_TRI_THRESHOLD: usize = 8192;
const BVH_BUILD_BUDGET_MS: f64 = 2.0;

struct PendingBvh {
    scene_hash: u64,
    result: Arc<Mutex<Option<(Vec<BvhNodeGpu>, Vec<RtTriangleGpu>)>>>,
}

pub struct RtAccel {
    pub node_count: u32,
    pub tri_count: u32,
    pub hw_tlas: Option<Tlas>,
    pub hw_available: bool,
    bvh_node_buffer: wgpu::Buffer,
    bvh_tri_buffer: wgpu::Buffer,
    blas_cache: BlasCache,
    skinned_blas_cache: SkinnedBlasCache,
    scene_hash: u64,
    pending_bvh: Option<PendingBvh>,
}

impl RtAccel {
    pub fn new(device: &Device, hw_available: bool) -> Self {
        let empty_node = BvhNodeGpu::default();
        let empty_tri = RtTriangleGpu::default();
        let bvh_node_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rt-bvh-nodes"),
            contents: bytemuck::bytes_of(&empty_node),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let bvh_tri_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rt-bvh-tris"),
            contents: bytemuck::bytes_of(&empty_tri),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let hw_tlas = if hw_available {
            Some(device.create_tlas(&wgpu::CreateTlasDescriptor {
                label: Some("rt-tlas"),
                max_instances: MAX_STATIC_RT_INSTANCES as u32,
                flags: wgpu::AccelerationStructureFlags::PREFER_FAST_TRACE,
                update_mode: wgpu::AccelerationStructureUpdateMode::Build,
            }))
        } else {
            None
        };
        Self {
            node_count: 0,
            tri_count: 0,
            hw_tlas,
            hw_available,
            bvh_node_buffer,
            bvh_tri_buffer,
            blas_cache: BlasCache::new(),
            skinned_blas_cache: SkinnedBlasCache::new(),
            scene_hash: 0,
            pending_bvh: None,
        }
    }

    pub fn node_buffer(&self) -> &wgpu::Buffer {
        &self.bvh_node_buffer
    }

    pub fn tri_buffer(&self) -> &wgpu::Buffer {
        &self.bvh_tri_buffer
    }

    pub fn tlas(&self) -> Option<&Tlas> {
        self.hw_tlas.as_ref()
    }

    fn scene_hash(
        instances: &[RtInstanceDesc],
        skinned_meshes: &[GpuSkinnedMeshEntry],
    ) -> u64 {
        let mut h = DefaultHasher::new();
        for inst in instances {
            inst.entity_id.hash(&mut h);
            inst.mesh_idx.hash(&mut h);
            inst.skinned_gpu_idx.hash(&mut h);
            for f in inst.transform.to_cols_array() {
                f.to_bits().hash(&mut h);
            }
            if let Some(gpu_idx) = inst.skinned_gpu_idx {
                if let Some(entry) = skinned_meshes.get(gpu_idx) {
                    for mat in entry.joint_palette() {
                        for row in mat {
                            for f in row {
                                f.to_bits().hash(&mut h);
                            }
                        }
                    }
                }
            }
        }
        h.finish()
    }

    fn upload_bvh(
        &mut self,
        nodes: Vec<BvhNodeGpu>,
        tris: Vec<RtTriangleGpu>,
        device: &Device,
        queue: &Queue,
    ) {
        self.node_count = nodes.len().min(u32::MAX as usize) as u32;
        self.tri_count = tris.len().min(u32::MAX as usize) as u32;

        if nodes.is_empty() {
            log::debug!("[RT] BVH vacío — sin geometría trazable");
            return;
        }

        let node_bytes = nodes.len() * std::mem::size_of::<BvhNodeGpu>();
        if self.bvh_node_buffer.size() < node_bytes as u64 {
            self.bvh_node_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rt-bvh-nodes"),
                size: node_bytes.max(256) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        queue.write_buffer(&self.bvh_node_buffer, 0, bytemuck::cast_slice(&nodes));

        let tri_bytes = tris.len() * std::mem::size_of::<RtTriangleGpu>();
        if self.bvh_tri_buffer.size() < tri_bytes as u64 {
            self.bvh_tri_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rt-bvh-tris"),
                size: tri_bytes.max(256) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        queue.write_buffer(&self.bvh_tri_buffer, 0, bytemuck::cast_slice(&tris));

        log::debug!(
            "[RT] BVH v2: {} nodos, {} triángulos",
            self.node_count,
            self.tri_count
        );
    }

    fn poll_pending_bvh(&mut self, device: &Device, queue: &Queue) {
        let Some(pending) = self.pending_bvh.take() else {
            return;
        };
        let ready = {
            let mut guard = pending.result.lock().expect("bvh pending lock");
            guard.take()
        };
        if let Some((nodes, tris)) = ready {
            self.scene_hash = pending.scene_hash;
            self.upload_bvh(nodes, tris, device, queue);
        } else {
            self.pending_bvh = Some(pending);
        }
    }

    pub fn sync_scene(
        &mut self,
        instances: &[RtInstanceDesc],
        meshes: &[Mesh],
        skinned_meshes: &[GpuSkinnedMeshEntry],
        device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
    ) {
        self.poll_pending_bvh(device, queue);

        let hash = Self::scene_hash(instances, skinned_meshes);
        if hash == self.scene_hash && self.node_count > 0 {
            return;
        }

        if let Some(pending) = &self.pending_bvh {
            if pending.scene_hash == hash {
                return;
            }
        }

        let mut world_tris = Vec::new();
        for inst in instances {
            world_tris.extend(instance_triangles_world(meshes, skinned_meshes, inst));
            if self.hw_available {
                if let Some(mesh_idx) = inst.mesh_idx {
                    if let Some(mesh) = meshes.get(mesh_idx) {
                        self.blas_cache.ensure(device, mesh_idx, mesh);
                    }
                }
                if let Some(gpu_idx) = inst.skinned_gpu_idx {
                    if let Some(entry) = skinned_meshes.get(gpu_idx) {
                        self.skinned_blas_cache.ensure(device, gpu_idx, entry);
                        self.skinned_blas_cache.update_pose(queue, gpu_idx, entry);
                    }
                }
            }
        }

        if self.hw_available {
            self.blas_cache.build_pending(encoder, meshes);
            self.skinned_blas_cache.build_updates(encoder);
            if let Some(tlas) = self.hw_tlas.as_mut() {
                let mut slot = 0usize;
                let mut skinned_slots = 0usize;
                for inst in instances {
                    if slot >= MAX_STATIC_RT_INSTANCES {
                        break;
                    }
                    if let Some(mesh_idx) = inst.mesh_idx {
                        if let Some(slot_inst) = tlas.get_mut_single(slot) {
                            if let Some(blas) = self.blas_cache.get(mesh_idx) {
                                *slot_inst = Some(TlasInstance::new(
                                    blas,
                                    mat4_to_tlas_transform(inst.transform),
                                    inst.entity_id & 0x00FF_FFFF,
                                    0xFF,
                                ));
                                slot += 1;
                            }
                        }
                    }
                }
                for inst in instances {
                    if slot >= MAX_STATIC_RT_INSTANCES {
                        break;
                    }
                    let Some(gpu_idx) = inst.skinned_gpu_idx else {
                        continue;
                    };
                    if skinned_slots >= MAX_SKINNED_RT_INSTANCES {
                        break;
                    }
                    if let Some(slot_inst) = tlas.get_mut_single(slot) {
                        if let Some(blas) = self.skinned_blas_cache.get(gpu_idx) {
                            *slot_inst = Some(TlasInstance::new(
                                blas,
                                mat4_to_tlas_transform(inst.transform),
                                inst.entity_id & 0x00FF_FFFF,
                                0xFF,
                            ));
                            slot += 1;
                            skinned_slots += 1;
                        }
                    }
                }
                for clear in slot..MAX_STATIC_RT_INSTANCES {
                    if let Some(slot_inst) = tlas.get_mut_single(clear) {
                        *slot_inst = None;
                    }
                }
            }
            if let Some(tlas) = self.hw_tlas.as_ref() {
                encoder.build_acceleration_structures(std::iter::empty(), std::iter::once(tlas));
            }
        }

        if world_tris.len() >= ASYNC_BVH_TRI_THRESHOLD {
            let build_start = Instant::now();
            let result = Arc::new(Mutex::new(None));
            let result_bg = Arc::clone(&result);
            let tris_bg = world_tris.clone();
            rayon::spawn(move || {
                let (nodes, tris) = build_bvh(tris_bg);
                *result_bg.lock().expect("bvh lock") = Some((nodes, tris));
            });
            self.pending_bvh = Some(PendingBvh {
                scene_hash: hash,
                result,
            });
            let elapsed_ms = build_start.elapsed().as_secs_f64() * 1000.0;
            if elapsed_ms > BVH_BUILD_BUDGET_MS as f64 {
                log::debug!(
                    "[RT] BVH async lanzado ({} triángulos, {:.2} ms encolado)",
                    world_tris.len(),
                    elapsed_ms
                );
            }
            return;
        }

        let build_start = Instant::now();
        let (nodes, tris) = build_bvh(world_tris);
        let elapsed_ms = build_start.elapsed().as_secs_f64() * 1000.0;
        if elapsed_ms > BVH_BUILD_BUDGET_MS as f64 {
            log::warn!(
                "[RT] BVH build superó budget ({:.2} ms > {:.2} ms)",
                elapsed_ms,
                BVH_BUILD_BUDGET_MS
            );
        }
        self.scene_hash = hash;
        self.upload_bvh(nodes, tris, device, queue);
    }
}
