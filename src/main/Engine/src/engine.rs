use std::collections::{HashMap, HashSet};
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
                        log::debug!("[audio] reproduciendo ({} muestras, {}ch, {}Hz, loop={})",
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
use crate::ecs::{MeshComponent, NameComponent, Transform, World};
use crate::gizmo::{self, GizmoBuffer};
use crate::ipc::{send_event, AnimationFrameData, AnimScriptData, EngineCommand, EngineEvent};
use crate::mesh::{self, Mesh};
use crate::config_3d::physics_3d::PhysicsWorld;
use crate::scripting::{ScriptEngine, ScriptCmd, EntitySnapshot};

pub(crate) enum UndoAction {
    RestoreTransform {
        id:       u32,
        position: [f32; 3],
        rotation: [f32; 4],
        scale:    [f32; 3],
    },
    RestoreTransforms {
        items: Vec<(u32, [f32; 3], [f32; 4], [f32; 3])>,
    },
    RemoveEntity { id: u32 },
}

#[derive(Clone)]
pub struct AnimationState {
    pub frames:        Vec<AnimationFrameData>,
    pub fps:           u32,
    pub loop_:         bool,
    pub flip_horizontal: bool,
    /// Audio pre-decodificado a muestras PCM durante SetAnimation.
    /// `None` si la animación no tiene audio o falló la decodificación.
    pub audio_decoded: Option<Arc<DecodedAudio>>,
    pub logical_w:     u32,
    pub logical_h:     u32,
    /// Scripts Lua que se ejecutan solo mientras esta animación está activa.
    pub scripts:       Vec<AnimScriptData>,
    /// Si false, ningún `PlayAnimation` puede interrumpirla hasta que termine.
    pub is_cancelable: bool,
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

// ── Uniform compartido por frame (group 0) ───────────────────────────────────
// Solo view_proj + cam_pos; el model matrix va en el instance buffer.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct SceneUniforms {
    pub(crate) view_proj: [[f32; 4]; 4],
    pub(crate) cam_pos:   [f32; 4],   // xyz = posición cámara, w = sin uso
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
    pub(crate) scene_buffer:       wgpu::Buffer,
    /// Bind group del buffer escena (group 0, binding 0).
    pub(crate) scene_bind_group:   wgpu::BindGroup,
    // Texturas (group 1) — todas en el atlas compartido
    /// Atlas de texturas: una sola textura GPU 4096×4096 que empaca todos los sprites.
    /// Todas las entidades comparten su bind group, eliminando los cambios de grupo por batch.
    pub(crate) atlas:              crate::texture::TextureAtlas,
    /// UV rects de cada textura en el atlas, indexados por `MeshComponent.tex_idx`.
    pub(crate) uv_rects:           Vec<[f32; 4]>,
    /// UV del pixel blanco 1×1 en (0,0) del atlas — fallback cuando tex_idx es inválido.
    pub(crate) fallback_uv:        [f32; 4],
    /// Caché de texturas estáticas PNG: path → UV rect en el atlas.
    pub(crate) static_tex_cache:   std::collections::HashMap<String, [f32; 4]>,
    /// Índice en `meshes[]` del quad unitario canónico (1×1 en origen).
    /// Todos los sprites 2D apuntan a este mesh; sus texturas individuales
    /// se almacenan en `textures[]` indexadas por `MeshComponent.tex_idx`.
    pub(crate) canonical_quad_idx: usize,
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
    pub selected_entities:   Vec<EntityId>,
    pub hovered_entity:      Option<EntityId>,
    pub hovered_gizmo_axis:  Option<usize>,
    pub active_gizmo_axis:   Option<usize>,
    // Spatial partitioning para picking/queries
    pub(crate) spatial_grid: crate::spatial::SpatialGrid,
    // Escenario 2D: lista de entidades ECS que actúan como fondos PNG.
    pub(crate) scenario_entities: Vec<EntityId>,
    // Personajes 2D: lista de entidades ECS que actúan como sprites de personaje.
    pub(crate) character_entities: Vec<EntityId>,
    // Fondo del mundo 2D: entidad especial no seleccionable que cubre todo el área.
    pub(crate) background_entity: Option<EntityId>,
    pub(crate) background_path:   Option<String>,
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
    /// Entidad fantasma para previsualizar el blueprint a colocar (Quick Build mode).
    pub(crate) quick_build_ghost_id: Option<EntityId>,
    /// Ruta de asset de la blueprint activa en Quick Build (para snap por igualdad de blueprint).
    pub(crate) quick_build_preview_path: Option<String>,
    /// Tipo de blueprint activa en Quick Build ("scenario" | "character").
    pub(crate) quick_build_preview_kind: Option<String>,
    /// Escala base de la blueprint activa (sin ajuste dinámico por Ctrl).
    pub(crate) quick_build_preview_scale: Option<[f32; 3]>,
    /// true = modo juego (simulación), false = modo editor.
    pub        preview_playing:  bool,
    /// Buffer de overlay de la herramienta activa (cruces + líneas de construcción).
    pub(crate) tool_overlay_buffer: GizmoBuffer,
    /// UV del PNG de hint de snap en español en el atlas, si se cargó correctamente.
    pub(crate) snap_hint_uv: Option<[f32; 4]>,
    /// Tamaño original del PNG de hint ES (ancho, alto) en píxeles.
    pub(crate) snap_hint_size: (f32, f32),
    /// UV del PNG de hint de snap en inglés en el atlas, si se cargó correctamente.
    pub(crate) snap_hint_uv_en: Option<[f32; 4]>,
    /// Tamaño original del PNG de hint EN (ancho, alto) en píxeles.
    pub(crate) snap_hint_size_en: (f32, f32),
    /// Locale activo del editor: "en" | "es". Determina qué imagen de hint se muestra.
    pub(crate) snap_locale: String,
    /// Controla visibilidad del hint de snap (solo editor 2D).
    pub(crate) show_snap_hint: bool,
    /// Alpha actual del hint [0..1] para fade in/out suave.
    pub(crate) snap_hint_alpha: f32,
    /// Entidades creadas por herramientas de dibujo (colisionadores).
    pub(crate) collider_entities: Vec<EntityId>,
    /// Entidades creadas por herramientas de dibujo (áreas de ejecución).
    pub(crate) execution_area_entities: Vec<EntityId>,
    /// Pares activos (trigger, actor) detectados en el frame anterior.
    pub(crate) execution_overlaps: HashSet<(EntityId, EntityId)>,
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
    /// Almacena (uv_rect_en_atlas, img_width, img_height) para evitar recargar de disco.
    /// Se limpia al cambiar de escena.
    pub(crate) anim_texture_cache: std::collections::HashMap<String, ([f32; 4], u32, u32)>,
    /// Overrides de UV rect para animaciones activas: tex_idx → uv_rect en el atlas.
    /// play_animation_frame escribe aquí; restore_animation_frame borra la entrada;
    /// el render loop lo lee con prioridad sobre uv_rects[].
    pub(crate) anim_overrides: std::collections::HashMap<usize, [f32; 4]>,
/// Animaciones guardadas: entity_id → (name → AnimationState).
    /// Permite almacenar TODAS las animaciones de una entidad y reproducir
    /// cualquiera por nombre sin reenviar datos desde el frontend.
    pub(crate) animations: HashMap<u32, HashMap<String, AnimationState>>,
    /// Animación actualmente en reproducción: entity_id → ActiveAnimation.
    pub(crate) active_animations: HashMap<u32, ActiveAnimation>,
    /// Nombre de animación predeterminada por entidad.
    pub(crate) default_animation_by_entity: HashMap<u32, String>,
    /// Override de flip horizontal por entidad para el playback actual.
    /// Reservado para forzados internos del motor; por defecto el flip se resuelve automáticamente.
    pub(crate) anim_flip_overrides: HashMap<u32, bool>,
    /// Dirección actual de mirada por entidad (true = derecha, false = izquierda).
    /// Se actualiza automáticamente con movimiento horizontal en scripts.
    pub(crate) entity_facing_right: HashMap<u32, bool>,
    /// Sistema de scripting Lua. Contiene la VM y los scripts adjuntos a entidades.
    pub(crate) script_engine: ScriptEngine,
    /// Almacén de sprites cargados (PNGs) para reutilización en el editor.
    /// No se renderizan directamente; actúan como biblioteca de imágenes.
    pub(crate) sprite_store: HashMap<String, (String, u32, u32)>, // path → (name, width, height)
    /// Historial simple para deshacer cambios del editor.
    pub(crate) undo_stack: Vec<UndoAction>,
    /// Historial para rehacer (Ctrl+Y). Se limpia al registrar una acción nueva.
    pub(crate) redo_stack: Vec<UndoAction>,
    /// Evita registrar nuevas entradas de undo mientras se está aplicando una.
    pub(crate) is_applying_undo: bool,
    // ── Debug metrics ────────────────────────────────────────────────────────
    metrics_last_emit:   Instant,
    metrics_frame_count: u32,
    last_draw_calls:     u32,
}

impl State {
    pub(crate) fn is_entity_name_taken(&self, name: &str, except_id: Option<u32>) -> bool {
        let target = name.trim().to_lowercase();
        if target.is_empty() {
            return false;
        }

        self.world.query::<NameComponent>().any(|(id, c)| {
            if except_id == Some(id) {
                return false;
            }
            c.name.trim().eq_ignore_ascii_case(&target)
        })
    }

    pub(crate) fn next_numbered_entity_name(&self, base: &str) -> String {
        let clean_base = base.trim();
        let prefix = format!("{}_", clean_base);
        let mut max_suffix: u32 = 0;

        for (_id, c) in self.world.query::<NameComponent>() {
            let current = c.name.trim();
            if let Some(rest) = current.strip_prefix(&prefix) {
                if let Ok(n) = rest.parse::<u32>() {
                    if n > max_suffix {
                        max_suffix = n;
                    }
                }
            }
        }

        format!("{}_{:02}", clean_base, max_suffix.saturating_add(1))
    }

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
        world.insert(plane_id, MeshComponent { mesh_idx: 0, tex_idx: 0 });
        // Textura checkerboard para el suelo (UV idx 0 ya en uv_rects)
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
            physics: PhysicsWorld::new(),
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
            sprite_store: HashMap::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            is_applying_undo: false,
            metrics_last_emit:   Instant::now(),
            metrics_frame_count: 0,
            last_draw_calls:     0,
        }
    }

    // ── Accesores ─────────────────────────────────────────────────────────────

    pub fn window(&self) -> &Arc<Window> { &self.window }
    pub fn size(&self)   -> PhysicalSize<u32> { self.size }
    pub fn is_preview_playing(&self) -> bool { self.preview_playing }

    pub fn push_undo_transform(&mut self, id: u32, position: [f32; 3], rotation: [f32; 4], scale: [f32; 3]) {
        if !self.is_applying_undo {
            self.redo_stack.clear();
        }
        self.undo_stack.push(UndoAction::RestoreTransform { id, position, rotation, scale });
    }

    pub fn push_undo_transforms(&mut self, items: Vec<(u32, [f32; 3], [f32; 4], [f32; 3])>) {
        if items.is_empty() {
            return;
        }
        if !self.is_applying_undo {
            self.redo_stack.clear();
        }
        self.undo_stack.push(UndoAction::RestoreTransforms { items });
    }

    pub fn apply_undo(&mut self) {
        let Some(action) = self.undo_stack.pop() else { return; };
        self.is_applying_undo = true;
        match action {
            UndoAction::RestoreTransform { id, position, rotation, scale } => {
                if let Some(t) = self.world.get::<Transform>(id) {
                    self.redo_stack.push(UndoAction::RestoreTransform {
                        id,
                        position: t.position.to_array(),
                        rotation: [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w],
                        scale: t.scale.to_array(),
                    });
                }
                self.handle_command(EngineCommand::SetTransform {
                    id,
                    position: Some(position),
                    rotation: Some(rotation),
                    scale: Some(scale),
                    track_undo: Some(false),
                });
            }
            UndoAction::RestoreTransforms { items } => {
                let mut redo_items: Vec<(u32, [f32; 3], [f32; 4], [f32; 3])> = Vec::new();
                for (id, _, _, _) in &items {
                    if let Some(t) = self.world.get::<Transform>(*id) {
                        redo_items.push((
                            *id,
                            t.position.to_array(),
                            [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w],
                            t.scale.to_array(),
                        ));
                    }
                }
                if !redo_items.is_empty() {
                    self.redo_stack.push(UndoAction::RestoreTransforms { items: redo_items });
                }
                for (id, position, rotation, scale) in items {
                    self.handle_command(EngineCommand::SetTransform {
                        id,
                        position: Some(position),
                        rotation: Some(rotation),
                        scale: Some(scale),
                        track_undo: Some(false),
                    });
                }
            }
            UndoAction::RemoveEntity { id } => {
                self.handle_command(EngineCommand::RemoveEntity { id });
                self.redo_stack.clear();
            }
        }
        self.is_applying_undo = false;
    }

    pub fn apply_redo(&mut self) {
        let Some(action) = self.redo_stack.pop() else { return; };
        self.is_applying_undo = true;
        match action {
            UndoAction::RestoreTransform { id, position, rotation, scale } => {
                if let Some(t) = self.world.get::<Transform>(id) {
                    self.undo_stack.push(UndoAction::RestoreTransform {
                        id,
                        position: t.position.to_array(),
                        rotation: [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w],
                        scale: t.scale.to_array(),
                    });
                }
                self.handle_command(EngineCommand::SetTransform {
                    id,
                    position: Some(position),
                    rotation: Some(rotation),
                    scale: Some(scale),
                    track_undo: Some(false),
                });
            }
            UndoAction::RestoreTransforms { items } => {
                let mut undo_items: Vec<(u32, [f32; 3], [f32; 4], [f32; 3])> = Vec::new();
                for (id, _, _, _) in &items {
                    if let Some(t) = self.world.get::<Transform>(*id) {
                        undo_items.push((
                            *id,
                            t.position.to_array(),
                            [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w],
                            t.scale.to_array(),
                        ));
                    }
                }
                if !undo_items.is_empty() {
                    self.undo_stack.push(UndoAction::RestoreTransforms { items: undo_items });
                }
                for (id, position, rotation, scale) in items {
                    self.handle_command(EngineCommand::SetTransform {
                        id,
                        position: Some(position),
                        rotation: Some(rotation),
                        scale: Some(scale),
                        track_undo: Some(false),
                    });
                }
            }
            UndoAction::RemoveEntity { .. } => {
                // No soportado: faltan snapshots completos para re-crear entidades eliminadas.
            }
        }
        self.is_applying_undo = false;
    }

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
            }            EngineCommand::SetBounds { x, y, width, height, .. } => {
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
            EngineCommand::LoadModel { path } => {
                self.load_model(&path);
            }
            EngineCommand::SetTransform { id, position, rotation, scale, track_undo } => {
                use glam::{Quat, Vec3};
                let before = self.world.get::<Transform>(id).cloned();
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
                // Si la entidad está mostrando frames animados, mantener sincronizada
                // la base (orig_pos/orig_scale) para que el siguiente frame respete
                // los cambios hechos desde el panel de Transformaciones.
                if let Some(saved) = self.anim_saved_transforms.get_mut(&id) {
                    if let Some(p) = position {
                        saved.0 = Vec3::from(p);
                    }
                    if let Some(s) = scale {
                        saved.1 = Vec3::from(s);
                    }
                }
                // Sincronizar el Rapier body si la entidad tiene física activa.
                // Sin esto, cuando el editor reposiciona una entidad via SetTransform
                // (p.ej. al cargar el proyecto o antes de reproducir una animación),
                // el cuerpo físico queda en su posición anterior y las colisiones
                // no ocurren donde el personaje aparece visualmente.
                if let Some(p) = position {
                    if self.camera_2d.is_some() {
                        self.physics_2d.teleport_entity(id, p[0], p[1]);
                    }
                }
                if let Some(prev) = before {
                    let prev_pos = prev.position.to_array();
                    let prev_rot = [prev.rotation.x, prev.rotation.y, prev.rotation.z, prev.rotation.w];
                    let prev_scl = prev.scale.to_array();
                    let next_pos = self.world.get::<Transform>(id).map(|t| t.position.to_array());
                    let next_rot = self.world.get::<Transform>(id).map(|t| [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w]);
                    let next_scl = self.world.get::<Transform>(id).map(|t| t.scale.to_array());
                    let should_track_undo = track_undo.unwrap_or(true);
                    if !self.is_applying_undo
                        && should_track_undo
                        && (next_pos != Some(prev_pos) || next_rot != Some(prev_rot) || next_scl != Some(prev_scl)) {
                        self.push_undo_transform(id, prev_pos, prev_rot, prev_scl);
                    }
                }
            }
            EngineCommand::SetEntityName { id, name, force } => {
                let next_name = name.trim();
                if next_name.is_empty() {
                    send_event(&EngineEvent::Error { message: "El nombre no puede estar vacio".to_string() });
                    return;
                }

                if !force && self.is_entity_name_taken(next_name, Some(id)) {
                    send_event(&EngineEvent::Error { message: format!("Ya existe una entidad con el nombre '{}'", next_name) });
                    return;
                }

                if let Some(existing) = self.world.get_mut::<NameComponent>(id) {
                    existing.name = next_name.to_string();
                } else {
                    self.world.insert(id, NameComponent { name: next_name.to_string() });
                }

                if self.selected_entity == Some(id) {
                    let transform = self.world.get::<Transform>(id).cloned().unwrap_or_default();
                    let position = transform.position.to_array();
                    let rotation = [
                        transform.rotation.x,
                        transform.rotation.y,
                        transform.rotation.z,
                        transform.rotation.w,
                    ];
                    let scale = transform.scale.to_array();
                    let physics_enabled = if self.camera_2d.is_some() {
                        self.physics_2d.has_physics(id)
                    } else {
                        self.physics.has_physics(id)
                    };
                    let physics_type = if self.camera_2d.is_some() {
                        self.physics_2d.get_body_type(id).to_string()
                    } else {
                        self.physics.get_body_type(id).to_string()
                    };

                    send_event(&EngineEvent::EntitySelected {
                        id,
                        name: next_name.to_string(),
                        position,
                        rotation,
                        scale,
                        physics_enabled,
                        physics_type,
                    });
                }
            }
            EngineCommand::SetScene { scene } => {
                match scene.as_str() {
                    "2D"      => self.setup_2d_platformer(),
                    "scratch" => self.setup_scratch(),
                    _         => log::info!("SetScene: escena '{}' no reconocida", scene),
                }
            }
            EngineCommand::LoadScenario { path, track_undo } => {
                self.load_scenario(&path);
                if track_undo.unwrap_or(false) {
                    if let Some(&id) = self.scenario_entities.last() {
                        self.undo_stack.push(UndoAction::RemoveEntity { id });
                        self.redo_stack.clear();
                        log::info!("[quick_build] escenario {id} registrado en undo");
                    }
                }
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
            EngineCommand::LoadCharacter { path, track_undo } => {
                self.load_character(&path);
                if track_undo.unwrap_or(false) {
                    if let Some(&id) = self.character_entities.last() {
                        self.undo_stack.push(UndoAction::RemoveEntity { id });
                        self.redo_stack.clear();
                        log::info!("[quick_build] personaje {id} registrado en undo");
                    }
                }
            }
            EngineCommand::SetCharacterScale { id, scale } => {
                self.set_character_scale(id, scale);
            }
            EngineCommand::DuplicateCharacter { id } => {
                self.duplicate_character(id);
            }
            EngineCommand::PlayAnimationFrame { id, path, pivot_x, pivot_y, logical_w, logical_h, src_x, src_y, src_w, src_h } => {
                if self.pivot_edit_mode.is_some() {
                    // Ignorar: el modo edición de pivot tiene prioridad para no interferir con la textura/escala
                    return;
                }
                self.play_animation_frame(id, &path, pivot_x, pivot_y, logical_w, logical_h, src_x.zip(src_y).zip(src_w.zip(src_h)).map(|((x, y), (w, h))| (x, y, w, h)), false);
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
                self.selected_entities.retain(|&e| e != id);
                if Some(id) == self.selected_entity {
                    self.selected_entity = self.selected_entities.last().copied();
                }
                if self.selected_entities.is_empty() && self.selected_entity.is_none() {
                    send_event(&EngineEvent::EntityDeselected);
                }
                if Some(id) == self.hovered_entity  {
                    self.hovered_entity  = None;
                    send_event(&EngineEvent::EntityUnhovered);
                }
                self.physics.remove_entity_body(id);
                self.physics_2d.remove_entity_body(id);
                self.scenario_entities.retain(|&e| e != id);
                self.character_entities.retain(|&e| e != id);
                self.collider_entities.retain(|&e| e != id);
                self.execution_area_entities.retain(|&e| e != id);
                self.execution_overlaps.retain(|(trigger_id, actor_id)| *trigger_id != id && *actor_id != id);
                self.anim_flip_overrides.remove(&id);
                self.entity_facing_right.remove(&id);
                self.default_animation_by_entity.remove(&id);
                self.script_engine.detach_entity(id);
                self.world.despawn(id);
                send_event(&EngineEvent::EntityRemoved { id });
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
            EngineCommand::SetGravity { gravity } => {
                self.physics_2d.set_gravity(-gravity.abs());
                log::info!("[physics] Gravedad actualizada: -{:.2}", gravity.abs());
            }
            EngineCommand::SetGridVisible { visible } => {
                self.grid_config.visible = visible;
                self.rebuild_grid();
            }
            EngineCommand::SetGridCellSize { size } => {
                self.grid_config.cell_size = size.clamp(0.05, 100.0);
                self.rebuild_grid();
            }
            EngineCommand::SetPreviewPlaying { playing } => {
                if self.preview_playing == playing {
                    return;
                }

                self.preview_playing = playing;

                if playing {
                    self.active_tool = ActiveTool::None;
                    self.tool_overlay_buffer = gizmo::build_from_vertices(&self.device, &[]);
                    if self.pivot_edit_mode.is_some() {
                        self.cancel_pivot_edit_mode();
                    }
                    if self.logical_area_mode.is_some() {
                        self.cancel_logical_area_mode();
                    }

                    if self.selected_entity.take().is_some() || !self.selected_entities.is_empty() {
                        self.selected_entities.clear();
                        send_event(&EngineEvent::EntityDeselected);
                    }
                    if self.hovered_entity.take().is_some() {
                        send_event(&EngineEvent::EntityUnhovered);
                    }
                    self.hovered_gizmo_axis = None;
                    self.active_gizmo_axis = None;

                    // Al entrar en modo juego, reproducir la animación predeterminada
                    // de cada entidad que tenga animaciones registradas.
                    // Limpiar caché de scripts compilados para que ediciones en el editor surtan efecto.
                    self.script_engine.clear_control_script_cache();
                    let entities_with_anims: Vec<u32> = self.animations.keys().copied().collect();
                    for entity_id in entities_with_anims {
                        let default_name = self.default_animation_by_entity
                            .get(&entity_id)
                            .cloned()
                            .or_else(|| {
                                self.animations
                                    .get(&entity_id)
                                    .and_then(|m| m.keys().next().cloned())
                            });
                        if let Some(name) = default_name {
                            self.handle_command(EngineCommand::PlayAnimation { id: entity_id, name });
                        }
                    }
                } else {
                    // Al volver al modo editor, detener todas las animaciones activas
                    // y mostrar el primer frame de la animación correspondiente.
                    let active: Vec<(u32, String)> = self.active_animations
                        .iter()
                        .map(|(&id, a)| (id, a.animation_name.clone()))
                        .collect();
                    self.active_animations.clear();
                    for (entity_id, anim_name) in active {
                        self.script_engine.detach_animation_scripts(entity_id);
                        self.show_first_frame_of_animation(entity_id, &anim_name);
                    }
                    self.stop_audio_internal();
                }

                self.execution_overlaps.clear();

                log::info!(
                    "[preview] modo {}",
                    if playing { "juego" } else { "editor" }
                );
            }
            EngineCommand::SetCtrlHeld { held } => {
                self.ctrl_held = held;
            }
            EngineCommand::SetCamera2d { x, y, half_h } => {
                if let Some(cam2d) = &mut self.camera_2d {
                    cam2d.x      = x;
                    cam2d.y      = y;
                    cam2d.half_h = half_h.clamp(1.0, 50.0);
                    log::debug!("Cámara 2D restaurada: x={x} y={y} half_h={half_h}");
                }
            }
            EngineCommand::LoadBackground { path } => {
                self.background_path = Some(path.clone());
                self.load_background(&path);
            }
            EngineCommand::ClearBackground => {
                self.background_path = None;
                self.clear_background();
            }
            EngineCommand::SetPhysics { id, enabled, body_type } => {
                let (pos, half) = if let Some(t) = self.world.get::<Transform>(id) {
                    // Forzar z=0 en la posición física: el Z del Transform es visual
                    // (orden de capas), pero Rapier trabaja en 3D real. Si dos cuerpos
                    // tienen Z distinto no colisionan aunque se solapen en XY.
                    let mut p = t.position.to_array();
                    p[2] = 0.0;
                    (p, (t.scale * 0.5).to_array())
                } else {
                    ([0.0_f32; 3], [0.5_f32; 3])
                };
                if self.camera_2d.is_some() {
                    self.physics_2d.set_entity_physics(id, enabled, &body_type, pos, half);
                } else {
                    self.physics.set_entity_physics(id, enabled, &body_type, pos, half);
                }
                log::debug!("Física {}: entidad {} tipo='{}'",
                    if enabled { "activada" } else { "desactivada" }, id, body_type);
                send_event(&EngineEvent::PhysicsChanged { entity_id: id, enabled, body_type });
            }
            EngineCommand::SetActiveTool { tool, preview_path, preview_kind, preview_scale, preview_src_rect } => {
                if tool.is_empty() {
                    // Solo cancelar si había una herramienta activa (evita eventos espurios al inicio)
                    let was_active = !matches!(self.active_tool, ActiveTool::None);
                    // Limpiar entidad fantasma de quick_build si existía
                    if let Some(ghost_id) = self.quick_build_ghost_id.take() {
                        self.world.despawn(ghost_id);
                    }
                    self.quick_build_preview_path = None;
                    self.quick_build_preview_kind = None;
                    self.quick_build_preview_scale = None;
                    self.active_tool = ActiveTool::None;
                    self.tool_overlay_buffer = gizmo::build_from_vertices(&self.device, &[]);
                    if was_active {
                        send_event(&EngineEvent::ToolCancelled);
                        log::info!("Herramienta cancelada");
                    }
                } else {
                    // Limpiar entidad fantasma previa si la hay
                    if let Some(ghost_id) = self.quick_build_ghost_id.take() {
                        self.world.despawn(ghost_id);
                    }
                    self.quick_build_preview_path = None;
                    self.quick_build_preview_kind = None;
                    self.quick_build_preview_scale = None;
                    match tool.as_str() {
                        "draw_collider" => {
                            self.active_tool = ActiveTool::DrawCollider { points_world: Vec::new(), cursor_world: None };
                            log::info!("Herramienta activa: dibujar colisionador (4 puntos)");
                        }
                        "draw_execution_area" => {
                            self.active_tool = ActiveTool::DrawExecutionArea { points_world: Vec::new(), cursor_world: None };
                            log::info!("Herramienta activa: dibujar área de ejecución (4 puntos)");
                        }
                        "quick_build_place" => {
                            self.active_tool = ActiveTool::QuickBuildPlace { cursor_world: None };
                            self.tool_overlay_buffer = crate::gizmo::build_from_vertices(&self.device, &[]);
                            // Cargar entidad fantasma si se proporcionaron datos del blueprint
                            if let (Some(path), Some(kind), Some(scale)) = (preview_path.as_deref(), preview_kind.as_deref(), preview_scale) {
                                self.quick_build_preview_path = Some(path.to_owned());
                                self.quick_build_preview_kind = Some(kind.to_owned());
                                self.quick_build_preview_scale = Some(scale);
                                self.quick_build_ghost_id = self.load_quick_build_ghost(path, kind, scale, preview_src_rect);
                            }
                            log::info!("Herramienta activa: construcción rápida");
                        }
                        _ => log::warn!("Herramienta desconocida: {}", tool),
                    }
                }
            }
            EngineCommand::CreateColliderFromPoints { points, track_undo } => {
                if self.camera_2d.is_some() {
                    self.create_collision_box_from_points(&points, track_undo.unwrap_or(true));
                } else {
                    log::warn!("CreateColliderFromPoints solo disponible en modo 2D");
                }
            }
            EngineCommand::CreateExecutionAreaFromPoints { points, track_undo } => {
                if self.camera_2d.is_some() {
                    self.create_execution_area_from_points(&points, track_undo.unwrap_or(true));
                } else {
                    log::warn!("CreateExecutionAreaFromPoints solo disponible en modo 2D");
                }
            }
            EngineCommand::Undo => {
                if self.undo_last_tool_step_2d() {
                    return;
                }
                self.apply_undo();
            }
            EngineCommand::Redo => {
                self.apply_redo();
            }
            EngineCommand::SetLocale { locale } => {
                eprintln!("[i18n] SetLocale recibido: {}", locale);
                log::info!("[IPC] SetLocale: {}", locale);
                self.snap_locale = locale;
            }
            EngineCommand::ReloadAsset { path } => {
                log::info!("[IPC] ReloadAsset: {}", path);
                // Buscar UV rect pre-asignado en el atlas (sin re-empacar, sin cambiar ECS).
                if let Some(&uv_rect) = self.static_tex_cache.get(&path) {
                    match std::fs::read(&path) {
                        Ok(bytes) => {
                            use image::ImageReader;
                            match ImageReader::new(std::io::Cursor::new(&bytes))
                                .with_guessed_format()
                                .map_err(|e| e.to_string())
                                .and_then(|r| r.decode().map_err(|e| e.to_string()))
                            {
                                Ok(img) => {
                                    let rgba = img.to_rgba8();
                                    self.atlas.update(&self.queue, rgba.as_raw(), uv_rect);
                                    log::info!("[hot-reload] Textura actualizada en atlas: {}", path);
                                }
                                Err(e) => log::warn!("[hot-reload] Error decodificando PNG '{}': {}", path, e),
                            }
                        }
                        Err(e) => log::warn!("[hot-reload] Error leyendo archivo '{}': {}", path, e),
                    }
                } else if self.background_path.as_deref() == Some(path.as_str()) {
                    // El fondo usa GpuTexture propia, no el atlas — recargarlo completo.
                    self.load_background(&path);
                    log::info!("[hot-reload] Fondo recargado: {}", path);
                } else {
                    log::warn!("[hot-reload] Path no encontrado en static_tex_cache ni como fondo: {}", path);
                }
            }
            EngineCommand::SetAnimation { id, name, frames, fps, loop_, flip_horizontal, audio_path, logical_w, logical_h, scripts, is_cancelable } => {
                log::debug!("[IPC] SetAnimation: entity_id={}, name='{}', frames={}, audio={:?}, scripts={}", id, name, frames.len(), audio_path, scripts.len());

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
                    log::debug!("[SetAnimation] audio decodificado: {} ({} muestras, {}ch, {}Hz)",
                        p, samples.len(), channels, sample_rate);
                    Some(Arc::new(DecodedAudio { samples, channels, sample_rate }))
                });

                // Pre-cargar todos los frames de la animación en la caché GPU.
                // El primer PlayAnimation ya no tendrá latencia de decode+upload.
                for frame in &frames {
                    self.preload_anim_frame_with_rect(
                        &frame.path,
                        frame.src_x.zip(frame.src_y).zip(frame.src_w.zip(frame.src_h)).map(|((x, y), (w, h))| (x, y, w, h)),
                    );
                }

                // Guardar animación en el almacén por entidad+nombre.
                self.animations
                    .entry(id)
                    .or_insert_with(HashMap::new)
                    .insert(name.clone(), AnimationState {
                        frames,
                        fps,
                        loop_,
                        flip_horizontal,
                        audio_decoded,
                        logical_w,
                        logical_h,
                        scripts,
                        is_cancelable,
                    });
                self.default_animation_by_entity
                    .entry(id)
                    .or_insert(name.clone());
                log::debug!("[IPC] Animación '{}' guardada y pre-cargada para entidad {}", name, id);
            }
            EngineCommand::SetDefaultAnimation { id, name } => {
                let exists = self.animations
                    .get(&id)
                    .map(|m| m.contains_key(&name))
                    .unwrap_or(false);
                if exists {
                    self.default_animation_by_entity.insert(id, name.clone());
                    log::debug!("[animation] predeterminada de entidad {} => {}", id, name);
                } else {
                    log::warn!("[animation] set_default_animation ignorado: '{}' no existe en entidad {}", name, id);
                }
            }
EngineCommand::PlayAnimation { id, name } => {
                log::debug!("[IPC] PlayAnimation: entity_id={}, name='{}'", id, name);

                // Bloquear si la animación activa no es cancelable
                if let Some(active) = self.active_animations.get(&id) {
                    if !active.finished {
                        let current_name = active.animation_name.clone();
                        let is_cancelable = self.animations
                            .get(&id)
                            .and_then(|m| m.get(&current_name))
                            .map(|a| a.is_cancelable)
                            .unwrap_or(true);
                        if !is_cancelable {
                            log::debug!("[animation] PlayAnimation '{}' bloqueado: la animación '{}' activa no es cancelable", name, current_name);
                            return;
                        }
                    }
                }

                // Detener animación previa (el Play de audio incluye clear interno)
                self.active_animations.remove(&id);

                let anim_opt = self.animations.get(&id)
                    .and_then(|m| m.get(&name))
                    .cloned();

                match anim_opt {
                    None => log::warn!("[IPC] Animación '{}' no encontrada para entidad {}", name, id),
                    Some(anim) => {
                        // Re-baseline de posición al estado actual antes de reproducir.
                        // La escala base se conserva para evitar acumulación al alternar
                        // animaciones con distinto logical_h (grow/shrink progresivo).
                        if let Some(t) = self.world.get::<Transform>(id).cloned() {
                            self.anim_saved_transforms
                                .entry(id)
                                .and_modify(|saved| {
                                    saved.0 = t.position;
                                })
                                .or_insert((t.position, t.scale));
                        }

                        // Capturar el tiempo ANTES del I/O de archivos para que
                        // last_frame_time refleje el inicio real del frame 0, no el
                        // tiempo después de cargar texturas/audio (puede ser 50-200ms más tarde).
                        let frame_start = Instant::now();
                        let effective_flip = self.resolve_animation_flip(id, &anim);

                        // Mostrar frame 0 (cache miss solo en el primer play)
                        if let Some(first_frame) = anim.frames.first() {
                            self.play_animation_frame(
                                id,
                                &first_frame.path,
                                first_frame.pivot_x,
                                first_frame.pivot_y,
                                anim.logical_w,
                                anim.logical_h,
                                first_frame.src_x.zip(first_frame.src_y).zip(first_frame.src_w.zip(first_frame.src_h)).map(|((x, y), (w, h))| (x, y, w, h)),
                                effective_flip,
                            );
                        }

                        // Iniciar audio desde PCM pre-decodificado (cero I/O, cero decode)
                        if let Some(ref audio_decoded) = anim.audio_decoded {
                            self.play_audio_internal(Arc::clone(audio_decoded), anim.loop_);
                        }

                        // Reemplazar los scripts de animación anteriores por los de la nueva.
                        // Los scripts de entidad (LoadScript) se preservan intactos.
                        self.script_engine.detach_animation_scripts(id);
                        for script in &anim.scripts {
                            let anim_path = format!("$anim$::{}::{}", name, script.name);
                            if let Err(e) = self.script_engine.attach_script(id, &anim_path, &script.source) {
                                log::error!("[scripting] Error cargando script de animación '{}': {}", anim_path, e);
                            }
                        }
                        if !anim.scripts.is_empty() {
                            log::debug!("[scripting] {} script(s) de animación '{}' cargados para entidad {}", anim.scripts.len(), name, id);
                        }

                        self.active_animations.insert(id, ActiveAnimation {
                            animation_name: name.clone(),
                            current_frame: 0,
                            last_frame_time: frame_start,
                            fps: anim.fps,
                            finished: false,
                        });
                        log::debug!("[animation] Iniciada '{}' para entidad {} (fps={}, frames={})", name, id, anim.fps, anim.frames.len());
                    }
                }
            }
            EngineCommand::StopAnimation { id } => {
                log::info!("[IPC] StopAnimation: entity_id={}", id);
                self.anim_flip_overrides.remove(&id);
                let stopped_animation_name = self.active_animations.remove(&id).map(|a| a.animation_name);
                if self.preview_playing {
                    let fallback_name = self.default_animation_by_entity
                        .get(&id)
                        .cloned()
                        .or_else(|| {
                            self.animations
                                .get(&id)
                                .and_then(|m| m.keys().next().cloned())
                        });
                    if let Some(name) = fallback_name {
                        self.show_first_frame_of_animation(id, &name);
                    } else {
                        self.restore_animation_frame(id);
                    }
                } else if let Some(name) = stopped_animation_name {
                    // En modo edición no hay fallback automático a la predeterminada.
                    self.show_first_frame_of_animation(id, &name);
                } else {
                    self.restore_animation_frame(id);
                }
                self.stop_audio_internal();
                // Descargar scripts de la animación que estaba activa.
                self.script_engine.detach_animation_scripts(id);
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
            EngineCommand::RunControlScript { id, control_key, path, source } => {
                if !self.preview_playing {
                    return;
                }

                // Heurística de dirección basada en control horizontal.
                match control_key.as_str() {
                    "A" | "LEFT" | "D-LEFT" => { self.entity_facing_right.insert(id, false); }
                    "D" | "RIGHT" | "D-RIGHT" => { self.entity_facing_right.insert(id, true); }
                    _ => {}
                }

                let snapshot = self.build_script_snapshot(id);
                match self.script_engine.run_control_script(id, &control_key, &path, &source, snapshot.as_ref()) {
                    Ok(commands) => self.apply_script_commands(commands),
                    Err(e) => log::error!("[control] Error ejecutando script '{}' ({}): {}", path, control_key, e),
                }
            }
            EngineCommand::UnloadScript { id } => {
                log::info!("[IPC] UnloadScript: entity_id={}", id);
                self.script_engine.detach_entity(id);
            }
            EngineCommand::LoadSprite { path, name } => {
                // Cargar PNG y almacenar sus dimensiones; no se crea entidad ECS.
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
                                self.sprite_store.insert(path.clone(), (name.clone(), w, h));
                                let path_for_log = path.clone();
                                let name_for_log = name.clone();
                                send_event(&EngineEvent::SpriteLoaded { path, name, width: w, height: h });
                                log::debug!("[sprite] cargado: {} ({}) ({}x{})", path_for_log, name_for_log, w, h);
                            }
                            Err(e) => {
                                log::error!("[sprite] error decodificando {}: {}", path, e);
                                send_event(&EngineEvent::Error { message: format!("Error al decodificar sprite: {e}") });
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("[sprite] error leyendo {}: {}", path, e);
                        send_event(&EngineEvent::Error { message: format!("No se pudo leer el sprite: {e}") });
                    }
                }
            }
            EngineCommand::RemoveSprite { path } => {
                if self.sprite_store.remove(&path).is_some() {
                    send_event(&EngineEvent::SpriteRemoved { path: path.clone() });
                    log::info!("[sprite] eliminado: {}", path);
                } else {
                    log::warn!("[sprite] intento de eliminar sprite inexistente: {}", path);
                }
            }
            EngineCommand::GetSpritesList => {
                let sprites: Vec<crate::ipc::SpriteInfo> = self.sprite_store
                    .iter()
                    .map(|(path, &(ref name, w, h))| crate::ipc::SpriteInfo { path: path.clone(), name: name.clone(), width: w, height: h })
                    .collect();
                let count = sprites.len();
                send_event(&EngineEvent::SpritesList { sprites });
                log::info!("[sprite] lista enviada: {} sprites", count);
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

    fn update_entity_facing_from_horizontal(&mut self, entity_id: u32, horizontal: f32) {
        const EPS: f32 = 0.0001;
        if horizontal.abs() <= EPS {
            return;
        }
        self.entity_facing_right.insert(entity_id, horizontal > 0.0);
    }

    fn resolve_animation_flip(&self, entity_id: u32, anim: &AnimationState) -> bool {
        if let Some(forced_flip) = self.anim_flip_overrides.get(&entity_id) {
            return *forced_flip;
        }

        // `anim.flip_horizontal` representa la orientación base de autoría:
        // false = dibujada mirando derecha, true = dibujada mirando izquierda.
        let facing_right = self.entity_facing_right.get(&entity_id).copied().unwrap_or(true);
        let target_is_left = !facing_right;
        anim.flip_horizontal ^ target_is_left
    }

    fn show_first_frame_of_animation(&mut self, entity_id: u32, animation_name: &str) {
        let frame_data = self.animations
            .get(&entity_id)
            .and_then(|m| m.get(animation_name))
            .and_then(|anim| {
                anim.frames.first().map(|first| {
                    let flip = self.resolve_animation_flip(entity_id, anim);
                    (
                        first.path.clone(),
                        first.pivot_x,
                        first.pivot_y,
                        anim.logical_w,
                        anim.logical_h,
                        first.src_x.zip(first.src_y).zip(first.src_w.zip(first.src_h)).map(|((x, y), (w, h))| (x, y, w, h)),
                        flip,
                    )
                })
            });

        if let Some((path, pivot_x, pivot_y, logical_w, logical_h, src_rect, flip_horizontal)) = frame_data {
            self.play_animation_frame(entity_id, &path, pivot_x, pivot_y, logical_w, logical_h, src_rect, flip_horizontal);
        }
    }

    pub(crate) fn update_animations(&mut self) {
        let now = Instant::now();
        let mut to_play: Vec<(u32, usize)> = Vec::new();
        let mut to_restore: Vec<(u32, String)> = Vec::new();

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
                // Re-aplicar el frame actual en cada tick para mantener el ajuste
                // de pivot aunque otro sistema (p.ej. física) haya escrito Transform.
                to_play.push((entity_id, active.current_frame));
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
                    to_restore.push((entity_id, active.animation_name.clone()));
                }
            } else {
                active.current_frame = next_frame_idx;
                to_play.push((entity_id, next_frame_idx));
            }
        }

        for (entity_id, frame_idx) in to_play {
            let (path, pivot_x, pivot_y, logical_w, logical_h, src_rect, flip_horizontal) = {
                let anim_name = self.active_animations.get(&entity_id)
                    .map(|a| a.animation_name.clone())
                    .unwrap_or_default();
                let anim = self.animations.get(&entity_id)
                    .and_then(|m| m.get(&anim_name))
                    .unwrap();
                let f = &anim.frames[frame_idx];
                let flip = self.resolve_animation_flip(entity_id, anim);
                (
                    f.path.clone(),
                    f.pivot_x,
                    f.pivot_y,
                    anim.logical_w,
                    anim.logical_h,
                    f.src_x.zip(f.src_y).zip(f.src_w.zip(f.src_h)).map(|((x, y), (w, h))| (x, y, w, h)),
                    flip,
                )
            };
            self.play_animation_frame(entity_id, &path, pivot_x, pivot_y, logical_w, logical_h, src_rect, flip_horizontal);
        }

        for (entity_id, animation_name) in to_restore {
            // Desenganche de scripts de animación cuando una animación no-loop termina.
            self.script_engine.detach_animation_scripts(entity_id);
            if self.preview_playing {
                let fallback_name = self.default_animation_by_entity
                    .get(&entity_id)
                    .cloned()
                    .or_else(|| {
                        self.animations
                            .get(&entity_id)
                            .and_then(|m| m.keys().next().cloned())
                    });
                if let Some(fname) = fallback_name {
                    self.handle_command(EngineCommand::PlayAnimation { id: entity_id, name: fname });
                } else {
                    self.show_first_frame_of_animation(entity_id, &animation_name);
                }
            } else {
                // En modo edición no ejecutar fallback a animación predeterminada.
                self.show_first_frame_of_animation(entity_id, &animation_name);
            }
            // El audio no-looping se agota solo cuando las muestras PCM terminan.
            // No enviamos Stop aquí para evitar que sobrescriba un Play ya encolado
            // si el usuario dispara la siguiente animación justo al terminar esta.
            log::debug!("[animation] Enviando AnimationFinished para entidad {}", entity_id);
            send_event(&EngineEvent::AnimationFinished { entity_id });
        }

