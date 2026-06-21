//! Reflejos: SSR, acumulación temporal, composite y debug views.

pub(crate) mod blas;
pub(crate) mod bvh;
pub(crate) mod probe_env;
pub(crate) mod skinned_blas;
pub(crate) mod skinned_rt;
pub(crate) mod rt_material;
pub(crate) mod rt_sparse;
pub(crate) mod rt_extensions;
pub(crate) mod rt_accel;
pub(crate) mod rt_reflections_v2;
pub(crate) mod rt_pipeline;
pub(crate) mod rt_pathtrace;
pub(crate) mod ssil;
pub(crate) mod tlas;
mod ssr_trace_readback;

use std::time::Instant;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;
use wgpu::{include_wgsl, Device, Queue, TextureFormat, TextureView};

use crate::config_3d::reflection_graphics::{
    ReflectionDebugView, ReflectionProfilerMs, ReflectionSettings,
};
use crate::engine::SceneUniforms;
use crate::reflections::rt_accel::RtAccel;
use crate::reflections::rt_pathtrace::RtPathTracePass;
use crate::reflections::rt_reflections_v2::RtReflectionPassV2;
use crate::reflections::ssil::SsilPass;
use crate::reflections::ssr_trace_readback::SsrTraceReadback;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SsrUniforms {
    inv_view_proj: [[f32; 4]; 4],
    view_proj: [[f32; 4]; 4],
    cam_pos: [f32; 4],
    resolution: [f32; 2],
    max_steps: u32,
    max_distance_m: f32,
    max_roughness: f32,
    step_size: f32,
    near_plane: f32,
    far_plane: f32,
    clear_color: [f32; 4],
    ssr_blur_enabled: f32,
    frame_index: u32,
    _struct_pad: f32,
    /// Padding implícito WGSL (struct uniform alineado a 16 B → 208 B total).
    _tail_pad: u32,
}

const _: () = assert!(std::mem::size_of::<SsrUniforms>() == 208);
const _: () = assert!(std::mem::size_of::<TemporalUniforms>() == 24);
const _: () = assert!(std::mem::size_of::<CompositeUniforms>() == 16);
const _: () = assert!(std::mem::size_of::<DebugUniforms>() == 256);

fn ssr_blur_enabled_for_ssr_pass(debug_view: ReflectionDebugView) -> f32 {
    match debug_view {
        ReflectionDebugView::SsrNoBlur | ReflectionDebugView::SsrHitColorRaw => 0.0,
        _ => 1.0,
    }
}

