//! Reflejos: SSR, acumulación temporal, composite y debug views.

pub(crate) mod probe_env;
pub(crate) mod policy;
pub(crate) mod probes_pipeline;
pub(crate) mod rt_pipeline;
pub(crate) mod ssr_pipeline;
pub(crate) mod frame;
pub(crate) mod quality_preset;
pub(crate) mod settings;
pub(crate) mod ssil;

use rt_pipeline::rt_accel::RtAccel;
use rt_pipeline::rt_reflections_v2::RtReflectionPassV2;

use std::time::Instant;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;
use wgpu::{include_wgsl, Device, Queue, TextureFormat, TextureView};

use crate::config_3d::reflection_graphics::{
    ReflectionDebugView, ReflectionProfilerMs, ReflectionSettings,
};
use crate::engine::SceneUniforms;
use crate::reflections::ssil::SsilPass;

const _: () = assert!(std::mem::size_of::<TemporalUniforms>() == 32);
const _: () = assert!(std::mem::size_of::<DenoiseUniforms>() == 32);
const _: () = assert!(std::mem::size_of::<CompositeUniforms>() == 16);

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TemporalUniforms {
    resolution: [f32; 2],
    blend: f32,
    enabled: f32,
    depth_reject_m: f32,
    gbuffer_scale: f32,
    near_plane: f32,
    far_plane: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct DenoiseUniforms {
    resolution: [f32; 2],
    depth_sigma: f32,
    normal_sigma: f32,
    luminance_sigma: f32,
    gbuffer_scale: f32,
    radius: f32,
    _pad: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CompositeUniforms {
    strength: f32,
    ssil_strength: f32,
    refl_mix: f32,
    _pad2: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct DebugUniforms {
    mode: u32,
    max_roughness: f32,
    near_plane: f32,
    far_plane: f32,
    cam_pos: [f32; 4],
    inv_view_proj: [[f32; 4]; 4],
    view_proj: [[f32; 4]; 4],
    view: [[f32; 4]; 4],
    inv_view: [[f32; 4]; 4],
    resolution: [f32; 2],
    gb_resolution: [f32; 2],
    max_distance_m: f32,
    coarse_resolution: f32,
    thickness_m: f32,
    binary_steps: u32,
    coarse_max_iters: u32,
    ssr_blur_enabled: f32,
    _pad: [f32; 2],
}

const _: () = assert!(std::mem::size_of::<DebugUniforms>() == 336);
const _: () = assert!(std::mem::size_of::<DebugUniforms>() % 16 == 0);

pub struct ReflectionPass {
    pub ssr: ssr_pipeline::SsrPipeline,
    temporal_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,
    debug_pipeline: wgpu::RenderPipeline,
    temporal_bgl: wgpu::BindGroupLayout,
    composite_bgl: wgpu::BindGroupLayout,
    debug_bgl: wgpu::BindGroupLayout,
    probe_sample_bgl: wgpu::BindGroupLayout,
    temporal_uniform_buffer: wgpu::Buffer,
    composite_uniform_buffer: wgpu::Buffer,
    debug_uniform_buffer: wgpu::Buffer,
    temporal_bind_groups: [wgpu::BindGroup; 2],
    composite_bind_group: wgpu::BindGroup,
    debug_bind_group: wgpu::BindGroup,
    _reflection_texture: wgpu::Texture,
    reflection_view: TextureView,
    _reflection_hit_uv_texture: wgpu::Texture,
    reflection_hit_uv_view: TextureView,
    _reflection_hit_uv_history_textures: [wgpu::Texture; 2],
    reflection_hit_uv_history_views: [TextureView; 2],
    _reflection_history_textures: [wgpu::Texture; 2],
    reflection_history_views: [TextureView; 2],
    _reflection_rt_scratch_texture: wgpu::Texture,
    reflection_rt_scratch_view: TextureView,
    _composite_scratch_texture: wgpu::Texture,
    composite_scratch_view: TextureView,
    reflection_history_index: u8,
    reflection_first_frame: bool,
    reflection_resolved_from_history: bool,
    linear_sampler: wgpu::Sampler,
    nearest_sampler: wgpu::Sampler,
    width: u32,
    height: u32,
    color_format: TextureFormat,
    present_format: TextureFormat,
    pub profiler: ReflectionProfilerMs,
    denoise_pipeline: wgpu::ComputePipeline,
    denoise_bgl: wgpu::BindGroupLayout,
    denoise_uniform_buffer: wgpu::Buffer,
    _denoise_scratch_texture: wgpu::Texture,
    denoise_scratch_view: TextureView,
    denoise_bind_group: wgpu::BindGroup,
    rt_pass: RtReflectionPassV2,
    rt_hw_available: bool,
    rt_tlas_log_cooldown: u32,
    ssil_pass: SsilPass,
    reflection_frame_index: u32,
    screen_fraction: f32,
    refl_width: u32,
    refl_height: u32,
    ssr_stats: ssr_pipeline::SsrStatsReadback,
}

impl ReflectionPass {
    pub fn new(
        device: &Device,
        color_format: TextureFormat,
        present_format: TextureFormat,
        width: u32,
        height: u32,
        probe_sample_bgl: &wgpu::BindGroupLayout,
        screen_fraction: f32,
        rt_hw_available: bool,
    ) -> Self {
        let linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("reflection-linear"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let nearest_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("reflection-nearest"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let refl_width = (width as f32 * screen_fraction).max(1.0) as u32;
        let refl_height = (height as f32 * screen_fraction).max(1.0) as u32;

        let (reflection_texture, reflection_view) =
            create_color_texture(device, color_format, refl_width, refl_height, "reflection");
        let (hit_uv_tex, hit_uv_view) =
            create_hit_uv_texture(device, refl_width, refl_height, "reflection-hit-uv");
        let (h0, hv0) = create_color_texture(device, color_format, refl_width, refl_height, "reflection-hist-0");
        let (h1, hv1) = create_color_texture(device, color_format, refl_width, refl_height, "reflection-hist-1");
        let (hit_uv_h0, hit_uv_hv0) =
            create_hit_uv_texture(device, refl_width, refl_height, "reflection-hit-uv-hist-0");
        let (hit_uv_h1, hit_uv_hv1) =
            create_hit_uv_texture(device, refl_width, refl_height, "reflection-hit-uv-hist-1");
        let (reflection_rt_scratch_texture, reflection_rt_scratch_view) =
            create_rt_scratch_texture(device, color_format, refl_width, refl_height);
        let (composite_scratch_texture, composite_scratch_view) =
            create_composite_scratch_texture(device, color_format, width, height);
        let (denoise_scratch_texture, denoise_scratch_view) = {
            let tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("denoise-scratch"),
                size: wgpu::Extent3d {
                    width: refl_width.max(1),
                    height: refl_height.max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: color_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
            (tex, view)
        };
        let temporal_shader = device.create_shader_module(include_wgsl!("temporal.wgsl"));
        let composite_shader = device.create_shader_module(include_wgsl!("composite.wgsl"));
        let debug_trace_source = format!(
            "{}\n{}\n{}",
            include_str!("ssr_pipeline/ssr_debug_trace.wgsl"),
            include_str!("debug.wgsl"),
            include_str!("ssr_pipeline/ssr_raymarch.wgsl"),
        );
        let debug_shader = load_refl_wgsl(device, "reflection-debug", &debug_trace_source);

        let temporal_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("reflection-temporal-bgl"),
            entries: &[
                uniform_entry(0, wgpu::ShaderStages::FRAGMENT),
                texture_entry(1, true),
                texture_entry(2, true),
                texture_entry(3, true),
                sampler_entry(4, true),
                texture_entry_typed(5, wgpu::TextureSampleType::Float { filterable: false }),
                texture_entry(6, true),
                texture_entry(7, true),
            ],
        });
        let composite_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("reflection-composite-bgl"),
            entries: &[
                uniform_entry(0, wgpu::ShaderStages::FRAGMENT),
                texture_entry(1, true),
                texture_entry(2, true),
                sampler_entry(3, true),
                texture_entry(4, true),
                sampler_entry(5, false),
            ],
        });
        let denoise_shader = load_refl_wgsl(device, "denoiser", include_str!("denoiser.wgsl"));
        let denoise_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("denoise-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: color_format,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });
        let denoise_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("denoise-pipeline"),
            layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("denoise-pl"),
                bind_group_layouts: &[Some(&denoise_bgl)],
                immediate_size: 0,
            })),
            module: &denoise_shader,
            entry_point: Some("cs_main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let debug_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("reflection-debug-bgl"),
            entries: &[
                uniform_entry(0, wgpu::ShaderStages::FRAGMENT),
                texture_entry(1, true),
                texture_entry_typed(2, wgpu::TextureSampleType::Float { filterable: false }),
                texture_entry(3, true),
                texture_entry(4, true),
                sampler_entry(5, true),
                sampler_entry(6, false),
                texture_entry_typed(7, wgpu::TextureSampleType::Float { filterable: false }),
                texture_entry(8, true),
                texture_entry_typed(9, wgpu::TextureSampleType::Float { filterable: false }),
                texture_entry_typed(10, wgpu::TextureSampleType::Float { filterable: false }),
            ],
        });

        let color_target = [Some(wgpu::ColorTargetState {
            format: color_format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })];
        let hit_uv_target = Some(wgpu::ColorTargetState {
            format: wgpu::TextureFormat::Rg16Float,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        });
        let temporal_color_targets = [color_target[0].clone(), hit_uv_target];
        let present_target = [Some(wgpu::ColorTargetState {
            format: present_format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })];

        let temporal_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("refl-temporal-pl"),
            bind_group_layouts: &[Some(&temporal_bgl)],
            immediate_size: 0,
        });
        let temporal_pipeline = device.create_render_pipeline(&fullscreen_pipeline_desc(
            "reflection-temporal-pipeline",
            &temporal_shader,
            "vs_main",
            &temporal_color_targets,
            Some(&temporal_pl),
        ));

        let composite_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("refl-composite-pl"),
            bind_group_layouts: &[Some(&composite_bgl)],
            immediate_size: 0,
        });
        let composite_pipeline = device.create_render_pipeline(&fullscreen_pipeline_desc(
            "reflection-composite-pipeline",
            &composite_shader,
            "vs_main",
            &color_target,
            Some(&composite_pl),
        ));

        let debug_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("refl-debug-pl"),
            bind_group_layouts: &[Some(&debug_bgl), Some(probe_sample_bgl)],
            immediate_size: 0,
        });
        let debug_pipeline = device.create_render_pipeline(&fullscreen_pipeline_desc(
            "reflection-debug-pipeline",
            &debug_shader,
            "vs_main",
            &present_target,
            Some(&debug_pl),
        ));

        let ssr = ssr_pipeline::SsrPipeline::new(device, color_format, width, height, probe_sample_bgl);
        let temporal_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("refl-temporal-uniforms"),
            contents: bytemuck::bytes_of(&TemporalUniforms {
                resolution: [width.max(1) as f32, height.max(1) as f32],
                blend: 0.55,
                enabled: 1.0,
                depth_reject_m: 0.35,
                gbuffer_scale: 1.0,
                near_plane: 0.1,
                far_plane: 1000.0,
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let composite_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("refl-composite-uniforms"),
            contents: bytemuck::bytes_of(&CompositeUniforms {
                strength: 1.0,
                ssil_strength: 0.0,
                refl_mix: 1.0,
                _pad2: 0.0,
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let debug_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("refl-debug-uniforms"),
            contents: bytemuck::bytes_of(&DebugUniforms {
                mode: 0,
                max_roughness: 1.0,
                near_plane: 0.1,
                far_plane: 1000.0,
                cam_pos: [0.0; 4],
                inv_view_proj: [[0.0; 4]; 4],
                view_proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
                view: glam::Mat4::IDENTITY.to_cols_array_2d(),
                inv_view: glam::Mat4::IDENTITY.to_cols_array_2d(),
                resolution: [width.max(1) as f32, height.max(1) as f32],
                gb_resolution: [width.max(1) as f32, height.max(1) as f32],
                max_distance_m: 50.0,
                coarse_resolution: 0.5,
                thickness_m: 0.3,
                binary_steps: 8,
                coarse_max_iters: 32,
                ssr_blur_enabled: 1.0,
                _pad: [0.0; 2],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let temporal_bind_groups = [
            make_temporal_bind_group(
                device,
                &temporal_bgl,
                &temporal_uniform_buffer,
                &reflection_view,
                &hv0,
                &reflection_view,
                &reflection_view,
                &hit_uv_view,
                &hit_uv_hv0,
                &linear_sampler,
            ),
            make_temporal_bind_group(
                device,
                &temporal_bgl,
                &temporal_uniform_buffer,
                &reflection_view,
                &hv1,
                &reflection_view,
                &reflection_view,
                &hit_uv_view,
                &hit_uv_hv1,
                &linear_sampler,
            ),
        ];
        let composite_bind_group = make_composite_bind_group(
            device,
            &composite_bgl,
            &composite_uniform_buffer,
            &reflection_view,
            &reflection_view,
            &reflection_view,
            &linear_sampler,
            &nearest_sampler,
        );
        let debug_bind_group = make_debug_bind_group(
            device,
            &debug_bgl,
            &debug_uniform_buffer,
            &reflection_view,
            &reflection_view,
            &reflection_view,
            &reflection_view,
            &reflection_view,
            &reflection_view,
            &reflection_view,
            &reflection_view,
            &linear_sampler,
            &nearest_sampler,
        );

        let denoise_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("denoise-uniforms"),
            contents: bytemuck::bytes_of(&DenoiseUniforms {
                resolution: [refl_width as f32, refl_height as f32],
                depth_sigma: 4.0,
                normal_sigma: 12.0,
                luminance_sigma: 8.0,
                gbuffer_scale: 1.0,
                radius: 3.0,
                _pad: 0.0,
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let denoise_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("denoise-bg"),
            layout: &denoise_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: denoise_uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&reflection_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&reflection_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&reflection_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&linear_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&denoise_scratch_view),
                },
            ],
        });

        Self {
            ssr,
            temporal_pipeline,
            composite_pipeline,
            debug_pipeline,
            temporal_bgl,
            composite_bgl,
            debug_bgl,
            probe_sample_bgl: probe_sample_bgl.clone(),
            temporal_uniform_buffer,
            composite_uniform_buffer,
            debug_uniform_buffer,
            temporal_bind_groups,
            composite_bind_group,
            debug_bind_group,
            _reflection_texture: reflection_texture,
            reflection_view,
            _reflection_hit_uv_texture: hit_uv_tex,
            reflection_hit_uv_view: hit_uv_view,
            _reflection_hit_uv_history_textures: [hit_uv_h0, hit_uv_h1],
            reflection_hit_uv_history_views: [hit_uv_hv0, hit_uv_hv1],
            _reflection_history_textures: [h0, h1],
            reflection_history_views: [hv0, hv1],
            _reflection_rt_scratch_texture: reflection_rt_scratch_texture,
            reflection_rt_scratch_view,
            _composite_scratch_texture: composite_scratch_texture,
            composite_scratch_view,
            reflection_history_index: 0,
            reflection_first_frame: true,
            reflection_resolved_from_history: false,
            linear_sampler,
            nearest_sampler,
            width,
            height,
            color_format,
            present_format,
            profiler: ReflectionProfilerMs::default(),
            denoise_pipeline,
            denoise_bgl,
            denoise_uniform_buffer,
            _denoise_scratch_texture: denoise_scratch_texture,
            denoise_scratch_view,
            denoise_bind_group,
            rt_pass: RtReflectionPassV2::new(
                device,
                color_format,
                refl_width,
                refl_height,
                rt_hw_available,
            ),
            rt_hw_available,
            rt_tlas_log_cooldown: 0,
            ssil_pass: SsilPass::new(device, color_format, refl_width, refl_height),
            reflection_frame_index: 0,
            screen_fraction,
            refl_width,
            refl_height,
            ssr_stats: ssr_pipeline::SsrStatsReadback::new(device),
        }
    }

    pub fn poll_ssr_debug_logs(&mut self, device: &Device) {
        self.ssr_stats.finish_and_log(device);
    }

    pub fn arm_ssr_debug_log(&mut self) {
        self.ssr_stats.arm_immediate();
    }

    pub fn set_screen_fraction(&mut self, device: &Device, screen_fraction: f32) {
        let new_w = (self.width as f32 * screen_fraction).max(1.0) as u32;
        let new_h = (self.height as f32 * screen_fraction).max(1.0) as u32;
        if new_w == self.refl_width && new_h == self.refl_height {
            self.screen_fraction = screen_fraction;
            return;
        }
        *self = Self::new(
            device,
            self.color_format,
            self.present_format,
            self.width,
            self.height,
            &self.probe_sample_bgl,
            screen_fraction,
            self.rt_hw_available,
        );
    }

    pub fn resize(&mut self, device: &Device, width: u32, height: u32) {
        if width == self.width && height == self.height {
            return;
        }
        *self = Self::new(
            device,
            self.color_format,
            self.present_format,
            width.max(1),
            height.max(1),
            &self.probe_sample_bgl,
            self.screen_fraction,
            self.rt_hw_available,
        );
    }

    pub fn invalidate_temporal(&mut self) {
        self.reflection_first_frame = true;
        self.reflection_resolved_from_history = false;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn run(
        &mut self,
        device: &Device,
        queue: &Queue,
        encoder: &mut wgpu::CommandEncoder,
        settings: ReflectionSettings,
        debug_view: ReflectionDebugView,
        _depth_view: &TextureView,
        normal_roughness_view: &TextureView,
        lit_scene_view: &TextureView,
        direct_view: &TextureView,
        ambient_view: &TextureView,
        surface_view: &TextureView,
        base_color_view: &TextureView,
        world_pos_view: &TextureView,
        depth_export_view: &TextureView,
        velocity_view: &TextureView,
        accel: &RtAccel,
        inv_view_proj: Mat4,
        view_proj: Mat4,
        view: Mat4,
        cam_pos: Vec3,
        near_plane: f32,
        far_plane: f32,
        clear_color: wgpu::Color,
        rt_available: bool,
        probe_bind_group: &wgpu::BindGroup,
        shadow_view: &TextureView,
        shadow_sampler: &wgpu::Sampler,
        scene_uniforms: &SceneUniforms,
        texture_bind_group: &wgpu::BindGroup,
        ssr_debug_mode: bool,
    ) -> bool {
        if !settings.active() {
            self.profiler = ReflectionProfilerMs::default();
            self.reflection_resolved_from_history = false;
            return false;
        }

        // Paso de marcha: bias/grosor de intersección; el espaciado del rayo va en el shader.
        let step_size = settings.ssr_thickness_m();
        let inv_view = view.inverse();
        let gbuffer_scale = self.width.max(1) as f32 / self.refl_width.max(1) as f32;
        let frame_index = self.reflection_frame_index;
        self.reflection_frame_index = self.reflection_frame_index.wrapping_add(1);


        let t_ssr = Instant::now();
        self.ssr.run(
            device,
            queue,
            encoder,
            &settings,
            self.refl_width,
            self.refl_height,
            self.width,
            self.height,
            gbuffer_scale,
            frame_index,
            depth_export_view,
            normal_roughness_view,
            lit_scene_view,
            direct_view,
            ambient_view,
            surface_view,
            base_color_view,
            world_pos_view,
            &self.reflection_view,
            &self.reflection_hit_uv_view,
            inv_view_proj,
            view_proj,
            view,
            near_plane,
            far_plane,
            clear_color,
            probe_bind_group,
        );
        self.profiler.ssr_ms = t_ssr.elapsed().as_secs_f32() * 1000.0;

        let t_rt = Instant::now();
        let rt_w = (self.width as f32 * settings.rt_resolution_scale).max(1.0) as u32;
        let rt_h = (self.height as f32 * settings.rt_resolution_scale).max(1.0) as u32;
        if settings.uses_rt() && rt_available {
            if !accel.has_traceable_geometry() {
                if self.rt_tlas_log_cooldown == 0 {
                    log::warn!("[RT] Sin geometría trazable (BVH/TLAS vacío)");
                    self.rt_tlas_log_cooldown = 180;
                } else {
                    self.rt_tlas_log_cooldown -= 1;
                }
            } else if self.rt_tlas_log_cooldown == 0 {
                if accel.hw_active() {
                    log::info!(
                        "[RT] TLAS HW: {} triángulos, {} instancias (SSR+RT híbrido, res {:.0}%)",
                        accel.hw_tri_count,
                        accel.instance_material_count,
                        settings.rt_resolution_scale * 100.0,
                    );
                } else {
                    log::info!(
                        "[RT] BVH CPU: {} nodos, {} triángulos (SSR+RT híbrido, res {:.0}%)",
                        accel.node_count,
                        accel.tri_count,
                        settings.rt_resolution_scale * 100.0,
                    );
                }
                log::info!(
                    "[RT] Tip: vista debug ssr_miss_green o reflejo de objeto detrás de la cámara"
                );
                self.rt_tlas_log_cooldown = 180;
            } else {
                self.rt_tlas_log_cooldown -= 1;
            }
            if accel.has_traceable_geometry() {
                self.rt_pass.dispatch(
                    device,
                    queue,
                    encoder,
                    accel,
                    &self.reflection_view,
                    &self.reflection_rt_scratch_view,
                    &self.reflection_hit_uv_view,
                    depth_export_view,
                    normal_roughness_view,
                    lit_scene_view,
                    direct_view,
                    surface_view,
                    base_color_view,
                    inv_view_proj,
                    view_proj,
                    cam_pos,
                    near_plane,
                    far_plane,
                    settings,
                    rt_w,
                    rt_h,
                    gbuffer_scale,
                    frame_index,
                    probe_bind_group,
                    shadow_view,
                    shadow_sampler,
                    scene_uniforms,
                    texture_bind_group,
                );
                encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &self._reflection_rt_scratch_texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyTextureInfo {
                        texture: &self._reflection_texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::Extent3d {
                        width: self.refl_width.max(1),
                        height: self.refl_height.max(1),
                        depth_or_array_layers: 1,
                    },
                );
            }
        }
        self.profiler.rt_ms = t_rt.elapsed().as_secs_f32() * 1000.0;

        let t1 = Instant::now();
        let temporal_blend = settings.temporal_blend;
        if temporal_blend > 0.0 {
            // Ping-pong: `reflection_history_index` apunta SIEMPRE al último resultado
            // (lo que lee el composite). Acumula SSR+RT mezclados en `reflection_view`.
            let read_idx = self.reflection_history_index as usize;
            let write_idx = 1 - read_idx;
            let temporal_u = TemporalUniforms {
                resolution: [self.refl_width as f32, self.refl_height as f32],
                blend: if self.reflection_first_frame {
                    0.0
                } else {
                    temporal_blend
                },
                enabled: 1.0,
                depth_reject_m: 0.35,
                gbuffer_scale,
                near_plane,
                far_plane,
            };
            queue.write_buffer(
                &self.temporal_uniform_buffer,
                0,
                bytemuck::bytes_of(&temporal_u),
            );
            self.temporal_bind_groups[write_idx] = make_temporal_bind_group(
                device,
                &self.temporal_bgl,
                &self.temporal_uniform_buffer,
                &self.reflection_view,
                &self.reflection_history_views[read_idx],
                velocity_view,
                depth_export_view,
                &self.reflection_hit_uv_view,
                &self.reflection_hit_uv_history_views[read_idx],
                &self.linear_sampler,
            );
            let out_view = &self.reflection_history_views[write_idx];
            let hit_uv_out_view = &self.reflection_hit_uv_history_views[write_idx];
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("reflection-temporal-pass"),
                    color_attachments: &[
                        Some(wgpu::RenderPassColorAttachment {
                            view: out_view,
                            resolve_target: None,
                            depth_slice: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                                store: wgpu::StoreOp::Store,
                            },
                        }),
                        Some(wgpu::RenderPassColorAttachment {
                            view: hit_uv_out_view,
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
                pass.set_pipeline(&self.temporal_pipeline);
                pass.set_bind_group(0, &self.temporal_bind_groups[write_idx], &[]);
                pass.draw(0..3, 0..1);
            }
            self.reflection_history_index = write_idx as u8;
            self.reflection_first_frame = false;
            self.reflection_resolved_from_history = true;

            // Denoiser bilateral: solo con RT (en SSR-only suaviza demasiado el detalle).
            let t_denoise = Instant::now();
            if debug_view == ReflectionDebugView::Final && settings.uses_denoise() {
                let denoise_profile = settings.denoise_profile().expect("uses_denoise");
                let denoise_u = DenoiseUniforms {
                    resolution: [self.refl_width as f32, self.refl_height as f32],
                    depth_sigma: denoise_profile.depth_sigma,
                    normal_sigma: denoise_profile.normal_sigma,
                    luminance_sigma: denoise_profile.luminance_sigma,
                    gbuffer_scale,
                    radius: denoise_profile.radius as f32,
                    _pad: 0.0,
                };
                queue.write_buffer(
                    &self.denoise_uniform_buffer,
                    0,
                    bytemuck::bytes_of(&denoise_u),
                );
                self.denoise_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("denoise-bg-frame"),
                    layout: &self.denoise_bgl,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: self.denoise_uniform_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(
                                &self.reflection_history_views[write_idx],
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(depth_export_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::TextureView(normal_roughness_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: wgpu::BindingResource::Sampler(&self.linear_sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 5,
                            resource: wgpu::BindingResource::TextureView(&self.denoise_scratch_view),
                        },
                    ],
                });
                {
                    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("denoise-pass"),
                        timestamp_writes: None,
                    });
                    pass.set_pipeline(&self.denoise_pipeline);
                    pass.set_bind_group(0, &self.denoise_bind_group, &[]);
                    let dispatch_x = (self.refl_width + 7) / 8;
                    let dispatch_y = (self.refl_height + 7) / 8;
                    pass.dispatch_workgroups(dispatch_x, dispatch_y, 1);
                }
                // Copiar resultado denoised → history (reemplaza la entrada ruidosa)
                encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &self._denoise_scratch_texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyTextureInfo {
                        texture: &self._reflection_history_textures[write_idx],
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::Extent3d {
                        width: self.refl_width.max(1),
                        height: self.refl_height.max(1),
                        depth_or_array_layers: 1,
                    },
                );
            }
            self.profiler.denoise_ms = t_denoise.elapsed().as_secs_f32() * 1000.0;
        } else {
            self.reflection_first_frame = true;
            self.reflection_resolved_from_history = false;
            self.profiler.denoise_ms = 0.0;
        }
        self.profiler.temporal_ms = t1.elapsed().as_secs_f32() * 1000.0;

        if debug_view != ReflectionDebugView::Final {
            let debug_u = DebugUniforms {
                mode: debug_view.shader_index(),
                max_roughness: settings.max_roughness_to_trace,
                near_plane,
                far_plane,
                cam_pos: [cam_pos.x, cam_pos.y, cam_pos.z, 1.0],
                inv_view_proj: inv_view_proj.to_cols_array_2d(),
                view_proj: view_proj.to_cols_array_2d(),
                view: view.to_cols_array_2d(),
                inv_view: inv_view.to_cols_array_2d(),
                resolution: [self.width as f32, self.height as f32],
                gb_resolution: [self.width as f32, self.height as f32],
                max_distance_m: settings.max_distance_m,
                coarse_resolution: settings.ssr_coarse_resolution(),
                thickness_m: settings.ssr_thickness_m(),
                binary_steps: settings.binary_steps,
                coarse_max_iters: settings.ssr_coarse_max_iters(),
                ssr_blur_enabled: ssr_blur_enabled_for_debug_shader(debug_view),
                _pad: [0.0; 2],
            };
            queue.write_buffer(&self.debug_uniform_buffer, 0, bytemuck::bytes_of(&debug_u));
            let refl_for_debug: &TextureView = if self.reflection_resolved_from_history {
                &self.reflection_history_views[self.reflection_history_index as usize]
            } else {
                &self.reflection_view
            };
            self.debug_bind_group = make_debug_bind_group(
                device,
                &self.debug_bgl,
                &self.debug_uniform_buffer,
                lit_scene_view,
                depth_export_view,
                normal_roughness_view,
                refl_for_debug,
                surface_view,
                direct_view,
                base_color_view,
                world_pos_view,
                &self.linear_sampler,
                &self.nearest_sampler,
            );
        }

        if ssr_debug_mode && self.ssr_stats.tick_and_want_sample() {
            self.ssr_stats.queue_sample(
                queue,
                encoder,
                device,
                depth_export_view,
                surface_view,
                direct_view,
                base_color_view,
                &self.reflection_view,
                &self.reflection_hit_uv_view,
                self.refl_width,
                self.refl_height,
                ssr_pipeline::SsrDebugLogSnapshot {
                    frame_index,
                    tier: settings.tier.wire(),
                    viewport_w: self.width,
                    viewport_h: self.height,
                    refl_w: self.refl_width,
                    refl_h: self.refl_height,
                    screen_fraction: self.screen_fraction,
                    gbuffer_scale,
                    coarse_resolution: settings.ssr_coarse_resolution(),
                    coarse_max_iters: settings.ssr_coarse_max_iters(),
                    binary_steps: settings.binary_steps,
                    step_m: step_size,
                    max_distance_m: settings.max_distance_m,
                    max_roughness: settings.max_roughness_to_trace,
                    temporal_blend,
                    ssr_ms: self.profiler.ssr_ms,
                    temporal_ms: self.profiler.temporal_ms,
                    composite_ms: self.profiler.composite_ms,
                },
            );
        }

        true
    }

    /// Textura SSR/RT que consume el composite (y el pase transparente).
    pub fn composite_reflection_view(&self) -> &TextureView {
        if self.reflection_resolved_from_history {
            &self.reflection_history_views[self.reflection_history_index as usize]
        } else {
            &self.reflection_view
        }
    }

    pub fn composite_into(
        &mut self,
        device: &Device,
        queue: &Queue,
        encoder: &mut wgpu::CommandEncoder,
        scene_view: &TextureView,
        scene_texture: &wgpu::Texture,
        strength: f32,
        ssil_strength: f32,
    ) {
        let t0 = Instant::now();
        let reflection_view: &TextureView = if self.reflection_resolved_from_history {
            &self.reflection_history_views[self.reflection_history_index as usize]
        } else {
            &self.reflection_view
        };
        let composite_u = CompositeUniforms {
            strength,
            ssil_strength,
            refl_mix: 1.0,
            _pad2: 0.0,
        };
        queue.write_buffer(
            &self.composite_uniform_buffer,
            0,
            bytemuck::bytes_of(&composite_u),
        );
        self.composite_bind_group = make_composite_bind_group(
            device,
            &self.composite_bgl,
            &self.composite_uniform_buffer,
            scene_view,
            reflection_view,
            &self.ssil_pass.output_view(),
            &self.linear_sampler,
            &self.nearest_sampler,
        );
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("reflection-composite-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.composite_scratch_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.composite_pipeline);
            pass.set_bind_group(0, &self.composite_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self._composite_scratch_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: scene_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: self.width.max(1),
                height: self.height.max(1),
                depth_or_array_layers: 1,
            },
        );
        self.profiler.composite_ms = t0.elapsed().as_secs_f32() * 1000.0;
    }

    pub fn run_debug_blit(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        surface_view: &TextureView,
        debug_view: ReflectionDebugView,
        probe_bind_group: &wgpu::BindGroup,
    ) {
        if debug_view == ReflectionDebugView::Final {
            return;
        }
        draw_fullscreen_pass(
            encoder,
            &self.debug_pipeline,
            &self.debug_bind_group,
            Some(probe_bind_group),
            surface_view,
            wgpu::LoadOp::Clear(wgpu::Color::BLACK),
            Some("reflection-debug-blit"),
        );
    }
}

fn fullscreen_pipeline_desc<'a>(
    label: &'a str,
    module: &'a wgpu::ShaderModule,
    entry: &'a str,
    color_target: &'a [Option<wgpu::ColorTargetState>],
    layout: Option<&'a wgpu::PipelineLayout>,
) -> wgpu::RenderPipelineDescriptor<'a> {
    wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout,
        vertex: wgpu::VertexState {
            module,
            entry_point: Some(entry),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some("fs_main"),
            targets: color_target,
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    }
}

