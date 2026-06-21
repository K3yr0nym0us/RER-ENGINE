//! BLAS por malla (wgpu acceleration structure) para RT hardware.

use std::collections::HashMap;

use wgpu::{
    Blas, BlasBuildEntry, BlasGeometries, BlasGeometrySizeDescriptors, BlasTriangleGeometry,
    BlasTriangleGeometrySizeDescriptor, CommandEncoder, CreateBlasDescriptor, Device,
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

    pub fn build_pending(&mut self, encoder: &mut CommandEncoder, meshes: &[Mesh]) {
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
        let mut builds = Vec::new();
        for (mesh_idx, size_desc, uses_rt_ibo) in &pending_sizes {
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
            builds.push(BlasBuildEntry {
                blas: &entry.blas,
                geometry: BlasGeometries::TriangleGeometries(vec![BlasTriangleGeometry {
                    size: size_desc,
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
        if !builds.is_empty() {
            encoder.build_acceleration_structures(builds.iter(), std::iter::empty());
        }
        self.pending_build.clear();
    }

    pub fn get(&self, mesh_idx: usize) -> Option<&Blas> {
        self.entries.get(&mesh_idx).map(|e| &e.blas)
    }
}
