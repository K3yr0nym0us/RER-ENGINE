//! Orquestador RT v2: BVH CPU + BLAS/TLAS hardware opcional.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use wgpu::util::DeviceExt;
use wgpu::{CommandEncoder, Device, Queue, Tlas, TlasInstance};

use crate::config_3d::model_animation::GpuSkinnedMeshEntry;
use crate::mesh::Mesh;
use super::blas::BlasCache;
use super::bvh::{build_bvh, BvhNodeGpu, RtTriangleGpu};
use super::rt_material::{
    instance_triangles_tagged, RtInstanceMaterialGpu, MAX_RT_MATERIALS,
};
use super::skinned_blas::SkinnedBlasCache;
use super::tlas::{
    mat4_to_tlas_transform, RtInstanceDesc, MAX_SKINNED_RT_INSTANCES, MAX_STATIC_RT_INSTANCES,
};

const ASYNC_BVH_TRI_THRESHOLD: usize = 8192;
const BVH_BUILD_BUDGET_MS: f64 = 2.0;

struct PendingBvh {
    static_hash: u64,
    result: Arc<Mutex<Option<(Vec<BvhNodeGpu>, Vec<RtTriangleGpu>)>>>,
}

pub struct RtAccel {
    pub node_count: u32,
    pub tri_count: u32,
    pub hw_tlas: Option<Tlas>,
    pub hw_available: bool,
    bvh_node_buffer: wgpu::Buffer,
    bvh_tri_buffer: wgpu::Buffer,
    hw_tri_buffer: wgpu::Buffer,
    instance_tri_base_buffer: wgpu::Buffer,
    pub hw_tri_count: u32,
    instance_material_buffer: wgpu::Buffer,
    pub instance_material_count: u32,
    blas_cache: BlasCache,
    skinned_blas_cache: SkinnedBlasCache,
    static_scene_hash: u64,
    pose_hash: u64,
    pending_bvh: Option<PendingBvh>,
    tlas_geom_dirty: bool,
    tlas_instance_dirty: bool,
    last_tlas_slot_count: usize,
}

impl RtAccel {
    pub fn new(device: &Device, hw_available: bool) -> Self {
        let empty_node = BvhNodeGpu::default();
        let empty_tri = RtTriangleGpu::default();
        let empty_mat = RtInstanceMaterialGpu::default();
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
        let hw_tri_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rt-hw-tris"),
            contents: bytemuck::bytes_of(&empty_tri),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let instance_tri_base_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rt-instance-tri-base"),
            contents: bytemuck::cast_slice(&[0u32; MAX_RT_MATERIALS]),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let instance_material_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rt-instance-materials"),
            contents: bytemuck::bytes_of(&empty_mat),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        // TLAS lazy: se crea en `ensure_hw` cuando el tier activo es High/Ultra.
        let _ = device;
        Self {
            node_count: 0,
            tri_count: 0,
            hw_tlas: None,
            hw_available,
            bvh_node_buffer,
            bvh_tri_buffer,
            hw_tri_buffer,
            instance_tri_base_buffer,
            hw_tri_count: 0,
            instance_material_buffer,
            instance_material_count: 0,
            blas_cache: BlasCache::new(),
            skinned_blas_cache: SkinnedBlasCache::new(),
            static_scene_hash: 0,
            pose_hash: 0,
            pending_bvh: None,
            tlas_geom_dirty: true,
            tlas_instance_dirty: true,
            last_tlas_slot_count: 0,
        }
    }

    pub fn node_buffer(&self) -> &wgpu::Buffer {
        &self.bvh_node_buffer
    }

    pub fn tri_buffer(&self) -> &wgpu::Buffer {
        &self.bvh_tri_buffer
    }

    pub fn hw_tri_buffer(&self) -> &wgpu::Buffer {
        &self.hw_tri_buffer
    }

    pub fn instance_tri_base_buffer(&self) -> &wgpu::Buffer {
        &self.instance_tri_base_buffer
    }

    pub fn instance_material_buffer(&self) -> &wgpu::Buffer {
        &self.instance_material_buffer
    }

    pub fn tlas(&self) -> Option<&Tlas> {
        self.hw_tlas.as_ref()
    }

    pub fn hw_active(&self) -> bool {
        self.hw_available && self.hw_tlas.is_some()
    }

    /// Hay geometría trazable (BVH CPU o TLAS HW con triángulos).
    pub fn has_traceable_geometry(&self) -> bool {
        self.node_count > 0 || (self.hw_active() && self.hw_tri_count > 0)
    }

