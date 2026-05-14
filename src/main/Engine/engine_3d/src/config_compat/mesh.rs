use wgpu::util::DeviceExt;

use crate::gizmo;

pub struct GridConfig {
    pub world_width: f32,
    pub world_height: f32,
    pub visible: bool,
    pub cell_size: f32,
}

impl Default for GridConfig {
    fn default() -> Self {
        Self {
            world_width: 100.0,
            world_height: 50.0,
            visible: false,
            cell_size: 1.0,
        }
    }
}

pub struct GridBuffer {
    pub vertex_buffer: wgpu::Buffer,
    pub vertex_count: u32,
}

pub fn build_grid(device: &wgpu::Device, _config: &GridConfig) -> GridBuffer {
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("grid-vbuf-stub"),
        contents: bytemuck::cast_slice(&[gizmo::GizmoVertex {
            position: [0.0, 0.0, 0.0],
            color: [0.0, 0.0, 0.0, 0.0],
        }]),
        usage: wgpu::BufferUsages::VERTEX,
    });
    GridBuffer {
        vertex_buffer,
        vertex_count: 0,
    }
}
