//! Dispatch disperso RT v2.1: máscara de tiles 16×16 + indirect dispatch.

use bytemuck::{Pod, Zeroable};
use wgpu::{Device, Queue, TextureView};

pub const RT_TILE_SIZE: u32 = 16;
pub const MAX_RT_TILES: u32 = 4096;
const TILE_THRESHOLD: f32 = 0.05;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct MaskUniforms {
    resolution: [f32; 2],
    max_roughness: f32,
    threshold: f32,
    tiles_x: u32,
    tiles_y: u32,
    gbuffer_scale: f32,
    _pad: f32,
}

const _: () = assert!(std::mem::size_of::<MaskUniforms>() == 32);

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct DispatchIndirectArgs {
    x: u32,
    y: u32,
    z: u32,
}

pub struct RtSparseDispatch {
    mask_pipeline: wgpu::ComputePipeline,
    prepare_pipeline: wgpu::ComputePipeline,
    copy_ssr_pipeline: wgpu::ComputePipeline,
    mask_bgl: wgpu::BindGroupLayout,
    prepare_bgl: wgpu::BindGroupLayout,
    copy_bgl: wgpu::BindGroupLayout,
    mask_uniform_buffer: wgpu::Buffer,
    tile_count_buffer: wgpu::Buffer,
    tile_list_buffer: wgpu::Buffer,
    indirect_buffer: wgpu::Buffer,
    width: u32,
    height: u32,
}

