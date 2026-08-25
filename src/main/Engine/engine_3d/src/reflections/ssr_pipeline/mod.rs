//! Screen-space reflections (SSR).

pub(crate) mod ssr_settings;
mod stats_readback;

use bytemuck::{Pod, Zeroable};
use glam::Mat4;
use wgpu::util::DeviceExt;
use wgpu::{Device, Queue, TextureFormat, TextureView};

use crate::config_3d::reflection_graphics::ReflectionSettings;
use crate::reflections::load_refl_wgsl;

pub use stats_readback::{SsrDebugLogSnapshot, SsrStatsReadback};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct SsrUniforms {
    resolution: [f32; 2],
    gb_resolution: [f32; 2],
    inv_view_proj: [[f32; 4]; 4],
    view_proj: [[f32; 4]; 4],
    view: [[f32; 4]; 4],
    inv_view: [[f32; 4]; 4],
    near_plane: f32,
    far_plane: f32,
    max_distance_m: f32,
    coarse_resolution: f32,
    thickness_m: f32,
    max_roughness: f32,
    binary_steps: u32,
    coarse_max_iters: u32,
    gbuffer_scale: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
}

/// Near para escala de grosor en marcha SSR (`depth_thickness / near`).
/// Lee `clip_from_view[3][2]`; en glam RH GL suele ser −1 → usamos `camera.near`.
fn ssr_march_near_plane(view_proj: Mat4, view: Mat4, camera_near: f32) -> f32 {
    let clip_from_view = view_proj * view.inverse();
    let clip_near_z = clip_from_view.w_axis.z.abs();
    if clip_near_z > 0.01 && (clip_near_z - camera_near).abs() < camera_near * 0.25 {
        clip_near_z
    } else {
        camera_near
    }
}

pub struct SsrPipeline {
    pub uniform_buffer: wgpu::Buffer,
    pipeline: wgpu::RenderPipeline,
    bgl: wgpu::BindGroupLayout,
    linear_sampler: wgpu::Sampler,
    nearest_sampler: wgpu::Sampler,
}

