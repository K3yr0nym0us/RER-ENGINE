use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use wgpu::{include_wgsl, util::DeviceExt};
use winit::window::Window;
use rer_engine_shared::gpu::{init_gpu, EngineGpuProfile, GpuInitError};

use crate::config_3d::physics_3d::PhysicsWorld;
use crate::config_3d::{Camera, WorldBounds3D};
use crate::config_compat::{ActiveTool, GridConfig};
use crate::ecs::World;
use crate::entity_save_meta::EntitySaveRegistry;
use crate::gizmo;
use crate::mesh;
use crate::scripting::ScriptEngine;
use crate::texture::TextureArray;

use super::{
    create_depth_texture, start_audio_thread, SceneUniforms, State, DEPTH_FORMAT,
    SHADOW_MAP_SIZE,
};
use crate::config_3d::directional_light::{
    DEFAULT_LIGHT_AMBIENT, DEFAULT_LIGHT_COLOR, DEFAULT_LIGHT_DIR, DEFAULT_LIGHT_INTENSITY,
    DEFAULT_SHADOW_DARKNESS,
};

impl State {
    /// Inicializa wgpu (3D: Vulkan; ver `rer_engine_shared::gpu`).
    pub async fn new(window: Arc<Window>) -> Result<Self, GpuInitError> {
        let size = window.inner_size();

        let (_instance, surface, adapter) =
            init_gpu(window.clone(), EngineGpuProfile::ThreeD).await?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("oxide-device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: Default::default(),
                },
                None,
            )
            .await
            .expect("no se pudo crear el Device");

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoNoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let depth_view = create_depth_texture(&device, &config);

        let camera = Camera::new();
        let aspect = size.width as f32 / size.height as f32;
        let p = camera.position();
        let identity_vp = glam::Mat4::IDENTITY.to_cols_array_2d();
        let cam_vp = camera.to_uniform(aspect).view_proj;
        let uniforms = SceneUniforms {
            view_proj: cam_vp,
            view_proj_stable: cam_vp,
            prev_view_proj: identity_vp,
            inv_view_proj: identity_vp,
            cam_pos: [p.x, p.y, p.z, 0.0],
            light_dir: [
                DEFAULT_LIGHT_DIR.x,
                DEFAULT_LIGHT_DIR.y,
                DEFAULT_LIGHT_DIR.z,
                DEFAULT_LIGHT_AMBIENT,
            ],
            light_color: [
                DEFAULT_LIGHT_COLOR.x,
                DEFAULT_LIGHT_COLOR.y,
                DEFAULT_LIGHT_COLOR.z,
                1.0,
            ],
            light_view_proj: identity_vp,
            light_params: [
                DEFAULT_LIGHT_INTENSITY,
                0.0,
                1.0 / SHADOW_MAP_SIZE as f32,
                1.0,
            ],
            jitter: [0.0; 4],
            shadow_bias: [
                crate::config_3d::directional_light::SHADOW_NORMAL_BIAS_MIN,
                crate::config_3d::directional_light::SHADOW_NORMAL_BIAS_MAX,
                crate::config_3d::directional_light::SHADOW_DEPTH_BIAS_CONST,
                crate::config_3d::directional_light::SHADOW_DEPTH_BIAS_SLOPE,
            ],
        };