impl RtSparseDispatch {
    pub fn new(device: &Device, color_format: wgpu::TextureFormat) -> Self {
        let mask_shader = crate::reflections::load_refl_wgsl(device, "rt-tile-mask", include_str!("rt_tile_mask.wgsl"));
        let prepare_shader =
            crate::reflections::load_refl_wgsl(device, "rt-prepare-indirect", include_str!("rt_prepare_indirect.wgsl"));
        let copy_shader = crate::reflections::load_refl_wgsl(device, "rt-copy-ssr", include_str!("rt_copy_ssr.wgsl"));

        let mask_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rt-mask-bgl"),
            entries: &[
                uniform_entry(0),
                storage_buffer_entry(1, false),
                storage_buffer_entry(2, false),
                texture_entry(3, false),
                depth_texture_entry(4),
                texture_entry(5, true),
                texture_entry(6, true),
                texture_entry(7, false),
            ],
        });
        let prepare_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rt-prepare-indirect-bgl"),
            entries: &[
                storage_buffer_entry(0, false),
                storage_buffer_entry(1, false),
            ],
        });
        let copy_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rt-copy-ssr-bgl"),
            entries: &[
                texture_entry(0, false),
                storage_texture_entry(1, color_format, false),
            ],
        });

        let mask_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rt-mask-pl"),
            bind_group_layouts: &[Some(&mask_bgl)],
            immediate_size: 0,
        });
        let prepare_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rt-prepare-pl"),
            bind_group_layouts: &[Some(&prepare_bgl)],
            immediate_size: 0,
        });
        let copy_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rt-copy-pl"),
            bind_group_layouts: &[Some(&copy_bgl)],
            immediate_size: 0,
        });

        let mask_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("rt-mask-pipeline"),
            layout: Some(&mask_pl),
            module: &mask_shader,
            entry_point: Some("cs_build_mask"),
            compilation_options: Default::default(),
            cache: None,
        });
        let prepare_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("rt-prepare-indirect-pipeline"),
            layout: Some(&prepare_pl),
            module: &prepare_shader,
            entry_point: Some("cs_prepare_indirect"),
            compilation_options: Default::default(),
            cache: None,
        });
        let copy_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("rt-copy-ssr-pipeline"),
            layout: Some(&copy_pl),
            module: &copy_shader,
            entry_point: Some("cs_main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let tile_count_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rt-tile-count"),
            size: 256,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::INDIRECT,
            mapped_at_creation: false,
        });
        let tile_list_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rt-tile-list"),
            size: (MAX_RT_TILES * 2 * std::mem::size_of::<u32>() as u32) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let indirect_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rt-indirect-dispatch"),
            size: std::mem::size_of::<DispatchIndirectArgs>() as u64,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::INDIRECT
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mask_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rt-mask-uniforms"),
            size: std::mem::size_of::<MaskUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            mask_pipeline,
            prepare_pipeline,
            copy_ssr_pipeline: copy_pipeline,
            mask_bgl,
            prepare_bgl,
            copy_bgl,
            mask_uniform_buffer,
            tile_count_buffer,
            tile_list_buffer,
            indirect_buffer,
            width: 1,
            height: 1,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width.max(1);
        self.height = height.max(1);
    }

    pub fn tile_list_buffer(&self) -> &wgpu::Buffer {
        &self.tile_list_buffer
    }

    pub fn tile_count_buffer(&self) -> &wgpu::Buffer {
        &self.tile_count_buffer
    }

    pub fn indirect_buffer(&self) -> &wgpu::Buffer {
        &self.indirect_buffer
    }

    pub fn prepare_frame(
        &self,
        device: &Device,
        queue: &Queue,
        encoder: &mut wgpu::CommandEncoder,
        ssr_view: &TextureView,
        output_view: &TextureView,
        depth_view: &TextureView,
        normal_roughness_view: &TextureView,
        surface_view: &TextureView,
        direct_view: &TextureView,
        max_roughness: f32,
        gbuffer_scale: f32,
    ) {
        queue.write_buffer(&self.tile_count_buffer, 0, bytemuck::bytes_of(&0u32));

        let tiles_x = (self.width + RT_TILE_SIZE - 1) / RT_TILE_SIZE;
        let tiles_y = (self.height + RT_TILE_SIZE - 1) / RT_TILE_SIZE;
        let mask_u = MaskUniforms {
            resolution: [self.width as f32, self.height as f32],
            max_roughness,
            threshold: TILE_THRESHOLD,
            tiles_x,
            tiles_y,
            gbuffer_scale,
            _pad: 0.0,
        };
        queue.write_buffer(&self.mask_uniform_buffer, 0, bytemuck::bytes_of(&mask_u));

        let copy_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rt-copy-ssr-bg"),
            layout: &self.copy_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(ssr_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(output_view),
                },
            ],
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("rt-copy-ssr-pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.copy_ssr_pipeline);
            pass.set_bind_group(0, &copy_bg, &[]);
            pass.dispatch_workgroups(
                (self.width + 7) / 8,
                (self.height + 7) / 8,
                1,
            );
        }

        let mask_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rt-mask-bg"),
            layout: &self.mask_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.mask_uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.tile_count_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.tile_list_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(ssr_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(normal_roughness_view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(surface_view),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(direct_view),
                },
            ],
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("rt-tile-mask-pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.mask_pipeline);
            pass.set_bind_group(0, &mask_bg, &[]);
            pass.dispatch_workgroups(tiles_x, tiles_y, 1);
        }

        let prepare_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rt-prepare-indirect-bg"),
            layout: &self.prepare_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.tile_count_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.indirect_buffer.as_entire_binding(),
                },
            ],
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("rt-prepare-indirect-pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.prepare_pipeline);
            pass.set_bind_group(0, &prepare_bg, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
    }
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
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

fn storage_buffer_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: if read_only {
                wgpu::BufferBindingType::Storage { read_only: true }
            } else {
                wgpu::BufferBindingType::Storage { read_only: false }
            },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn depth_texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn texture_entry(binding: u32, filterable: bool) -> wgpu::BindGroupLayoutEntry {
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

fn storage_texture_entry(
    binding: u32,
    format: wgpu::TextureFormat,
    read_write: bool,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::StorageTexture {
            access: if read_write {
                wgpu::StorageTextureAccess::ReadWrite
            } else {
                wgpu::StorageTextureAccess::WriteOnly
            },
            format,
            view_dimension: wgpu::TextureViewDimension::D2,
        },
        count: None,
    }
}