impl SsrPipeline {
    pub fn new(
        device: &Device,
        color_format: TextureFormat,
        width: u32,
        height: u32,
        probe_sample_bgl: &wgpu::BindGroupLayout,
    ) -> Self {
        let ssr_source = format!(
            "{}\n{}",
            include_str!("ssr.wgsl"),
            include_str!("ssr_raymarch.wgsl"),
        );
        let shader = load_refl_wgsl(device, "ssr", &ssr_source);

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ssr-bgl"),
            entries: &[
                bgl_uniform(0, wgpu::ShaderStages::FRAGMENT),
                bgl_texture(1, false, wgpu::ShaderStages::FRAGMENT),
                bgl_texture(2, true, wgpu::ShaderStages::FRAGMENT),
                bgl_texture(3, true, wgpu::ShaderStages::FRAGMENT),
                bgl_sampler(4, true, wgpu::ShaderStages::FRAGMENT),
                bgl_sampler(5, false, wgpu::ShaderStages::FRAGMENT),
                bgl_texture(6, false, wgpu::ShaderStages::FRAGMENT),
                bgl_texture(7, false, wgpu::ShaderStages::FRAGMENT),
                bgl_texture(8, false, wgpu::ShaderStages::FRAGMENT),
                bgl_texture(9, false, wgpu::ShaderStages::FRAGMENT),
                bgl_texture(10, false, wgpu::ShaderStages::FRAGMENT),
            ],
        });

        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ssr-pl"),
            bind_group_layouts: &[Some(&bgl), Some(probe_sample_bgl)],
            immediate_size: 0,
        });

        let color_target = Some(wgpu::ColorTargetState {
            format: color_format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        });
        let hit_uv_target = Some(wgpu::ColorTargetState {
            format: wgpu::TextureFormat::Rg16Float,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ssr-pipeline"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[color_target, hit_uv_target],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ssr-linear"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let nearest_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ssr-nearest"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ssr-uniforms"),
            contents: bytemuck::bytes_of(&SsrUniforms {
                resolution: [width.max(1) as f32, height.max(1) as f32],
                gb_resolution: [width.max(1) as f32, height.max(1) as f32],
                inv_view_proj: Mat4::IDENTITY.to_cols_array_2d(),
                view_proj: Mat4::IDENTITY.to_cols_array_2d(),
                view: Mat4::IDENTITY.to_cols_array_2d(),
                inv_view: Mat4::IDENTITY.to_cols_array_2d(),
                near_plane: 0.1,
                far_plane: 1000.0,
                max_distance_m: 50.0,
                coarse_resolution: 0.5,
                thickness_m: 0.3,
                max_roughness: 0.7,
                binary_steps: 8,
                coarse_max_iters: 32,
                gbuffer_scale: 1.0,
                _pad0: 0.0,
                _pad1: 0.0,
                _pad2: 0.0,
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        Self {
            uniform_buffer,
            pipeline,
            bgl,
            linear_sampler,
            nearest_sampler,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn run(
        &self,
        device: &Device,
        queue: &Queue,
        encoder: &mut wgpu::CommandEncoder,
        settings: &ReflectionSettings,
        refl_width: u32,
        refl_height: u32,
        viewport_w: u32,
        viewport_h: u32,
        gbuffer_scale: f32,
        _frame_index: u32,
        depth_view: &TextureView,
        normal_roughness_view: &TextureView,
        lit_scene_view: &TextureView,
        direct_view: &TextureView,
        ambient_view: &TextureView,
        surface_view: &TextureView,
        base_color_view: &TextureView,
        world_pos_view: &TextureView,
        reflection_view: &TextureView,
        hit_uv_view: &TextureView,
        inv_view_proj: Mat4,
        view_proj: Mat4,
        view: Mat4,
        near_plane: f32,
        far_plane: f32,
        _clear_color: wgpu::Color,
        probe_bind_group: &wgpu::BindGroup,
    ) {
        let inv_view = view.inverse();
        let march_near = ssr_march_near_plane(view_proj, view, near_plane);
        let uniforms = SsrUniforms {
            resolution: [refl_width.max(1) as f32, refl_height.max(1) as f32],
            gb_resolution: [viewport_w.max(1) as f32, viewport_h.max(1) as f32],
            inv_view_proj: inv_view_proj.to_cols_array_2d(),
            view_proj: view_proj.to_cols_array_2d(),
            view: view.to_cols_array_2d(),
            inv_view: inv_view.to_cols_array_2d(),
            near_plane: march_near,
            far_plane,
            max_distance_m: settings.max_distance_m,
            coarse_resolution: ssr_settings::ssr_coarse_resolution(settings.tier),
            thickness_m: ssr_settings::ssr_thickness_m(settings.tier),
            max_roughness: settings.max_roughness_to_trace,
            binary_steps: settings.ssr_binary_steps(),
            coarse_max_iters: settings.ssr_coarse_max_iters(),
            gbuffer_scale,
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ssr-bg"),
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
                    resource: wgpu::BindingResource::Sampler(&self.linear_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&self.nearest_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(direct_view),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(surface_view),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::TextureView(base_color_view),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: wgpu::BindingResource::TextureView(ambient_view),
                },
                wgpu::BindGroupEntry {
                    binding: 10,
                    resource: wgpu::BindingResource::TextureView(world_pos_view),
                },
            ],
        });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ssr-pass"),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment {
                        view: reflection_view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: hit_uv_view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                ],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.set_bind_group(1, probe_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}

fn bgl_uniform(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn bgl_texture(
    binding: u32,
    filterable: bool,
    visibility: wgpu::ShaderStages,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn bgl_sampler(
    binding: u32,
    linear: bool,
    visibility: wgpu::ShaderStages,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Sampler(if linear {
            wgpu::SamplerBindingType::Filtering
        } else {
            wgpu::SamplerBindingType::NonFiltering
        }),
        count: None,
    }
}