self.active_animations.retain(|_, a| !a.finished);
    }

    /// Notifica al State qué eje del gizmo está siendo arrastrado (None = sin drag).
    pub fn set_active_gizmo_axis(&mut self, axis: Option<usize>) {
        self.active_gizmo_axis = axis;
    }

    /// Muestra/oculta el hint visual de snap a cuadrícula en el viewport 2D.
    pub fn set_snap_hint_visible(&mut self, visible: bool) {
        self.show_snap_hint = visible;
    }

    fn update_snap_hint_alpha(&mut self) {
        let target = if self.show_snap_hint && !self.preview_playing && self.camera_2d.is_some() {
            1.0_f32
        } else {
            0.0_f32
        };
        // Suavizado exponencial frame-rate independiente.
        // Menor k => transición más visible y menos "instantánea".
        let k = if target > self.snap_hint_alpha { 4.2_f32 } else { 3.4_f32 };
        let blend = 1.0 - (-k * self.delta_time.max(0.0)).exp();
        self.snap_hint_alpha += (target - self.snap_hint_alpha) * blend;
        if (self.snap_hint_alpha - target).abs() < 0.001 {
            self.snap_hint_alpha = target;
        }
    }

    fn build_snap_hint_instance_2d(&self) -> Option<mesh::InstanceData> {
        if self.snap_hint_alpha <= 0.003 || self.preview_playing {
            return None;
        }
        let (uv, img_w, img_h) = if self.snap_locale == "en" {
            let uv = self.snap_hint_uv_en.or(self.snap_hint_uv)?;
            let (w, h) = if self.snap_hint_uv_en.is_some() { self.snap_hint_size_en } else { self.snap_hint_size };
            (uv, w, h)
        } else {
            let uv = self.snap_hint_uv.or(self.snap_hint_uv_en)?;
            let (w, h) = if self.snap_hint_uv.is_some() { self.snap_hint_size } else { self.snap_hint_size_en };
            (uv, w, h)
        };
        let Some(cam) = &self.camera_2d else {
            return None;
        };
        if self.size.width == 0 || self.size.height == 0 {
            return None;
        }
        if img_w <= 0.0 || img_h <= 0.0 {
            return None;
        }

        let aspect = self.size.width as f32 / self.size.height as f32;
        let half_w = cam.half_h * aspect;
        let world_per_px_x = (half_w * 2.0) / self.size.width as f32;
        let world_per_px_y = (cam.half_h * 2.0) / self.size.height as f32;

        let margin_px = 18.0_f32;
        // Tamaño proporcional al viewport pero con tope para evitar que se vea enorme.
        let max_width_px = (self.size.width as f32 * 0.22).clamp(120.0, 320.0);
        let scale_px = (max_width_px / img_w).min(1.0);
        let draw_w_px = img_w * scale_px;
        let draw_h_px = img_h * scale_px;

        let draw_w_world = draw_w_px * world_per_px_x;
        let draw_h_world = draw_h_px * world_per_px_y;
        let margin_x_world = margin_px * world_per_px_x;
        let margin_y_world = margin_px * world_per_px_y;

        // Easing para que se perciba mejor la transición.
        let a = self.snap_hint_alpha.clamp(0.0, 1.0);
        let eased_alpha = a * a * (3.0 - 2.0 * a);
        let scale_in = 0.92 + 0.08 * eased_alpha;
        let slide_px = (1.0 - eased_alpha) * 14.0;

        let center_x = cam.x - half_w + margin_x_world + draw_w_world * 0.5;
        let center_y = cam.y + cam.half_h - margin_y_world - draw_h_world * 0.5 - slide_px * world_per_px_y;
        let model = glam::Mat4::from_translation(glam::vec3(center_x, center_y, 0.9))
            * glam::Mat4::from_scale(glam::vec3(draw_w_world * scale_in, draw_h_world * scale_in, 1.0));
        let mut inst = mesh::InstanceData::new(model, 0.0, uv);
        inst.flag_pad[1] = eased_alpha;
        Some(inst)
    }

    /// Centro de selección para gizmo/grupo. Si no hay grupo, usa selected_entity.
    pub(crate) fn selection_center(&self) -> Option<glam::Vec3> {
        if !self.selected_entities.is_empty() {
            let mut sum = glam::Vec3::ZERO;
            let mut count = 0usize;
            for &id in &self.selected_entities {
                if let Some(t) = self.world.get::<Transform>(id) {
                    sum += t.position;
                    count += 1;
                }
            }
            if count > 0 {
                return Some(sum / count as f32);
            }
        }
        self.selected_entity
            .and_then(|id| self.world.get::<Transform>(id).map(|t| t.position))
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
                let facing_right = self.entity_facing_right.get(&id).copied().unwrap_or(true);
                let facing_sign = if facing_right { 1.0 } else { -1.0 };
                let animations: Vec<String> = self.animations
                    .get(&id)
                    .map(|m| m.keys().cloned().collect())
                    .unwrap_or_default();
                map.insert(id, EntitySnapshot { id, x, y, scale_x, scale_y, facing_right, facing_sign, animations });
            }
            map
        };

        let commands = self.script_engine.tick(self.delta_time, &snapshots);
        self.apply_script_commands(commands);
    }

    pub(crate) fn build_script_snapshot(&self, id: u32) -> Option<EntitySnapshot> {
        let (x, y, scale_x, scale_y) = if let Some(t) = self.world.get::<Transform>(id) {
            (t.position.x, t.position.y, t.scale.x, t.scale.y)
        } else {
            (0.0, 0.0, 1.0, 1.0)
        };
        let facing_right = self.entity_facing_right.get(&id).copied().unwrap_or(true);
        let facing_sign = if facing_right { 1.0 } else { -1.0 };

        let animations: Vec<String> = self.animations
            .get(&id)
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();

        Some(EntitySnapshot { id, x, y, scale_x, scale_y, facing_right, facing_sign, animations })
    }

    /// Aplica los comandos generados por los scripts al estado del motor.
    pub(crate) fn apply_script_commands(&mut self, commands: Vec<ScriptCmd>) {
        for cmd in commands {
            match cmd {
                ScriptCmd::SetPosition { id, x, y } => {
                    let horizontal = self.world.get::<Transform>(id)
                        .map(|t| x - t.position.x)
                        .unwrap_or(0.0);
                    self.update_entity_facing_from_horizontal(id, horizontal);
                    if let Some(t) = self.world.get_mut::<Transform>(id) {
                        t.position.x = x;
                        t.position.y = y;
                    }
                    // Sincronizar el origen de animación para que play_animation_frame
                    // no sobreescriba la posición con el valor pre-movimiento.
                    if let Some(saved) = self.anim_saved_transforms.get_mut(&id) {
                        saved.0.x = x;
                        saved.0.y = y;
                    }
                    // Sincronizar el Rapier body para que physics.step() no resetee la posición.
                    self.physics_2d.teleport_entity(id, x, y);
                }
                ScriptCmd::Translate { id, dx, dy } => {
                    self.update_entity_facing_from_horizontal(id, dx);
                    if let Some(t) = self.world.get_mut::<Transform>(id) {
                        t.position.x += dx;
                        t.position.y += dy;
                    }
                    // Propagar el desplazamiento al origen guardado de animación,
                    // de lo contrario cada frame de animación resetea la posición a orig_pos.
                    if let Some(saved) = self.anim_saved_transforms.get_mut(&id) {
                        saved.0.x += dx;
                        saved.0.y += dy;
                        log::debug!("[script/translate] entidad {} saved_x={:.3} (+{:.3})", id, saved.0.x, dx);
                    } else {
                        log::warn!("[script/translate] entidad {} SIN entrada en anim_saved_transforms — translate no acumulado", id);
                    }
                    // Sincronizar el Rapier body para que physics.step() no resetee la posición.
                    let new_pos = self.world.get::<Transform>(id)
                        .map(|t| (t.position.x, t.position.y));
                    if let Some((nx, ny)) = new_pos {
                        self.physics_2d.teleport_entity(id, nx, ny);
                    }
                }
                ScriptCmd::SetScale { id, sx, sy } => {
                    if let Some(t) = self.world.get_mut::<Transform>(id) {
                        t.scale.x = sx;
                        t.scale.y = sy;
                    }
                    // Mantener la escala base de animación en sync con scripts.
                    if let Some(saved) = self.anim_saved_transforms.get_mut(&id) {
                        saved.1.x = sx;
                        saved.1.y = sy;
                    }
                }
                ScriptCmd::PlayAnimation { id, name } => {
                    // Si la animación solicitada ya está activa en esa entidad,
                    // ignorar para evitar el bucle on_start → play_animation → on_start.
                    let already_active = self.active_animations.get(&id)
                        .map(|a| a.animation_name == name)
                        .unwrap_or(false);
                    if !already_active {
                        self.handle_command(EngineCommand::PlayAnimation { id, name });
                    }
                }
                ScriptCmd::SetDefaultAnimation { id, name } => {
                    self.handle_command(EngineCommand::SetDefaultAnimation { id, name });
                }
                ScriptCmd::StopAnimation { id } => {
                    self.handle_command(EngineCommand::StopAnimation { id });
                }
                ScriptCmd::SetPhysics { id, enabled, body_type } => {
                    // Evitar recrear el cuerpo Rapier si ya tiene el estado correcto.
                    // Destruir y recrear cada frame resetea la velocidad a 0, lo que
                    // impide que la gravedad acumule y que las colisiones funcionen.
                    let already_same = if enabled {
                        self.physics_2d.has_physics(id)
                            && self.physics_2d.get_body_type(id) == body_type
                    } else {
                        !self.physics_2d.has_physics(id)
                    };
                    if !already_same {
                        self.handle_command(EngineCommand::SetPhysics { id, enabled, body_type });
                    }
                }
                ScriptCmd::MoveEntity { id, speed, dir_x, dir_y } => {
                    self.update_entity_facing_from_horizontal(id, speed * dir_x);
                    // Aplica velocidad lineal al Rapier body usando shape cast para
                    // detectar obstáculos antes de aplicar. Si no tiene física activa,
                    // se aplica fallback por traslación directa para facilitar pruebas.
                    if self.preview_playing {
                        let moved = self.physics_2d.move_physics_entity(id, speed, dir_x, dir_y, self.delta_time);
                        if !moved {
                            let dx = speed * dir_x * self.delta_time;
                            let dy = speed * dir_y * self.delta_time;
                            if let Some(t) = self.world.get_mut::<Transform>(id) {
                                t.position.x += dx;
                                t.position.y += dy;
                            }
                            if let Some(saved) = self.anim_saved_transforms.get_mut(&id) {
                                saved.0.x += dx;
                                saved.0.y += dy;
                            }
                            log::warn!("[script/move_entity] entidad {} sin cuerpo físico activo — aplicado fallback translate", id);
                        }
                    } else {
                        // En modo editor no corremos el step de físicas; para pruebas
                        // manuales movemos por traslación directa respetando delta_time.
                        let dx = speed * dir_x * self.delta_time;
                        let dy = speed * dir_y * self.delta_time;
                        if let Some(t) = self.world.get_mut::<Transform>(id) {
                            t.position.x += dx;
                            t.position.y += dy;
                        }
                        if let Some(saved) = self.anim_saved_transforms.get_mut(&id) {
                            saved.0.x += dx;
                            saved.0.y += dy;
                        }
                    }
                }
                ScriptCmd::MoveEntityFacing { id, speed, amount_x, dir_y } => {
                    let facing_right = self.entity_facing_right.get(&id).copied().unwrap_or(true);
                    let facing_sign = if facing_right { 1.0 } else { -1.0 };
                    let dir_x = amount_x.abs() * facing_sign;

                    // Aplica velocidad lineal al Rapier body usando shape cast para
                    // detectar obstáculos antes de aplicar. Si no tiene física activa,
                    // se aplica fallback por traslación directa para facilitar pruebas.
                    if self.preview_playing {
                        let moved = self.physics_2d.move_physics_entity(id, speed, dir_x, dir_y, self.delta_time);
                        if !moved {
                            let dx = speed * dir_x * self.delta_time;
                            let dy = speed * dir_y * self.delta_time;
                            if let Some(t) = self.world.get_mut::<Transform>(id) {
                                t.position.x += dx;
                                t.position.y += dy;
                            }
                            if let Some(saved) = self.anim_saved_transforms.get_mut(&id) {
                                saved.0.x += dx;
                                saved.0.y += dy;
                            }
                            log::warn!("[script/move_entity_facing] entidad {} sin cuerpo físico activo — aplicado fallback translate", id);
                        }
                    } else {
                        // En modo editor no corremos el step de físicas; para pruebas
                        // manuales movemos por traslación directa respetando delta_time.
                        let dx = speed * dir_x * self.delta_time;
                        let dy = speed * dir_y * self.delta_time;
                        if let Some(t) = self.world.get_mut::<Transform>(id) {
                            t.position.x += dx;
                            t.position.y += dy;
                        }
                        if let Some(saved) = self.anim_saved_transforms.get_mut(&id) {
                            saved.0.x += dx;
                            saved.0.y += dy;
                        }
                    }
                }
                ScriptCmd::Log { message } => {
                    // Evitar spam en stderr/frontend durante input continuo.
                    // El mensaje queda disponible solo en nivel debug.
                    log::debug!("[script] {message}");
                }
            }
        }
    }

    // ── Update ───────────────────────────────────────────────────────────────

    pub fn update(&mut self) {
        let now         = Instant::now();
        self.delta_time = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;
        self.update_snap_hint_alpha();

        // Emitir métricas de debug ~1 vez por segundo.
        self.metrics_frame_count += 1;
        if now.duration_since(self.metrics_last_emit) >= std::time::Duration::from_secs(1) {
            let elapsed_secs = now.duration_since(self.metrics_last_emit).as_secs_f32();
            let fps = self.metrics_frame_count as f32 / elapsed_secs;
            let physics_bodies = if self.camera_2d.is_some() {
                self.physics_2d.body_count()
            } else {
                self.physics.body_count()
            };
            send_event(&EngineEvent::DebugMetrics {
                fps,
                frame_time_ms:  self.delta_time * 1000.0,
                draw_calls:     self.last_draw_calls,
                physics_bodies,
            });
            self.metrics_last_emit   = now;
            self.metrics_frame_count = 0;
        }
        if self.camera_2d.is_some() {
            // Scripts corren siempre (editor + juego) para facilitar pruebas rápidas.
            self.update_scripts();
            if self.preview_playing {
                // En modo juego sí aplicamos físicas completas.
                self.physics_2d.step(self.delta_time, &mut self.world);
                // Sincronizar anim_saved_transforms con la posición post-physics (ya bloqueada
                // por colisiones) para que update_animations() no restaure la posición original.
                self.sync_physics_anim_origins();
                self.update_execution_areas_2d();
            }
        } else {
            self.update_scripts();
            if self.preview_playing {
                self.physics.step(self.delta_time, &mut self.world);
            }
        }
    }

    /// Sincroniza anim_saved_transforms desde la posición actual del Transform
    /// para entidades que tienen física activa y están en medio de una animación.
    /// Necesario para que move_physics_entity funcione con animaciones de pivot.
    fn sync_physics_anim_origins(&mut self) {
        let ids: Vec<u32> = self.anim_saved_transforms.keys().copied().collect();
        for id in ids {
            if self.physics_2d.has_physics(id) {
                if let Some(t) = self.world.get::<Transform>(id) {
                    let (px, py) = (t.position.x, t.position.y);
                    if let Some(saved) = self.anim_saved_transforms.get_mut(&id) {
                        saved.0.x = px;
                        saved.0.y = py;
                    }
                }
            }
        }
    }

    // ── Render ───────────────────────────────────────────────────────────────

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        self.update_animations();
        let mut draw_calls: u32 = 0;

        // ── Paso 0: reconstruir spatial grid para picking ──────────────────────────
        self.spatial_grid.clear();
        for &entity in self.world.entities() {
            if let Some(t) = self.world.get::<crate::ecs::Transform>(entity) {
                let sx = t.scale.x.abs() * 0.5;
                let sy = t.scale.y.abs() * 0.5;
                let min_x = t.position.x - sx;
                let min_y = t.position.y - sy;
                let max_x = t.position.x + sx;
                let max_y = t.position.y + sy;
                self.spatial_grid.insert_entity(entity, [min_x, min_y, max_x, max_y]);
            }
        }

        let output  = self.surface.get_current_texture()?;
        let view    = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut enc = self.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("render-encoder") },
        );

        // ── Paso 1: escribir uniforms de escena compartidos (view_proj + cam_pos) ──
        {
            let scene_uni = if let Some(cam2d) = &self.camera_2d {
                build_scene_uniforms_2d(cam2d, self.size)
            } else {
                build_scene_uniforms(&self.camera, self.size)
            };
            self.queue.write_buffer(&self.scene_buffer, 0, bytemuck::cast_slice(&[scene_uni]));
        }

        // ── Paso 2: recopilar entidades visibles (frustum culling + sort layer+Z) ──
        let aspect_fc = self.size.width as f32 / self.size.height as f32;
        let frustum_vp_3d: Option<glam::Mat4> = self.camera_2d.is_none().then(|| {
            let raw = self.camera.to_uniform(aspect_fc).view_proj;
            glam::Mat4::from_cols_array_2d(&raw)
        });
        // query2<MeshComponent, Transform> itera solo entidades con ambos componentes,
        // evitando el scan de todas las entidades + doble lookup de hash por entidad.
        let mut entities: Vec<(crate::ecs::EntityId, usize, usize, Mat4, i32, f32)> =
            self.world.query2::<MeshComponent, crate::ecs::Transform>()
            .filter_map(|(id, mc, t)| {
                let mesh_idx = mc.mesh_idx;
                let tex_idx  = mc.tex_idx;
                // ── Frustum culling ──────────────────────────────────────────
                let visible = if let Some(cam2d) = &self.camera_2d {
                    is_visible_2d(cam2d, t.position, t.scale, aspect_fc)
                } else if let Some(vp) = &frustum_vp_3d {
                    let radius = t.scale.x.abs().max(t.scale.y.abs()).max(t.scale.z.abs()) * 0.87;
                    is_visible_3d(vp, t.position, radius)
                } else {
                    true
                };
                if !visible { return None; }
                let model_mat = t.to_matrix();
                let layer     = self.world.get::<crate::ecs::RenderLayer>(id).map(|rl| rl.value).unwrap_or(0);
                let z         = t.position.z;
                Some((id, mesh_idx, tex_idx, model_mat, layer, z))
            }).collect();
        // Sort by (layer ASC, z ASC) — lower layer first, within layer sort by z (back-to-front)
        entities.sort_by(|a, b| {
            let layer_cmp = a.4.cmp(&b.4);
            if layer_cmp != std::cmp::Ordering::Equal {
                layer_cmp
            } else {
                a.5.partial_cmp(&b.5).unwrap_or(std::cmp::Ordering::Equal)
            }
        });

        // ── Paso 3: agrupar en batches por mesh_idx ────────────────────────
        // Con el atlas todas las entidades comparten el mismo bind group,
        // así que solo agrupamos por geometría (mesh_idx).
        // El UV rect viaja en cada instancia — no hay cambio de bind group entre batches.
        struct Batch {
            mesh_idx:  usize,
            instances: Vec<mesh::InstanceData>,
        }
        let mut batches: Vec<Batch> = Vec::new();
        for (entity_id, mesh_idx, tex_idx, model_matrix, _layer, _z) in &entities {
            if self.preview_playing
                && (self.collider_entities.contains(entity_id)
                    || self.execution_area_entities.contains(entity_id))
            {
                continue;
            }
            let is_selected = self.selected_entity == Some(*entity_id)
                || self.selected_entities.contains(entity_id);
            let flag = if self.preview_playing {
                0.0_f32
            } else if is_selected {
                1.0_f32   // dorado
            } else if self.hovered_entity == Some(*entity_id) {
                2.0_f32   // cian
            } else {
                0.0_f32
            };
            // anim_overrides tiene prioridad sobre uv_rects[]:
            // durante una animación activa evita mutar la UV base de la entidad.
            let uv = self.anim_overrides.get(tex_idx)
                .copied()
                .or_else(|| self.uv_rects.get(*tex_idx).copied())
                .unwrap_or(self.fallback_uv);
            let mut inst = mesh::InstanceData::new(*model_matrix, flag, uv);
            inst.flag_pad[2] = if self.world.get::<crate::config_2d::ColliderMarker>(*entity_id).is_some() {
                1.0_f32
            } else if self.world.get::<crate::config_2d::ExecutionAreaMarker>(*entity_id).is_some() {
                2.0_f32
            } else {
                0.0_f32
            };
            // Extender el último batch si coincide mesh (mismo UV rect viaja por instancia)
            let can_extend = batches.last().map_or(false, |b| b.mesh_idx == *mesh_idx);
            if can_extend {
                batches.last_mut().unwrap().instances.push(inst);
            } else {
                batches.push(Batch { mesh_idx: *mesh_idx, instances: vec![inst] });
            }
        }

        // ── Paso 4: crear buffers de instancias en GPU ──────────────────────
        let instance_buffers: Vec<wgpu::Buffer> = batches.iter().map(|b| {
            use wgpu::util::DeviceExt;
            self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label:    Some("inst-buf"),
                contents: bytemuck::cast_slice(&b.instances),
                usage:    wgpu::BufferUsages::VERTEX,
            })
        }).collect();

        // ── Paso 5: render pass principal ──────────────────────────────────
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

            // El bind group 0 (view_proj + cam_pos) es compartido por todos los batches.
            pass.set_bind_group(0, &self.scene_bind_group, &[]);
            // El bind group 1 (atlas) es compartido por TODOS los sprites — se setea UNA vez.
            pass.set_bind_group(1, self.atlas.bind_group.as_ref(), &[]);

            for (batch, inst_buf) in batches.iter().zip(instance_buffers.iter()) {
                let Some(mesh) = self.meshes.get(batch.mesh_idx) else { continue };
                pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, inst_buf.slice(..));
                pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.index_count, 0, 0..batch.instances.len() as u32);
                draw_calls += 1;
            }
        }

        // ── Grid pass (solo modo 2D; borde siempre visible, líneas según config) ──
        if !self.preview_playing {
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
                draw_calls += 1;
            }
        }

        // ── Tool overlay pass (solo modo 2D; cruces + líneas de construcción) ──
        if !self.preview_playing && self.camera_2d.is_some() && self.tool_overlay_buffer.vertex_count > 0 {
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
            draw_calls += 1;
        }

        // ── Snap hint pass (PNG en viewport 2D durante drag de gizmo) ──────
        if let Some(hint_inst) = self.build_snap_hint_instance_2d() {
            use wgpu::util::DeviceExt;
            let hint_inst_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label:    Some("snap-hint-inst-buf"),
                contents: bytemuck::cast_slice(&[hint_inst]),
                usage:    wgpu::BufferUsages::VERTEX,
            });

            if let Some(mesh) = self.meshes.get(self.canonical_quad_idx) {
                let mut hint_pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("snap-hint-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view:           &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load:  wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load:  wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    occlusion_query_set:      None,
                    timestamp_writes:         None,
                });
                hint_pass.set_pipeline(&self.render_pipeline_2d);
                hint_pass.set_bind_group(0, &self.scene_bind_group, &[]);
                hint_pass.set_bind_group(1, self.atlas.bind_group.as_ref(), &[]);
                hint_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                hint_pass.set_vertex_buffer(1, hint_inst_buf.slice(..));
                hint_pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                hint_pass.draw_indexed(0..mesh.index_count, 0, 0..1);
                draw_calls += 1;
            }
        }

        // ── Gizmos (segundo pass, sin depth) ─────────────────────────────────
        // Ocultar gizmo durante el modo edición de pivot: las flechas de movimiento
        // robarían el foco e impedirían hacer click libremente sobre el asset.
        if !self.preview_playing {
            if let Some(origin) = self.selection_center().filter(|_| self.pivot_edit_mode.is_none()) {
            let aspect   = self.size.width as f32 / self.size.height as f32;
            let vp = if let Some(cam2d) = &self.camera_2d {
                cam2d.view_proj(aspect).to_cols_array_2d()
            } else {
                self.camera.to_uniform(aspect).view_proj
            };

            // Situar el gizmo en el centro de selección (single o multi-select)
            let gizmo_model = glam::Mat4::from_translation(origin);

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
            draw_calls += 1;
            }
        }

        self.queue.submit(std::iter::once(enc.finish()));
        self.last_draw_calls = draw_calls;
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

