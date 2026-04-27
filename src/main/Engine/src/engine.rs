use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3 as GlamVec3};
use wgpu::{include_wgsl, util::DeviceExt};
use winit::{dpi::PhysicalSize, window::Window};
use rodio;
use rodio::Source as RodioSource;

// ---------------------------------------------------------------------------
// Thread dedicado de audio
// ---------------------------------------------------------------------------

/// Audio pre-decodificado a muestras PCM listas para reproducción instantánea.
/// Se genera una vez en `SetAnimation` y se reutiliza en cada `PlayAnimation`.
pub struct DecodedAudio {
    pub samples:     Vec<i16>,
    pub channels:    u16,
    pub sample_rate: u32,
}

/// Comandos enviados al thread de audio.
pub enum AudioCmd {
    /// Reproducir audio desde muestras PCM ya decodificadas en RAM.
    /// El Sink nunca se destruye — solo se vacía la cola y se agrega el nuevo source.
    Play { audio: Arc<DecodedAudio>, loop_: bool },
    /// Detener el audio en curso (vacía la cola, el Sink sigue vivo).
    Stop,
}

/// Single-slot "latest wins": solo el comando más reciente importa.
/// Si el thread de audio está ocupado procesando y llegan 10 Play seguidos,
/// solo se ejecuta el último — sin acumulación de cola.
type AudioSlot = Arc<(Mutex<Option<AudioCmd>>, Condvar)>;

/// Envía un comando al thread de audio sobreescribiendo cualquier
/// comando pendiente aún no procesado.
fn send_audio(slot: &AudioSlot, cmd: AudioCmd) {
    let (lock, cvar) = &**slot;
    *lock.lock().unwrap() = Some(cmd);
    cvar.notify_one();
}

/// Lanza el thread dedicado de audio.
///
/// Diseño:
///   - Un único `OutputStream` (conexión ALSA) vive todo el tiempo del thread.
///   - Cada `Play` crea un Sink NUEVO desde el handle existente (sin sink.clear()).
///     `sink.clear()` puede deadlock en WSL/ALSA cuando el stream subyacente se invalida;
///     un Sink fresco evita ese riesgo completamente.
///   - Sonidos no-looping: `sink.detach()` → fire & forget, múltiples SFX simultáneos.
///   - Sonido looping: se guarda en `loop_sink` y se reemplaza en el siguiente Play.
///   - `Sink::try_new(&handle)` es O(1) (solo envía un mensaje al mixer existente),
///     muy distinto de `OutputStream::try_default()` que abre un nuevo dispositivo ALSA.
fn start_audio_thread() -> Option<AudioSlot> {
    let slot: AudioSlot = Arc::new((Mutex::new(None), Condvar::new()));
    let slot_thread = Arc::clone(&slot);
    std::thread::Builder::new()
        .name("audio".into())
        .spawn(move || {
            let (_stream, handle) = match rodio::OutputStream::try_default() {
                Ok(pair) => pair,
                Err(e) => {
                    log::error!("[audio] thread: no se pudo abrir dispositivo: {e}");
                    return;
                }
            };

            let (lock, cvar) = &*slot_thread;
            // Sink para sonido looping (música, ambience). None = ninguno activo.
            let mut loop_sink: Option<rodio::Sink> = None;

            loop {
                let cmd = {
                    let mut guard = lock.lock().unwrap();
                    loop {
                        if let Some(cmd) = guard.take() {
                            break cmd;
                        }
                        guard = cvar.wait(guard).unwrap();
                    }
                };
                match cmd {
                    AudioCmd::Stop => {
                        // Detener música looping si la hay; drop() detiene el Sink.
                        if let Some(s) = loop_sink.take() {
                            drop(s);
                            log::info!("[audio] música detenida");
                        } else {
                            log::info!("[audio] detenido (sin loop activo)");
                        }
                    }
                    AudioCmd::Play { audio, loop_ } => {
                        // Crear un Sink fresco por reproducción — evita sink.clear() y
                        // permite múltiples SFX simultáneos vía detach().
                        let sink = match rodio::Sink::try_new(&handle) {
                            Ok(s) => s,
                            Err(e) => {
                                log::error!("[audio] no se pudo crear sink: {e}");
                                continue;
                            }
                        };
                        let source = rodio::buffer::SamplesBuffer::new(
                            audio.channels,
                            audio.sample_rate,
                            audio.samples.clone(),
                        );
                        if loop_ {
                            // Reemplazar música anterior (drop detiene la anterior).
                            if let Some(prev) = loop_sink.take() { drop(prev); }
                            sink.append(source.repeat_infinite());
                            sink.play();
                            loop_sink = Some(sink);
                        } else {
                            // SFX one-shot: fire & forget. Varios pueden solaparse.
                            sink.append(source);
                            sink.play();
                            sink.detach();
                        }
                        log::info!("[audio] reproduciendo ({} muestras, {}ch, {}Hz, loop={})",
                            audio.samples.len(), audio.channels, audio.sample_rate, loop_);
                    }
                }
            }
        })
        .expect("no se pudo crear el thread de audio");
    log::info!("[audio] dispositivo de audio inicializado");
    Some(slot)
}
use crate::config_2d::{GridBuffer, GridConfig};
use crate::config_2d::ActiveTool;

