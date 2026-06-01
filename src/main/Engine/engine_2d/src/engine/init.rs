use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use wgpu::{include_wgsl, util::DeviceExt};
use winit::window::Window;
use rer_engine_shared::gpu::{init_gpu, EngineGpuProfile, GpuInitError};

use crate::config_2d::{ActiveTool, GridConfig, PhysicsWorld2D};
use crate::entity_save_meta::EntitySaveRegistry;
use crate::config_compat::Camera;
use crate::ecs::{MeshComponent, World};
use crate::gizmo;
use crate::mesh;
use crate::scripting::ScriptEngine;
use crate::texture::GpuTexture;

use super::audio::start_audio_thread;
use super::render_helpers::{build_scene_uniforms, create_depth_texture};
use super::types::DEPTH_FORMAT;
use super::State;

impl State {
    /// Inicializa wgpu (motor 2D: siempre Vulkan vía `rer_engine_shared::gpu`).
    pub async fn new(window: Arc<Window>) -> Result<Self, GpuInitError> {
        let size = window.inner_size();

        let (_instance, surface, adapter) =
            init_gpu(window.clone(), EngineGpuProfile::TwoD).await?;

        // ── Device & Queue ────────────────────────────────────────────────────
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label:             Some("oxide-device"),
                    required_features: wgpu::Features::empty(),
                    required_limits:   wgpu::Limits::default(),
                    memory_hints:      Default::default(),
                },
                None,
            )
            .await
            .expect("no se pudo crear el Device");

        // ── Surface config ────────────────────────────────────────────────────
        let caps   = surface.get_capabilities(&adapter);
        let format = caps.formats.iter().find(|f| f.is_srgb()).copied()
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage:                         wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width:                         size.width.max(1),
            height:                        size.height.max(1),
            present_mode:                  wgpu::PresentMode::AutoNoVsync,
            alpha_mode:                    caps.alpha_modes[0],
            view_formats:                  vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // ── Depth texture ─────────────────────────────────────────────────────
        let depth_view = create_depth_texture(&device, &config);

        // ── Uniforms buffer (compartido por todo el frame) ──────────────────
        let camera   = Camera::new();
        let uniforms = build_scene_uniforms(&camera, size);

        // ── Bind group layout group 0 (uniforms) ─────────────────────────────────
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label:   Some("scene-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding:    0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty:                 wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size:   None,
                },
                count: None,
            }],
        });
        // Buffer de uniforms de escena (view_proj + cam_pos, compartido por todos los sprites)
        let scene_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label:    Some("scene-uniforms"),
            contents: bytemuck::cast_slice(&[uniforms]),
            usage:    wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let scene_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:   Some("scene-bg"),
            layout:  &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding:  0,
                resource: scene_buf.as_entire_binding(),
            }],
        });

        // ── Bind group layout group 1 (textura) + atlas ─────────────────────────────
        let texture_bgl = GpuTexture::bind_group_layout(&device);
        let mut atlas   = crate::texture::TextureAtlas::new(&device, &queue, &texture_bgl);
        let fallback_uv = crate::texture::TextureAtlas::fallback_uv();

        // Checkerboard para el plano de suelo (tex_idx=0).
        // Patrón bakeado 128×128 con tiles de 8px para no depender del tiling con Repeat.
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
        let uv_rects = vec![checker_uv];   // idx 0 = checkerboard (plano de suelo)

        // Hint visual de snap (Ctrl): carga versión ES y EN del PNG en el atlas.
        let load_snap_hint = |atlas: &mut crate::texture::TextureAtlas, queue: &wgpu::Queue, filename: &str| -> (Option<[f32; 4]>, (f32, f32)) {
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
        };
        let (snap_hint_uv, snap_hint_size) = load_snap_hint(&mut atlas, &queue, "tooltip-btn-ctrl-to-auto-adjust.png");
        let (snap_hint_uv_en, snap_hint_size_en) = load_snap_hint(&mut atlas, &queue, "tooltip-btn-ctrl-to-auto-adjust-english.png");

        // ── Pipeline ─────────────────────────────────────────────────────────
        let shader = device.create_shader_module(include_wgsl!("../shader.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label:                Some("pipeline-layout"),
            bind_group_layouts:   &[&bgl, &texture_bgl],
            push_constant_ranges: &[],
        });
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:  Some("main-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module:              &shader,
                entry_point:         "vs_main",
                buffers:             &[mesh::Vertex::desc(), mesh::InstanceData::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module:      &shader,
                entry_point: "fs_main",
                targets:     &[Some(wgpu::ColorTargetState {
                    format,
                    blend:      Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format:              DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare:       wgpu::CompareFunction::Less,
                stencil:             wgpu::StencilState::default(),
                bias:                wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview:   None,
            cache:       None,
        });

        // Pipeline 2D: sin depth-write ni depth-test — el orden back-to-front
        // ya garantiza el orden correcto y el alpha blending funciona bien.
        let render_pipeline_2d = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:  Some("main-pipeline-2d"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module:              &shader,
                entry_point:         "vs_main",
                buffers:             &[mesh::Vertex::desc(), mesh::InstanceData::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module:      &shader,
                entry_point: "fs_main",
                targets:     &[
                    Some(wgpu::ColorTargetState {
                        format,
                        blend:      Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                ],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format:              DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare:       wgpu::CompareFunction::Always,
                stencil:             wgpu::StencilState::default(),
                bias:                wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview:   None,
            cache:       None,
        });

        let render_pipeline_overlay = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:  Some("sprite-overlay-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module:              &shader,
                entry_point:         "vs_main",
                buffers:             &[mesh::Vertex::desc(), mesh::InstanceData::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module:      &shader,
                entry_point: "fs_overlay",
                targets:     &[Some(wgpu::ColorTargetState {
                    format,
                    blend:      Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format:              DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare:       wgpu::CompareFunction::Always,
                stencil:             wgpu::StencilState::default(),
                bias:                wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview:   None,
            cache:       None,
        });

        // ── Escenario base del editor 2D ─────────────────────────────────────
        let ground_plane = crate::config_compat::mesh::create_ground_plane(&device);
        let meshes       = vec![ground_plane];
        let mut world    = World::new();
        // Entidad del plano
        let plane_id = world.spawn(Some("Ground"));
        world.insert(plane_id, MeshComponent { mesh_idx: 0, tex_idx: 0 });
        // Textura checkerboard para el plano base (UV idx 0 ya en uv_rects)
        // Cámara base del editor para estados sin cámara ortográfica inicializada.
        let mut camera = Camera::new();
        camera.target   = glam::Vec3::new(0.0, 1.75, 5.0);
        camera.pitch    = 0.0;
        camera.yaw      = -std::f32::consts::FRAC_PI_2;
        camera.distance = 0.01;  // muy cerca — simula la posición del ojo

        // ── Pipeline de gizmos (LineList, sin depth write) ───────────────────
        let gizmo_shader = device.create_shader_module(include_wgsl!("../gizmo.wgsl"));
        let gizmo_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label:   Some("gizmo-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding:    0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty:                 wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size:   None,
                },
                count: None,
            }],
        });
        // Uniform de gizmo: view_proj + model + flags (144 bytes)
        let gizmo_uni_data: [[f32; 4]; 9] = [
            [1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0],
            [-1.0, -1.0, 0.0, 0.0], // flags: hovered_axis, active_axis
        ];
        let gizmo_buffer_uni = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label:    Some("gizmo-uni"),
            contents: bytemuck::cast_slice(&gizmo_uni_data),
            usage:    wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let gizmo_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:   Some("gizmo-bg"),
            layout:  &gizmo_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding:  0,
                resource: gizmo_buffer_uni.as_entire_binding(),
            }],
        });
        let gizmo_pl_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label:                Some("gizmo-pl-layout"),
            bind_group_layouts:   &[&gizmo_bgl],
            push_constant_ranges: &[],
        });
        let gizmo_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:  Some("gizmo-pipeline"),
            layout: Some(&gizmo_pl_layout),
            vertex: wgpu::VertexState {
                module:      &gizmo_shader,
                entry_point: "vs_main",
                buffers:     &[gizmo::GizmoVertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module:      &gizmo_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format:     config.format,
                    blend:      Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology:   wgpu::PrimitiveTopology::TriangleList,
                cull_mode:  None,
                ..Default::default()
            },
            // Sin depth test — los gizmos siempre visibles
            depth_stencil: None,
            multisample:   wgpu::MultisampleState::default(),
            multiview:     None,
            cache:         None,
        });
        let gizmo_buffer = gizmo::build_axes(&device, 1.14);
        let tool_overlay_buffer_init = gizmo::build_from_vertices(&device, &[]);

        // ── Pipeline de grid (LineList, sin depth, reutiliza shader de gizmo) ──
        let grid_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:  Some("grid-pipeline"),
            layout: Some(&gizmo_pl_layout),
            vertex: wgpu::VertexState {
                module:      &gizmo_shader,
                entry_point: "vs_main",
                buffers:     &[gizmo::GizmoVertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module:      &gizmo_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format:     config.format,
                    blend:      Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology:  wgpu::PrimitiveTopology::LineList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample:   wgpu::MultisampleState::default(),
            multiview:     None,
            cache:         None,
        });
        // Buffer de uniforms del grid (view_proj se actualiza en render; model = identity; flags = -1)
        let grid_uni_identity: [[f32; 4]; 9] = [
            [1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0],
            [-1.0, -1.0, 0.0, 0.0],
        ];
        let grid_buffer_uni = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label:    Some("grid-uni"),
            contents: bytemuck::cast_slice(&grid_uni_identity),
            usage:    wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let grid_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:   Some("grid-bg"),
            layout:  &gizmo_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding:  0,
                resource: grid_buffer_uni.as_entire_binding(),
            }],
        });
        let grid_config = GridConfig::default();
        let grid_buffer = crate::config_2d::build_grid(&device, &grid_config);

        // ── Audio: thread dedicado ──────────────────────────────────────────────
        // El thread de audio vive independiente del render thread para que la
        // creaci\u00f3n/destrucci\u00f3n de Sinks de rodio (que puede bloquear en ALSA/PulseAudio)
        // nunca detenga el render loop ni cause drift en el timing de animaciones.
        let audio_slot = start_audio_thread();

        let scene_target = crate::scene_target::SceneTarget::new(
            &device,
            format,
            size.width,
            size.height,
        );

        Ok(Self {
            window,
            surface,
            device,
            queue,
            config,
            size,
            clear_color: wgpu::Color { r: 0.06, g: 0.06, b: 0.10, a: 1.0 },
            render_pipeline,
            render_pipeline_2d,
            render_pipeline_overlay,
            depth_view,
            scene_target,
            scene_buffer:     scene_buf,
            scene_bind_group: scene_bg,
            atlas,
            uv_rects,
            fallback_uv,
            static_tex_cache: std::collections::HashMap::new(),
            canonical_quad_idx: 0,
            camera,
            camera_2d: None,   // se activa al recibir SetScene { scene: "2D" }

            meshes,
            world,
            last_frame:  Instant::now(),
            delta_time:  0.0,
            gizmo_pipeline,
            gizmo_buffer,
            gizmo_bind_group,
            gizmo_buffer_uni,
            physics_2d: PhysicsWorld2D::new(),
            selected_entity:      None,
            selected_entities:    Vec::new(),
            hovered_entity:      None,
            hovered_gizmo_axis:  None,
            active_gizmo_axis:   None,
            spatial_grid: crate::spatial::SpatialGrid::new(),
            scenario_entities:      Vec::new(),
            character_entities:     Vec::new(),
            background_entity:       None,
            background_path:         None,
            grid_config,
            grid_pipeline,
            grid_buffer,
            grid_bind_group,
            grid_buffer_uni,
            ctrl_held: false,
            active_tool: ActiveTool::None,
            quick_build_ghost_id: None,
            quick_build_preview_path: None,
            quick_build_preview_kind: None,
            quick_build_preview_scale: None,
            preview_playing: false,
            debug_mode: false,
            vsync_enabled: false,
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
            visual_offsets: std::collections::HashMap::new(),
            pivot_edit_mode:    None,
            logical_area_mode:  None,
            audio_slot,
            anim_texture_cache: std::collections::HashMap::new(),
            anim_overrides:     std::collections::HashMap::new(),
            animations:            HashMap::new(),
            active_animations:     HashMap::new(),
            default_animation_by_entity: HashMap::new(),
            anim_flip_overrides:   HashMap::new(),
            entity_facing_right:   HashMap::new(),
            script_engine: ScriptEngine::new()
                .expect("Error al inicializar el motor de scripting Lua"),
            control_bindings_by_entity: HashMap::new(),
            sprite_store: HashMap::new(),
            sound_store: HashMap::new(),
            font_store: HashMap::new(),
            background_store: HashMap::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            is_applying_undo: false,
            process_metrics_sampler: rer_engine_shared::process_metrics::ProcessMetricsSampler::new(),
            metrics_last_emit:   Instant::now(),
            metrics_frame_count: 0,
            last_draw_calls:     0,
            autosave_enabled:    false,
            autosave_last_tick:  Instant::now(),
            target_fps:          60,
            blocked_on_keep_horizontal: HashMap::new(),
            pending_slides:      HashMap::new(),
            save_registry: EntitySaveRegistry::new(),
        })
    }
}