fn build_scene_uniforms(camera: &Camera, size: PhysicalSize<u32>) -> SceneUniforms {
    let aspect    = size.width as f32 / size.height as f32;
    let view_proj = camera.to_uniform(aspect).view_proj;
    let p = camera.position();
    SceneUniforms {
        view_proj,
        cam_pos: [p.x, p.y, p.z, 0.0],
    }
}

fn build_scene_uniforms_2d(cam: &Camera2D, size: PhysicalSize<u32>) -> SceneUniforms {
    let aspect    = size.width as f32 / size.height as f32;
    let view_proj = cam.view_proj(aspect).to_cols_array_2d();
    let p = cam.position();
    SceneUniforms {
        view_proj,
        cam_pos: [p.x, p.y, p.z, 0.0],
    }
}

// ── Frustum culling ───────────────────────────────────────────────────────────

/// Culling 2D: comprueba si el AABB de la entidad es visible en el rectángulo
/// ortográfico de la cámara. Se añade un margen proporcional al zoom para evitar
/// popping en entidades con geometría más grande que su Transform (ej. sprites con pivot).
///
/// Retorna `true` si la entidad DEBE dibujarse.
pub(crate) fn is_visible_2d(cam: &Camera2D, pos: GlamVec3, scale: GlamVec3, aspect: f32) -> bool {
    let half_w = cam.half_h * aspect;
    // Margen de seguridad: la mitad del lado mayor de la entidad
    let margin = scale.x.abs().max(scale.y.abs()) * 0.5;
    let min_x = cam.x - half_w  - margin;
    let max_x = cam.x + half_w  + margin;
    let min_y = cam.y - cam.half_h - margin;
    let max_y = cam.y + cam.half_h + margin;

    let e_min_x = pos.x - scale.x.abs() * 0.5;
    let e_max_x = pos.x + scale.x.abs() * 0.5;
    let e_min_y = pos.y - scale.y.abs() * 0.5;
    let e_max_y = pos.y + scale.y.abs() * 0.5;

    e_max_x >= min_x && e_min_x <= max_x && e_max_y >= min_y && e_min_y <= max_y
}

