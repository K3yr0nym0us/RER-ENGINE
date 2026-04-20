use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3 as GlamVec3};
use wgpu::{include_wgsl, util::DeviceExt};
use winit::{dpi::PhysicalSize, window::Window};

use crate::camera::{Camera, Camera2D};
use crate::ecs::{MeshComponent, Transform, World};
use crate::gizmo::{self, GizmoBuffer};
use crate::ipc::{send_event, EngineCommand, EngineEvent};
use crate::mesh::{self, Mesh};
use crate::physics::PhysicsWorld;
use crate::texture::GpuTexture;
use crate::ecs::EntityId;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

// ── Uniform que combina view_proj + model ─────────────────────────────────────
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct SceneUniforms {
    view_proj: [[f32; 4]; 4],
    model:     [[f32; 4]; 4],
    cam_pos:   [f32; 4],   // xyz = posición cámara, w = unused
}

// ─────────────────────────────────────────────────────────────────────────────
pub struct State {
    window:           Arc<Window>,
    surface:          wgpu::Surface<'static>,
    device:           wgpu::Device,
    queue:            wgpu::Queue,
    config:           wgpu::SurfaceConfiguration,
    size:             PhysicalSize<u32>,
    clear_color:      wgpu::Color,
    render_pipeline:  wgpu::RenderPipeline,
    depth_view:       wgpu::TextureView,
    // Uniforms (group 0)
    scene_buffer:     wgpu::Buffer,
    scene_bind_group: wgpu::BindGroup,
    // Texturas (group 1)
    texture_bgl:      wgpu::BindGroupLayout,
    textures:         Vec<wgpu::BindGroup>,  // una por mesh
    fallback_tex_bg:  wgpu::BindGroup,       // blanca 1x1
    // Cámara
    pub camera:       Camera,
    /// Cámara 2D ortográfica activa cuando se carga una escena 2D.
    /// `None` = modo 3D (usa `camera`).
    pub camera_2d:    Option<Camera2D>,
    // Escena y mallas
    meshes:           Vec<Mesh>,
    world:            World,
    // Tiempo
    last_frame:       Instant,
    pub delta_time:   f32,
    // Gizmos
    gizmo_pipeline:   wgpu::RenderPipeline,
    gizmo_buffer:     GizmoBuffer,
    gizmo_bgl:        wgpu::BindGroupLayout,
    gizmo_bind_group: wgpu::BindGroup,
    gizmo_buffer_uni: wgpu::Buffer,
    // Física
    pub physics:      PhysicsWorld,
    // Selección
    pub selected_entity:     Option<EntityId>,
    pub hovered_entity:      Option<EntityId>,
    pub hovered_gizmo_axis:  Option<usize>,
    pub active_gizmo_axis:   Option<usize>,
}