fn ssr_blur_enabled_for_debug_shader(debug_view: ReflectionDebugView) -> f32 {
    match debug_view {
        ReflectionDebugView::SsrHitColorRaw | ReflectionDebugView::SsrNoBlur => 0.0,
        _ => 1.0,
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TemporalUniforms {
    resolution: [f32; 2],
    blend: f32,
    enabled: f32,
    depth_reject_m: f32,
    gbuffer_scale: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CompositeUniforms {
    strength: f32,
    ssil_strength: f32,
    shadow_influence: f32,
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
    resolution: [f32; 2],
    max_steps: u32,
    max_distance_m: f32,
    step_size: f32,
    ssr_blur_enabled: f32,
    _struct_pad: [f32; 2],
}

pub struct ReflectionPass {
    ssr_pipeline: wgpu::RenderPipeline,
    #[allow(dead_code)]
    ssr_log_pipeline: wgpu::RenderPipeline,
    temporal_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,
    debug_pipeline: wgpu::RenderPipeline,
    ssr_bgl: wgpu::BindGroupLayout,
    temporal_bgl: wgpu::BindGroupLayout,
    composite_bgl: wgpu::BindGroupLayout,
    debug_bgl: wgpu::BindGroupLayout,
    ssr_uniform_buffer: wgpu::Buffer,
    temporal_uniform_buffer: wgpu::Buffer,
    composite_uniform_buffer: wgpu::Buffer,
    debug_uniform_buffer: wgpu::Buffer,
    ssr_bind_group: wgpu::BindGroup,
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
    /// Scratch con COPY_SRC para readback CPU en vista `ssr_hit_color_uv_delta`.
    _debug_scratch_texture: wgpu::Texture,
    debug_scratch_view: TextureView,
    debug_present_blit_pipeline: wgpu::RenderPipeline,
    _debug_present_blit_bgl: wgpu::BindGroupLayout,
    debug_present_blit_bind_group: wgpu::BindGroup,
    reflection_history_index: u8,
    reflection_first_frame: bool,
    /// Tras temporal, el resultado válido está en history.
    reflection_resolved_from_history: bool,
    linear_sampler: wgpu::Sampler,
    nearest_sampler: wgpu::Sampler,
    width: u32,
    height: u32,
    color_format: TextureFormat,
    present_format: TextureFormat,
    pub profiler: ReflectionProfilerMs,
    rt_pass: RtReflectionPassV2,
    path_trace_pass: RtPathTracePass,
    ssil_pass: SsilPass,
    rt_hw_available: bool,
    /// SSR centro readback (desactivado; evita spam en consola).
    #[allow(dead_code)]
    _ssr_log_texture: wgpu::Texture,
    #[allow(dead_code)]
    ssr_log_view: TextureView,
    #[allow(dead_code)]
    ssr_trace_readback: SsrTraceReadback,
    rt_tlas_log_cooldown: u32,
    reflection_frame_index: u32,
    /// Resolución interna SSR/temporal/RT/SSIL (½ viewport).
    refl_width: u32,
    refl_height: u32,
}

impl ReflectionPass {
    pub fn new(
        device: &Device,
        color_format: TextureFormat,
        present_format: TextureFormat,
        width: u32,
        height: u32,
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

        let refl_width = (width / 2).max(1);
        let refl_height = (height / 2).max(1);

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
        let (debug_scratch_texture, debug_scratch_view) =
            create_debug_scratch_texture(device, present_format, width, height);
        let (ssr_log_texture, ssr_log_view) = create_ssr_log_texture(device);

        let ssr_shader = load_refl_wgsl(device, "ssr", include_str!("ssr.wgsl"));
        let temporal_shader = device.create_shader_module(include_wgsl!("temporal.wgsl"));
        let composite_shader = device.create_shader_module(include_wgsl!("composite.wgsl"));
        let debug_shader = load_refl_wgsl(device, "reflection-debug", include_str!("debug.wgsl"));

        let ssr_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ssr-bgl"),
            entries: &[
                uniform_entry(0, wgpu::ShaderStages::FRAGMENT),
                texture_entry_typed(1, wgpu::TextureSampleType::Float { filterable: false }),
                texture_entry(2, true),
                texture_entry(3, true),
                sampler_entry(4, true),
                sampler_entry(5, false),
                texture_entry(6, true),
                texture_entry_typed(7, wgpu::TextureSampleType::Float { filterable: false }),
                texture_entry_typed(8, wgpu::TextureSampleType::Float { filterable: false }),
            ],
        });
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
                texture_entry_typed(5, wgpu::TextureSampleType::Float { filterable: false }),
            ],
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
        let ssr_color_targets = [color_target[0].clone(), hit_uv_target.clone()];
        let temporal_color_targets = [color_target[0].clone(), hit_uv_target];
        let present_target = [Some(wgpu::ColorTargetState {
            format: present_format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })];

        let ssr_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ssr-pl"),
            bind_group_layouts: &[Some(&ssr_bgl)],
            immediate_size: 0,
        });
        let ssr_pipeline = device.create_render_pipeline(&fullscreen_pipeline_desc(
            "ssr-pipeline",
            &ssr_shader,
            "vs_main",
            &ssr_color_targets,
            Some(&ssr_pl),
        ));

        let ssr_log_target = [Some(wgpu::ColorTargetState {
            format: wgpu::TextureFormat::Rgba32Float,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })];
        let ssr_log_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ssr-log-pipeline"),
            layout: Some(&ssr_pl),
            vertex: wgpu::VertexState {
                module: &ssr_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &ssr_shader,
                entry_point: Some("fs_log"),
                targets: &ssr_log_target,
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

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
            bind_group_layouts: &[Some(&debug_bgl)],
            immediate_size: 0,
        });
        let debug_pipeline = device.create_render_pipeline(&fullscreen_pipeline_desc(
            "reflection-debug-pipeline",
            &debug_shader,
            "vs_main",
            &present_target,
            Some(&debug_pl),
        ));

        let debug_present_blit_shader =
            device.create_shader_module(include_wgsl!("../taa/taa_blit.wgsl"));
        let debug_present_blit_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("reflection-debug-present-blit-bgl"),
            entries: &[
                texture_entry(0, true),
                sampler_entry(1, true),
            ],
        });
        let debug_present_blit_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("reflection-debug-present-blit-pl"),
            bind_group_layouts: &[Some(&debug_present_blit_bgl)],
            immediate_size: 0,
        });
        let debug_present_blit_pipeline = device.create_render_pipeline(&fullscreen_pipeline_desc(
            "reflection-debug-present-blit",
            &debug_present_blit_shader,
            "vs_main",
            &present_target,
            Some(&debug_present_blit_pl),
        ));
        let debug_present_blit_bind_group = make_present_blit_bind_group(
            device,
            &debug_present_blit_bgl,
            &debug_scratch_view,
            &linear_sampler,
        );

        let ssr_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ssr-uniforms"),
            contents: bytemuck::bytes_of(&SsrUniforms {
                inv_view_proj: Mat4::IDENTITY.to_cols_array_2d(),
                view_proj: Mat4::IDENTITY.to_cols_array_2d(),
                cam_pos: [0.0; 4],
                resolution: [width.max(1) as f32, height.max(1) as f32],
                max_steps: 32,
                max_distance_m: 20.0,
                max_roughness: 0.82,
                step_size: 0.15,
                near_plane: 0.1,
                far_plane: 1000.0,
                clear_color: [0.1, 0.1, 0.12, 1.0],
                ssr_blur_enabled: 1.0,
                frame_index: 0,
                _struct_pad: 0.0,
                _tail_pad: 0,
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let temporal_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("refl-temporal-uniforms"),
            contents: bytemuck::bytes_of(&TemporalUniforms {
                resolution: [width.max(1) as f32, height.max(1) as f32],
                blend: 0.55,
                enabled: 1.0,
                depth_reject_m: 0.35,
                gbuffer_scale: 1.0,
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let composite_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("refl-composite-uniforms"),
            contents: bytemuck::bytes_of(&CompositeUniforms {
                strength: 1.0,
                ssil_strength: 0.0,
                shadow_influence: 0.1,
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
                resolution: [width.max(1) as f32, height.max(1) as f32],
                max_steps: 32,
                max_distance_m: 20.0,
                step_size: 0.15,
                ssr_blur_enabled: 1.0,
                _struct_pad: [0.0; 2],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let ssr_bind_group = make_ssr_bind_group(
            device,
            &ssr_bgl,
            &ssr_uniform_buffer,
            &reflection_view,
            &reflection_view,
            &reflection_view,
            &reflection_view,
            &reflection_view,
            &reflection_view,
            &linear_sampler,
            &nearest_sampler,
        );
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
            &hit_uv_view,
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
            &linear_sampler,
            &nearest_sampler,
        );

        Self {
            ssr_pipeline,
            ssr_log_pipeline,
            temporal_pipeline,
            composite_pipeline,
            debug_pipeline,
            ssr_bgl,
            temporal_bgl,
            composite_bgl,
            debug_bgl,
            ssr_uniform_buffer,
            temporal_uniform_buffer,
            composite_uniform_buffer,
            debug_uniform_buffer,
            ssr_bind_group,
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
            _debug_scratch_texture: debug_scratch_texture,
            debug_scratch_view,
            debug_present_blit_pipeline,
            _debug_present_blit_bgl: debug_present_blit_bgl,
            debug_present_blit_bind_group,
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
            rt_pass: RtReflectionPassV2::new(device, color_format, refl_width, refl_height, rt_hw_available),
            path_trace_pass: RtPathTracePass::new(device, refl_width, refl_height),
            ssil_pass: SsilPass::new(device, color_format, refl_width, refl_height),
            rt_hw_available,
            _ssr_log_texture: ssr_log_texture,
            ssr_log_view,
            ssr_trace_readback: SsrTraceReadback::new(device),
            rt_tlas_log_cooldown: 0,
            reflection_frame_index: 0,
            refl_width,
            refl_height,
        }
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
        surface_view: &TextureView,
        base_color_view: &TextureView,
        depth_export_view: &TextureView,
        velocity_view: &TextureView,
        accel: &mut RtAccel,
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
    ) -> bool {
        if !settings.active() {
            self.profiler = ReflectionProfilerMs::default();
            self.reflection_resolved_from_history = false;
            return false;
        }

        // Paso de marcha acotado: pasos gruesos (max_distance/steps) saltan superficies
        // cercanas (p.ej. el suelo bajo las esferas) y producen reflejos imperfectos. Se
        // limita a ≤0.25 m para impactos precisos en campo cercano; el rango efectivo
        // (steps × paso) sigue siendo suficiente para la escena.
        let step_size = (settings.max_distance_m / settings.max_steps.max(1) as f32)
            .clamp(0.05, 0.25);
        let gbuffer_scale = self.width.max(1) as f32 / self.refl_width.max(1) as f32;
        let frame_index = self.reflection_frame_index;
        self.reflection_frame_index = self.reflection_frame_index.wrapping_add(1);

        if debug_view == ReflectionDebugView::PathTrace {
            if settings.rt_enabled && rt_available && accel.node_count > 0 {
                self.path_trace_pass.dispatch(
                    device,
                    queue,
                    encoder,
                    accel,
                    depth_export_view,
                    surface_view,
                    direct_view,
                    base_color_view,
                    inv_view_proj,
                    view_proj,
                    cam_pos,
                    near_plane,
                    far_plane,
                    settings,
                    frame_index,
                    probe_bind_group,
                    shadow_view,
                    shadow_sampler,
                    scene_uniforms,
                );
            }
            let debug_u = DebugUniforms {
                mode: debug_view.shader_index(),
                max_roughness: settings.max_roughness_to_trace,
                near_plane,
                far_plane,
                cam_pos: [cam_pos.x, cam_pos.y, cam_pos.z, 1.0],
                inv_view_proj: inv_view_proj.to_cols_array_2d(),
                view_proj: view_proj.to_cols_array_2d(),
                view: view.to_cols_array_2d(),
                resolution: [self.width as f32, self.height as f32],
                max_steps: settings.max_steps,
                max_distance_m: settings.max_distance_m,
                step_size: (settings.max_distance_m / settings.max_steps.max(1) as f32).clamp(0.05, 0.25),
                ssr_blur_enabled: 0.0,
                _struct_pad: [0.0; 2],
            };
            queue.write_buffer(&self.debug_uniform_buffer, 0, bytemuck::bytes_of(&debug_u));
            self.debug_bind_group = make_debug_bind_group(
                device,
                &self.debug_bgl,
                &self.debug_uniform_buffer,
                lit_scene_view,
                depth_export_view,
                normal_roughness_view,
                self.path_trace_pass.output_view(),
                surface_view,
                direct_view,
                base_color_view,
                &self.linear_sampler,
                &self.nearest_sampler,
            );
            self.reflection_resolved_from_history = false;
            return true;
        }

        if crate::reflections::rt_extensions::rt_diffuse_gi_enabled(&settings) {
            self.ssil_pass.dispatch(
                device,
                queue,
                encoder,
                depth_export_view,
                normal_roughness_view,
                lit_scene_view,
                direct_view,
                surface_view,
                inv_view_proj,
                view_proj,
                cam_pos,
                near_plane,
                far_plane,
                gbuffer_scale,
            );
        }

        let ssr_blur = ssr_blur_enabled_for_ssr_pass(debug_view);
        let ssr_u = SsrUniforms {
            inv_view_proj: inv_view_proj.to_cols_array_2d(),
            view_proj: view_proj.to_cols_array_2d(),
            cam_pos: [cam_pos.x, cam_pos.y, cam_pos.z, 1.0],
            resolution: [self.refl_width as f32, self.refl_height as f32],
            max_steps: settings.max_steps,
            max_distance_m: settings.max_distance_m,
            max_roughness: settings.max_roughness_to_trace,
            step_size,
            near_plane,
            far_plane,
            clear_color: [clear_color.r as f32, clear_color.g as f32, clear_color.b as f32, 1.0],
            ssr_blur_enabled: ssr_blur,
            frame_index,
            _struct_pad: 0.0,
            _tail_pad: 0,
        };
        queue.write_buffer(&self.ssr_uniform_buffer, 0, bytemuck::bytes_of(&ssr_u));

        self.ssr_bind_group = make_ssr_bind_group(
            device,
            &self.ssr_bgl,
            &self.ssr_uniform_buffer,
            depth_export_view,
            normal_roughness_view,
            lit_scene_view,
            direct_view,
            surface_view,
            base_color_view,
            &self.linear_sampler,
            &self.nearest_sampler,
        );

        let t0 = Instant::now();
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ssr-pass"),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment {
                        view: &self.reflection_view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: &self.reflection_hit_uv_view,
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
            pass.set_pipeline(&self.ssr_pipeline);
            pass.set_bind_group(0, &self.ssr_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.profiler.ssr_ms = t0.elapsed().as_secs_f32() * 1000.0;

        let t2 = Instant::now();
        if settings.rt_enabled && rt_available {
            if !accel.has_traceable_geometry() {
                if self.rt_tlas_log_cooldown == 0 {
                    log::warn!("[RT] Sin geometría trazable (BVH/TLAS vacío)");
                    self.rt_tlas_log_cooldown = 180;
                } else {
                    self.rt_tlas_log_cooldown -= 1;
                }
            } else if self.rt_tlas_log_cooldown == 0 {
                if accel.hw_active() && accel.node_count == 0 {
                    log::info!(
                        "[RT] TLAS HW: {} triángulos, {} instancias",
                        accel.hw_tri_count,
                        accel.instance_material_count
                    );
                } else {
                    log::info!(
                        "[RT] BVH v2: {} nodos, {} triángulos",
                        accel.node_count,
                        accel.tri_count
                    );
                }
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
                self.refl_width,
                self.refl_height,
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
        self.profiler.rt_ms = t2.elapsed().as_secs_f32() * 1000.0;

        let t1 = Instant::now();
        if settings.temporal_blend > 0.0 {
            // Ping-pong: `reflection_history_index` apunta SIEMPRE al último resultado
            // (lo que lee el composite). Acumula SSR+RT mezclados en `reflection_view`.
            let read_idx = self.reflection_history_index as usize;
            let write_idx = 1 - read_idx;
            let temporal_u = TemporalUniforms {
                resolution: [self.refl_width as f32, self.refl_height as f32],
                blend: if self.reflection_first_frame {
                    0.0
                } else {
                    settings.temporal_blend
                },
                enabled: 1.0,
                depth_reject_m: 0.35,
                gbuffer_scale,
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
        } else {
            self.reflection_first_frame = true;
            self.reflection_resolved_from_history = false;
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
                resolution: [self.width as f32, self.height as f32],
                max_steps: settings.max_steps,
                max_distance_m: settings.max_distance_m,
                step_size,
                ssr_blur_enabled: ssr_blur_enabled_for_debug_shader(debug_view),
                _struct_pad: [0.0; 2],
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
                &self.linear_sampler,
                &self.nearest_sampler,
            );
        }

        true
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
        shadow_mask_view: &TextureView,
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
            shadow_influence: 0.1,
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
            shadow_mask_view,
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
    ) {
        if debug_view == ReflectionDebugView::Final {
            return;
        }
        let uv_delta_log = debug_view == ReflectionDebugView::SsrHitUvWorldScreenDelta;
        let render_target = if uv_delta_log {
            &self.debug_scratch_view
        } else {
            surface_view
        };
        draw_fullscreen_pass(
            encoder,
            &self.debug_pipeline,
            &self.debug_bind_group,
            render_target,
            wgpu::LoadOp::Clear(wgpu::Color::BLACK),
            Some("reflection-debug-blit"),
        );
        if uv_delta_log {
            draw_fullscreen_pass(
                encoder,
                &self.debug_present_blit_pipeline,
                &self.debug_present_blit_bind_group,
                surface_view,
                wgpu::LoadOp::Load,
                Some("reflection-debug-present-blit"),
            );
        }
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

fn create_debug_scratch_texture(
    device: &Device,
    format: TextureFormat,
    width: u32,
    height: u32,
) -> (wgpu::Texture, TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("reflection-debug-scratch"),
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

fn create_ssr_log_texture(device: &Device) -> (wgpu::Texture, TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("ssr-log"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn draw_fullscreen_pass(
    encoder: &mut wgpu::CommandEncoder,
    pipeline: &wgpu::RenderPipeline,
    bind_group: &wgpu::BindGroup,
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
    pass.draw(0..3, 0..1);
}

fn make_present_blit_bind_group(
    device: &Device,
    layout: &wgpu::BindGroupLayout,
    source: &TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("reflection-debug-present-blit-bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(source),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
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

fn make_ssr_bind_group(
    device: &Device,
    layout: &wgpu::BindGroupLayout,
    uniforms: &wgpu::Buffer,
    depth: &TextureView,
    normal_roughness: &TextureView,
    lit_scene: &TextureView,
    direct: &TextureView,
    surface: &TextureView,
    base_color: &TextureView,
    linear: &wgpu::Sampler,
    nearest: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ssr-bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(depth),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(normal_roughness),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(lit_scene),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(linear),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::Sampler(nearest),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::TextureView(direct),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::TextureView(surface),
            },
            wgpu::BindGroupEntry {
                binding: 8,
                resource: wgpu::BindingResource::TextureView(base_color),
            },
        ],
    })
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
    sampler: &wgpu::Sampler,
    shadow_mask: &TextureView,
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
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(ssil),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(shadow_mask),
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
