use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use glam::Vec3 as GlamVec3;
use winit::{dpi::PhysicalSize, window::Window};

#[path = "../save_snapshot.rs"]
mod save_snapshot;

mod animation_play_state;
mod animations;
mod audio;
mod commands;
mod entity_restore;
mod import_scene;
mod init;
mod load_proyect;
mod render;
mod render_helpers;
mod scene_scripts;
mod scripts;
mod snap_hint;
mod tick;
pub(crate) mod types;
mod undo_entity;

use audio::AudioSlot;
#[allow(unused_imports)]
pub use audio::{AudioCmd, DecodedAudio};
use render_helpers::create_depth_texture;
#[allow(unused_imports)]
pub(crate) use render_helpers::is_visible_2d;
pub use types::{ActiveAnimation, AnimTextureCacheEntry, AnimationState};
pub(crate) use types::{EntityTransformSnapshot, PendingSlide, UndoAction};

use crate::config_2d::ActiveTool;
use crate::config_2d::{GridBuffer, GridConfig};

use crate::config_2d::Camera2D;
use crate::config_2d::PhysicsWorld2D;
use crate::config_compat::Camera;
use crate::ecs::{NameComponent, Transform, World};
use crate::entity_save_meta::EntitySaveRegistry;
use crate::gizmo::GizmoBuffer;
use crate::ipc::{EngineCommand, EngineCommandCommon};
use crate::mesh::Mesh;
use crate::scripting::ScriptEngine;

use crate::ecs::EntityId;

