//! RT reflexiones v1: compute contra TLAS de AABB estáticos (sin BLAS por triángulo).
//! Skinned: excluidos del TLAS; contribuyen vía SSR.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;
use wgpu::{Device, Queue, TextureFormat, TextureView};

use crate::config_3d::reflection_graphics::ReflectionSettings;
use crate::reflections::accel::{StaticInstanceGpu, MAX_STATIC_REFLECTION_INSTANCES};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct RtUniforms {
    inv_view_proj: [[f32; 4]; 4],
    view_proj: [[f32; 4]; 4],
    cam_pos: [f32; 4],
    resolution: [f32; 2],
    instance_count: u32,
    max_distance_m: f32,
    max_roughness: f32,
    rt_blend: f32,
    step_size: f32,
    near_plane: f32,
    far_plane: f32,
    /// Padding WGSL: tamaño de struct múltiplo de 16 (192 B).
    _struct_pad: [f32; 3],
}

const _: () = assert!(std::mem::size_of::<RtUniforms>() == 192);

pub struct RtReflectionPass {
    pipeline: wgpu::ComputePipeline,
    bgl: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    nearest_sampler: wgpu::Sampler,
}

impl RtReflectionPass {
    pub fn new(device: &Device, color_format: TextureFormat, width: u32, height: u32) -> Self {
        let nearest_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("rt-refl-nearest"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let shader = super::load_refl_wgsl(device, "rt-refl", include_str!("rt_reflections.wgsl"));

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rt-refl-bgl"),
            entries: &[
                uniform_entry(0),
                storage_buffer_entry(1, true),
                storage_texture_entry(2, color_format, false),
                depth_texture_entry(3),
                texture_entry(4, true),
                texture_entry(5, true),
                texture_entry(6, true),
                sampler_entry(7, false),
                texture_entry(8, false),
                texture_entry(9, false),
                texture_entry(10, false),
            ],
        });

        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rt-refl-pl"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("rt-refl-pipeline"),
            layout: Some(&pl),
            module: &shader,
            entry_point: Some("cs_main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rt-refl-uniforms"),
            contents: bytemuck::bytes_of(&RtUniforms {
                inv_view_proj: Mat4::IDENTITY.to_cols_array_2d(),
                view_proj: Mat4::IDENTITY.to_cols_array_2d(),
                cam_pos: [0.0; 4],
                resolution: [width.max(1) as f32, height.max(1) as f32],
                instance_count: 0,
                max_distance_m: 40.0,
                max_roughness: 0.88,
                rt_blend: 0.85,
                step_size: 0.15,
                near_plane: 0.1,
                far_plane: 1000.0,
                _struct_pad: [0.0; 3],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rt-refl-instances"),
            size: (MAX_STATIC_REFLECTION_INSTANCES * std::mem::size_of::<StaticInstanceGpu>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = make_bind_group(
            device,
            &bgl,
            &uniform_buffer,
            &instance_buffer,
            &dummy_texture_view(device, color_format, width, height),
            &dummy_texture_view(device, color_format, width, height),
            &dummy_depth_view(device, width, height),
            &dummy_texture_view(device, color_format, width, height),
            &dummy_texture_view(device, color_format, width, height),
            &dummy_texture_view(device, color_format, width, height),
            &dummy_texture_view(device, color_format, width, height),
            &dummy_texture_view(device, color_format, width, height),
            &nearest_sampler,
        );

        Self {
            pipeline,
            bgl,
            uniform_buffer,
            instance_buffer,
            bind_group,
            nearest_sampler,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn dispatch(
        &mut self,
        device: &Device,
        queue: &Queue,
        encoder: &mut wgpu::CommandEncoder,
        instances: &[StaticInstanceGpu],
        ssr_view: &TextureView,
        output_view: &TextureView,
        depth_view: &TextureView,
        normal_roughness_view: &TextureView,
        ambient_view: &TextureView,
        direct_view: &TextureView,
        surface_view: &TextureView,
        base_color_view: &TextureView,
        inv_view_proj: Mat4,
        view_proj: Mat4,
        cam_pos: Vec3,
        near_plane: f32,
        far_plane: f32,
        settings: ReflectionSettings,
        width: u32,
        height: u32,
    ) {
        if instances.is_empty() {
            return;
        }

        log::debug!(
            "[reflexiones] RT estáticos: {} instancias AABB",
            instances.len().min(MAX_STATIC_REFLECTION_INSTANCES)
        );

        let count = instances.len().min(MAX_STATIC_REFLECTION_INSTANCES);
        queue.write_buffer(
            &self.instance_buffer,
            0,
            bytemuck::cast_slice(&instances[..count]),
        );

        let step_size = (settings.max_distance_m / settings.max_steps.max(1) as f32)
            .clamp(0.05, 0.25);

        let uniforms = RtUniforms {
            inv_view_proj: inv_view_proj.to_cols_array_2d(),
            view_proj: view_proj.to_cols_array_2d(),
            cam_pos: [cam_pos.x, cam_pos.y, cam_pos.z, 1.0],
            resolution: [width.max(1) as f32, height.max(1) as f32],
            instance_count: count as u32,
            max_distance_m: settings.max_distance_m,
            max_roughness: settings.max_roughness_to_trace,
            rt_blend: settings.rt_blend,
            step_size,
            near_plane,
            far_plane,
            _struct_pad: [0.0; 3],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        self.bind_group = make_bind_group(
            device,
            &self.bgl,
            &self.uniform_buffer,
            &self.instance_buffer,
            output_view,
            ssr_view,
            depth_view,
            normal_roughness_view,
            ambient_view,
            direct_view,
            surface_view,
            base_color_view,
            &self.nearest_sampler,
        );

        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("rt-refl-pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        let wg_x = (width + 7) / 8;
        let wg_y = (height + 7) / 8;
        pass.dispatch_workgroups(wg_x, wg_y, 1);
    }
}

pub fn adapter_supports_rt(device: &wgpu::Device) -> bool {
    // v2: el compute AABB no sustituye VK_KHR_ray_tracing_pipeline (ver rust-raytracing).
    // Hasta integrar BLAS/TLAS + raygen/closesthit en wgpu, desactivado para evitar grano.
    let _ = device;
    false
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: std::num::NonZeroU64::new(std::mem::size_of::<RtUniforms>() as u64),
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

fn storage_texture_entry(
    binding: u32,
    format: TextureFormat,
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
            sample_type: wgpu::TextureSampleType::Float {
                filterable,
            },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn sampler_entry(binding: u32, linear: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Sampler(if linear {
            wgpu::SamplerBindingType::Filtering
        } else {
            wgpu::SamplerBindingType::NonFiltering
        }),
        count: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn make_bind_group(
    device: &Device,
    layout: &wgpu::BindGroupLayout,
    uniforms: &wgpu::Buffer,
    instances: &wgpu::Buffer,
    output: &TextureView,
    ssr: &TextureView,
    depth: &TextureView,
    normal_roughness: &TextureView,
    ambient: &TextureView,
    direct: &TextureView,
    surface: &TextureView,
    base_color: &TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("rt-refl-bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: instances.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(output),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(depth),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(normal_roughness),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(ambient),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::TextureView(direct),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 8,
                resource: wgpu::BindingResource::TextureView(ssr),
            },
            wgpu::BindGroupEntry {
                binding: 9,
                resource: wgpu::BindingResource::TextureView(surface),
            },
            wgpu::BindGroupEntry {
                binding: 10,
                resource: wgpu::BindingResource::TextureView(base_color),
            },
        ],
    })
}

fn dummy_texture_view(
    device: &Device,
    format: TextureFormat,
    width: u32,
    height: u32,
) -> TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("rt-refl-dummy-color"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

fn dummy_depth_view(device: &Device, width: u32, height: u32) -> TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("rt-refl-dummy-depth"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}