        let scene_uniform_size =
            std::num::NonZeroU64::new(std::mem::size_of::<SceneUniforms>() as u64);
        let shadow_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow-map"),
            size: wgpu::Extent3d {
                width: SHADOW_MAP_SIZE,
                height: SHADOW_MAP_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let shadow_map_view = shadow_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("shadow-map-view"),
            ..Default::default()
        });
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: scene_uniform_size,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });
        let scene_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("scene-uniforms"),
            contents: bytemuck::cast_slice(&[uniforms]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let scene_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene-bg"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: scene_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&shadow_map_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&shadow_sampler),
                },
            ],
        });
        let shadow_pass_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow-pass-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: scene_uniform_size,
                },
                count: None,
            }],
        });
        let shadow_pass_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow-pass-bg"),
            layout: &shadow_pass_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: scene_buf.as_entire_binding(),
            }],
        });

        let hud_identity = SceneUniforms {
            view_proj: identity_vp,
            view_proj_stable: identity_vp,
            prev_view_proj: identity_vp,
            inv_view_proj: identity_vp,
            cam_pos: [0.0, 0.0, 5.0, 0.0],
            light_dir: [0.0, 1.0, 0.0, 1.0],
            light_color: [1.0, 1.0, 1.0, 0.0],
            light_view_proj: identity_vp,
            light_params: [DEFAULT_LIGHT_INTENSITY, 0.0, 0.0, 0.0],
            jitter: [0.0; 4],
            shadow_bias: [0.0; 4],
        };
        let hud_scene_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("hud-scene-uniforms"),
            contents: bytemuck::cast_slice(&[hud_identity]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let hud_scene_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hud-scene-bg"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: hud_scene_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&shadow_map_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&shadow_sampler),
                },
            ],
        });

        let texture_bgl = TextureArray::bind_group_layout(&device);
        let mut texture_array = crate::texture::TextureArray::new(&device, &queue, &texture_bgl);
        let fallback_layer = crate::texture::TextureArray::fallback_layer();

        let checker_pixels = {
            const S: u32 = 128;
            const TILE: u32 = 8;
            let mut px: Vec<u8> = Vec::with_capacity((S * S * 4) as usize);
            for y in 0..S {
                for x in 0..S {
                    let light = ((x / TILE + y / TILE) % 2) == 0;
                    let (r, g, b): (u8, u8, u8) = if light { (58, 61, 80) } else { (30, 32, 48) };
                    px.extend_from_slice(&[r, g, b, 255]);
                }
            }
            px
        };
        let checker_layer = texture_array.pack(&queue, &checker_pixels, 128, 128);
        let tex_layers = vec![checker_layer];

        let screen_hud_bgl = crate::screen_hud_image::ScreenHudAtlas::bind_group_layout(&device);
        let mut screen_hud_atlas =
            crate::screen_hud_image::ScreenHudAtlas::new(&device, &queue, &screen_hud_bgl);
        let fps_exit_hint_es = screen_hud_atlas
            .pack_png_from_engine_assets(&queue, "tooltip-btn-esc-salir.png");
        let fps_exit_hint_en = screen_hud_atlas
            .pack_png_from_engine_assets(&queue, "tooltip-btn-esc-exit.png");

        let shader = device.create_shader_module(include_wgsl!("../shader.wgsl"));
        let screen_hud_shader =
            device.create_shader_module(include_wgsl!("../shader_screen_hud.wgsl"));
        let shadow_mask_target = Some(wgpu::ColorTargetState {
            format: crate::taa::SHADOW_MASK_FORMAT,
            blend: None,
            write_mask: wgpu::ColorWrites::RED,
        });
        let depth_export_target = Some(wgpu::ColorTargetState {
            format: crate::taa::DEPTH_EXPORT_FORMAT,
            blend: None,
            write_mask: wgpu::ColorWrites::RED,
        });
        let velocity_target = Some(wgpu::ColorTargetState {
            format: crate::taa::VELOCITY_FORMAT,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        });
        let mrt_targets = [
            Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            }),
            shadow_mask_target.clone(),
            Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            }),
            depth_export_target.clone(),
            velocity_target.clone(),
        ];
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pipeline-layout"),
            bind_group_layouts: &[&bgl, &texture_bgl],
            push_constant_ranges: &[],
        });
        let shadow_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shadow-pipeline-layout"),
            bind_group_layouts: &[&shadow_pass_bgl],
            push_constant_ranges: &[],
        });
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("main-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[mesh::Vertex::desc(), mesh::InstanceData::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &mrt_targets,
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow-pipeline"),
            layout: Some(&shadow_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_shadow",
                buffers: &[mesh::Vertex::desc(), mesh::InstanceData::desc()],
                compilation_options: Default::default(),
            },
            fragment: None,
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: 3,
                    slope_scale: 2.5,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let screen_hud_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("screen-hud-pipeline-layout"),
                bind_group_layouts: &[&bgl, &screen_hud_bgl],
                push_constant_ranges: &[],
            });
        let screen_hud_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("screen-hud-pipeline"),
                layout: Some(&screen_hud_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &screen_hud_shader,
                    entry_point: "vs_screen_hud",
                    buffers: &[mesh::Vertex::desc(), mesh::InstanceData::desc()],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &screen_hud_shader,
                    entry_point: "fs_screen_hud",
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: false,
                    depth_compare: wgpu::CompareFunction::Always,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

        let render_pipeline_overlay = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sprite-overlay-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[mesh::Vertex::desc(), mesh::InstanceData::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_overlay",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let ground_plane = crate::config_3d::mesh_3d::create_ground_plane(&device);
        let hud_quad_mesh = mesh::create_unit_quad_xy(&device);
        let meshes = vec![ground_plane];
        let world = World::new();
        let mut camera = Camera::new();
        camera.target = glam::Vec3::new(0.0, 1.75, 5.0);
        camera.pitch = 0.0;
        camera.yaw = -std::f32::consts::FRAC_PI_2;
        camera.distance = 0.01;

        let gizmo_shader = device.create_shader_module(include_wgsl!("../gizmo.wgsl"));
        let gizmo_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gizmo-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let gizmo_uni_data: [[f32; 4]; 9] = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
            [-1.0, -1.0, 0.0, 0.0],
        ];
        let gizmo_buffer_uni = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gizmo-uni"),
            contents: bytemuck::cast_slice(&gizmo_uni_data),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let gizmo_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gizmo-bg"),
            layout: &gizmo_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: gizmo_buffer_uni.as_entire_binding(),
            }],
        });
        let gizmo_pl_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gizmo-pl-layout"),
            bind_group_layouts: &[&gizmo_bgl],
            push_constant_ranges: &[],
        });
        let gizmo_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gizmo-pipeline"),
            layout: Some(&gizmo_pl_layout),
            vertex: wgpu::VertexState {
                module: &gizmo_shader,
                entry_point: "vs_main",
                buffers: &[gizmo::GizmoVertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &gizmo_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let gizmo_buffer = gizmo::build_axes(&device, 1.14);
        let tool_overlay_buffer_init = gizmo::build_from_vertices(&device, &[]);

        let grid_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("grid-pipeline"),
            layout: Some(&gizmo_pl_layout),
            vertex: wgpu::VertexState {
                module: &gizmo_shader,
                entry_point: "vs_main",
                buffers: &[gizmo::GizmoVertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &gizmo_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let grid_uni_identity: [[f32; 4]; 9] = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
            [-1.0, -1.0, 0.0, 0.0],
        ];
        let grid_buffer_uni = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("grid-uni"),
            contents: bytemuck::cast_slice(&grid_uni_identity),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let grid_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("grid-bg"),
            layout: &gizmo_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: grid_buffer_uni.as_entire_binding(),
            }],
        });
        let joint_uniform_size =
            std::num::NonZeroU64::new((crate::config_3d::model_asset::MAX_JOINTS * 64) as u64)
                .unwrap();
        let joint_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("joint-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(joint_uniform_size),
                },
                count: None,
            }],
        });
        let skinned_shader = device.create_shader_module(include_wgsl!("../shader_skinned.wgsl"));
        let skinned_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("skinned-pipeline-layout"),
            bind_group_layouts: &[&bgl, &texture_bgl, &joint_bgl],
            push_constant_ranges: &[],
        });
        let skinned_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("skinned-main-pipeline"),
            layout: Some(&skinned_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &skinned_shader,
                entry_point: "vs_main_skinned",
                buffers: &[mesh::SkinnedVertex::desc(), mesh::SkinnedInstanceData::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &skinned_shader,
                entry_point: "fs_main_skinned",
                targets: &mrt_targets,
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let skinned_shadow_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("skinned-shadow-layout"),
            bind_group_layouts: &[&shadow_pass_bgl, &joint_bgl],
            push_constant_ranges: &[],
        });
        let skinned_shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("skinned-shadow-pipeline"),
            layout: Some(&skinned_shadow_layout),
            vertex: wgpu::VertexState {
                module: &skinned_shader,
                entry_point: "vs_shadow_skinned",
                buffers: &[mesh::SkinnedVertex::desc(), mesh::SkinnedInstanceData::desc()],
                compilation_options: Default::default(),
            },
            fragment: None,
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: 3,
                    slope_scale: 2.5,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let grid_config = GridConfig::default();
        let world_bounds_3d = WorldBounds3D::default();
        let world_bounds_buffer = world_bounds_3d.build_buffer(&device);
        let crosshair_buffer = gizmo::build_crosshair(&device);

        let audio_slot = start_audio_thread();

        let (model_preload_tx, model_preload_rx) =
            crate::config_3d::static_model_cache::create_model_preload_channel();
        let taa = crate::taa::TaaPass::new(
            &device,
            format,
            size.width,
            size.height,
        );

        let state = Self {
            window,
            surface,
            device,
            queue,
            config,
            size,
            clear_color: wgpu::Color {
                r: 0.06,
                g: 0.06,
                b: 0.10,
                a: 1.0,
            },
            render_pipeline,
            render_pipeline_overlay,
            shadow_pipeline,
            _shadow_texture: shadow_texture,
            depth_view,
            taa,
            vsync_enabled: false,
            prev_view_proj: identity_vp,
            scene_buffer: scene_buf,
            scene_bind_group: scene_bg,
            shadow_pass_bind_group,
            hud_scene_bind_group: hud_scene_bg,
            texture_array,
            tex_layers,
            fallback_layer,
            hud_quad_mesh,
            screen_hud_pipeline,
            screen_hud_atlas,
            camera,
            editor_orbit_target: glam::Vec3::ZERO,
            editor_viewport_yaw: -std::f32::consts::FRAC_PI_4,
            editor_viewport_pitch: crate::config_3d::character_anchor::EDITOR_DEFAULT_ORBIT_PITCH,
            editor_viewport_distance: 3.0,
            fps_editor_frustum_distance: 2.5,
            play_camera_eye_position: glam::Vec3::ZERO,
            play_camera_follow_mode: crate::ipc::PlayCameraFollowMode::MoveWithCharacter,
            play_camera_follow_offset: glam::Vec3::ZERO,
            play_camera_follow_offset_local: glam::Vec3::ZERO,
            meshes,
            world,
            last_frame: Instant::now(),
            delta_time: 0.0,
            gizmo_pipeline,
            gizmo_buffer,
            gizmo_bind_group,
            gizmo_buffer_uni,
            physics: PhysicsWorld::new(),
            selected_entity: None,
            selected_entities: Vec::new(),
            hovered_entity: None,
            hovered_gizmo_axis: None,
            active_gizmo_axis: None,
            spatial_grid: crate::spatial::SpatialGrid::new(),
            scenario_entities: Vec::new(),
            character_entities: Vec::new(),
            background_entity: None,
            background_path: None,
            grid_config,
            grid_pipeline,
            grid_bind_group,
            grid_buffer_uni,
            world_bounds_3d,
            world_bounds_buffer,
            crosshair_buffer,
            ctrl_held: false,
            active_tool: ActiveTool::None,
            quick_build_ghost_id: None,
            quick_build_preview_path: None,
            quick_build_preview_kind: None,
            quick_build_preview_scale: None,
            quick_build_blueprint: None,
            blueprint_registry: HashMap::new(),
            entity_blueprint_ids: HashMap::new(),
            entity_colision: HashMap::new(),
            preview_playing: false,
            debug_mode: false,
            preview_entity_transform_snapshots: HashMap::new(),
            preview_fp_view_snapshot: None,
            play_controller_velocity: glam::Vec3::ZERO,
            play_controller_on_floor: true,
            play_controller_jump_queued: false,
            play_controller_jump_request_active: false,
            play_controller_jump_request_prev: false,
            play_controller_script_input: HashSet::new(),
            play_controller_lua_walk_speed: None,
            play_controller_lua_sprint_multiplier: None,
            play_controller_lua_jump_speed: None,
            play_character_entity: None,
            editor_camera_entity: None,
            play_character_mesh_forward_xz: glam::Vec2::new(0.0, 1.0),
            play_character_mesh_extents: None,
            play_session_body_yaw_baseline: 0.0,
            play_session_camera_yaw_baseline: 0.0,
            tool_overlay_buffer: tool_overlay_buffer_init,
            snap_locale: "en".to_string(),
            fps_exit_hint_es,
            fps_exit_hint_en,
            fps_exit_hint_alpha: 0.0,
            anim_saved_transforms: std::collections::HashMap::new(),
            pivot_edit_mode: None,
            logical_area_mode: None,
            audio_slot,
            animations: HashMap::new(),
            active_animations: HashMap::new(),
            default_animation_by_entity: HashMap::new(),
            entity_facing_right: HashMap::new(),
            script_engine: ScriptEngine::new()
                .expect("Error al inicializar el motor de scripting Lua"),
            control_bindings_by_entity: HashMap::new(),
            sprite_store: HashMap::new(),
            model_store: HashMap::new(),
            static_model_cache: HashMap::new(),
            model_preload_rx,
            model_preload_tx,
            model_preload_inflight: HashSet::new(),
            model_preload_gpu_queue: Vec::new(),
            pending_load_models: Vec::new(),
            pending_entity_model_replaces: Vec::new(),
            sound_store: HashMap::new(),
            background_store: HashMap::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            is_applying_undo: false,
            process_metrics_sampler: rer_engine_shared::process_metrics::ProcessMetricsSampler::new(),
            metrics_last_emit: Instant::now(),
            metrics_frame_count: 0,
            last_draw_calls: 0,
            autosave_enabled: false,
            autosave_last_tick: Instant::now(),
            save_registry: EntitySaveRegistry::new(),
            target_fps: 60,
            sun_entity: None,
            sun_icon_mesh_idx: None,
            sun_icon_tex_idx: None,
            editor_box_mesh_idx: None,
            editor_box_tex_idx: None,
            directional_light_dir: DEFAULT_LIGHT_DIR.normalize(),
            directional_light_color: DEFAULT_LIGHT_COLOR,
            directional_light_ambient: DEFAULT_LIGHT_AMBIENT,
            light_intensity: DEFAULT_LIGHT_INTENSITY,
            shadow_darkness: DEFAULT_SHADOW_DARKNESS,
            scene_instance_pool: crate::engine::types::InstanceBufferPool::new(),
            shadow_instance_pool: crate::engine::types::InstanceBufferPool::new(),
            skinned_instance_pool: crate::engine::types::InstanceBufferPool::new(),
            texture_path_layers: HashMap::new(),
            model_assets: std::collections::HashMap::new(),
            model_animation_bindings: std::collections::HashMap::new(),
            active_model_clips: std::collections::HashMap::new(),
            model_clip_defaults: std::collections::HashMap::new(),
            skinned_gpu_meshes: Vec::new(),
            skinned_render_pipeline,
            skinned_shadow_pipeline,
            joint_bind_group_layout: Some(joint_bgl),
            mount_save_on_empty_world: false,
            restoring_save_manifest: false,
        };
        Ok(state)
    }
}