    /// Reserva TLAS/BLAS solo cuando el tier pide RT (High/Ultra).
    pub fn ensure_hw(&mut self, device: &Device) {
        if !self.hw_available || self.hw_tlas.is_some() {
            return;
        }
        self.hw_tlas = Some(device.create_tlas(&wgpu::CreateTlasDescriptor {
            label: Some("rt-tlas"),
            max_instances: MAX_STATIC_RT_INSTANCES as u32,
            flags: wgpu::AccelerationStructureFlags::PREFER_FAST_TRACE
                | wgpu::AccelerationStructureFlags::ALLOW_UPDATE,
            update_mode: wgpu::AccelerationStructureUpdateMode::Build,
        }));
        self.tlas_geom_dirty = true;
        self.tlas_instance_dirty = true;
    }

    fn static_scene_hash(instances: &[RtInstanceDesc]) -> u64 {
        let mut h = DefaultHasher::new();
        for inst in instances {
            inst.entity_id.hash(&mut h);
            inst.mesh_idx.hash(&mut h);
            inst.skinned_gpu_idx.hash(&mut h);
            for f in inst.transform.to_cols_array() {
                f.to_bits().hash(&mut h);
            }
        }
        h.finish()
    }

    fn pose_hash(
        instances: &[RtInstanceDesc],
        skinned_meshes: &[GpuSkinnedMeshEntry],
    ) -> u64 {
        let mut h = DefaultHasher::new();
        for inst in instances {
            if let Some(gpu_idx) = inst.skinned_gpu_idx {
                if let Some(entry) = skinned_meshes.get(gpu_idx) {
                    gpu_idx.hash(&mut h);
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

    fn active_skinned_gpu_indices(instances: &[RtInstanceDesc]) -> HashSet<usize> {
        instances
            .iter()
            .filter_map(|inst| inst.skinned_gpu_idx)
            .collect()
    }

    fn build_hw_triangle_lookup(
        instances: &[RtInstanceDesc],
        meshes: &[Mesh],
        skinned_meshes: &[GpuSkinnedMeshEntry],
    ) -> (Vec<RtTriangleGpu>, [u32; MAX_RT_MATERIALS]) {
        let mut tris = Vec::new();
        let mut bases = [u32::MAX; MAX_RT_MATERIALS];
        for (slot, inst) in instances.iter().enumerate() {
            if slot >= MAX_RT_MATERIALS {
                break;
            }
            bases[slot] = tris.len() as u32;
            let tagged = instance_triangles_tagged(meshes, skinned_meshes, inst, slot as u32);
            tris.extend(tagged.into_iter().map(RtTriangleGpu::from));
        }
        (tris, bases)
    }

    fn upload_hw_triangles(
        &mut self,
        tris: &[RtTriangleGpu],
        bases: &[u32; MAX_RT_MATERIALS],
        device: &Device,
        queue: &Queue,
    ) {
        self.hw_tri_count = tris.len().min(u32::MAX as usize) as u32;
        if tris.is_empty() {
            return;
        }
        let tri_bytes = tris.len() * std::mem::size_of::<RtTriangleGpu>();
        if self.hw_tri_buffer.size() < tri_bytes as u64 {
            self.hw_tri_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rt-hw-tris"),
                size: tri_bytes.max(256) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        queue.write_buffer(&self.hw_tri_buffer, 0, bytemuck::cast_slice(tris));
        queue.write_buffer(
            &self.instance_tri_base_buffer,
            0,
            bytemuck::cast_slice(bases),
        );
    }

    fn upload_materials(
        &mut self,
        materials: &[RtInstanceMaterialGpu],
        device: &Device,
        queue: &Queue,
    ) {
        self.instance_material_count = materials.len().min(u32::MAX as usize) as u32;
        if materials.is_empty() {
            return;
        }
        let bytes = materials.len() * std::mem::size_of::<RtInstanceMaterialGpu>();
        if self.instance_material_buffer.size() < bytes as u64 {
            self.instance_material_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rt-instance-materials"),
                size: bytes.max(64) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        queue.write_buffer(
            &self.instance_material_buffer,
            0,
            bytemuck::cast_slice(materials),
        );
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
            self.static_scene_hash = pending.static_hash;
            self.upload_bvh(nodes, tris, device, queue);
        } else {
            self.pending_bvh = Some(pending);
        }
    }

    pub fn sync_scene(
        &mut self,
        materials: &[RtInstanceMaterialGpu],
        instances: &[RtInstanceDesc],
        meshes: &[Mesh],
        skinned_meshes: &[GpuSkinnedMeshEntry],
        device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
        build_cpu_bvh: bool,
    ) {
        if build_cpu_bvh {
            self.poll_pending_bvh(device, queue);
        }
        self.upload_materials(materials, device, queue);

        let static_hash = Self::static_scene_hash(instances);
        let pose_hash = Self::pose_hash(instances, skinned_meshes);
        let static_unchanged = static_hash == self.static_scene_hash && self.node_count > 0;
        let pose_only = static_unchanged && pose_hash != self.pose_hash;

        if self.hw_active() {
            self.sync_hw_accel(
                instances,
                meshes,
                skinned_meshes,
                device,
                queue,
                encoder,
                static_hash,
                pose_hash,
            );
        }

        if !build_cpu_bvh {
            self.static_scene_hash = static_hash;
            self.pose_hash = pose_hash;
            return;
        }

        if static_unchanged {
            if pose_only {
                self.pose_hash = pose_hash;
            }
            return;
        }

        if let Some(pending) = &self.pending_bvh {
            if pending.static_hash == static_hash {
                return;
            }
        }

        let mut world_tris = Vec::new();
        for (slot, inst) in instances.iter().enumerate() {
            if slot >= MAX_RT_MATERIALS {
                break;
            }
            world_tris.extend(instance_triangles_tagged(
                meshes,
                skinned_meshes,
                inst,
                slot as u32,
            ));
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
                static_hash,
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
        self.static_scene_hash = static_hash;
        self.pose_hash = pose_hash;
        self.upload_bvh(nodes, tris, device, queue);
    }

    fn sync_hw_accel(
        &mut self,
        instances: &[RtInstanceDesc],
        meshes: &[Mesh],
        skinned_meshes: &[GpuSkinnedMeshEntry],
        device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
        static_hash: u64,
        pose_hash: u64,
    ) {
        let geom_changed = static_hash != self.static_scene_hash;
        let pose_changed = pose_hash != self.pose_hash;
        if geom_changed || pose_changed {
            self.tlas_geom_dirty = true;
            self.tlas_instance_dirty = true;
        }

        for inst in instances {
            if let Some(mesh_idx) = inst.mesh_idx {
                if let Some(mesh) = meshes.get(mesh_idx) {
                    if !self.blas_cache.contains(mesh_idx) {
                        self.blas_cache.ensure(device, mesh_idx, mesh);
                        self.tlas_geom_dirty = true;
                    }
                }
            }
            if let Some(gpu_idx) = inst.skinned_gpu_idx {
                if let Some(entry) = skinned_meshes.get(gpu_idx) {
                    if !self.skinned_blas_cache.contains(gpu_idx) {
                        self.skinned_blas_cache.ensure(device, gpu_idx, entry);
                        self.tlas_geom_dirty = true;
                    }
                    self.skinned_blas_cache.update_pose(queue, gpu_idx, entry);
                }
            }
        }

        // Take TLAS ownership to avoid self-borrow conflicts with BLAS builds
        let mut hw_tlas = self.hw_tlas.take();

        // Instance setup: populate TLAS slots
        let mut slot = 0usize;
        if let Some(ref mut tlas) = hw_tlas {
            let mut skinned_slots = 0usize;
            for (mat_slot, inst) in instances.iter().enumerate() {
                if slot >= MAX_STATIC_RT_INSTANCES {
                    break;
                }
                if let Some(mesh_idx) = inst.mesh_idx {
                    if let Some(slot_inst) = tlas.get_mut_single(slot) {
                        if let Some(blas) = self.blas_cache.get(mesh_idx) {
                            *slot_inst = Some(TlasInstance::new(
                                blas,
                                mat4_to_tlas_transform(inst.transform),
                                mat_slot as u32,
                                0xFF,
                            ));
                            slot += 1;
                        }
                    }
                }
            }
            for (mat_slot, inst) in instances.iter().enumerate() {
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
                            mat_slot as u32,
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

        let (hw_tris, tri_bases) =
            Self::build_hw_triangle_lookup(instances, meshes, skinned_meshes);
        self.upload_hw_triangles(&hw_tris, &tri_bases, device, queue);

        // Collect all BLAS builds
        let mut blas_builds = Vec::new();
        let mut blas_build_sizes = Vec::new();
        self.blas_cache
            .build_pending(meshes, &mut blas_builds, &mut blas_build_sizes);
        let active_skinned = Self::active_skinned_gpu_indices(instances);
        self.skinned_blas_cache
            .build_updates(&active_skinned, &mut blas_builds);

        let rebuild_tlas = self.tlas_geom_dirty
            || self.tlas_instance_dirty
            || slot != self.last_tlas_slot_count;

        // Build BLAS + TLAS together in one call
        if rebuild_tlas || !blas_builds.is_empty() {
            if let Some(ref tlas) = hw_tlas {
                encoder.build_acceleration_structures(blas_builds.iter(), std::iter::once(tlas));
            } else if !blas_builds.is_empty() {
                encoder.build_acceleration_structures(blas_builds.iter(), std::iter::empty());
            }
            self.tlas_geom_dirty = false;
            self.tlas_instance_dirty = false;
            self.last_tlas_slot_count = slot;
        }

        self.hw_tlas = hw_tlas;
        self.static_scene_hash = static_hash;
        self.pose_hash = pose_hash;
    }
}
