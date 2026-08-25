//! RT reflexiones v2: BVH compute (fallback) + ray query hardware (Vulkan).

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;
use wgpu::{Device, Queue, TextureFormat, TextureView};

use super::rt_accel::RtAccel;
use super::rt_extensions;
use super::rt_scratch::RtScratchCopy;
use crate::config_3d::reflection_graphics::ReflectionSettings;
use crate::engine::SceneUniforms;
use crate::reflections::probe_env::ProbeEnvPass;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct RtUniforms {
    inv_view_proj: [[f32; 4]; 4],
    view_proj: [[f32; 4]; 4],
    cam_pos: [f32; 4],
    resolution: [f32; 2],
    node_count: u32,
    tri_count: u32,
    max_distance_m: f32,
    max_roughness: f32,
    rt_blend: f32,
    step_size: f32,
    near_plane: f32,
    far_plane: f32,
    frame_index: u32,
    material_count: u32,
    gbuffer_scale: f32,
    material_quality: f32,
}

const _: () = assert!(std::mem::size_of::<RtUniforms>() == 200);

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct RtHwUniforms {
    inv_view_proj: [[f32; 4]; 4],
    view_proj: [[f32; 4]; 4],
    cam_pos: [f32; 4],
    resolution: [f32; 2],
    max_distance_m: f32,
    max_roughness: f32,
    rt_blend: f32,
    step_size: f32,
    near_plane: f32,
    far_plane: f32,
    frame_index: u32,
    material_count: u32,
    gbuffer_scale: f32,
    material_quality: f32,
}

const _: () = assert!(std::mem::size_of::<RtHwUniforms>() == 192);

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

const _: () = assert!(std::mem::size_of::<RtLightUniform>() == 144);

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
            if rt_extensions::rt_second_bounce_enabled(settings) {
                1.0
            } else {
                0.0
            },
            if rt_extensions::rt_diffuse_gi_enabled(settings) {
                1.0
            } else {
                0.0
            },
            if rt_extensions::rt_shadows_enabled(settings) {
                1.0
            } else {
                0.0
            },
        ],
    }
}

enum RtPipelineMode {
    Bvh,
    HardwareRayQuery,
}

pub struct RtReflectionPassV2 {
    mode: RtPipelineMode,
    bvh_pipeline: wgpu::ComputePipeline,
    bvh_bgl: wgpu::BindGroupLayout,
    hw_pipeline: Option<wgpu::ComputePipeline>,
    hw_bgl: Option<wgpu::BindGroupLayout>,
    scratch: RtScratchCopy,
    uniform_buffer: wgpu::Buffer,
    hw_uniform_buffer: wgpu::Buffer,
    bvh_bind_group: wgpu::BindGroup,
    hw_bind_group: Option<wgpu::BindGroup>,
    rt_light_bgl: wgpu::BindGroupLayout,
    rt_light_buffer: wgpu::Buffer,
    /// Cache del bind group de luz/sombra (buffer uniform estable; shadow view estable por tier).
    rt_light_bind_group: Option<wgpu::BindGroup>,
    width: u32,
    height: u32,
}