use crate::config_3d::Camera;
use crate::config_2d::Camera2D;
use crate::config_2d::PhysicsWorld2D;
use crate::ecs::{MeshComponent, Transform, World};
use crate::gizmo::{self, GizmoBuffer};
use crate::ipc::{send_event, AnimationFrameData, EngineCommand, EngineEvent};
use crate::mesh::{self, Mesh};
use crate::config_3d::physics_3d::PhysicsWorld;
use crate::scripting::{ScriptEngine, ScriptCmd, EntitySnapshot};

#[derive(Clone)]
pub struct AnimationState {
    pub frames:        Vec<AnimationFrameData>,
    pub fps:           u32,
    pub loop_:         bool,
    /// Audio pre-decodificado a muestras PCM durante SetAnimation.
    /// `None` si la animación no tiene audio o falló la decodificación.
    pub audio_decoded: Option<Arc<DecodedAudio>>,
    pub logical_w:     u32,
    pub logical_h:     u32,
}

pub struct ActiveAnimation {
    pub animation_name: String,
    pub current_frame: usize,
    pub last_frame_time: Instant,
    pub fps: u32,
    pub finished: bool,
}
use crate::texture::GpuTexture;
use crate::ecs::EntityId;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

// ── Uniform que combina view_proj + model ─────────────────────────────────────
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct SceneUniforms {
    pub(crate) view_proj: [[f32; 4]; 4],
    pub(crate) model:     [[f32; 4]; 4],
    pub(crate) cam_pos:   [f32; 4],   // xyz = posición cámara, w = unused
}

