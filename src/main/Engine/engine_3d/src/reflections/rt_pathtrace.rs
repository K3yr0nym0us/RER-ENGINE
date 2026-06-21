//! Modo PathTrace debug (referencia RTIOW, 2 rebotes acotados).

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;
use wgpu::{Device, Queue, TextureFormat, TextureView};

use crate::config_3d::reflection_graphics::ReflectionSettings;
use crate::engine::SceneUniforms;
use crate::reflections::probe_env::ProbeEnvPass;
use crate::reflections::rt_accel::RtAccel;
use crate::reflections::rt_extensions;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct RtLightUniform {
    light_dir: [f32; 4],
    light_view_proj: [[f32; 4]; 4],
    light_params: [f32; 4],
    shadow_bias: [f32; 4],
    light_color: [f32; 4],
    rt_flags: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PathUniforms {
    inv_view_proj: [[f32; 4]; 4],
    view_proj: [[f32; 4]; 4],
    cam_pos: [f32; 4],
    resolution: [f32; 2],
    node_count: u32,
    tri_count: u32,
    max_distance_m: f32,
    near_plane: f32,
    far_plane: f32,
    frame_index: u32,
    max_bounces: u32,
    material_count: u32,
    _pad: [u32; 2],
}

const _: () = assert!(std::mem::size_of::<PathUniforms>() == 192);

pub struct RtPathTracePass {
    pipeline: wgpu::ComputePipeline,
    bgl: wgpu::BindGroupLayout,
    rt_light_bgl: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
    rt_light_buffer: wgpu::Buffer,
    _texture: wgpu::Texture,
    output_view: TextureView,
    width: u32,
    height: u32,
}

impl RtPathTracePass {
    pub fn new(device: &Device, width: u32, height: u32) -> Self {
        let shader = super::load_refl_wgsl(device, "rt-pathtrace", include_str!("rt_pathtrace.wgsl"));
        let probe_bgl = ProbeEnvPass::sample_bind_group_layout(device);
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rt-pathtrace-bgl"),
            entries: &[
                uniform_entry(0),
                storage_buffer_entry(1, true),
                storage_buffer_entry(2, true),
                storage_texture_entry(3, TextureFormat::Rgba16Float, false),
                texture_entry(4, false),
                texture_entry(5, false),
                texture_entry(6, false),
                texture_entry(7, false),
                storage_buffer_entry(8, true),
            ],
        });
        let rt_light_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rt-pathtrace-light-bgl"),
            entries: &[
                shadow_depth_texture_entry(0),
                shadow_sampler_entry(1),
                uniform_entry(2),
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rt-pathtrace-pl"),
            bind_group_layouts: &[Some(&bgl), Some(&probe_bgl), Some(&rt_light_bgl)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("rt-pathtrace-pipeline"),
            layout: Some(&pl),
            module: &shader,
            entry_point: Some("cs_main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rt-pathtrace-uniforms"),
            contents: bytemuck::bytes_of(&PathUniforms {
                inv_view_proj: Mat4::IDENTITY.to_cols_array_2d(),
                view_proj: Mat4::IDENTITY.to_cols_array_2d(),
                cam_pos: [0.0; 4],
                resolution: [width.max(1) as f32, height.max(1) as f32],
                node_count: 0,
                tri_count: 0,
                max_distance_m: 80.0,
                near_plane: 0.1,
                far_plane: 1000.0,
                frame_index: 0,
                max_bounces: 2,
                material_count: 0,
                _pad: [0; 2],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let rt_light_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rt-pathtrace-light"),
            contents: bytemuck::bytes_of(&RtLightUniform {
                light_dir: [0.0, 1.0, 0.0, 1.0],
                light_view_proj: Mat4::IDENTITY.to_cols_array_2d(),
                light_params: [1.0, 0.0, 0.0, 0.0],
                shadow_bias: [0.0; 4],
                light_color: [1.0, 1.0, 1.0, 1.0],
                rt_flags: [0.0; 4],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let (texture, output_view) = create_path_texture(device, width, height);
        Self {
            pipeline,
            bgl,
            rt_light_bgl,
            uniform_buffer,
            rt_light_buffer,
            _texture: texture,
            output_view,
            width: width.max(1),
            height: height.max(1),
        }
    }

    pub fn resize(&mut self, device: &Device, width: u32, height: u32) {
        self.width = width.max(1);
        self.height = height.max(1);
        let (texture, view) = create_path_texture(device, self.width, self.height);
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
        accel: &RtAccel,
        depth_view: &TextureView,
        surface_view: &TextureView,
        direct_view: &TextureView,
        base_color_view: &TextureView,
        inv_view_proj: Mat4,
        view_proj: Mat4,
        cam_pos: Vec3,
        near_plane: f32,
        far_plane: f32,
        settings: ReflectionSettings,
        frame_index: u32,
        probe_bind_group: &wgpu::BindGroup,
        shadow_view: &TextureView,
        shadow_sampler: &wgpu::Sampler,
        scene_uniforms: &SceneUniforms,
    ) {
        if accel.node_count == 0 {
            return;
        }
        let rt_light = rt_light_from_scene(scene_uniforms, &settings);
        queue.write_buffer(&self.rt_light_buffer, 0, bytemuck::bytes_of(&rt_light));
        let uniforms = PathUniforms {
            inv_view_proj: inv_view_proj.to_cols_array_2d(),
            view_proj: view_proj.to_cols_array_2d(),
            cam_pos: [cam_pos.x, cam_pos.y, cam_pos.z, 1.0],
            resolution: [self.width as f32, self.height as f32],
            node_count: accel.node_count,
            tri_count: accel.tri_count,
            max_distance_m: settings.max_distance_m,
            near_plane,
            far_plane,
            frame_index,
            max_bounces: 2,
            material_count: accel.instance_material_count,
            _pad: [0; 2],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rt-pathtrace-bg"),
            layout: &self.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: accel.node_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: accel.tri_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&self.output_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(surface_view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(direct_view),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(base_color_view),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: accel.instance_material_buffer().as_entire_binding(),
                },
            ],
        });
        let rt_light_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rt-pathtrace-light-bg"),
            layout: &self.rt_light_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(shadow_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.rt_light_buffer.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("rt-pathtrace-pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.set_bind_group(1, probe_bind_group, &[]);
        pass.set_bind_group(2, &rt_light_bind_group, &[]);
        pass.dispatch_workgroups(
            (self.width + 7) / 8,
            (self.height + 7) / 8,
            1,
        );
    }
}

fn rt_light_from_scene(scene: &SceneUniforms, settings: &ReflectionSettings) -> RtLightUniform {
    RtLightUniform {
        light_dir: scene.light_dir,
        light_view_proj: scene.light_view_proj,
        light_params: scene.light_params,
        shadow_bias: scene.shadow_bias,
        light_color: scene.light_color,
        rt_flags: [
            if rt_extensions::dielectric_rt_enabled(settings) {
                1.0
            } else {
                0.0
            },
            if rt_extensions::rt_diffuse_gi_enabled(settings) {
                1.0
            } else {
                0.0
            },
            0.0,
            0.0,
        ],
    }
}

fn create_path_texture(device: &Device, width: u32, height: u32) -> (wgpu::Texture, TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("rt-pathtrace-output"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
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

fn shadow_depth_texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Depth,
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

fn shadow_sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
        count: None,
    }
}