impl RtReflectionPassV2 {
    pub fn new(
        device: &Device,
        color_format: TextureFormat,
        width: u32,
        height: u32,
        hw_available: bool,
    ) -> Self {
        let bvh_shader =
            crate::reflections::load_refl_wgsl(device, "rt-bvh", include_str!("rt_bvh.wgsl"));
        let bvh_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rt-bvh-bgl"),
            entries: &[
                uniform_entry(0),
                storage_buffer_entry(1, true),
                storage_buffer_entry(2, true),
                storage_texture_entry(3, color_format, false),
                texture_entry(4, false),
                depth_texture_entry(5),
                texture_entry(6, true),
                texture_entry(7, true),
                texture_entry(8, false),
                texture_entry(9, false),
                texture_entry(10, false),
                storage_buffer_entry(11, true),
                texture_entry(12, true),
            ],
        });
        let probe_bgl = ProbeEnvPass::sample_bind_group_layout(device);
        let texture_bgl = crate::texture::TextureArray::bind_group_layout(device);
        let rt_light_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rt-light-bgl"),
            entries: &[
                shadow_depth_texture_entry(0),
                shadow_sampler_entry(1),
                uniform_entry(2),
            ],
        });
        let bvh_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rt-bvh-pl"),
            bind_group_layouts: &[
                Some(&bvh_bgl),
                Some(&probe_bgl),
                Some(&rt_light_bgl),
                Some(&texture_bgl),
            ],
            immediate_size: 0,
        });
        let bvh_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("rt-bvh-pipeline"),
            layout: Some(&bvh_pl),
            module: &bvh_shader,
            entry_point: Some("cs_main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let scratch = RtScratchCopy::new(device, color_format);

        let (hw_pipeline, hw_bgl) = if hw_available {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("rt-ray-query"),
                source: wgpu::ShaderSource::Wgsl(
                    format!(
                        "enable wgpu_ray_query;\n{}\n{}",
                        include_str!("../reflection_math.wgsl"),
                        include_str!("rt_ray_query.wgsl")
                    )
                    .into(),
                ),
            });
            let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("rt-hw-bgl"),
                entries: &[
                    uniform_entry(0),
                    acceleration_structure_entry(1),
                    storage_texture_entry(2, color_format, false),
                    texture_entry(3, false),
                    depth_texture_entry(4),
                    texture_entry(5, true),
                    texture_entry(6, true),
                    texture_entry(7, true),
                    texture_entry(8, false),
                    texture_entry(9, false),
                    storage_buffer_entry(10, true),
                    storage_buffer_entry(11, true),
                    storage_buffer_entry(12, true),
                    texture_entry(13, true),
                ],
            });
            let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("rt-hw-pl"),
                bind_group_layouts: &[
                    Some(&bgl),
                    Some(&probe_bgl),
                    Some(&rt_light_bgl),
                    Some(&texture_bgl),
                ],
                immediate_size: 0,
            });
            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("rt-hw-pipeline"),
                layout: Some(&pl),
                module: &shader,
                entry_point: Some("cs_main"),
                compilation_options: Default::default(),
                cache: None,
            });
            (Some(pipeline), Some(bgl))
        } else {
            (None, None)
        };

        let mode = if hw_pipeline.is_some() {
            RtPipelineMode::HardwareRayQuery
        } else {
            RtPipelineMode::Bvh
        };

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rt-bvh-uniforms"),
            contents: bytemuck::bytes_of(&RtUniforms {
                inv_view_proj: Mat4::IDENTITY.to_cols_array_2d(),
                view_proj: Mat4::IDENTITY.to_cols_array_2d(),
                cam_pos: [0.0; 4],
                resolution: [width.max(1) as f32, height.max(1) as f32],
                node_count: 0,
                tri_count: 0,
                max_distance_m: 40.0,
                max_roughness: 0.88,
                rt_blend: 0.85,
                step_size: 0.15,
                near_plane: 0.1,
                far_plane: 1000.0,
                frame_index: 0,
                material_count: 0,
                gbuffer_scale: 1.0,
                material_quality: 0.0,
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let hw_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rt-hw-uniforms"),
            contents: bytemuck::bytes_of(&RtHwUniforms {
                inv_view_proj: Mat4::IDENTITY.to_cols_array_2d(),
                view_proj: Mat4::IDENTITY.to_cols_array_2d(),
                cam_pos: [0.0; 4],
                resolution: [width.max(1) as f32, height.max(1) as f32],
                max_distance_m: 40.0,
                max_roughness: 0.88,
                rt_blend: 0.85,
                step_size: 0.15,
                near_plane: 0.1,
                far_plane: 1000.0,
                frame_index: 0,
                material_count: 0,
                gbuffer_scale: 1.0,
                material_quality: 0.0,
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let rt_light_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rt-light-uniforms"),
            contents: bytemuck::bytes_of(&RtLightUniform {
                light_dir: [0.0, 1.0, 0.0, 1.0],
                light_view_proj: Mat4::IDENTITY.to_cols_array_2d(),
                light_params: [0.0; 4],
                shadow_bias: [0.0; 4],
                light_color: [1.0, 1.0, 1.0, 0.0],
                rt_flags: [0.0; 4],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let dummy_material = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rt-dummy-material"),
            size: 32,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        let dummy_node = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rt-dummy-node"),
            size: 16,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let dummy_tri = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rt-dummy-tri"),
            size: 48,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        let bvh_bind_group = make_bvh_bind_group(
            device,
            &bvh_bgl,
            &uniform_buffer,
            &dummy_node,
            &dummy_tri,
            &dummy_texture_view(device, color_format, width, height),
            &dummy_texture_view(device, color_format, width, height),
            &dummy_depth_view(device, width, height),
            &dummy_texture_view(device, color_format, width, height),
            &dummy_texture_view(device, color_format, width, height),
            &dummy_texture_view(device, color_format, width, height),
            &dummy_texture_view(device, color_format, width, height),
            &dummy_texture_view(device, color_format, width, height),
            &dummy_material,
            &dummy_texture_view(device, color_format, width, height),
        );

        Self {
            mode,
            bvh_pipeline,
            bvh_bgl,
            hw_pipeline,
            hw_bgl,
            scratch,
            uniform_buffer,
            hw_uniform_buffer,
            bvh_bind_group,
            hw_bind_group: None,
            rt_light_bgl,
            rt_light_buffer,
            rt_light_bind_group: None,
            width: width.max(1),
            height: height.max(1),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn dispatch(
        &mut self,
        device: &Device,
        queue: &Queue,
        encoder: &mut wgpu::CommandEncoder,
        accel: &RtAccel,
        ssr_view: &TextureView,
        output_view: &TextureView,
        ssr_hit_uv_view: &TextureView,
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
        gbuffer_scale: f32,
        frame_index: u32,
        probe_bind_group: &wgpu::BindGroup,
        shadow_view: &TextureView,
        shadow_sampler: &wgpu::Sampler,
        scene_uniforms: &SceneUniforms,
        texture_bind_group: &wgpu::BindGroup,
    ) {
        if !accel.has_traceable_geometry() {
            return;
        }

        self.width = width.max(1);
        self.height = height.max(1);
        self.scratch.resize(self.width, self.height);

        let step_size =
            (settings.max_distance_m / settings.max_steps.max(1) as f32).clamp(0.05, 0.25);

        let rt_light = rt_light_from_scene(scene_uniforms, &settings);
        queue.write_buffer(&self.rt_light_buffer, 0, bytemuck::bytes_of(&rt_light));
        if self.rt_light_bind_group.is_none() {
            self.rt_light_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("rt-light-bg"),
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
            }));
        }
        let rt_light_bind_group = self.rt_light_bind_group.as_ref().expect("rt-light-bg");

        self.scratch
            .seed_output_from_ssr(device, encoder, ssr_view, output_view);

        let wg_x = self.width.div_ceil(8);
        let wg_y = self.height.div_ceil(8);

        match self.mode {
            RtPipelineMode::HardwareRayQuery => {
                let Some(tlas) = accel.tlas() else {
                    dispatch_bvh(
                        self,
                        device,
                        queue,
                        encoder,
                        accel,
                        ssr_view,
                        output_view,
                        ssr_hit_uv_view,
                        depth_view,
                        normal_roughness_view,
                        ambient_view,
                        direct_view,
                        surface_view,
                        base_color_view,
                        inv_view_proj,
                        view_proj,
                        cam_pos,
                        near_plane,
                        far_plane,
                        settings,
                        step_size,
                        width,
                        height,
                        gbuffer_scale,
                        frame_index,
                        probe_bind_group,
                        shadow_view,
                        shadow_sampler,
                        scene_uniforms,
                        texture_bind_group,
                    );
                    return;
                };
                let Some(hw_bgl) = self.hw_bgl.as_ref() else {
                    return;
                };
                let hw_u = RtHwUniforms {
                    inv_view_proj: inv_view_proj.to_cols_array_2d(),
                    view_proj: view_proj.to_cols_array_2d(),
                    cam_pos: [cam_pos.x, cam_pos.y, cam_pos.z, 1.0],
                    resolution: [width.max(1) as f32, height.max(1) as f32],
                    max_distance_m: settings.max_distance_m,
                    max_roughness: settings.max_roughness_to_trace,
                    rt_blend: settings.rt_blend,
                    step_size,
                    near_plane,
                    far_plane,
                    frame_index,
                    material_count: accel.instance_material_count,
                    gbuffer_scale,
                    material_quality: settings.rt_material_quality().shader_value(),
                };
                queue.write_buffer(&self.hw_uniform_buffer, 0, bytemuck::bytes_of(&hw_u));
                self.hw_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("rt-hw-bg"),
                    layout: hw_bgl,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: self.hw_uniform_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: tlas.as_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(output_view),
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
                            resource: wgpu::BindingResource::TextureView(ambient_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 7,
                            resource: wgpu::BindingResource::TextureView(direct_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 8,
                            resource: wgpu::BindingResource::TextureView(surface_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 9,
                            resource: wgpu::BindingResource::TextureView(base_color_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 10,
                            resource: accel.instance_material_buffer().as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 11,
                            resource: accel.hw_tri_buffer().as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 12,
                            resource: accel.instance_tri_base_buffer().as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 13,
                            resource: wgpu::BindingResource::TextureView(ssr_hit_uv_view),
                        },
                    ],
                }));
                let Some(bg) = self.hw_bind_group.as_ref() else {
                    return;
                };
                let Some(pipeline) = self.hw_pipeline.as_ref() else {
                    return;
                };
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("rt-hw-pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, bg, &[]);
                pass.set_bind_group(1, probe_bind_group, &[]);
                pass.set_bind_group(2, rt_light_bind_group, &[]);
                pass.set_bind_group(3, texture_bind_group, &[]);
                pass.dispatch_workgroups(wg_x, wg_y, 1);
            }
            RtPipelineMode::Bvh => {
                dispatch_bvh(
                    self,
                    device,
                    queue,
                    encoder,
                    accel,
                    ssr_view,
                    output_view,
                    ssr_hit_uv_view,
                    depth_view,
                    normal_roughness_view,
                    ambient_view,
                    direct_view,
                    surface_view,
                    base_color_view,
                    inv_view_proj,
                    view_proj,
                    cam_pos,
                    near_plane,
                    far_plane,
                    settings,
                    step_size,
                    width,
                    height,
                    gbuffer_scale,
                    frame_index,
                    probe_bind_group,
                    shadow_view,
                    shadow_sampler,
                    scene_uniforms,
                    texture_bind_group,
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn dispatch_bvh(
    pass_state: &mut RtReflectionPassV2,
    device: &Device,
    queue: &Queue,
    encoder: &mut wgpu::CommandEncoder,
    accel: &RtAccel,
    ssr_view: &TextureView,
    output_view: &TextureView,
    ssr_hit_uv_view: &TextureView,
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
    step_size: f32,
    width: u32,
    height: u32,
    gbuffer_scale: f32,
    frame_index: u32,
    probe_bind_group: &wgpu::BindGroup,
    shadow_view: &TextureView,
    shadow_sampler: &wgpu::Sampler,
    scene_uniforms: &SceneUniforms,
    texture_bind_group: &wgpu::BindGroup,
) {
    let rt_light = rt_light_from_scene(scene_uniforms, &settings);
    queue.write_buffer(
        &pass_state.rt_light_buffer,
        0,
        bytemuck::bytes_of(&rt_light),
    );
    if pass_state.rt_light_bind_group.is_none() {
        pass_state.rt_light_bind_group =
            Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("rt-light-bg-bvh"),
                layout: &pass_state.rt_light_bgl,
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
                        resource: pass_state.rt_light_buffer.as_entire_binding(),
                    },
                ],
            }));
    }
    let rt_light_bind_group = pass_state
        .rt_light_bind_group
        .as_ref()
        .expect("rt-light-bg");

    let uniforms = RtUniforms {
        inv_view_proj: inv_view_proj.to_cols_array_2d(),
        view_proj: view_proj.to_cols_array_2d(),
        cam_pos: [cam_pos.x, cam_pos.y, cam_pos.z, 1.0],
        resolution: [width.max(1) as f32, height.max(1) as f32],
        node_count: accel.node_count,
        tri_count: accel.tri_count,
        max_distance_m: settings.max_distance_m,
        max_roughness: settings.max_roughness_to_trace,
        rt_blend: settings.rt_blend,
        step_size,
        near_plane,
        far_plane,
        frame_index,
        material_count: accel.instance_material_count,
        gbuffer_scale,
        material_quality: settings.rt_material_quality().shader_value(),
    };
    queue.write_buffer(&pass_state.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
    pass_state.bvh_bind_group = make_bvh_bind_group(
        device,
        &pass_state.bvh_bgl,
        &pass_state.uniform_buffer,
        accel.node_buffer(),
        accel.tri_buffer(),
        output_view,
        ssr_view,
        depth_view,
        normal_roughness_view,
        ambient_view,
        direct_view,
        surface_view,
        base_color_view,
        accel.instance_material_buffer(),
        ssr_hit_uv_view,
    );
    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
        label: Some("rt-bvh-pass"),
        timestamp_writes: None,
    });
    pass.set_pipeline(&pass_state.bvh_pipeline);
    pass.set_bind_group(0, &pass_state.bvh_bind_group, &[]);
    pass.set_bind_group(1, probe_bind_group, &[]);
    pass.set_bind_group(2, rt_light_bind_group, &[]);
    pass.set_bind_group(3, texture_bind_group, &[]);
    let wg_x = width.div_ceil(8);
    let wg_y = height.div_ceil(8);
    pass.dispatch_workgroups(wg_x, wg_y, 1);
}

pub fn adapter_supports_rt(adapter: &wgpu::Adapter) -> bool {
    adapter
        .features()
        .contains(wgpu::Features::EXPERIMENTAL_RAY_QUERY)
}

pub fn request_rt_device_features(adapter: &wgpu::Adapter) -> wgpu::Features {
    let mut features = wgpu::Features::empty();
    if adapter
        .features()
        .contains(wgpu::Features::EXPERIMENTAL_RAY_QUERY)
    {
        features |= wgpu::Features::EXPERIMENTAL_RAY_QUERY;
    }
    features
}

pub fn request_rt_device_limits(adapter: &wgpu::Adapter, rt_available: bool) -> wgpu::Limits {
    if !rt_available {
        return wgpu::Limits::default();
    }
    let mut limits = wgpu::Limits::default().using_acceleration_structure_values(adapter.limits());
    if limits.max_acceleration_structures_per_shader_stage == 0 {
        limits = limits.using_minimum_supported_acceleration_structure_values();
    }
    limits
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

fn acceleration_structure_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::AccelerationStructure {
            vertex_return: false,
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

fn shadow_sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
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

#[allow(clippy::too_many_arguments)]
fn make_bvh_bind_group(
    device: &Device,
    layout: &wgpu::BindGroupLayout,
    uniforms: &wgpu::Buffer,
    nodes: &wgpu::Buffer,
    tris: &wgpu::Buffer,
    output: &TextureView,
    ssr: &TextureView,
    depth: &TextureView,
    normal_roughness: &TextureView,
    ambient: &TextureView,
    direct: &TextureView,
    surface: &TextureView,
    base_color: &TextureView,
    instance_materials: &wgpu::Buffer,
    ssr_hit_uv: &TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("rt-bvh-bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: nodes.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: tris.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(output),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(ssr),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(depth),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::TextureView(normal_roughness),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::TextureView(ambient),
            },
            wgpu::BindGroupEntry {
                binding: 8,
                resource: wgpu::BindingResource::TextureView(direct),
            },
            wgpu::BindGroupEntry {
                binding: 9,
                resource: wgpu::BindingResource::TextureView(surface),
            },
            wgpu::BindGroupEntry {
                binding: 10,
                resource: wgpu::BindingResource::TextureView(base_color),
            },
            wgpu::BindGroupEntry {
                binding: 11,
                resource: instance_materials.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 12,
                resource: wgpu::BindingResource::TextureView(ssr_hit_uv),
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
            label: Some("rt-v2-dummy"),
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
            label: Some("rt-v2-dummy-depth"),
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