// ─────────────────────────────────────────────────────────────────────────────
pub struct State {
    pub(crate) window:           Arc<Window>,
    pub(crate) surface:          wgpu::Surface<'static>,
    pub(crate) device:           wgpu::Device,
    pub(crate) queue:            wgpu::Queue,
    pub(crate) config:           wgpu::SurfaceConfiguration,
    pub(crate) size:             PhysicalSize<u32>,
    pub(crate) clear_color:      wgpu::Color,
    pub(crate) render_pipeline:     wgpu::RenderPipeline,
    /// Pipeline para modo 2D: sin depth-write, CompareFunction::Always.
    /// Permite que el alpha blending funcione correctamente con back-to-front sort.
    pub(crate) render_pipeline_2d:  wgpu::RenderPipeline,
    pub(crate) depth_view:       wgpu::TextureView,
    // Uniforms (group 0) — un buffer por malla para que cada draw call
    // tenga sus propios datos y write_buffer no sobreescriba el anterior.
    pub(crate) scene_bgl:          wgpu::BindGroupLayout,
    pub(crate) entity_buffers:     Vec<wgpu::Buffer>,
    pub(crate) entity_bind_groups: Vec<wgpu::BindGroup>,
    // Texturas (group 1)
    pub(crate) texture_bgl:      wgpu::BindGroupLayout,
    pub(crate) textures:         Vec<wgpu::BindGroup>,  // una por mesh
    pub(crate) fallback_tex_bg:  wgpu::BindGroup,       // blanca 1x1
    // Cámara
    pub camera:       Camera,
    /// Cámara 2D ortográfica activa cuando se carga una escena 2D.
    /// `None` = modo 3D (usa `camera`).
    pub camera_2d:    Option<Camera2D>,
    // Escena y mallas
    pub(crate) meshes:           Vec<Mesh>,
    pub(crate) world:            World,
    // Tiempo
    pub(crate) last_frame:       Instant,
    pub        delta_time:       f32,
    // Gizmos
    pub(crate) gizmo_pipeline:   wgpu::RenderPipeline,
    pub(crate) gizmo_buffer:     GizmoBuffer,
    pub(crate) gizmo_bind_group: wgpu::BindGroup,
    pub(crate) gizmo_buffer_uni: wgpu::Buffer,
    // Física
    pub physics:      PhysicsWorld,
    pub physics_2d:   PhysicsWorld2D,
    // Selección
    pub selected_entity:     Option<EntityId>,
    pub hovered_entity:      Option<EntityId>,
    pub hovered_gizmo_axis:  Option<usize>,
    pub active_gizmo_axis:   Option<usize>,
    // Escenario 2D: lista de entidades ECS que actúan como fondos PNG.
    pub(crate) scenario_entities: Vec<EntityId>,
    // Personajes 2D: lista de entidades ECS que actúan como sprites de personaje.
    pub(crate) character_entities: Vec<EntityId>,
    // Fondo del mundo 2D: entidad especial no seleccionable que cubre todo el área.
    pub(crate) background_entity: Option<EntityId>,
    // Grid 2D: cuadrícula y límites del mundo.
    pub(crate) grid_config:      GridConfig,
    pub(crate) grid_pipeline:    wgpu::RenderPipeline,
    pub(crate) grid_buffer:      GridBuffer,
    pub(crate) grid_bind_group:  wgpu::BindGroup,
    pub(crate) grid_buffer_uni:  wgpu::Buffer,
    /// Estado de la tecla Ctrl (enviado por IPC desde Electron, ya que la ventana embebida
    /// no recibe keyboard events directamente).
    pub(crate) ctrl_held:        bool,
    /// Herramienta de dibujo activa en modo 2D.
    pub        active_tool:      ActiveTool,
    /// Buffer de overlay de la herramienta activa (cruces + líneas de construcción).
    pub(crate) tool_overlay_buffer: GizmoBuffer,
    /// Entidades creadas por herramientas de dibujo (colisionadores).
    pub(crate) collider_entities: Vec<EntityId>,
    /// Transforms originales guardados antes de aplicar un frame de animación
    /// (posición, escala). Se restauran con RestoreAnimationFrame.
    pub(crate) anim_saved_transforms: std::collections::HashMap<u32, (GlamVec3, GlamVec3)>,
    /// Estado del modo edición de pivot: (entity_id, frame_path, img_w, img_h).
    /// Cuando es Some, el siguiente click izquierdo en el viewport calcula el pivot.
    pub pivot_edit_mode: Option<(u32, String, u32, u32)>,
    /// Modo visualización del área lógica: Some(entity_id) cuando el overlay naranja está activo.
    pub logical_area_mode: Option<u32>,
    /// Canal para enviar comandos de audio al thread dedicado.
    /// El thread de audio vive independiente del render thread para que
    /// la creaci\u00f3n/destrucci\u00f3n de Sinks de rodio (que puede bloquear en ALSA/PulseAudio)
    /// nunca detenga el render loop ni cause drift en el timing de animaciones.
    pub(crate) audio_slot: Option<AudioSlot>,
    /// Caché de texturas GPU para frames de animación, indexada por ruta absoluta.
    /// Almacena (BindGroup, img_width, img_height) para evitar recargar de disco,
    /// redecodificar y resubir a GPU en cada tick. Se limpia al cambiar de escena.
    pub(crate) anim_texture_cache: std::collections::HashMap<String, (std::sync::Arc<wgpu::BindGroup>, u32, u32)>,
    /// Overrides de textura para animaciones activas: tex_position → bind group.
    /// Play_animation_frame escribe aquí en lugar de mutar textures[],
    /// así la textura base de la entidad nunca se sobreescribe.
    /// Restore_animation_frame borra la entrada; el render loop vuelve a textures[].
    pub(crate) anim_overrides: std::collections::HashMap<usize, std::sync::Arc<wgpu::BindGroup>>,
/// Animaciones guardadas: entity_id → (name → AnimationState).
    /// Permite almacenar TODAS las animaciones de una entidad y reproducir
    /// cualquiera por nombre sin reenviar datos desde el frontend.
    pub(crate) animations: HashMap<u32, HashMap<String, AnimationState>>,
    /// Animación actualmente en reproducción: entity_id → ActiveAnimation.
    pub(crate) active_animations: HashMap<u32, ActiveAnimation>,
    /// Sistema de scripting Lua. Contiene la VM y los scripts adjuntos a entidades.
    pub(crate) script_engine: ScriptEngine,
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
        // Buffer de uniforms para la malla inicial (plano de suelo 3D)
        let init_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label:    Some("entity-uniforms-0"),
            contents: bytemuck::cast_slice(&[uniforms]),
            usage:    wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let init_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:   Some("entity-bg-0"),
            layout:  &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding:  0,
                resource: init_buf.as_entire_binding(),
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
                buffers:             &[mesh::Vertex::desc()],
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

        // ── Escenario base: plano de suelo — primera persona ─────────────────
        let ground_plane = crate::config_3d::mesh_3d::create_ground_plane(&device);
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

