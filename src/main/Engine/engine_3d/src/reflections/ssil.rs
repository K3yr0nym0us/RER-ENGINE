//! SSIL acotado (indirecto difuso screen-space) — tier Ultra.

use bytemuck::{Pod, Zeroable};
use glam::Mat4;
use wgpu::util::DeviceExt;
use wgpu::{Device, Queue, TextureFormat, TextureView};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SsilUniforms {
    inv_view_proj: [[f32; 4]; 4],
    view_proj: [[f32; 4]; 4],
    cam_pos: [f32; 4],
    resolution: [f32; 2],
    near_plane: f32,
    far_plane: f32,
    sample_radius: f32,
    strength: f32,
    depth_reject_m: f32,
    _pad: u32,
}

pub struct SsilPass {
    pipeline: wgpu::ComputePipeline,
    bgl: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
    _texture: wgpu::Texture,
    output_view: TextureView,
    width: u32,
    height: u32,
}

impl SsilPass {
    pub fn new(device: &Device, color_format: TextureFormat, width: u32, height: u32) -> Self {
        let _ = color_format;
        let shader = super::load_refl_wgsl(device, "ssil", include_str!("ssil.wgsl"));
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ssil-bgl"),
            entries: &[
                bgl_uniform(0),
                bgl_texture(1, false),
                bgl_texture(2, true),
                bgl_texture(3, true),
                bgl_texture(4, false),
                bgl_texture(5, false),
                bgl_storage(6, TextureFormat::Rgba8Unorm),
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ssil-pl"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("ssil-pipeline"),
            layout: Some(&pl),
            module: &shader,
            entry_point: Some("cs_main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ssil-uniforms"),
            contents: bytemuck::bytes_of(&SsilUniforms {
                inv_view_proj: Mat4::IDENTITY.to_cols_array_2d(),
                view_proj: Mat4::IDENTITY.to_cols_array_2d(),
                cam_pos: [0.0; 4],
                resolution: [width.max(1) as f32, height.max(1) as f32],
                near_plane: 0.1,
                far_plane: 1000.0,
                sample_radius: 14.0,
                strength: 0.18,
                depth_reject_m: 0.12,
                _pad: 0,
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let (texture, output_view) = create_ssil_texture(device, width, height);
        Self {
            pipeline,
            bgl,
            uniform_buffer,
            _texture: texture,
            output_view,
            width: width.max(1),
            height: height.max(1),
        }
    }

    pub fn resize(&mut self, device: &Device, width: u32, height: u32) {
        self.width = width.max(1);
        self.height = height.max(1);
        let (texture, view) = create_ssil_texture(device, self.width, self.height);
        self._texture = texture;
        self.output_view = view;
    }

    pub fn output_view(&self) -> &TextureView {
        &self.output_view
    }

    #[allow(clippy::too_many_arguments)]
    pub fn dispatch(
        &self,
        device: &Device,
        queue: &Queue,
        encoder: &mut wgpu::CommandEncoder,
        depth_view: &TextureView,
        normal_roughness_view: &TextureView,
        lit_scene_view: &TextureView,
        direct_view: &TextureView,
        surface_view: &TextureView,
        inv_view_proj: Mat4,
        view_proj: Mat4,
        cam_pos: glam::Vec3,
        near_plane: f32,
        far_plane: f32,
    ) {
        let uniforms = SsilUniforms {
            inv_view_proj: inv_view_proj.to_cols_array_2d(),
            view_proj: view_proj.to_cols_array_2d(),
            cam_pos: [cam_pos.x, cam_pos.y, cam_pos.z, 1.0],
            resolution: [self.width as f32, self.height as f32],
            near_plane,
            far_plane,
            sample_radius: 14.0,
            strength: 0.18,
            depth_reject_m: 0.12,
            _pad: 0,
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ssil-bg"),
            layout: &self.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(normal_roughness_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(lit_scene_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(direct_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(surface_view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&self.output_view),
                },
            ],
        });
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("ssil-pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(
            (self.width + 7) / 8,
            (self.height + 7) / 8,
            1,
        );
    }
}

fn create_ssil_texture(device: &Device, width: u32, height: u32) -> (wgpu::Texture, TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("ssil-output"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn bgl_uniform(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn bgl_texture(binding: u32, filterable: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn bgl_storage(binding: u32, format: TextureFormat) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::StorageTexture {
            access: wgpu::StorageTextureAccess::WriteOnly,
            format,
            view_dimension: wgpu::TextureViewDimension::D2,
        },
        count: None,
    }
}