fn create_hit_uv_texture(
    device: &Device,
    width: u32,
    height: u32,
    label: &str,
) -> (wgpu::Texture, TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rg16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn create_color_texture(
    device: &Device,
    format: TextureFormat,
    width: u32,
    height: u32,
    label: &str,
) -> (wgpu::Texture, TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn create_rt_scratch_texture(
    device: &Device,
    format: TextureFormat,
    width: u32,
    height: u32,
) -> (wgpu::Texture, TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("reflection-rt-scratch"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn create_composite_scratch_texture(
    device: &Device,
    format: TextureFormat,
    width: u32,
    height: u32,
) -> (wgpu::Texture, TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("reflection-composite-scratch"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}



fn draw_fullscreen_pass(
    encoder: &mut wgpu::CommandEncoder,
    pipeline: &wgpu::RenderPipeline,
    bind_group: &wgpu::BindGroup,
    probe_bind_group: Option<&wgpu::BindGroup>,
    output_view: &TextureView,
    load: wgpu::LoadOp<wgpu::Color>,
    label: Option<&str>,
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label,
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: output_view,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        occlusion_query_set: None,
        timestamp_writes: None,
        multiview_mask: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bind_group, &[]);
    if let Some(probe_bg) = probe_bind_group {
        pass.set_bind_group(1, probe_bg, &[]);
    }
    pass.draw(0..3, 0..1);
}



fn uniform_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
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

fn texture_entry(binding: u32, filterable: bool) -> wgpu::BindGroupLayoutEntry {
    texture_entry_typed(
        binding,
        if filterable {
            wgpu::TextureSampleType::Float { filterable: true }
        } else {
            wgpu::TextureSampleType::Float { filterable: false }
        },
    )
}

fn texture_entry_typed(
    binding: u32,
    sample_type: wgpu::TextureSampleType,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn sampler_entry(binding: u32, linear: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(if linear {
            wgpu::SamplerBindingType::Filtering
        } else {
            wgpu::SamplerBindingType::NonFiltering
        }),
        count: None,
    }
}

fn ssr_blur_enabled_for_debug_shader(_debug_view: ReflectionDebugView) -> f32 {
    1.0
}

fn make_temporal_bind_group(
    device: &Device,
    layout: &wgpu::BindGroupLayout,
    uniforms: &wgpu::Buffer,
    curr: &TextureView,
    history: &TextureView,
    velocity: &TextureView,
    depth: &TextureView,
    hit_uv_curr: &TextureView,
    hit_uv_history: &TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("refl-temporal-bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(curr),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(history),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(velocity),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(depth),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::TextureView(hit_uv_curr),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::TextureView(hit_uv_history),
            },
        ],
    })
}

fn make_composite_bind_group(
    device: &Device,
    layout: &wgpu::BindGroupLayout,
    uniforms: &wgpu::Buffer,
    scene: &TextureView,
    reflection: &TextureView,
    ssil: &TextureView,
    linear_sampler: &wgpu::Sampler,
    nearest_sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("refl-composite-bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(scene),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(reflection),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(linear_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(ssil),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::Sampler(nearest_sampler),
            },
        ],
    })
}

fn make_debug_bind_group(
    device: &Device,
    layout: &wgpu::BindGroupLayout,
    uniforms: &wgpu::Buffer,
    scene: &TextureView,
    depth: &TextureView,
    normal_roughness: &TextureView,
    reflection: &TextureView,
    surface: &TextureView,
    direct: &TextureView,
    base_color: &TextureView,
    world_pos: &TextureView,
    linear: &wgpu::Sampler,
    nearest: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("refl-debug-bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(scene),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(depth),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(normal_roughness),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(reflection),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::Sampler(linear),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::Sampler(nearest),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::TextureView(surface),
            },
            wgpu::BindGroupEntry {
                binding: 8,
                resource: wgpu::BindingResource::TextureView(direct),
            },
            wgpu::BindGroupEntry {
                binding: 9,
                resource: wgpu::BindingResource::TextureView(base_color),
            },
            wgpu::BindGroupEntry {
                binding: 10,
                resource: wgpu::BindingResource::TextureView(world_pos),
            },
        ],
    })
}

pub(crate) fn load_refl_wgsl(device: &Device, label: &'static str, body: &str) -> wgpu::ShaderModule {
    let source = format!(
        "{}\n{}",
        include_str!("reflection_math.wgsl"),
        body
    );
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    })
}