        Self {
            window,
            surface,
            device,
            queue,
            config,
            size,
            clear_color: wgpu::Color { r: 0.06, g: 0.06, b: 0.10, a: 1.0 },
            render_pipeline,
            render_pipeline_2d,
            depth_view,
            texture_bgl,
            textures: vec![checker_tex_bg],   // índice 0 = plano de suelo
            scene_bgl: bgl,
            entity_buffers:     vec![init_buf],
            entity_bind_groups: vec![init_bg],
            fallback_tex_bg,
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
            physics: PhysicsWorld::new(),
            physics_2d: PhysicsWorld2D::new(),
            selected_entity:      None,
            hovered_entity:      None,
            hovered_gizmo_axis:  None,
            active_gizmo_axis:   None,
            scenario_entities:      Vec::new(),
            character_entities:     Vec::new(),
            background_entity:       None,
            grid_config,
            grid_pipeline,
            grid_buffer,
            grid_bind_group,
            grid_buffer_uni,
            ctrl_held: false,
            active_tool: ActiveTool::None,
            tool_overlay_buffer: tool_overlay_buffer_init,
            collider_entities: Vec::new(),
            anim_saved_transforms: std::collections::HashMap::new(),
            pivot_edit_mode:    None,
            logical_area_mode:  None,
            audio_slot,
            anim_texture_cache: std::collections::HashMap::new(),
            anim_overrides:     std::collections::HashMap::new(),
            animations:        HashMap::new(),
            active_animations: HashMap::new(),
            script_engine: ScriptEngine::new()
                .expect("Error al inicializar el motor de scripting Lua"),
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
            }
            EngineCommand::HideWindow => {
                self.window.set_visible(false);
            }
            EngineCommand::ShowWindow => {
                self.window.set_visible(true);
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
            EngineCommand::LoadScenario { path } => {
                self.load_scenario(&path);
            }
            EngineCommand::SetScenarioScale { id, scale } => {
                let marker = self.world.get::<crate::config_2d::ScenarioMarker>(id).cloned();
                if let Some(m) = marker {
                    let aspect = m.img_width as f32 / m.img_height.max(1) as f32;
                    let new_h  = m.base_world_h * scale.clamp(0.05, 20.0);
                    let new_w  = new_h * aspect;
                    if let Some(t) = self.world.get_mut::<Transform>(id) {
                        t.scale = GlamVec3::new(new_w, new_h, 1.0);
                    }
                }
            }
            EngineCommand::DuplicateScenario { id } => {
                self.duplicate_scenario(id);
            }
            EngineCommand::LoadCharacter { path } => {
                self.load_character(&path);
            }
            EngineCommand::SetCharacterScale { id, scale } => {
                self.set_character_scale(id, scale);
            }
            EngineCommand::DuplicateCharacter { id } => {
                self.duplicate_character(id);
            }
            EngineCommand::PlayAnimationFrame { id, path, pivot_x, pivot_y, logical_w, logical_h } => {
                if self.pivot_edit_mode.is_some() {
                    // Ignorar: el modo edición de pivot tiene prioridad para no interferir con la textura/escala
                    return;
                }
                self.play_animation_frame(id, &path, pivot_x, pivot_y, logical_w, logical_h);
            }
            EngineCommand::RestoreAnimationFrame { id } => {
                self.restore_animation_frame(id);
            }
            EngineCommand::SetPivotEditMode { id, frame_path, pivot_x, pivot_y } => {
                self.enter_pivot_edit_mode(id, &frame_path, pivot_x, pivot_y);
            }
            EngineCommand::CancelPivotEditMode => {
                self.cancel_pivot_edit_mode();
            }
            EngineCommand::SetLogicalAreaMode { id, w, h } => {
                self.enter_logical_area_mode(id, w, h);
            }
            EngineCommand::CancelLogicalAreaMode => {
                self.cancel_logical_area_mode();
            }
            EngineCommand::PlayAudio { path, loop_ } => {
                // Decodificar a PCM y enviar al Sink persistente.
                let decoded = std::fs::read(&path).ok()
                    .and_then(|b| {
                        let cursor = std::io::Cursor::new(b);
                        rodio::Decoder::new(cursor).ok().map(|dec| {
                            let ch = dec.channels();
                            let sr = dec.sample_rate();
                            let s: Vec<i16> = dec.collect();
                            Arc::new(DecodedAudio { samples: s, channels: ch, sample_rate: sr })
                        })
                    });
                match decoded {
                    Some(audio) => self.play_audio_internal(audio, loop_),
                    None => log::error!("[audio] no se pudo cargar o decodificar: {path}"),
                }
            }
            EngineCommand::StopAudio => {
                self.stop_audio_internal();
                log::info!("[audio] detenido por comando externo");
            }
            EngineCommand::RemoveEntity { id } => {
                if Some(id) == self.selected_entity { self.selected_entity = None; }
                if Some(id) == self.hovered_entity  { self.hovered_entity  = None; }
                self.physics.remove_entity_body(id);
                self.physics_2d.remove_entity_body(id);
                self.scenario_entities.retain(|&e| e != id);
                self.character_entities.retain(|&e| e != id);
                self.collider_entities.retain(|&e| e != id);
                self.script_engine.detach_entity(id);
                self.world.despawn(id);
            }
            EngineCommand::SetWorldSize { width, height } => {
                self.grid_config.world_width  = width.max(1.0);
                self.grid_config.world_height = height.max(1.0);
                self.rebuild_grid();
                // Redimensionar el fondo si existe
                if let Some(bg_id) = self.background_entity {
                    if let Some(t) = self.world.get_mut::<Transform>(bg_id) {
                        t.scale = GlamVec3::new(self.grid_config.world_width, self.grid_config.world_height, 1.0);
                    }
                }
            }
            EngineCommand::SetGridVisible { visible } => {
                self.grid_config.visible = visible;
                self.rebuild_grid();
            }
            EngineCommand::SetGridCellSize { size } => {
                self.grid_config.cell_size = size.clamp(0.05, 100.0);
                self.rebuild_grid();
            }
            EngineCommand::SetCtrlHeld { held } => {
                self.ctrl_held = held;
            }
            EngineCommand::SetCamera2d { x, y, half_h } => {
                if let Some(cam2d) = &mut self.camera_2d {
                    cam2d.x      = x;
                    cam2d.y      = y;
                    cam2d.half_h = half_h.clamp(1.0, 50.0);
                    log::info!("Cámara 2D restaurada: x={x} y={y} half_h={half_h}");
                }
            }
            EngineCommand::LoadBackground { path } => {
                self.load_background(&path);
            }
            EngineCommand::SetPhysics { id, enabled, body_type } => {
                let (pos, half) = if let Some(t) = self.world.get::<Transform>(id) {
                    (t.position.to_array(), (t.scale * 0.5).to_array())
                } else {
                    ([0.0_f32; 3], [0.5_f32; 3])
                };
                if self.camera_2d.is_some() {
                    self.physics_2d.set_entity_physics(id, enabled, &body_type, pos, half);
                } else {
                    self.physics.set_entity_physics(id, enabled, &body_type, pos, half);
                }
                log::info!("Física {}: entidad {} tipo='{}'",
                    if enabled { "activada" } else { "desactivada" }, id, body_type);
            }
            EngineCommand::SetActiveTool { tool } => {
                if tool.is_empty() {
                    self.active_tool = ActiveTool::None;
                    self.tool_overlay_buffer = gizmo::build_from_vertices(&self.device, &[]);
                    send_event(&EngineEvent::ToolCancelled);
                    log::info!("Herramienta cancelada");
                } else {
                    match tool.as_str() {
                        "draw_collider" => {
                            self.active_tool = ActiveTool::DrawCollider { points_world: Vec::new() };
                            log::info!("Herramienta activa: dibujar colisionador (4 puntos)");
                        }
                        _ => log::warn!("Herramienta desconocida: {}", tool),
                    }
                }
            }
            EngineCommand::CreateColliderFromPoints { points } => {
                if self.camera_2d.is_some() {
                    self.create_collision_box_from_points(&points);
                } else {
                    log::warn!("CreateColliderFromPoints solo disponible en modo 2D");
                }
            }
            EngineCommand::SetAnimation { id, name, frames, fps, loop_, audio_path, logical_w, logical_h } => {
                log::info!("[IPC] SetAnimation: entity_id={}, name='{}', frames={}, audio={:?}", id, name, frames.len(), audio_path);

                // Pre-decodificar audio a muestras PCM durante SetAnimation.
                // En PlayAnimation solo se clona un Vec<i16> — cero I/O, cero decode.
                let audio_decoded: Option<Arc<DecodedAudio>> = audio_path.as_deref().and_then(|p| {
                    let bytes = match std::fs::read(p) {
                        Ok(b) => b,
                        Err(e) => { log::warn!("[SetAnimation] error leyendo audio {}: {}", p, e); return None; }
                    };
                    let cursor = std::io::Cursor::new(bytes);
                    let decoder = match rodio::Decoder::new(cursor) {
                        Ok(d) => d,
                        Err(e) => { log::warn!("[SetAnimation] error decodificando audio {}: {}", p, e); return None; }
                    };
                    let channels    = decoder.channels();
                    let sample_rate = decoder.sample_rate();
                    let samples: Vec<i16> = decoder.collect();
                    log::info!("[SetAnimation] audio decodificado: {} ({} muestras, {}ch, {}Hz)",
                        p, samples.len(), channels, sample_rate);
                    Some(Arc::new(DecodedAudio { samples, channels, sample_rate }))
                });

                // Pre-cargar todos los frames de la animación en la caché GPU.
                // El primer PlayAnimation ya no tendrá latencia de decode+upload.
                for frame in &frames {
                    self.preload_anim_frame(&frame.path);
                }

                // Guardar animación en el almacén por entidad+nombre.
                self.animations
                    .entry(id)
                    .or_insert_with(HashMap::new)
                    .insert(name.clone(), AnimationState {
                        frames,
                        fps,
                        loop_,
                        audio_decoded,
                        logical_w,
                        logical_h,
                    });
                log::info!("[IPC] Animación '{}' guardada y pre-cargada para entidad {}", name, id);
            }
EngineCommand::PlayAnimation { id, name } => {
                log::info!("[IPC] PlayAnimation: entity_id={}, name='{}'", id, name);
                // Detener animación previa (el Play de audio incluye clear interno)
                self.active_animations.remove(&id);
                self.restore_animation_frame(id);

                let anim_opt = self.animations.get(&id)
                    .and_then(|m| m.get(&name))
                    .cloned();

                match anim_opt {
                    None => log::warn!("[IPC] Animación '{}' no encontrada para entidad {}", name, id),
                    Some(anim) => {
                        // Capturar el tiempo ANTES del I/O de archivos para que
                        // last_frame_time refleje el inicio real del frame 0, no el
                        // tiempo después de cargar texturas/audio (puede ser 50-200ms más tarde).
                        let frame_start = Instant::now();

                        // Mostrar frame 0 (cache miss solo en el primer play)
                        if let Some(first_frame) = anim.frames.first() {
                            self.play_animation_frame(id, &first_frame.path, first_frame.pivot_x, first_frame.pivot_y, anim.logical_w, anim.logical_h);
                        }

                        // Iniciar audio desde PCM pre-decodificado (cero I/O, cero decode)
                        if let Some(ref audio_decoded) = anim.audio_decoded {
                            self.play_audio_internal(Arc::clone(audio_decoded), anim.loop_);
                        }

                        self.active_animations.insert(id, ActiveAnimation {
                            animation_name: name.clone(),
                            current_frame: 0,
                            last_frame_time: frame_start,
                            fps: anim.fps,
                            finished: false,
                        });
                        log::info!("[animation] Iniciada '{}' para entidad {} (fps={}, frames={})", name, id, anim.fps, anim.frames.len());
                    }
                }
            }
            EngineCommand::StopAnimation { id } => {
                log::info!("[IPC] StopAnimation: entity_id={}", id);
                self.active_animations.remove(&id);
                self.restore_animation_frame(id);
                self.stop_audio_internal();
                send_event(&EngineEvent::AnimationFinished { entity_id: id });
                log::info!("[animation] Stopped for entity {}", id);
            }
            EngineCommand::LoadScript { id, path, source } => {
                log::info!("[IPC] LoadScript: entity_id={} path={}", id, path);
                if let Err(e) = self.script_engine.attach_script(id, &path, &source) {
                    log::error!("[scripting] Error cargando script '{}': {}", path, e);
                    send_event(&EngineEvent::Error {
                        message: format!("Error en script '{path}': {e}"),
                    });
                }
            }
            EngineCommand::UnloadScript { id } => {
                log::info!("[IPC] UnloadScript: entity_id={}", id);
                self.script_engine.detach_entity(id);
            }
            EngineCommand::Shutdown => {}
        }
    }

    /// Reconstruye el vertex buffer de la cuadrícula con la configuración actual.
    pub(crate) fn rebuild_grid(&mut self) {
        self.grid_buffer = crate::config_2d::build_grid(&self.device, &self.grid_config);
    }

    fn play_audio_internal(&mut self, audio: Arc<DecodedAudio>, loop_: bool) {
        if let Some(slot) = &self.audio_slot {
            send_audio(slot, AudioCmd::Play { audio, loop_ });
        } else {
            log::error!("[audio] thread de audio no disponible");
        }
    }

    fn stop_audio_internal(&mut self) {
        if let Some(slot) = &self.audio_slot {
            send_audio(slot, AudioCmd::Stop);
        }
    }

    pub(crate) fn update_animations(&mut self) {
        let now = Instant::now();
        let mut to_play: Vec<(u32, usize)> = Vec::new();
        let mut to_restore: Vec<u32> = Vec::new();

        let entity_ids: Vec<u32> = self.active_animations.keys().copied().collect();
        for entity_id in entity_ids {
            let active = match self.active_animations.get_mut(&entity_id) {
                Some(a) => a,
                None => continue,
            };

            if active.finished {
                continue;
            }
            // Nota: la lógica de avance de frames se hace abajo con corrección de drift

            let anim_state = match self.animations.get(&entity_id)
                .and_then(|m| m.get(&active.animation_name)) {
                Some(a) => a,
                None => continue,
            };

            let frame_duration_ms = 1000u64 / active.fps.max(1) as u64;
            let frame_duration = std::time::Duration::from_millis(frame_duration_ms);
            let elapsed = now.duration_since(active.last_frame_time);

            if elapsed < frame_duration {
                continue;
            }

            // Cuántos frames debieron haberse mostrado (recuperación de lag/stutter).
            // Con `= now` el error se acumula; con `+= frame_duration` el reloj es exacto.
            let frames_to_advance = (elapsed.as_millis() / frame_duration_ms as u128).max(1) as usize;
            let total_frames = anim_state.frames.len();

            // Avanzar el reloj de animación por el número exacto de frames,
            // no resincronizar a `now` (eso causaría deriva acumulada).
            active.last_frame_time += frame_duration * frames_to_advance as u32;
            // Salvaguarda: si el motor estuvo suspendido/bloqueado demasiado tiempo,
            // resincronizar para evitar una ráfaga de frames al retomar.
            if now.duration_since(active.last_frame_time) > frame_duration * 3 {
                active.last_frame_time = now - frame_duration;
            }

            let next_frame_idx = active.current_frame + frames_to_advance;

            if next_frame_idx >= total_frames {
                if anim_state.loop_ {
                    active.current_frame = next_frame_idx % total_frames;
                    to_play.push((entity_id, active.current_frame));
                } else {
                    active.finished = true;
                    to_restore.push(entity_id);
                }
            } else {
                active.current_frame = next_frame_idx;
                to_play.push((entity_id, next_frame_idx));
            }
        }

        for (entity_id, frame_idx) in to_play {
            let (path, pivot_x, pivot_y, logical_w, logical_h) = {
                let anim_name = self.active_animations.get(&entity_id)
                    .map(|a| a.animation_name.clone())
                    .unwrap_or_default();
                let anim = self.animations.get(&entity_id)
                    .and_then(|m| m.get(&anim_name))
                    .unwrap();
                let f = &anim.frames[frame_idx];
                (f.path.clone(), f.pivot_x, f.pivot_y, anim.logical_w, anim.logical_h)
            };
            self.play_animation_frame(entity_id, &path, pivot_x, pivot_y, logical_w, logical_h);
        }

        for entity_id in to_restore {
            self.restore_animation_frame(entity_id);
            // El audio no-looping se agota solo cuando las muestras PCM terminan.
            // No enviamos Stop aquí para evitar que sobrescriba un Play ya encolado
            // si el usuario dispara la siguiente animación justo al terminar esta.
            log::info!("[animation] Enviando AnimationFinished para entidad {}", entity_id);
            send_event(&EngineEvent::AnimationFinished { entity_id });
        }

self.active_animations.retain(|_, a| !a.finished);
    }

    /// Notifica al State qué eje del gizmo está siendo arrastrado (None = sin drag).
    pub fn set_active_gizmo_axis(&mut self, axis: Option<usize>) {
        self.active_gizmo_axis = axis;
    }

    // ── Scripts ───────────────────────────────────────────────────────────────

    /// Ejecuta un tick del motor de scripting y aplica los comandos generados.
    fn update_scripts(&mut self) {
        // Build snapshots for entities that have scripts attached
        let snapshots: HashMap<u32, EntitySnapshot> = {
            let entity_ids: Vec<u32> = self.script_engine.entity_ids().to_vec();
            let mut map = HashMap::new();
            for id in entity_ids {
                let (x, y, scale_x, scale_y) = if let Some(t) = self.world.get::<Transform>(id) {
                    (t.position.x, t.position.y, t.scale.x, t.scale.y)
                } else {
                    (0.0, 0.0, 1.0, 1.0)
                };
                let animations: Vec<String> = self.animations
                    .get(&id)
                    .map(|m| m.keys().cloned().collect())
                    .unwrap_or_default();
                map.insert(id, EntitySnapshot { id, x, y, scale_x, scale_y, animations });
            }
            map
        };

        let commands = self.script_engine.tick(self.delta_time, &snapshots);
        self.apply_script_commands(commands);
    }

    /// Aplica los comandos generados por los scripts al estado del motor.
    fn apply_script_commands(&mut self, commands: Vec<ScriptCmd>) {
        for cmd in commands {
            match cmd {
                ScriptCmd::SetPosition { id, x, y } => {
                    if let Some(t) = self.world.get_mut::<Transform>(id) {
                        t.position.x = x;
                        t.position.y = y;
                    }
                }
                ScriptCmd::Translate { id, dx, dy } => {
                    if let Some(t) = self.world.get_mut::<Transform>(id) {
                        t.position.x += dx;
                        t.position.y += dy;
                    }
                }
                ScriptCmd::SetScale { id, sx, sy } => {
                    if let Some(t) = self.world.get_mut::<Transform>(id) {
                        t.scale.x = sx;
                        t.scale.y = sy;
                    }
                }
                ScriptCmd::PlayAnimation { id, name } => {
                    self.handle_command(EngineCommand::PlayAnimation { id, name });
                }
                ScriptCmd::StopAnimation { id } => {
                    self.handle_command(EngineCommand::StopAnimation { id });
                }
                ScriptCmd::Log { message } => {
                    eprintln!("[script] {message}");
                }
            }
        }
    }

    // ── Update ───────────────────────────────────────────────────────────────

    pub fn update(&mut self) {
        let now         = Instant::now();
        self.delta_time = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;
        if self.camera_2d.is_some() {
            self.physics_2d.step(self.delta_time, &mut self.world);
        } else {
            self.physics.step(self.delta_time, &mut self.world);
        }
        self.update_scripts();
    }

    // ── Render ───────────────────────────────────────────────────────────────

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        self.update_animations();

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

            // En 2D usamos el pipeline sin depth-write: el sort back-to-front
            // más el alpha blending se encargan del orden correcto, y no hay
            // bloqueo de píxeles transparentes por profundidad.
            if self.camera_2d.is_some() {
                pass.set_pipeline(&self.render_pipeline_2d);
            } else {
                pass.set_pipeline(&self.render_pipeline);
            }

            // Iterar entidades con MeshComponent.
            // Las entidades de escenario (ScenarioMarker) están en Z=-1, por lo que
            // el depth test las deja detrás de las entidades normales (Z=0).
            // Ordenamos por Z ascendente para garantizar que se dibujen primero
            // incluso si el depth test falla en algunos drivers GL/EGL software.
            let mut entities: Vec<_> = self.world.entities().iter().copied().filter_map(|id| {
                let mesh_idx  = self.world.get::<MeshComponent>(id)?.mesh_idx;
                let model_mat = self.world.get::<Transform>(id)?.to_matrix();
                let z         = self.world.get::<crate::ecs::Transform>(id).map_or(0.0, |t| t.position.z);
                Some((id, mesh_idx, model_mat, z))
            }).collect();
            entities.sort_by(|a, b| a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal));

            for (entity_id, idx, model_matrix, _z) in entities {
                let (Some(mesh), Some(entity_buf), Some(entity_bg)) = (
                    self.meshes.get(idx),
                    self.entity_buffers.get(idx),
                    self.entity_bind_groups.get(idx),
                ) else { continue };
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
                    entity_buf, 0, bytemuck::cast_slice(&[uniforms]),
                );
                pass.set_bind_group(0, entity_bg, &[]);
                // anim_overrides tiene prioridad sobre textures[]:
                // durante una animación activa evita mutar la textura base.
                let tex_bg: &wgpu::BindGroup = self.anim_overrides.get(&idx)
                    .map(|a| a.as_ref())
                    .or_else(|| self.textures.get(idx))
                    .unwrap_or(&self.fallback_tex_bg);
                pass.set_bind_group(1, tex_bg, &[]);
                pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                pass.set_index_buffer(
                    mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32,
                );
                pass.draw_indexed(0..mesh.index_count, 0, 0..1);
            }
        }

        // ── Grid pass (solo modo 2D; borde siempre visible, líneas según config) ──
        if let Some(cam2d) = &self.camera_2d {
                let aspect   = self.size.width as f32 / self.size.height as f32;
                let vp       = cam2d.view_proj(aspect).to_cols_array_2d();
                // Uniforms: view_proj + model identity + flags -1
                let grid_uni: [[f32; 4]; 9] = [
                    vp[0], vp[1], vp[2], vp[3],
                    [1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0],
                    [-1.0, -1.0, 0.0, 0.0],
                ];
                self.queue.write_buffer(&self.grid_buffer_uni, 0, bytemuck::cast_slice(&grid_uni));

                let mut grd_pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("grid-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view:           &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load:  wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set:      None,
                    timestamp_writes:         None,
                });
                grd_pass.set_pipeline(&self.grid_pipeline);
                grd_pass.set_bind_group(0, &self.grid_bind_group, &[]);
                grd_pass.set_vertex_buffer(0, self.grid_buffer.vertex_buffer.slice(..));
                grd_pass.draw(0..self.grid_buffer.vertex_count, 0..1);
        }

        // ── Tool overlay pass (solo modo 2D; cruces + líneas de construcción) ──
        if self.camera_2d.is_some() && self.tool_overlay_buffer.vertex_count > 0 {
            let mut tool_pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("tool-overlay-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view:           &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load:  wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set:      None,
                timestamp_writes:         None,
            });
            tool_pass.set_pipeline(&self.grid_pipeline);          // LineList, sin depth
            tool_pass.set_bind_group(0, &self.grid_bind_group, &[]); // view_proj actualizado
            tool_pass.set_vertex_buffer(0, self.tool_overlay_buffer.vertex_buffer.slice(..));
            tool_pass.draw(0..self.tool_overlay_buffer.vertex_count, 0..1);
        }

        // ── Gizmos (segundo pass, sin depth) ─────────────────────────────────
        // Ocultar gizmo durante el modo edición de pivot: las flechas de movimiento
        // robarían el foco e impedirían hacer click libremente sobre el asset.
        if let Some(sel_id) = self.selected_entity.filter(|_| self.pivot_edit_mode.is_none()) {
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
