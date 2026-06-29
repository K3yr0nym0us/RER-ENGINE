//! BLAS por malla (wgpu acceleration structure) para RT hardware.

use std::collections::HashMap;

use wgpu::{
    Blas, BlasBuildEntry, BlasGeometries, BlasGeometrySizeDescriptors, BlasTriangleGeometry,
    BlasTriangleGeometrySizeDescriptor, CreateBlasDescriptor, Device,
};

use crate::mesh::Mesh;

pub struct BlasCache {
    entries: HashMap<usize, BlasEntry>,
    pending_build: Vec<usize>,
}

struct BlasEntry {
    blas: Blas,
    rt_index_count: u32,
    uses_rt_ibo: bool,
}

impl BlasCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            pending_build: Vec::new(),
        }
    }

    pub fn contains(&self, mesh_idx: usize) -> bool {
        self.entries.contains_key(&mesh_idx)
    }

    pub fn ensure(&mut self, device: &Device, mesh_idx: usize, mesh: &Mesh) -> Option<&Blas> {
        let rt_count = mesh.rt_index_count();
        if rt_count == 0 || rt_count % 3 != 0 || mesh.rt_indices.is_empty() {
            return None;
        }
        let uses_rt_ibo = mesh.rt_index_buffer.is_some();
        if !self.entries.contains_key(&mesh_idx) {
            let sizes = BlasGeometrySizeDescriptors::Triangles {
                descriptors: vec![BlasTriangleGeometrySizeDescriptor {
                    vertex_format: wgpu::VertexFormat::Float32x3,
                    vertex_count: mesh.rt_positions.len() as u32,
                    index_format: Some(wgpu::IndexFormat::Uint32),
                    index_count: Some(rt_count),
                    flags: wgpu::AccelerationStructureGeometryFlags::OPAQUE,
                }],
            };
            let blas = device.create_blas(
                &CreateBlasDescriptor {
                    label: Some(&format!("rt-blas-{mesh_idx}")),
                    flags: wgpu::AccelerationStructureFlags::PREFER_FAST_TRACE,
                    update_mode: wgpu::AccelerationStructureUpdateMode::Build,
                },
                sizes,
            );
            self.entries.insert(
                mesh_idx,
                BlasEntry {
                    blas,
                    rt_index_count: rt_count,
                    uses_rt_ibo,
                },
            );
            self.pending_build.push(mesh_idx);
        }
        self.entries.get(&mesh_idx).map(|e| &e.blas)
    }

    pub fn build_pending<'a>(
        &'a mut self,
        meshes: &'a [Mesh],
        out: &mut Vec<BlasBuildEntry<'a>>,
        sizes: &'a mut Vec<BlasTriangleGeometrySizeDescriptor>,
    ) {
        if self.pending_build.is_empty() {
            return;
        }
        let mut pending_sizes: Vec<(usize, BlasTriangleGeometrySizeDescriptor, bool)> = Vec::new();
        for &mesh_idx in &self.pending_build {
            let Some(entry) = self.entries.get(&mesh_idx) else {
                continue;
            };
            let Some(mesh) = meshes.get(mesh_idx) else {
                continue;
            };
            if entry.rt_index_count % 3 != 0 {
                continue;
            }
            pending_sizes.push((
                mesh_idx,
                BlasTriangleGeometrySizeDescriptor {
                    vertex_format: wgpu::VertexFormat::Float32x3,
                    vertex_count: mesh.rt_positions.len() as u32,
                    index_format: Some(wgpu::IndexFormat::Uint32),
                    index_count: Some(entry.rt_index_count),
                    flags: wgpu::AccelerationStructureGeometryFlags::OPAQUE,
                },
                entry.uses_rt_ibo,
            ));
        }
        // First pass: push all size descriptors into the output vec (mutable access only)
        let mut build_params: Vec<(usize, bool)> = Vec::new();
        for (mesh_idx, size_desc, uses_rt_ibo) in pending_sizes {
            sizes.push(size_desc);
            build_params.push((mesh_idx, uses_rt_ibo));
        }
        // Second pass: create builds referencing sizes (immutable access only)
        let base = sizes.len() - build_params.len();
        for (i, (mesh_idx, uses_rt_ibo)) in build_params.iter().enumerate() {
            let Some(entry) = self.entries.get(mesh_idx) else {
                continue;
            };
            let Some(mesh) = meshes.get(*mesh_idx) else {
                continue;
            };
            let ibo = if *uses_rt_ibo {
                mesh.rt_index_buffer()
            } else {
                &mesh.index_buffer
            };
            let sd = &sizes[base + i];
            out.push(BlasBuildEntry {
                blas: &entry.blas,
                geometry: BlasGeometries::TriangleGeometries(vec![BlasTriangleGeometry {
                    size: sd,
                    vertex_buffer: &mesh.vertex_buffer,
                    first_vertex: 0,
                    vertex_stride: std::mem::size_of::<crate::mesh::Vertex>() as wgpu::BufferAddress,
                    index_buffer: Some(ibo),
                    first_index: Some(0),
                    transform_buffer: None,
                    transform_buffer_offset: None,
                }]),
            });
        }
        self.pending_build.clear();
    }

    pub fn get(&self, mesh_idx: usize) -> Option<&Blas> {
        self.entries.get(&mesh_idx).map(|e| &e.blas)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.pending_build.clear();
    }
}
