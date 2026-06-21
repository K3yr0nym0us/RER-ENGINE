//! BLAS Vulkan para mallas skinned (refit por frame).

use std::collections::HashMap;

use std::collections::HashSet;

use wgpu::util::DeviceExt;
use wgpu::{
    Blas, BlasBuildEntry, BlasGeometries, BlasGeometrySizeDescriptors, BlasTriangleGeometry,
    BlasTriangleGeometrySizeDescriptor, CommandEncoder, CreateBlasDescriptor, Device, Queue,
};

use crate::config_3d::model_animation::GpuSkinnedMeshEntry;
use crate::reflections::skinned_rt::skinned_local_positions;

pub struct SkinnedBlasCache {
    entries: HashMap<usize, SkinnedBlasEntry>,
}

struct SkinnedBlasEntry {
    blas: Blas,
    index_buffer: wgpu::Buffer,
    deformed_pos_buffer: wgpu::Buffer,
    geometry_size: BlasTriangleGeometrySizeDescriptor,
}

impl SkinnedBlasCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn contains(&self, gpu_idx: usize) -> bool {
        self.entries.contains_key(&gpu_idx)
    }

    pub fn ensure(&mut self, device: &Device, gpu_idx: usize, entry: &GpuSkinnedMeshEntry) {
        if self.entries.contains_key(&gpu_idx) {
            return;
        }
        let mesh = &entry.rt_mesh;
        let index_count = mesh.indices.len() as u32;
        if index_count == 0 || index_count % 3 != 0 || mesh.vertices.is_empty() {
            return;
        }
        let vertex_count = mesh.vertices.len() as u32;
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("rt-skinned-ibo-{gpu_idx}")),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::BLAS_INPUT,
        });
        let deformed_pos_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("rt-skinned-pos-{gpu_idx}")),
            size: (vertex_count as u64) * 12,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::BLAS_INPUT,
            mapped_at_creation: false,
        });
        let geometry_size = BlasTriangleGeometrySizeDescriptor {
            vertex_format: wgpu::VertexFormat::Float32x3,
            vertex_count,
            index_format: Some(wgpu::IndexFormat::Uint32),
            index_count: Some(index_count),
            flags: wgpu::AccelerationStructureGeometryFlags::OPAQUE,
        };
        let sizes = BlasGeometrySizeDescriptors::Triangles {
            descriptors: vec![geometry_size.clone()],
        };
        let blas = device.create_blas(
            &CreateBlasDescriptor {
                label: Some(&format!("rt-skinned-blas-{gpu_idx}")),
                flags: wgpu::AccelerationStructureFlags::PREFER_FAST_TRACE,
                update_mode: wgpu::AccelerationStructureUpdateMode::Build,
            },
            sizes,
        );
        self.entries.insert(
            gpu_idx,
            SkinnedBlasEntry {
                blas,
                index_buffer,
                deformed_pos_buffer,
                geometry_size,
            },
        );
    }

    pub fn update_pose(&self, queue: &Queue, gpu_idx: usize, entry: &GpuSkinnedMeshEntry) {
        let Some(slot) = self.entries.get(&gpu_idx) else {
            return;
        };
        let positions = skinned_local_positions(entry);
        if positions.len() != slot.geometry_size.vertex_count as usize {
            return;
        }
        queue.write_buffer(
            &slot.deformed_pos_buffer,
            0,
            bytemuck::cast_slice(&positions),
        );
    }

    pub fn build_updates(&self, encoder: &mut CommandEncoder, active: &HashSet<usize>) {
        let mut builds = Vec::new();
        for (&gpu_idx, slot) in self.entries.iter() {
            if !active.contains(&gpu_idx) {
                continue;
            }
            builds.push(BlasBuildEntry {
                blas: &slot.blas,
                geometry: BlasGeometries::TriangleGeometries(vec![BlasTriangleGeometry {
                    size: &slot.geometry_size,
                    vertex_buffer: &slot.deformed_pos_buffer,
                    first_vertex: 0,
                    vertex_stride: 12,
                    index_buffer: Some(&slot.index_buffer),
                    first_index: Some(0),
                    transform_buffer: None,
                    transform_buffer_offset: None,
                }]),
            });
        }
        if !builds.is_empty() {
            encoder.build_acceleration_structures(builds.iter(), std::iter::empty());
        }
    }

    pub fn get(&self, gpu_idx: usize) -> Option<&Blas> {
        self.entries.get(&gpu_idx).map(|e| &e.blas)
    }
}