impl State {
    /// `is_embed`: si es true, fuerza el backend GL/EGL en vez de Vulkan.
    /// Vulkan (incluso llvmpipe) no soporta presentar en child X11 windows;
    /// EGL sí lo hace mediante software fallback.
    pub async fn new(window: Arc<Window>, is_embed: bool) -> Self {
        let size = window.inner_size();

        // ── Instance & Surface ───────────────────────────────────────────────
        // En modo embed usamos GL (EGL software) porque Vulkan no puede crear
        // una VkSurfaceKHR válida sobre una ventana hijo X11 de otro proceso.
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

        // ── Adapter ──────────────────────────────────────────────────────────
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference:       wgpu::PowerPreference::HighPerformance,
                compatible_surface:     Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("no se encontró adapter compatible");
        log::info!("Adapter: {}", adapter.get_info().name);

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
            present_mode:                  wgpu::PresentMode::AutoVsync,
            alpha_mode:                    caps.alpha_modes[0],
            view_formats:                  vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // ── Depth texture ─────────────────────────────────────────────────────
        let depth_view = create_depth_texture(&device, &config);

        // ── Uniforms buffer ───────────────────────────────────────────────────
        let camera   = Camera::new();
        let uniforms = build_uniforms(&camera, Mat4::IDENTITY, size, 0.0);
        let scene_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label:    Some("scene-uniforms"),
            contents: bytemuck::cast_slice(&[uniforms]),
            usage:    wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

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
        let scene_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:   Some("scene-bg"),
            layout:  &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding:  0,
                resource: scene_buffer.as_entire_binding(),
            }],
        });

        // ── Bind group layout group 1 (textura) + fallback blanco ────────────────
        let texture_bgl   = GpuTexture::bind_group_layout(&device);
        let fallback_tex  = GpuTexture::white(&device, &queue);
        let fallback_tex_bg = fallback_tex.create_bind_group(&device, &texture_bgl);

        // ── Pipeline ─────────────────────────────────────────────────────────
        let shader = device.create_shader_module(include_wgsl!("shader.wgsl"));
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
                buffers:             &[mesh::Vertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module:      &shader,
                entry_point: "fs_main",
                targets:     &[Some(wgpu::ColorTargetState {
                    format,
                    blend:      Some(wgpu::BlendState::REPLACE),
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

        // ── Escenario base: plano de suelo — primera persona ─────────────────
        let ground_plane = mesh::create_ground_plane(&device);
        let meshes       = vec![ground_plane];
        let mut world    = World::new();
        // Entidad del plano
        let plane_id = world.spawn(Some("Ground"));
        world.insert(plane_id, MeshComponent { mesh_idx: 0 });
        // Textura checkerboard para el suelo (índice 0 en self.textures)
        let checker_tex    = crate::texture::GpuTexture::checkerboard(&device, &queue, 2);
        let checker_tex_bg = checker_tex.create_bind_group(&device, &texture_bgl);
        // Cámara en primera persona: ojos a 1.75 m de altura mirando hacia +Z
        let mut camera = Camera::new();
        camera.target   = glam::Vec3::new(0.0, 1.75, 5.0);
        camera.pitch    = 0.0;
        camera.yaw      = -std::f32::consts::FRAC_PI_2;
        camera.distance = 0.01;  // muy cerca — simula la posición del ojo

        // ── Pipeline de gizmos (LineList, sin depth write) ───────────────────
        let gizmo_shader = device.create_shader_module(include_wgsl!("gizmo.wgsl"));
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

        Self {
            window,
            surface,
            device,
            queue,
            config,
            size,
            clear_color: wgpu::Color { r: 0.06, g: 0.06, b: 0.10, a: 1.0 },
            render_pipeline,
            depth_view,
            scene_buffer,
            scene_bind_group,
            texture_bgl,
            textures: vec![checker_tex_bg],   // índice 0 = plano de suelo
            fallback_tex_bg,
            camera,
            camera_2d: None,   // se activa al recibir SetScene { scene: "2D" }
            meshes,
            world,
            last_frame:  Instant::now(),
            delta_time:  0.0,
            gizmo_pipeline,
            gizmo_buffer,
            gizmo_bgl,
            gizmo_bind_group,
            gizmo_buffer_uni,
            physics: PhysicsWorld::new(),
            selected_entity:      None,
            hovered_entity:      None,
            hovered_gizmo_axis:  None,
            active_gizmo_axis:   None,
        }
    }

    // ── Accesores ─────────────────────────────────────────────────────────────

    pub fn window(&self) -> &Arc<Window> { &self.window }
    pub fn size(&self)   -> PhysicalSize<u32> { self.size }

    // ── Resize ───────────────────────────────────────────────────────────────

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 { return; }
        self.size          = new_size;
        self.config.width  = new_size.width;
        self.config.height = new_size.height;
        self.surface.configure(&self.device, &self.config);
        self.depth_view = create_depth_texture(&self.device, &self.config);
    }

    // ── Comandos IPC ─────────────────────────────────────────────────────────

    pub fn handle_command(&mut self, cmd: EngineCommand) {
        match cmd {
            EngineCommand::Ping => {
                send_event(&EngineEvent::Pong);
            }
            EngineCommand::SetClearColor { r, g, b } => {
                self.clear_color = wgpu::Color { r, g, b, a: 1.0 };
            }
            EngineCommand::Resize { width, height } => {
                self.resize(PhysicalSize::new(width, height));
            }            EngineCommand::SetBounds { x, y, width, height } => {
                // Mover la ventana hijo dentro del padre X11
                let _ = self.window.set_outer_position(
                    winit::dpi::PhysicalPosition::new(x, y)
                );
                // Redimensionar superficie wgpu
                self.resize(PhysicalSize::new(width, height));
                // Pedir al compositor que aplique el nuevo tamaño
                let _ = self.window.request_inner_size(
                    winit::dpi::PhysicalSize::new(width, height)
                );
            }            EngineCommand::LoadModel { path } => {
                self.load_model(&path);
            }
            EngineCommand::SetTransform { id, position, rotation, scale } => {
                use glam::{Quat, Vec3};
                if let Some(transform) = self.world.get_mut::<Transform>(id) {
                    if let Some(p) = position {
                        transform.position = Vec3::from(p);
                    }
                    if let Some(r) = rotation {
                        transform.rotation = Quat::from_xyzw(r[0], r[1], r[2], r[3]);
                    }
                    if let Some(s) = scale {
                        transform.scale = Vec3::from(s);
                    }
                }
            }
            EngineCommand::SetScene { scene } => {
                match scene.as_str() {
                    "2D"      => self.setup_2d_platformer(),
                    "scratch" => self.setup_scratch(),
                    _         => log::info!("SetScene: escena '{}' no reconocida", scene),
                }
            }
            EngineCommand::Shutdown => {}
        }
    }

    // ── Escena "desde cero" — cubo de referencia ─────────────────────────────

    fn setup_scratch(&mut self) {
        self.world.clear();
        self.meshes.clear();
        self.textures.clear();
        self.selected_entity = None;
        self.hovered_entity  = None;
        self.camera_2d       = None;  // volver a modo 3D

        // Cubo central con textura blanca (fallback)
        let cube = mesh::create_cube(&self.device);
        self.meshes.push(cube);
        self.textures.push(
            GpuTexture::white(&self.device, &self.queue)
                .create_bind_group(&self.device, &self.texture_bgl),
        );
        let cube_id = self.world.spawn(Some("Cube"));
        self.world.insert(cube_id, MeshComponent { mesh_idx: 0 });

        // Cámara orbital por defecto mirando el cubo
        self.camera = Camera::new();
        self.clear_color = wgpu::Color { r: 0.06, g: 0.06, b: 0.10, a: 1.0 };

        log::info!("Escena 'desde cero' cargada: cubo de referencia");
    }

    // ── Escena 2D — plataformer vista lateral (Hollow Knight style) ──────────

    fn setup_2d_platformer(&mut self) {
        // Limpiar escena previa
        self.world.clear();
        self.meshes.clear();
        self.textures.clear();
        self.selected_entity  = None;
        self.hovered_entity   = None;

        // Quad unitario centrado en el origen (0,0) — el Transform lo posiciona y escala.
        // Esto es imprescindible para que el picking AABB, el gizmo y el hover funcionen.

        // -- Suelo: quad unitario × Transform(scale=40×1.5, pos=(0,0.75)) ----
        let ground_mesh = mesh::create_quad_xy(&self.device, 0.0, 0.0, 1.0, 1.0, "ground-unit");
        self.meshes.push(ground_mesh);
        let ground_tex = GpuTexture::solid_color(&self.device, &self.queue, 122, 106, 88);
        self.textures.push(ground_tex.create_bind_group(&self.device, &self.texture_bgl));
        let ground_id = self.world.spawn(Some("Ground"));
        self.world.insert(ground_id, MeshComponent { mesh_idx: 0 });
        self.world.insert(ground_id, crate::ecs::Transform {
            position: GlamVec3::new(0.0, 0.75, 0.0),
            scale:    GlamVec3::new(40.0, 1.5, 1.0),
            ..Default::default()
        });

        // -- Personaje: quad unitario × Transform(scale=1.0×1.5, pos=(0,2.25)) --
        let player_mesh = mesh::create_quad_xy(&self.device, 0.0, 0.0, 1.0, 1.0, "player-unit");
        self.meshes.push(player_mesh);
        let player_tex = GpuTexture::solid_color(&self.device, &self.queue, 232, 220, 200);
        self.textures.push(player_tex.create_bind_group(&self.device, &self.texture_bgl));
        let player_id = self.world.spawn(Some("Player"));
        self.world.insert(player_id, MeshComponent { mesh_idx: 1 });
        self.world.insert(player_id, crate::ecs::Transform {
            position: GlamVec3::new(0.0, 2.25, 0.0),
            scale:    GlamVec3::new(1.0, 1.5, 1.0),
            ..Default::default()
        });

        // -- Cámara ortográfica -----------------------------------------------
        self.camera_2d = Some(Camera2D {
            x:      0.0,
            y:      2.0,
            half_h: 3.5,
            near:  -100.0,
            far:    100.0,
        });

        // Fondo oscuro azulado (estilo Hollow Knight)
        self.clear_color = wgpu::Color { r: 0.04, g: 0.04, b: 0.10, a: 1.0 };

        log::info!("Escena 2D cargada: plataformer vista lateral");
    }

    fn load_model(&mut self, path: &str) {
        match mesh::load_glb(&self.device, Path::new(path)) {
            Ok((gltf_meshes, images)) => {
                self.world.clear();
                self.meshes.clear();
                self.textures.clear();

                let count = gltf_meshes.len();
                for (i, gm) in gltf_meshes.into_iter().enumerate() {
                    let tex_bg = if let Some(tex_idx) = gm.tex_index {
                        if let Some(img_data) = images.get(tex_idx) {
                            let gpu_tex = GpuTexture::from_gltf_image(
                                &self.device, &self.queue, img_data,
                                &format!("tex-{tex_idx}"),
                            );
                            gpu_tex.create_bind_group(&self.device, &self.texture_bgl)
                        } else {
                            GpuTexture::white(&self.device, &self.queue)
                                .create_bind_group(&self.device, &self.texture_bgl)
                        }
                    } else {
                        GpuTexture::white(&self.device, &self.queue)
                            .create_bind_group(&self.device, &self.texture_bgl)
                    };

                    self.meshes.push(gm.mesh);
                    self.textures.push(tex_bg);

                    let label = format!("Mesh {i}");
                    let id = self.world.spawn(Some(&label));
                    self.world.insert(id, MeshComponent { mesh_idx: i });
                    send_event(&EngineEvent::ModelLoaded { id });
                }
                log::info!("Modelo cargado: {path} ({count} malla/s)");
            }
            Err(e) => {
                log::error!("Error cargando modelo: {e}");
                send_event(&EngineEvent::Error { message: e });
            }
        }
    }

    // ── Proyección 2D ortográfica ─────────────────────────────────────────────

    /// Proyecta un punto XY de mundo a píxeles de pantalla usando la cámara 2D.
    fn project_to_screen_2d(&self, cam: &Camera2D, p: GlamVec3) -> Option<(f32, f32)> {
        let w  = self.size.width  as f32;
        let h  = self.size.height as f32;
        let vp = cam.view_proj(w / h);
        let c  = vp * glam::Vec4::new(p.x, p.y, p.z, 1.0);
        if c.w.abs() < 1e-6 { return None; }
        Some(((c.x / c.w + 1.0) * 0.5 * w, (1.0 - c.y / c.w) * 0.5 * h))
    }

    /// Picking 2D: convierte un píxel a coordenadas de mundo XY y prueba AABB.
    pub fn pick_entity_2d(&mut self, pixel_x: f32, pixel_y: f32) {
        let cam = match &self.camera_2d {
            Some(c) => Camera2D { x: c.x, y: c.y, half_h: c.half_h, near: c.near, far: c.far },
            None    => return,
        };
        let w      = self.size.width  as f32;
        let h      = self.size.height as f32;
        let aspect = w / h;
        let half_w = cam.half_h * aspect;
        // NDC → mundo
        let wx = cam.x + ((pixel_x / w) * 2.0 - 1.0) * half_w;
        let wy = cam.y + (1.0 - (pixel_y / h) * 2.0) * cam.half_h;

        let mut hit: Option<EntityId> = None;
        for &entity in self.world.entities() {
            if let Some(transform) = self.world.get::<Transform>(entity) {
                // AABB en XY usando la escala de la entidad
                let p  = transform.position;
                let sx = transform.scale.x * 0.5;
                let sy = transform.scale.y * 0.5;
                if wx >= p.x - sx && wx <= p.x + sx && wy >= p.y - sy && wy <= p.y + sy {
                    hit = Some(entity);
                    break;
                }
            }
        }
        match hit {
            Some(entity) => {
                if self.selected_entity == Some(entity) { return; }
                self.selected_entity = Some(entity);
                let name      = self.world.name(entity).unwrap_or("Entity").to_string();
                let transform = self.world.get::<Transform>(entity).cloned().unwrap_or_default();
                let pos = transform.position.to_array();
                let rot = [transform.rotation.x, transform.rotation.y,
                           transform.rotation.z, transform.rotation.w];
                let scl = transform.scale.to_array();
                send_event(&EngineEvent::EntitySelected { id: entity, name, position: pos, rotation: rot, scale: scl });
            }
            None => {
                if self.selected_entity.is_some() {
                    self.selected_entity = None;
                    send_event(&EngineEvent::EntityDeselected);
                }
            }
        }
    }

    /// Eje del gizmo 2D más cercano al píxel (0=X, 1=Y). Solo X e Y en 2D.
    pub fn pick_gizmo_axis_2d(&self, pixel_x: f32, pixel_y: f32) -> Option<usize> {
        let sel_id = self.selected_entity?;
        let origin = self.world.get::<Transform>(sel_id)?.position;
        let cam = self.camera_2d.as_ref()?;
        let so = self.project_to_screen_2d(cam, origin)?;

        const LEN: f32 = 1.2;
        const THRESH: f32 = 16.0;
        // Solo ejes X e Y (índices 0 y 1)
        let dirs = [GlamVec3::X, GlamVec3::Y];
        let mut best: Option<(f32, usize)> = None;
        for (i, &dir) in dirs.iter().enumerate() {
            if let Some(tip) = self.project_to_screen_2d(cam, origin + dir * LEN) {
                let d = point_to_segment_2d(pixel_x, pixel_y, so.0, so.1, tip.0, tip.1);
                if d < THRESH && best.map_or(true, |(bd, _)| d < bd) {
                    best = Some((d, i));
                }
            }
        }
        best.map(|(_, i)| i)
    }

    /// Arrastra la entidad seleccionada sobre el eje X o Y en modo 2D.
    pub fn drag_gizmo_2d(&mut self, pixel_x: f32, pixel_y: f32, last_x: f32, last_y: f32, axis_idx: usize) {
        let sel_id = match self.selected_entity { Some(id) => id, None => return };
        let cam = match &self.camera_2d {
            Some(c) => Camera2D { x: c.x, y: c.y, half_h: c.half_h, near: c.near, far: c.far },
            None    => return,
        };
        let origin = match self.world.get::<Transform>(sel_id) {
            Some(t) => t.position,
            None    => return,
        };
        // Solo X o Y, ignorar Z
        let axis_world = if axis_idx == 0 { GlamVec3::X } else { GlamVec3::Y };
        let so  = match self.project_to_screen_2d(&cam, origin)          { Some(p) => p, None => return };
        let se  = match self.project_to_screen_2d(&cam, origin + axis_world) { Some(p) => p, None => return };
        let ax  = se.0 - so.0;
        let ay  = se.1 - so.1;
        let len = (ax * ax + ay * ay).sqrt();
        if len < 1e-4 { return; }
        let dx = pixel_x - last_x;
        let dy = pixel_y - last_y;
        let world_delta = (dx * ax + dy * ay) / (len * len);
        let name = self.world.name(sel_id).unwrap_or("Entity").to_string();
        if let Some(t) = self.world.get_mut::<Transform>(sel_id) {
            t.position += axis_world * world_delta;
            let pos = t.position.to_array();
            let rot = [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w];
            let scl = t.scale.to_array();
            send_event(&EngineEvent::EntitySelected { id: sel_id, name, position: pos, rotation: rot, scale: scl });
        }
    }

    // ── Picking (ray cast CPU) ────────────────────────────────────────────────

    /// Rayo desde píxel de pantalla — devuelve la entidad más cercana (si hay).
    fn ray_cast(&self, pixel_x: f32, pixel_y: f32) -> Option<EntityId> {
        use glam::Vec4;

        let w      = self.size.width  as f32;
        let h      = self.size.height as f32;
        let aspect = w / h;

        let ndc_x =  (2.0 * pixel_x / w) - 1.0;
        let ndc_y = -(2.0 * pixel_y / h) + 1.0;

        let inv_proj = self.camera.proj_matrix(aspect).inverse();
        let inv_view = self.camera.view_matrix().inverse();

        let clip_dir  = Vec4::new(ndc_x, ndc_y, -1.0, 0.0);
        let view_dir  = inv_proj * clip_dir;
        let view_dir  = Vec4::new(view_dir.x, view_dir.y, -1.0, 0.0);
        let world_dir = (inv_view * view_dir).truncate().normalize();
        let ray_origin = self.camera.position();

        let mut closest: Option<(f32, EntityId)> = None;
        for &entity in self.world.entities() {
            if let Some(transform) = self.world.get::<Transform>(entity) {
                let center = transform.position;
                let radius = transform.scale.x.max(transform.scale.y).max(transform.scale.z) * 0.866;
                let oc   = ray_origin - center;
                let b    = oc.dot(world_dir);
                let c    = oc.dot(oc) - radius * radius;
                let disc = b * b - c;
                if disc >= 0.0 {
                    let t = -b - disc.sqrt();
                    if t > 0.0 && closest.map_or(true, |(ct, _)| t < ct) {
                        closest = Some((t, entity));
                    }
                }
            }
        }
        closest.map(|(_, id)| id)
    }

    // ── Gizmo picking & drag ──────────────────────────────────────────────────

    /// Proyecta un punto 3D a coordenadas de pantalla en píxeles.
    fn project_to_screen(&self, p: GlamVec3) -> Option<(f32, f32)> {
        let w  = self.size.width  as f32;
        let h  = self.size.height as f32;
        let vp = self.camera.proj_matrix(w / h) * self.camera.view_matrix();
        let c  = vp * glam::Vec4::new(p.x, p.y, p.z, 1.0);
        if c.w <= 0.0 { return None; }
        Some(((c.x / c.w + 1.0) * 0.5 * w, (1.0 - c.y / c.w) * 0.5 * h))
    }

    /// Devuelve el índice del eje del gizmo más cercano al píxel (0=X,1=Y,2=Z),
    /// o None si el click no está sobre ninguno.
    pub fn pick_gizmo_axis(&self, pixel_x: f32, pixel_y: f32) -> Option<usize> {
        let sel_id = self.selected_entity?;
        let origin = self.world.get::<Transform>(sel_id)?.position;
        let so     = self.project_to_screen(origin)?;

        const LEN: f32 = 1.2;
        const THRESH: f32 = 16.0; // píxeles, más fácil de agarrar
        let dirs = [GlamVec3::X, GlamVec3::Y, GlamVec3::Z];

        let mut best: Option<(f32, usize)> = None;
        for (i, &dir) in dirs.iter().enumerate() {
            if let Some(tip) = self.project_to_screen(origin + dir * LEN) {
                let d = point_to_segment_2d(pixel_x, pixel_y, so.0, so.1, tip.0, tip.1);
                if d < THRESH && best.map_or(true, |(bd, _)| d < bd) {
                    best = Some((d, i));
                }
            }
        }
        best.map(|(_, i)| i)
    }

    /// Mueve la entidad seleccionada a lo largo del eje `axis_idx` según el
    /// desplazamiento del cursor desde (last_x, last_y) a (pixel_x, pixel_y).
    pub fn drag_gizmo(&mut self, pixel_x: f32, pixel_y: f32, last_x: f32, last_y: f32, axis_idx: usize) {
        let sel_id = match self.selected_entity { Some(id) => id, None => return };
        let w = self.size.width  as f32;
        let h = self.size.height as f32;
        let aspect = w / h;

        let origin = match self.world.get::<Transform>(sel_id) {
            Some(t) => t.position,
            None    => return,
        };

        let vp = self.camera.proj_matrix(aspect) * self.camera.view_matrix();
        let axis_world = [GlamVec3::X, GlamVec3::Y, GlamVec3::Z][axis_idx];

        let project = |p: GlamVec3| -> Option<(f32, f32)> {
            let c = vp * glam::Vec4::new(p.x, p.y, p.z, 1.0);
            if c.w <= 0.0 { return None; }
            Some(((c.x / c.w + 1.0) * 0.5 * w, (1.0 - c.y / c.w) * 0.5 * h))
        };

        let (s0x, s0y) = match project(origin)            { Some(p) => p, None => return };
        let (s1x, s1y) = match project(origin + axis_world) { Some(p) => p, None => return };

        let ax = s1x - s0x;
        let ay = s1y - s0y;
        let axis_len = (ax * ax + ay * ay).sqrt();
        if axis_len < 1e-4 { return; }

        // Proyectar delta del cursor sobre la dirección del eje en pantalla
        let dx = pixel_x - last_x;
        let dy = pixel_y - last_y;
        let world_delta = (dx * ax + dy * ay) / (axis_len * axis_len);

        let name = self.world.name(sel_id).unwrap_or("Entity").to_string();
        if let Some(t) = self.world.get_mut::<Transform>(sel_id) {
            t.position += axis_world * world_delta;
            let pos = t.position.to_array();
            let rot = [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w];
            let scl = t.scale.to_array();
            send_event(&EngineEvent::EntitySelected { id: sel_id, name, position: pos, rotation: rot, scale: scl });
        }
    }

    /// Actualiza `hovered_entity` y `hovered_gizmo_axis` según la posición del cursor (sin IPC).
    pub fn update_hover(&mut self, pixel_x: f32, pixel_y: f32) {
        self.hovered_entity    = self.ray_cast(pixel_x, pixel_y);
        self.hovered_gizmo_axis = self.pick_gizmo_axis(pixel_x, pixel_y);
    }

    /// Hover en modo 2D: AABB en XY + detección de eje de gizmo 2D.
    pub fn update_hover_2d(&mut self, pixel_x: f32, pixel_y: f32) {
        let cam = match &self.camera_2d {
            Some(c) => Camera2D { x: c.x, y: c.y, half_h: c.half_h, near: c.near, far: c.far },
            None    => return,
        };
        let w      = self.size.width  as f32;
        let h      = self.size.height as f32;
        let aspect = w / h;
        let half_w = cam.half_h * aspect;
        let wx = cam.x + ((pixel_x / w) * 2.0 - 1.0) * half_w;
        let wy = cam.y + (1.0 - (pixel_y / h) * 2.0) * cam.half_h;

        self.hovered_entity = None;
        for &entity in self.world.entities() {
            if let Some(t) = self.world.get::<Transform>(entity) {
                let sx = t.scale.x * 0.5;
                let sy = t.scale.y * 0.5;
                if wx >= t.position.x - sx && wx <= t.position.x + sx
                && wy >= t.position.y - sy && wy <= t.position.y + sy {
                    self.hovered_entity = Some(entity);
                    break;
                }
            }
        }
        self.hovered_gizmo_axis = self.pick_gizmo_axis_2d(pixel_x, pixel_y);
    }

    /// Notifica al State qué eje del gizmo está siendo arrastrado (None = sin drag).
    pub fn set_active_gizmo_axis(&mut self, axis: Option<usize>) {
        self.active_gizmo_axis = axis;
    }

    /// Selecciona la entidad bajo el cursor. No emite IPC si ya estaba seleccionada.
    pub fn pick_entity(&mut self, pixel_x: f32, pixel_y: f32) {
        match self.ray_cast(pixel_x, pixel_y) {
            Some(entity) => {
                // Evitar duplicar el evento si ya estaba seleccionado
                if self.selected_entity == Some(entity) { return; }
                self.selected_entity = Some(entity);
                let name      = self.world.name(entity).unwrap_or("Entity").to_string();
                let transform = self.world.get::<Transform>(entity).cloned().unwrap_or_default();
                let position  = transform.position.to_array();
                let rotation  = [
                    transform.rotation.x, transform.rotation.y,
                    transform.rotation.z, transform.rotation.w,
                ];
                let scale = transform.scale.to_array();
                send_event(&EngineEvent::EntitySelected { id: entity, name, position, rotation, scale });
            }
            None => {
                if self.selected_entity.is_some() {
                    self.selected_entity = None;
                    send_event(&EngineEvent::EntityDeselected);
                }
            }
        }
    }

    // ── Update ───────────────────────────────────────────────────────────────

    pub fn update(&mut self) {
        let now         = Instant::now();
        self.delta_time = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;
        // Paso de simulación física
        self.physics.step(self.delta_time, &mut self.world);
    }

    // ── Render ───────────────────────────────────────────────────────────────

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output  = self.surface.get_current_texture()?;
        let view    = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut enc = self.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("render-encoder") },
        );

        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("render-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view:           &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load:  wgpu::LoadOp::Clear(self.clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load:  wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes:    None,
            });

            pass.set_pipeline(&self.render_pipeline);

            // Iterar entidades con MeshComponent
            let entities: Vec<_> = self.world.entities().iter().copied().filter_map(|id| {
                let mesh_idx  = self.world.get::<MeshComponent>(id)?.mesh_idx;
                let model_mat = self.world.get::<Transform>(id)?.to_matrix();
                Some((id, mesh_idx, model_mat))
            }).collect();

            for (entity_id, idx, model_matrix) in entities {
                if let Some(mesh) = self.meshes.get(idx) {
                    let flag = if self.selected_entity == Some(entity_id) {
                        1.0_f32   // dorado
                    } else if self.hovered_entity == Some(entity_id) {
                        2.0_f32   // cian
                    } else {
                        0.0_f32
                    };
                    let uniforms = if let Some(cam2d) = &self.camera_2d {
                        build_uniforms_2d(cam2d, model_matrix, self.size, flag)
                    } else {
                        build_uniforms(&self.camera, model_matrix, self.size, flag)
                    };
                    self.queue.write_buffer(
                        &self.scene_buffer, 0, bytemuck::cast_slice(&[uniforms]),
                    );
                    pass.set_bind_group(0, &self.scene_bind_group, &[]);
                    let tex_bg = self.textures.get(idx)
                        .unwrap_or(&self.fallback_tex_bg);
                    pass.set_bind_group(1, tex_bg, &[]);
                    pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                    pass.set_index_buffer(
                        mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32,
                    );
                    pass.draw_indexed(0..mesh.index_count, 0, 0..1);
                }
            }
        }

        // ── Gizmos (segundo pass, sin depth) ─────────────────────────────────
        if let Some(sel_id) = self.selected_entity {
            let aspect   = self.size.width as f32 / self.size.height as f32;
            let vp = if let Some(cam2d) = &self.camera_2d {
                cam2d.view_proj(aspect).to_cols_array_2d()
            } else {
                self.camera.to_uniform(aspect).view_proj
            };

            // Situar el gizmo en el centro de la entidad seleccionada
            let gizmo_model = self.world.get::<Transform>(sel_id)
                .map(|t| glam::Mat4::from_translation(t.position))
                .unwrap_or(glam::Mat4::IDENTITY);

            let gm = gizmo_model.to_cols_array_2d();
            let h_ax = self.hovered_gizmo_axis.map(|a| a as f32).unwrap_or(-1.0);
            let a_ax = self.active_gizmo_axis.map(|a| a as f32).unwrap_or(-1.0);
            let gizmo_uni: [[f32; 4]; 9] = [
                vp[0], vp[1], vp[2], vp[3],
                gm[0], gm[1], gm[2], gm[3],
                [h_ax, a_ax, 0.0, 0.0],
            ];
            self.queue.write_buffer(
                &self.gizmo_buffer_uni, 0, bytemuck::cast_slice(&gizmo_uni),
            );

            let mut gpass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("gizmo-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view:           &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load:  wgpu::LoadOp::Load,   // preservar frame anterior
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set:      None,
                timestamp_writes:         None,
            });
            gpass.set_pipeline(&self.gizmo_pipeline);
            gpass.set_bind_group(0, &self.gizmo_bind_group, &[]);
            gpass.set_vertex_buffer(0, self.gizmo_buffer.vertex_buffer.slice(..));
            gpass.draw(0..self.gizmo_buffer.vertex_count, 0..1);
        }

        self.queue.submit(std::iter::once(enc.finish()));
        output.present();
        Ok(())
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn create_depth_texture(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
) -> wgpu::TextureView {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth-texture"),
        size: wgpu::Extent3d {
            width:                 config.width.max(1),
            height:                config.height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count:    1,
        dimension:       wgpu::TextureDimension::D2,
        format:          DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

fn build_uniforms(camera: &Camera, model: Mat4, size: PhysicalSize<u32>, flag: f32) -> SceneUniforms {
    let aspect    = size.width as f32 / size.height as f32;
    let view_proj = camera.to_uniform(aspect).view_proj;
    let p = camera.position();
    SceneUniforms {
        view_proj,
        model: model.to_cols_array_2d(),
        cam_pos: [p.x, p.y, p.z, flag],
    }
}

fn build_uniforms_2d(cam: &Camera2D, model: Mat4, size: PhysicalSize<u32>, flag: f32) -> SceneUniforms {
    let aspect    = size.width as f32 / size.height as f32;
    let view_proj = cam.view_proj(aspect).to_cols_array_2d();
    let p = cam.position();
    SceneUniforms {
        view_proj,
        model: model.to_cols_array_2d(),
        cam_pos: [p.x, p.y, p.z, flag],
    }
}

/// Distancia 2D desde el punto (px,py) al segmento [(ax,ay),(bx,by)].
fn point_to_segment_2d(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let dx = bx - ax;
    let dy = by - ay;
    let len_sq = dx * dx + dy * dy;
    if len_sq < 1e-6 {
        return ((px - ax).powi(2) + (py - ay).powi(2)).sqrt();
    }
    let t  = ((px - ax) * dx + (py - ay) * dy) / len_sq;
    let t  = t.clamp(0.0, 1.0);
    let cx = ax + t * dx;
    let cy = ay + t * dy;
    ((px - cx).powi(2) + (py - cy).powi(2)).sqrt()
}