pub struct State {
    pub(crate) window: Arc<Window>,
    pub(crate) surface: wgpu::Surface<'static>,
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) config: wgpu::SurfaceConfiguration,
    pub(crate) size: PhysicalSize<u32>,
    pub(crate) clear_color: wgpu::Color,
    pub(crate) render_pipeline: wgpu::RenderPipeline,
    /// Pipeline para modo 2D: sin depth-write, CompareFunction::Always.
    /// Permite que el alpha blending funcione correctamente con back-to-front sort.
    pub(crate) render_pipeline_2d: wgpu::RenderPipeline,
    /// Sprites sobre swapchain (1 color target; p.ej. hints tras el blit de escena).
    pub(crate) render_pipeline_overlay: wgpu::RenderPipeline,
    pub(crate) depth_view: wgpu::TextureView,
    /// Escena offscreen + blit al swapchain (sin TAA).
    pub(crate) scene_target: crate::scene_target::SceneTarget,
    // Uniforms (group 0) — un buffer por malla para que cada draw call
    // tenga sus propios datos y write_buffer no sobreescriba el anterior.
    pub(crate) scene_buffer: wgpu::Buffer,
    /// Bind group del buffer escena (group 0, binding 0).
    pub(crate) scene_bind_group: wgpu::BindGroup,
    // Texturas (group 1) — todas en el atlas compartido
    /// Atlas de texturas: una sola textura GPU 4096×4096 que empaca todos los sprites.
    /// Todas las entidades comparten su bind group, eliminando los cambios de grupo por batch.
    pub(crate) atlas: crate::texture::TextureAtlas,
    /// UV rects de cada textura en el atlas, indexados por `MeshComponent.tex_idx`.
    pub(crate) uv_rects: Vec<[f32; 4]>,
    /// UV del pixel blanco 1×1 en (0,0) del atlas — fallback cuando tex_idx es inválido.
    pub(crate) fallback_uv: [f32; 4],
    /// Caché de texturas estáticas PNG: path → UV rect en el atlas.
    pub(crate) static_tex_cache: std::collections::HashMap<String, [f32; 4]>,
    /// Índice en `meshes[]` del quad unitario canónico (1×1 en origen).
    /// Todos los sprites 2D apuntan a este mesh; sus texturas individuales
    /// se almacenan en `textures[]` indexadas por `MeshComponent.tex_idx`.
    pub(crate) canonical_quad_idx: usize,
    // Cámara
    pub camera: Camera,
    /// Cámara 2D ortográfica activa cuando se carga una escena 2D.
    /// Si es `None`, se usa la cámara base del editor.
    pub camera_2d: Option<Camera2D>,
    // Escena y mallas
    pub(crate) meshes: Vec<Mesh>,
    pub(crate) world: World,
    // Tiempo
    pub(crate) last_frame: Instant,
    pub delta_time: f32,
    // Gizmos
    pub(crate) gizmo_pipeline: wgpu::RenderPipeline,
    pub(crate) gizmo_buffer: GizmoBuffer,
    pub(crate) gizmo_bind_group: wgpu::BindGroup,
    pub(crate) gizmo_buffer_uni: wgpu::Buffer,
    // Física
    pub physics_2d: PhysicsWorld2D,
    // Selección
    pub selected_entity: Option<EntityId>,
    pub selected_entities: Vec<EntityId>,
    pub hovered_entity: Option<EntityId>,
    pub hovered_gizmo_axis: Option<usize>,
    pub active_gizmo_axis: Option<usize>,
    // Spatial partitioning para picking/queries
    pub(crate) spatial_grid: crate::spatial::SpatialGrid,
    // Escenario 2D: lista de entidades ECS que actúan como fondos PNG.
    pub(crate) scenario_entities: Vec<EntityId>,
    // Personajes 2D: lista de entidades ECS que actúan como sprites de personaje.
    pub(crate) character_entities: Vec<EntityId>,
    // Plantillas de proyectil 2D.
    pub(crate) projectile_entities: Vec<EntityId>,
    pub(crate) entity_projectile_config:
        std::collections::HashMap<EntityId, crate::config_2d::projectiles::ProjectileConfig>,
    pub(crate) active_projectiles: Vec<crate::config_2d::projectiles::ActiveProjectile>,
    // Fondo del mundo 2D: entidad especial no seleccionable que cubre todo el área.
    pub(crate) background_entity: Option<EntityId>,
    pub(crate) background_path: Option<String>,
    // Grid 2D: cuadrícula y límites del mundo.
    pub(crate) grid_config: GridConfig,
    pub(crate) grid_pipeline: wgpu::RenderPipeline,
    pub(crate) grid_buffer: GridBuffer,
    pub(crate) grid_bind_group: wgpu::BindGroup,
    pub(crate) grid_buffer_uni: wgpu::Buffer,
    /// Estado de la tecla Ctrl (enviado por IPC desde Electron, ya que la ventana overlay
    /// no recibe keyboard events directamente).
    pub(crate) ctrl_held: bool,
    pub(crate) shift_held: bool,
    /// Herramienta de dibujo activa en modo 2D.
    pub active_tool: ActiveTool,
    /// Entidad fantasma para previsualizar el blueprint a colocar (Quick Build mode).
    pub(crate) quick_build_ghost_id: Option<EntityId>,
    /// Ruta de asset de la blueprint activa en Quick Build (para snap por igualdad de blueprint).
    pub(crate) quick_build_preview_path: Option<String>,
    /// Tipo de blueprint activa en Quick Build ("scenario" | "character").
    pub(crate) quick_build_preview_kind: Option<String>,
    /// Escala base de la blueprint activa (sin ajuste dinámico por Ctrl).
    pub(crate) quick_build_preview_scale: Option<[f32; 3]>,
    /// true = modo juego (simulación), false = modo editor.
    pub preview_playing: bool,
    /// Última posición del cursor en píxeles durante play (para aim en scripts de control).
    pub(crate) play_mouse_px: Option<(f32, f32)>,
    /// Edición de UI del jugador (cuadrícula de trabajo NDC).
    pub(crate) player_ui_edit_active: bool,
    pub(crate) ui_work_grid_buffer: GizmoBuffer,
    pub(crate) player_ui_edit_scope: Option<String>,
    pub(crate) player_ui_edit_screen_id: Option<String>,
    pub(crate) player_ui_active_player_screen_id: Option<String>,
    pub(crate) player_ui_player_screen_names: HashMap<String, String>,
    pub(crate) player_ui_text_boxes:
        HashMap<String, Vec<crate::config_2d::player_ui::PlayerUiTextBox>>,
    pub(crate) player_ui_buttons: HashMap<String, Vec<crate::config_2d::player_ui::PlayerUiButton>>,
    pub(crate) player_ui_images: HashMap<String, Vec<crate::config_2d::player_ui::PlayerUiImage>>,
    pub(crate) player_ui_objects: HashMap<String, Vec<crate::config_2d::player_ui::PlayerUiObject>>,
    pub(crate) player_ui_object_draw:
        Option<crate::config_2d::player_ui::object::PlayerUiObjectDrawSession>,
    pub(crate) player_ui_object_draw_overlay: GizmoBuffer,
    pub(crate) player_ui_text_next_id: u32,
    pub(crate) player_ui_text_overlay_buffer: GizmoBuffer,
    pub(crate) player_ui_text_atlas: crate::screen_hud_image::ScreenHudAtlas,
    pub(crate) player_ui_hud_texture_cache:
        HashMap<String, crate::screen_hud_image::ScreenHudPackedImage>,
    pub(crate) player_ui_font_cache: HashMap<String, std::sync::Arc<ab_glyph::FontArc>>,
    pub(crate) player_ui_glyph_instances: Vec<crate::mesh::InstanceData>,
    pub(crate) player_ui_glyph_instance_buffer: Option<wgpu::Buffer>,
    pub(crate) player_ui_selected_text_id: Option<u32>,
    pub(crate) player_ui_selected_button_id: Option<u32>,
    pub(crate) player_ui_selected_image_id: Option<u32>,
    pub(crate) player_ui_selected_object_id: Option<u32>,
    pub(crate) player_ui_text_editing_id: Option<u32>,
    pub(crate) player_ui_text_caret: usize,
    pub(crate) player_ui_caret_blink_epoch: Instant,
    pub(crate) player_ui_caret_buffer: GizmoBuffer,
    pub(crate) player_ui_text_drag: Option<crate::config_2d::player_ui::PlayerUiTextDrag>,
    pub(crate) player_ui_last_text_click: Option<(u32, Instant)>,
    pub(crate) screen_hud_pipeline: wgpu::RenderPipeline,
    pub(crate) hud_scene_bind_group: wgpu::BindGroup,
    pub(crate) hud_quad_mesh: Mesh,
    pub(crate) hud_image_store: HashMap<String, crate::hud_image_asset::HudImageAssetMeta>,
    /// Muestra los colliders overlay incluso en modo juego (debug toggle).
    pub(crate) debug_mode: bool,
    /// V-Sync activado en el swapchain.
    pub(crate) vsync_enabled: bool,
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
    /// Offset visual acumulado por pivot de animación: entidades con física
    /// mantienen t.position = body position; este offset se suma al renderizar.
    pub(crate) visual_offsets: std::collections::HashMap<u32, GlamVec3>,
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
    pub(crate) anim_texture_cache: std::collections::HashMap<String, AnimTextureCacheEntry>,
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
    /// Sistema de scripting Rhai. Contiene la VM y los scripts adjuntos a entidades.
    pub(crate) script_engine: ScriptEngine,
    /// Mapa de bindings de control runtime. El frontend solo sincroniza esta
    /// configuración; el motor resuelve y ejecuta los scripts al detectar input.
    pub(crate) control_bindings_by_entity: HashMap<u32, crate::ipc::ControlBindingsData>,
    /// Almacén de sprites cargados (PNGs) para reutilización en el editor.
    /// No se renderizan directamente; actúan como biblioteca de imágenes.
    pub(crate) sprite_store: HashMap<String, (String, u32, u32)>, // path → (name, width, height)
    /// Almacén de sonidos registrados para reutilización en el editor.
    /// path → name; la reproducción se hace directamente con play_audio.
    pub(crate) sound_store: HashMap<String, String>, // path → name
    /// Almacén de fuentes registradas para reutilización en el editor.
    pub(crate) font_store: HashMap<String, String>, // path → name
    /// Almacén de fondos registrados para reutilización en el editor.
    /// path → name; el fondo activo se gestiona por separado con load_background.
    pub(crate) background_store: HashMap<String, String>, // path → name
    /// Historial simple para deshacer cambios del editor.
    pub(crate) undo_stack: Vec<UndoAction>,
    /// Historial para rehacer (Ctrl+Y). Se limpia al registrar una acción nueva.
    pub(crate) redo_stack: Vec<UndoAction>,
    /// Evita registrar nuevas entradas de undo mientras se está aplicando una.
    pub(crate) is_applying_undo: bool,
    // ── Debug metrics ────────────────────────────────────────────────────────
    process_metrics_sampler: rer_engine_shared::process_metrics::ProcessMetricsSampler,
    metrics_last_emit: Instant,
    metrics_frame_count: u32,
    last_draw_calls: u32,
    autosave_enabled: bool,
    autosave_last_tick: Instant,
    /// Límite de FPS del bucle (sincronizado con `set_target_fps`).
    pub(crate) target_fps: u64,
    /// Bloqueo persistente de input horizontal para `on_keep`.
    /// Mientras exista, la misma dirección no vuelve a ejecutar el callback de control.
    pub(crate) blocked_on_keep_horizontal: HashMap<u32, f32>,
    /// Deslizamientos suaves en curso iniciados desde `on_press`.
    /// Cada frame, la entidad avanza hacia su destino a la velocidad indicada
    /// usando el shape-cast kinematic (colisiones incluidas).
    pub(crate) pending_slides: HashMap<u32, PendingSlide>,
    pub(crate) save_registry: EntitySaveRegistry,
    /// Suprime logs de plantilla/setup durante `import_scene` (carga .save o cambio de escena).
    pub(crate) suppress_scene_setup_logs: bool,
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
        let names = self
            .world
            .query::<NameComponent>()
            .map(|(_, c)| c.name.clone());
        rer_engine_shared::editor_defaults::next_numbered_entity_label(base, names)
    }

    // ── Accesores ─────────────────────────────────────────────────────────────

    pub fn window(&self) -> &Arc<Window> {
        &self.window
    }
    pub fn size(&self) -> PhysicalSize<u32> {
        self.size
    }
    pub fn is_preview_playing(&self) -> bool {
        self.preview_playing
    }

    /// Sincroniza el body 2D con una mutacion externa del `Transform`.
    /// No reemplaza el movimiento de gameplay: ese debe pasar por
    /// `move_physics_entity()` o por las rutas kinematic del runtime.
    pub(crate) fn sync_physics_2d_body_from_transform(&mut self, id: u32) {
        let Some((x, y)) = self
            .world
            .get::<Transform>(id)
            .map(|t| (t.position.x, t.position.y))
        else {
            return;
        };
        self.physics_2d.sync_body_from_transform(id, x, y);
    }

    /// Variante directa para los casos donde ya se conocen las coordenadas.
    pub(crate) fn sync_physics_2d_body_from_xy(&mut self, id: u32, x: f32, y: f32) {
        self.physics_2d.sync_body_from_transform(id, x, y);
    }

    pub fn push_undo_transform(
        &mut self,
        id: u32,
        position: [f32; 3],
        rotation: [f32; 4],
        scale: [f32; 3],
    ) {
        if !self.is_applying_undo {
            self.redo_stack.clear();
        }
        self.undo_stack.push(UndoAction::RestoreTransform {
            id,
            position,
            rotation,
            scale,
        });
    }

    pub fn push_undo_transforms(&mut self, items: Vec<EntityTransformSnapshot>) {
        if items.is_empty() {
            return;
        }
        if !self.is_applying_undo {
            self.redo_stack.clear();
        }
        self.undo_stack
            .push(UndoAction::RestoreTransforms { items });
    }

    pub fn apply_undo(&mut self) {
        let Some(action) = self.undo_stack.pop() else {
            return;
        };
        self.is_applying_undo = true;
        match action {
            UndoAction::RestoreTransform {
                id,
                position,
                rotation,
                scale,
            } => {
                if let Some(t) = self.world.get::<Transform>(id) {
                    self.redo_stack.push(UndoAction::RestoreTransform {
                        id,
                        position: t.position.to_array(),
                        rotation: [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w],
                        scale: t.scale.to_array(),
                    });
                }
                self.handle_command(EngineCommand::Common(EngineCommandCommon::SetTransform {
                    id,
                    position: Some(position),
                    rotation: Some(rotation),
                    scale: Some(scale),
                    track_undo: Some(false),
                    position_axis: None,
                    scale_axis: None,
                    body_rotation_only: None,
                    rotation_euler_delta: None,
                    rotation_euler_degrees: None,
                }));
            }
            UndoAction::RestoreTransforms { items } => {
                let mut redo_items: Vec<EntityTransformSnapshot> = Vec::new();
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
                    self.redo_stack
                        .push(UndoAction::RestoreTransforms { items: redo_items });
                }
                for (id, position, rotation, scale) in items {
                    self.handle_command(EngineCommand::Common(EngineCommandCommon::SetTransform {
                        id,
                        position: Some(position),
                        rotation: Some(rotation),
                        scale: Some(scale),
                        track_undo: Some(false),
                        position_axis: None,
                        scale_axis: None,
                        body_rotation_only: None,
                        rotation_euler_delta: None,
                        rotation_euler_degrees: None,
                    }));
                }
            }
            UndoAction::RemoveEntity { snapshot } => {
                let id = snapshot.id;
                self.handle_command(EngineCommand::Common(EngineCommandCommon::RemoveEntity {
                    id,
                }));
                self.redo_stack.push(UndoAction::RestoreEntity { snapshot });
            }
            UndoAction::RestoreEntity { snapshot } => {
                self.restore_entity_from_undo_snapshot(&snapshot);
                self.redo_stack.push(UndoAction::RemoveEntity { snapshot });
            }
            UndoAction::RestorePlayerUiHud { snapshot } => {
                if let Some(current) =
                    self.capture_player_ui_hud_undo_snapshot_with_key(&snapshot.key)
                {
                    self.redo_stack
                        .push(UndoAction::RestorePlayerUiHud { snapshot: current });
                }
                self.restore_player_ui_hud_undo_snapshot(snapshot);
            }
        }
        self.is_applying_undo = false;
    }

    pub fn apply_redo(&mut self) {
        let Some(action) = self.redo_stack.pop() else {
            return;
        };
        self.is_applying_undo = true;
        match action {
            UndoAction::RestoreTransform {
                id,
                position,
                rotation,
                scale,
            } => {
                if let Some(t) = self.world.get::<Transform>(id) {
                    self.undo_stack.push(UndoAction::RestoreTransform {
                        id,
                        position: t.position.to_array(),
                        rotation: [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w],
                        scale: t.scale.to_array(),
                    });
                }
                self.handle_command(EngineCommand::Common(EngineCommandCommon::SetTransform {
                    id,
                    position: Some(position),
                    rotation: Some(rotation),
                    scale: Some(scale),
                    track_undo: Some(false),
                    position_axis: None,
                    scale_axis: None,
                    body_rotation_only: None,
                    rotation_euler_delta: None,
                    rotation_euler_degrees: None,
                }));
            }
            UndoAction::RestoreTransforms { items } => {
                let mut undo_items: Vec<EntityTransformSnapshot> = Vec::new();
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
                    self.undo_stack
                        .push(UndoAction::RestoreTransforms { items: undo_items });
                }
                for (id, position, rotation, scale) in items {
                    self.handle_command(EngineCommand::Common(EngineCommandCommon::SetTransform {
                        id,
                        position: Some(position),
                        rotation: Some(rotation),
                        scale: Some(scale),
                        track_undo: Some(false),
                        position_axis: None,
                        scale_axis: None,
                        body_rotation_only: None,
                        rotation_euler_delta: None,
                        rotation_euler_degrees: None,
                    }));
                }
            }
            UndoAction::RestoreEntity { snapshot } => {
                self.restore_entity_from_undo_snapshot(&snapshot);
                self.undo_stack.push(UndoAction::RemoveEntity { snapshot });
            }
            UndoAction::RemoveEntity { snapshot } => {
                let id = snapshot.id;
                self.handle_command(EngineCommand::Common(EngineCommandCommon::RemoveEntity {
                    id,
                }));
                self.undo_stack.push(UndoAction::RestoreEntity { snapshot });
            }
            UndoAction::RestorePlayerUiHud { snapshot } => {
                if let Some(current) =
                    self.capture_player_ui_hud_undo_snapshot_with_key(&snapshot.key)
                {
                    self.undo_stack
                        .push(UndoAction::RestorePlayerUiHud { snapshot: current });
                }
                self.restore_player_ui_hud_undo_snapshot(snapshot);
            }
        }
        self.is_applying_undo = false;
    }

    // ── Resize ───────────────────────────────────────────────────────────────

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        let requested = new_size;
        let new_size =
            rer_engine_shared::wgpu_surface::clamp_surface_physical_size(&self.device, new_size);
        if new_size.width != requested.width || new_size.height != requested.height {
            log::warn!(
                "[render-2d] tamaño de viewport {}x{} recortado a {}x{} (límite GPU)",
                requested.width,
                requested.height,
                new_size.width,
                new_size.height
            );
        }
        self.size = new_size;
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        self.surface.configure(&self.device, &self.config);
        self.depth_view = create_depth_texture(&self.device, &self.config);
        self.scene_target.resize(
            &self.device,
            self.config.format,
            new_size.width,
            new_size.height,
        );
        if self.player_ui_edit_active {
            self.rebuild_player_ui_screen_grid();
        }
    }

    /// Activa o desactiva V-Sync reconfigurendo el swapchain.
    pub fn set_vsync(&mut self, enabled: bool) {
        self.vsync_enabled = enabled;
        self.config.present_mode = if enabled {
            wgpu::PresentMode::AutoVsync
        } else {
            wgpu::PresentMode::AutoNoVsync
        };
        self.surface.configure(&self.device, &self.config);
        log::info!(
            "[vsync] V-Sync {}",
            if enabled { "activado" } else { "desactivado" }
        );
    }

    /// Reconstruye el vertex buffer de la cuadrícula con la configuración actual.
    pub(crate) fn rebuild_grid(&mut self) {
        self.grid_buffer = crate::config_2d::build_grid(&self.device, &self.grid_config);
    }

    /// Notifica al State qué eje del gizmo está siendo arrastrado (None = sin drag).
    pub fn set_active_gizmo_axis(&mut self, axis: Option<usize>) {
        self.active_gizmo_axis = axis;
    }

    /// Muestra/oculta el hint visual de snap a cuadrícula en el viewport 2D.
    pub fn set_snap_hint_visible(&mut self, visible: bool) {
        self.show_snap_hint = visible;
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

    pub(crate) fn selection_max_extent_2d(&self) -> Option<f32> {
        let ids: Vec<u32> = if !self.selected_entities.is_empty() {
            self.selected_entities.clone()
        } else {
            self.selected_entity.into_iter().collect()
        };
        if ids.is_empty() {
            return None;
        }
        let mut max_extent = 0.0f32;
        for id in ids {
            let t = self.world.get::<Transform>(id)?;
            let extent = t.scale.x.abs().max(t.scale.y.abs());
            max_extent = max_extent.max(extent);
        }
        Some(max_extent.max(1e-4))
    }

    pub(crate) fn transform_gizmo_world_scale_2d(&self) -> Option<f32> {
        let cam = self.camera_2d.as_ref()?;
        let _ = self.selection_center()?;
        let screen = rer_engine_shared::gizmo::world_scale_ortho_2d(cam.half_h, self.size.height);
        Some(rer_engine_shared::gizmo::clamp_scale_for_selection(
            screen,
            self.selection_max_extent_2d(),
        ))
    }
}