/// Culling 3D: extrae los 6 planos del frustum de la view_proj y testea
/// si una esfera de radio `radius` centrada en `center` es visible.
///
/// Los planos se normalizan para que la distancia sea métrica.
/// Retorna `true` si la entidad DEBE dibujarse.
pub(crate) fn is_visible_3d(view_proj: &glam::Mat4, center: GlamVec3, radius: f32) -> bool {
    let m = view_proj.to_cols_array_2d();
    // Filas de la matriz (columna-major en glam → transponer para filas)
    let r0 = [m[0][0], m[1][0], m[2][0], m[3][0]];
    let r1 = [m[0][1], m[1][1], m[2][1], m[3][1]];
    let r2 = [m[0][2], m[1][2], m[2][2], m[3][2]];
    let r3 = [m[0][3], m[1][3], m[2][3], m[3][3]];

    // Los 6 planos del frustum (izq, der, abajo, arriba, cerca, lejos)
    let planes: [[f32; 4]; 6] = [
        [r3[0]+r0[0], r3[1]+r0[1], r3[2]+r0[2], r3[3]+r0[3]], // izquierda
        [r3[0]-r0[0], r3[1]-r0[1], r3[2]-r0[2], r3[3]-r0[3]], // derecha
        [r3[0]+r1[0], r3[1]+r1[1], r3[2]+r1[2], r3[3]+r1[3]], // abajo
        [r3[0]-r1[0], r3[1]-r1[1], r3[2]-r1[2], r3[3]-r1[3]], // arriba
        [r3[0]+r2[0], r3[1]+r2[1], r3[2]+r2[2], r3[3]+r2[3]], // cerca
        [r3[0]-r2[0], r3[1]-r2[1], r3[2]-r2[2], r3[3]-r2[3]], // lejos
    ];

    for plane in &planes {
        let len = (plane[0]*plane[0] + plane[1]*plane[1] + plane[2]*plane[2]).sqrt();
        if len < 1e-6 { continue; }
        let dist = (plane[0]*center.x + plane[1]*center.y + plane[2]*center.z + plane[3]) / len;
        if dist < -radius {
            return false; // esfera completamente fuera de este plano
        }
    }
    true
}
