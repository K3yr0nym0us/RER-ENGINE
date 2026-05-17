use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use wgpu::{include_wgsl, util::DeviceExt};
use winit::window::Window;

use crate::config_3d::physics_3d::PhysicsWorld;
use crate::config_3d::{Camera, WorldBounds3D};
use crate::config_compat::{ActiveTool, GridConfig, PhysicsWorld2D};
use crate::ecs::{MeshComponent, World};
use crate::gizmo;
use crate::mesh;
use crate::scripting::ScriptEngine;
use crate::texture::GpuTexture;

use super::{build_scene_uniforms, create_depth_texture, start_audio_thread, State, DEPTH_FORMAT};

impl State {
    /// `is_embed`: si es true, fuerza el backend GL/EGL en vez de Vulkan.
    /// Vulkan (incluso llvmpipe) no soporta presentar en child X11 windows;
    /// EGL sí lo hace mediante software fallback.
    pub async fn new(window: Arc<Window>, is_embed: bool) -> Self {
        let size = window.inner_size();

        let backends = if is_embed {
            wgpu::Backends::GL
        } else {
            wgpu::Backends::all()
        };
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            ..Default::default()
        });
        let surface = instance
            .create_surface(Arc::clone(&window))
            .expect("no se pudo crear la Surface");

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("no se encontró adapter compatible");
        log::info!("Adapter: {}", adapter.get_info().name);

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
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let depth_view = create_depth_texture(&device, &config);

        let camera = Camera::new();
        let uniforms = build_scene_uniforms(&camera, size);

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let scene_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("scene-uniforms"),
            contents: bytemuck::cast_slice(&[uniforms]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let scene_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene-bg"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: scene_buf.as_entire_binding(),
            }],
        });

        let texture_bgl = GpuTexture::bind_group_layout(&device);
        let mut atlas = crate::texture::TextureAtlas::new(&device, &queue, &texture_bgl);
        let fallback_uv = crate::texture::TextureAtlas::fallback_uv();

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
        let checker_uv = atlas.pack(&queue, &checker_pixels, 128, 128);
        let uv_rects = vec![checker_uv];

        let (snap_hint_uv, snap_hint_size) =
            load_snap_hint_asset(&mut atlas, &queue, "tooltip-btn-ctrl-to-auto-adjust.png");
        let (snap_hint_uv_en, snap_hint_size_en) = load_snap_hint_asset(
            &mut atlas,
            &queue,
            "tooltip-btn-ctrl-to-auto-adjust-english.png",
        );

        let shader = device.create_shader_module(include_wgsl!("../shader.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pipeline-layout"),
            bind_group_layouts: &[&bgl, &texture_bgl],
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
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
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

        let render_pipeline_2d = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("main-pipeline-2d"),
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
        let meshes = vec![ground_plane];
        let mut world = World::new();
        let plane_id = world.spawn(Some("Ground"));
        world.insert(plane_id, MeshComponent { mesh_idx: 0, tex_idx: 0 });
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
        let grid_config = GridConfig::default();
        let grid_buffer = crate::config_compat::build_grid(&device, &grid_config);
        let world_bounds_3d = WorldBounds3D::default();
        let world_bounds_buffer = world_bounds_3d.build_buffer(&device);
        let crosshair_buffer = gizmo::build_crosshair(&device);

        let audio_slot = start_audio_thread();

        Self {
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
            render_pipeline_2d,
            depth_view,
            scene_buffer: scene_buf,
            scene_bind_group: scene_bg,
            atlas,
            uv_rects,
            fallback_uv,
            static_tex_cache: std::collections::HashMap::new(),
            canonical_quad_idx: 0,
            camera,
            fp_editor_frustum_distance: 2.5,
            camera_2d: None,
            meshes,
            world,
            last_frame: Instant::now(),
            delta_time: 0.0,
            gizmo_pipeline,
            gizmo_buffer,
            gizmo_bind_group,
            gizmo_buffer_uni,
            physics: PhysicsWorld::new(),
            physics_2d: PhysicsWorld2D::new(),
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
            grid_buffer,
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
            preview_playing: false,
            first_person_velocity: glam::Vec3::ZERO,
            first_person_on_floor: true,
            first_person_jump_queued: false,
            first_person_jump_request_active: false,
            first_person_jump_request_prev: false,
            first_person_script_input: HashSet::new(),
            first_person_lua_walk_speed: None,
            first_person_lua_sprint_multiplier: None,
            first_person_lua_jump_speed: None,
            first_person_player_entity: None,
            tool_overlay_buffer: tool_overlay_buffer_init,
            snap_hint_uv,
            snap_hint_size,
            snap_hint_uv_en,
            snap_hint_size_en,
            snap_locale: "en".to_string(),
            show_snap_hint: false,
            snap_hint_alpha: 0.0,
            collider_entities: Vec::new(),
            execution_area_entities: Vec::new(),
            execution_overlaps: HashSet::new(),
            anim_saved_transforms: std::collections::HashMap::new(),
            pivot_edit_mode: None,
            logical_area_mode: None,
            audio_slot,
            anim_texture_cache: std::collections::HashMap::new(),
            anim_overrides: std::collections::HashMap::new(),
            animations: HashMap::new(),
            active_animations: HashMap::new(),
            default_animation_by_entity: HashMap::new(),
            anim_flip_overrides: HashMap::new(),
            entity_facing_right: HashMap::new(),
            script_engine: ScriptEngine::new()
                .expect("Error al inicializar el motor de scripting Lua"),
            control_bindings_by_entity: HashMap::new(),
            sprite_store: HashMap::new(),
            model_store: HashMap::new(),
            sound_store: HashMap::new(),
            background_store: HashMap::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            is_applying_undo: false,
            metrics_last_emit: Instant::now(),
            metrics_frame_count: 0,
            last_draw_calls: 0,
            autosave_enabled: false,
            autosave_last_tick: Instant::now(),
        }
    }
}

pub(crate) fn load_snap_hint_asset(
    atlas: &mut crate::texture::TextureAtlas,
    queue: &wgpu::Queue,
    filename: &str,
) -> (Option<[f32; 4]>, (f32, f32)) {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../assets")
        .join(filename);
    match std::fs::read(&path) {
        Ok(bytes) => {
            use image::ImageReader;
            match ImageReader::new(std::io::Cursor::new(&bytes))
                .with_guessed_format()
                .map_err(|e| e.to_string())
                .and_then(|r| r.decode().map_err(|e| e.to_string()))
            {
                Ok(img) => {
                    let img = img.to_rgba8();
                    let (w, h) = img.dimensions();
                    let uv = atlas.pack(queue, img.as_raw(), w, h);
                    (Some(uv), (w as f32, h as f32))
                }
                Err(e) => {
                    log::warn!("[snap-hint] Error decodificando '{}': {}", path.display(), e);
                    (None, (0.0, 0.0))
                }
            }
        }
        Err(e) => {
            log::warn!("[snap-hint] No se pudo leer '{}': {}", path.display(), e);
            (None, (0.0, 0.0))
        }
    }
}
