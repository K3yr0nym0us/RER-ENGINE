// ── Lógica exclusiva del modo 2D (plataformer, vista lateral) ────────────────
//
// Contiene:
//  · camera_2d            — Camera2D (ortográfica) con pan()
//  · grid_2d              — GridConfig, GridBuffer, build_grid
//  · setup_2d_platformer  — inicialización de la escena 2D
//  · load_scenario        — carga un PNG como fondo de escenario
//  · project_to_screen_2d — proyecta un punto 3D a píxeles (cámara ortográfica)
//  · pick_entity_2d       — picking por AABB en el plano XY
//  · pick_gizmo_axis_2d   — eje del gizmo más cercano al cursor
//  · drag_gizmo_2d        — arrastre de entidad sobre eje X o Y
//  · update_hover_2d      — hover AABB + detección de eje de gizmo

pub(crate) mod camera_2d;
pub(crate) use camera_2d::Camera2D;

pub(crate) mod grid_2d;
pub(crate) use grid_2d::{GridBuffer, GridConfig, build_grid};

pub(crate) mod physics_2d;
pub(crate) use physics_2d::PhysicsWorld2D;

use std::fs;

use glam::Vec3 as GlamVec3;
use crate::ecs::{EntityId, MeshComponent, Transform};
use crate::engine::State;
use crate::config_shared::point_to_segment_2d;
use crate::ipc::{send_event, EngineEvent};
use crate::mesh::{upload, Mesh, Vertex};
use crate::gizmo::{self, GizmoVertex};
use crate::texture::GpuTexture;

// ── Componente exclusivo del modo 2D ─────────────────────────────────────────

/// Marca una entidad como escenario PNG en una escena 2D.
#[derive(Debug, Clone)]
pub(crate) struct ScenarioMarker {
    pub img_width:    u32,
    pub img_height:   u32,
    /// Altura base en unidades de mundo (user_scale = 1.0).
    pub base_world_h: f32,
    /// Ruta del PNG original, necesaria para duplicar la entidad.
    pub path:         String,
}

/// Marca una entidad como personaje PNG en una escena 2D.
#[derive(Debug, Clone)]
pub(crate) struct CharacterMarker {
    pub img_width:    u32,
    pub img_height:   u32,
    /// Altura base en unidades de mundo (user_scale = 1.0).
    pub base_world_h: f32,
    /// Ruta del PNG original, necesaria para duplicar la entidad.
    pub path:         String,
}

// ── Herramientas de dibujo ─────────────────────────────────────────────────

/// Estado de la herramienta activa de dibujo (solo en modo 2D).
#[derive(Debug)]
pub(crate) enum ActiveTool {
    None,
    DrawCollider { points_world: Vec<[f32; 2]> },
}

impl Default for ActiveTool {
    fn default() -> Self { ActiveTool::None }
}

impl ActiveTool {
    pub(crate) fn is_active(&self) -> bool { !matches!(self, ActiveTool::None) }
}

/// Marca una entidad ECS como colisionador creado con la herramienta de dibujo.
#[derive(Debug, Clone)]
pub(crate) struct ColliderMarker {
    pub points_world: [[f32; 2]; 4],
}
impl State {
    // ── Inicialización ────────────────────────────────────────────────────────

    /// Configura la escena 2D de plataformas con un único rectángulo (player).
    pub(crate) fn setup_2d_platformer(&mut self) {
        // Limpiar escena previa y escenarios de fondo
        self.scenario_entities.clear();
        self.character_entities.clear();
        self.collider_entities.clear();
        self.background_entity = None;
        self.active_tool = ActiveTool::None;
        self.tool_overlay_buffer = gizmo::build_from_vertices(&self.device, &[]);
        self.physics_2d.clear();
        self.world.clear();
        self.meshes.clear();
        self.textures.clear();
        self.anim_texture_cache.clear();
        self.anim_overrides.clear();
        self.entity_buffers.clear();
        self.entity_bind_groups.clear();
        self.selected_entity = None;
        self.hovered_entity  = None;

        // Quad unitario en el origen — el Transform lo escala/posiciona.
        // Imprescindible para que picking AABB, gizmo y hover funcionen.

        // -- Personaje: quad 1.0 × 1.5, centrado en (0, 0) -------------------
        let player_mesh = create_quad_xy(&self.device, 0.0, 0.0, 1.0, 1.0, "player-unit");
        self.meshes.push(player_mesh);
        let player_tex = GpuTexture::solid_color(&self.device, &self.queue, 232, 220, 200);
        self.textures.push(player_tex.create_bind_group(&self.device, &self.texture_bgl));
        let (b, bg) = self.alloc_entity_uniform();
        self.entity_buffers.push(b);
        self.entity_bind_groups.push(bg);
        let player_id = self.world.spawn(Some("Player"));
        self.world.insert(player_id, MeshComponent { mesh_idx: 0 });
        self.world.insert(player_id, crate::ecs::Transform {
            position: GlamVec3::new(0.0, 0.0, 0.0),
            scale:    GlamVec3::new(1.0, 1.5, 1.0),
            ..Default::default()
        });
        self.world.insert(player_id, CharacterMarker {
            img_width:    0,
            img_height:   0,
            base_world_h: 1.5,
            path:         "[Player]".to_owned(),
        });
        self.character_entities.push(player_id);

        // -- Cámara ortográfica -----------------------------------------------
        self.camera_2d = Some(Camera2D {
            x:      0.0,
            y:      0.0,
            half_h: 3.5,
            near:  -100.0,
            far:    100.0,
        });

        // Fondo oscuro azulado (estilo Hollow Knight)
        self.clear_color = wgpu::Color { r: 0.04, g: 0.04, b: 0.10, a: 1.0 };

        // Notificar al editor el ID y transform inicial del jugador
        send_event(&EngineEvent::PlayerReady {
            id:       player_id,
            position: [0.0, 0.0, 0.0],
            scale:    [1.0, 1.5, 1.0],
        });
        send_event(&EngineEvent::CharacterLoaded { id: player_id, path: "[Player]".to_owned() });

        log::info!("Escena 2D cargada: plataformer vista lateral");
    }

    // ── Escenario PNG de fondo ────────────────────────────────────────────────

    /// Carga una imagen PNG del disco y la registra como entidad ECS de escenario.
    /// La entidad se posiciona en Z=-1 (detrás de todo), mantiene las proporciones
    /// de la imagen y puede seleccionarse, arrastrarse y escalarse como cualquier entidad.
    pub(crate) fn load_scenario(&mut self, path: &str) {
        let bytes = match fs::read(path) {
            Ok(b)  => b,
            Err(e) => {
                log::error!("[load_scenario] error leyendo {path}: {e}");
                send_event(&EngineEvent::Error { message: format!("No se pudo leer el escenario: {e}") });
                return;
            }
        };

        use image::ImageReader;
        use std::io::Cursor;
        let img = match ImageReader::new(Cursor::new(&bytes))
            .with_guessed_format()
            .map_err(|e| e.to_string())
            .and_then(|r| r.decode().map_err(|e| e.to_string()))
        {
            Ok(i)  => i.to_rgba8(),
            Err(e) => {
                log::error!("[load_scenario] error decodificando PNG {path}: {e}");
                send_event(&EngineEvent::Error { message: format!("Error al decodificar PNG: {e}") });
                return;
            }
        };

        let (img_width, img_height) = img.dimensions();
        let aspect       = img_width as f32 / img_height.max(1) as f32;
        // Altura base fija en unidades de mundo, independiente del zoom actual.
        // Usar cam.half_h provocaría que el mismo PNG cargue a tamaños distintos
        // si el usuario ha hecho zoom entre cargas.
        // 7.0 = 2.0 × half_h inicial (3.5), y es la referencia para scale=1.0.
        let base_world_h = 7.0_f32;
        let base_world_w = base_world_h * aspect;

        let gpu_tex  = GpuTexture::from_rgba(&self.device, &self.queue, &img, img_width, img_height, "scenario");
        let tex_bg   = gpu_tex.create_bind_group(&self.device, &self.texture_bgl);
        let mesh     = create_quad_xy(&self.device, 0.0, 0.0, 1.0, 1.0, "scenario-quad");
        let mesh_idx = self.meshes.len();
        self.meshes.push(mesh);
        self.textures.push(tex_bg);
        let (buf, bg) = self.alloc_entity_uniform();
        self.entity_buffers.push(buf);
        self.entity_bind_groups.push(bg);

        let sc_id = self.world.spawn(Some("Escenario"));
        self.world.insert(sc_id, MeshComponent { mesh_idx });
        self.world.insert(sc_id, Transform {
            position: GlamVec3::new(0.0, 0.0, -1.0),
            scale:    GlamVec3::new(base_world_w, base_world_h, 1.0),
            ..Default::default()
        });
        self.world.insert(sc_id, ScenarioMarker { img_width, img_height, base_world_h, path: path.to_owned() });
        self.scenario_entities.push(sc_id);

        send_event(&EngineEvent::ScenarioLoaded { id: sc_id, path: path.to_owned() });
        log::info!("[load_scenario] entidad {sc_id} creada {img_width}×{img_height}: {path}");
    }

    /// Duplica un escenario existente: crea una nueva entidad con el mismo PNG
    /// ligeramente desplazada (offset +1 en X e Y) para que sea visible.
    pub(crate) fn duplicate_scenario(&mut self, id: u32) {
        let path = match self.world.get::<ScenarioMarker>(id) {
            Some(m) => m.path.clone(),
            None => {
                log::warn!("[duplicate_scenario] entidad {id} no tiene ScenarioMarker");
                return;
            }
        };
        // Offset para que el duplicado sea visible sobre el original
        let offset = {
            let count = self.scenario_entities.len() as f32;
            GlamVec3::new(count * 0.5, count * 0.5, 0.0)
        };
        self.load_scenario(&path);
        // Aplicar offset a la entidad recién creada
        if let Some(&new_id) = self.scenario_entities.last() {
            if let Some(t) = self.world.get_mut::<Transform>(new_id) {
                t.position += offset;
            }
        }
    }

    // ── Fondo del mundo ───────────────────────────────────────────────────────

    /// Carga una imagen PNG o GIF como fondo del mundo 2D.
    /// Se escala automáticamente al tamaño del mundo (worldWidth × worldHeight)
    /// y se posiciona en Z=-10 (detrás de escenarios y personajes).
    /// Si ya existía un fondo previo, lo elimina antes de crear el nuevo.
    pub(crate) fn load_background(&mut self, path: &str) {
        // Eliminar fondo previo si existe
        if let Some(old_id) = self.background_entity.take() {
            self.world.despawn(old_id);
        }

        let bytes = match std::fs::read(path) {
            Ok(b)  => b,
            Err(e) => {
                log::error!("[load_background] error leyendo {path}: {e}");
                send_event(&EngineEvent::Error { message: format!("No se pudo leer el fondo: {e}") });
                return;
            }
        };

        use image::ImageReader;
        use std::io::Cursor;
        let img = match ImageReader::new(Cursor::new(&bytes))
            .with_guessed_format()
            .map_err(|e| e.to_string())
            .and_then(|r| r.decode().map_err(|e| e.to_string()))
        {
            Ok(i)  => i.to_rgba8(),
            Err(e) => {
                log::error!("[load_background] error decodificando imagen {path}: {e}");
                send_event(&EngineEvent::Error { message: format!("Error al decodificar imagen: {e}") });
                return;
            }
        };

        let (img_w, img_h) = img.dimensions();
        let world_w = self.grid_config.world_width;
        let world_h = self.grid_config.world_height;

        let gpu_tex  = GpuTexture::from_rgba(&self.device, &self.queue, &img, img_w, img_h, "background");
        let tex_bg   = gpu_tex.create_bind_group(&self.device, &self.texture_bgl);
        let mesh     = create_quad_xy(&self.device, 0.0, 0.0, 1.0, 1.0, "background-quad");
        let mesh_idx = self.meshes.len();
        self.meshes.push(mesh);
        self.textures.push(tex_bg);
        let (buf, bg) = self.alloc_entity_uniform();
        self.entity_buffers.push(buf);
        self.entity_bind_groups.push(bg);

        let bg_id = self.world.spawn(Some("Background"));
        self.world.insert(bg_id, MeshComponent { mesh_idx });
        self.world.insert(bg_id, Transform {
            position: GlamVec3::new(0.0, 0.0, -10.0),
            scale:    GlamVec3::new(world_w, world_h, 1.0),
            ..Default::default()
        });
        // No seleccionable para que no interfiera con el picking
        self.world.insert(bg_id, crate::ecs::NonSelectable);
        self.background_entity = Some(bg_id);

        send_event(&EngineEvent::BackgroundLoaded { path: path.to_owned() });
        log::info!("[load_background] fondo cargado {img_w}×{img_h} escala {world_w}×{world_h}: {path}");
    }

    // ── Personaje PNG ─────────────────────────────────────────────────────────

    /// Carga una imagen PNG del disco y la registra como entidad ECS de personaje.
    /// Se posiciona en Z=0 (mismo plano que el jugador) y puede seleccionarse,
    /// arrastrarse y escalarse como cualquier entidad.
    pub(crate) fn load_character(&mut self, path: &str) {
        let bytes = match std::fs::read(path) {
            Ok(b)  => b,
            Err(e) => {
                log::error!("[load_character] error leyendo {path}: {e}");
                send_event(&EngineEvent::Error { message: format!("No se pudo leer el personaje: {e}") });
                return;
            }
        };

        use image::ImageReader;
        use std::io::Cursor;
        let img = match ImageReader::new(Cursor::new(&bytes))
            .with_guessed_format()
            .map_err(|e| e.to_string())
            .and_then(|r| r.decode().map_err(|e| e.to_string()))
        {
            Ok(i)  => i.to_rgba8(),
            Err(e) => {
                log::error!("[load_character] error decodificando PNG {path}: {e}");
                send_event(&EngineEvent::Error { message: format!("Error al decodificar PNG: {e}") });
                return;
            }
        };

        let (img_width, img_height) = img.dimensions();
        let aspect       = img_width as f32 / img_height.max(1) as f32;
        let base_world_h = 2.0_f32; // altura base razonable para un personaje
        let base_world_w = base_world_h * aspect;

        let gpu_tex  = GpuTexture::from_rgba(&self.device, &self.queue, &img, img_width, img_height, "character");
        let tex_bg   = gpu_tex.create_bind_group(&self.device, &self.texture_bgl);
        let mesh     = create_quad_xy(&self.device, 0.0, 0.0, 1.0, 1.0, "character-quad");
        let mesh_idx = self.meshes.len();
        self.meshes.push(mesh);
        self.textures.push(tex_bg);
        let (buf, bg) = self.alloc_entity_uniform();
        self.entity_buffers.push(buf);
        self.entity_bind_groups.push(bg);

        let ch_id = self.world.spawn(Some("Personaje"));
        self.world.insert(ch_id, MeshComponent { mesh_idx });
        self.world.insert(ch_id, Transform {
            position: GlamVec3::new(0.0, 0.0, 0.0),
            scale:    GlamVec3::new(base_world_w, base_world_h, 1.0),
            ..Default::default()
        });
        self.world.insert(ch_id, CharacterMarker { img_width, img_height, base_world_h, path: path.to_owned() });
        self.character_entities.push(ch_id);

        send_event(&EngineEvent::CharacterLoaded { id: ch_id, path: path.to_owned() });
        log::info!("[load_character] entidad {ch_id} creada {img_width}×{img_height}: {path}");
    }

    /// Ajusta la escala de un personaje 2D preservando proporciones.
    pub(crate) fn set_character_scale(&mut self, id: u32, scale: f32) {
        let marker = self.world.get::<CharacterMarker>(id).cloned();
        if let Some(m) = marker {
            let aspect = m.img_width as f32 / m.img_height.max(1) as f32;
            let new_h  = m.base_world_h * scale.clamp(0.05, 20.0);
            let new_w  = new_h * aspect;
            if let Some(t) = self.world.get_mut::<Transform>(id) {
                t.scale = GlamVec3::new(new_w, new_h, 1.0);
            }
        }
    }

    /// Duplica un personaje existente: crea una nueva entidad con el mismo PNG
    /// ligeramente desplazada para que sea visible.
    /// Si el personaje es el jugador por defecto ([Player]), crea un nuevo quad blanco.
    pub(crate) fn duplicate_character(&mut self, id: u32) {
        let path = match self.world.get::<CharacterMarker>(id) {
            Some(m) => m.path.clone(),
            None => {
                log::warn!("[duplicate_character] entidad {id} no tiene CharacterMarker");
                return;
            }
        };
        let offset = {
            let count = self.character_entities.len() as f32;
            GlamVec3::new(count * 0.5, count * 0.5, 0.0)
        };
        if path == "[Player]" {
            // Crear un nuevo quad blanco (igual al jugador por defecto)
            let mesh     = create_quad_xy(&self.device, 0.0, 0.0, 1.0, 1.0, "player-unit");
            let mesh_idx = self.meshes.len();
            self.meshes.push(mesh);
            let tex = GpuTexture::solid_color(&self.device, &self.queue, 232, 220, 200);
            self.textures.push(tex.create_bind_group(&self.device, &self.texture_bgl));
            let (buf, bg) = self.alloc_entity_uniform();
            self.entity_buffers.push(buf);
            self.entity_bind_groups.push(bg);
            let new_id = self.world.spawn(Some("Player"));
            self.world.insert(new_id, MeshComponent { mesh_idx });
            self.world.insert(new_id, Transform {
                position: GlamVec3::new(offset.x, offset.y, 0.0),
                scale:    GlamVec3::new(1.0, 1.5, 1.0),
                ..Default::default()
            });
            self.world.insert(new_id, CharacterMarker {
                img_width: 0, img_height: 0,
                base_world_h: 1.5,
                path: "[Player]".to_owned(),
            });
            self.character_entities.push(new_id);
            send_event(&EngineEvent::CharacterLoaded { id: new_id, path: "[Player]".to_owned() });
            log::info!("[duplicate_character] nuevo quad jugador creado: entidad {new_id}");
        } else {
            self.load_character(&path);
            if let Some(&new_id) = self.character_entities.last() {
                if let Some(t) = self.world.get_mut::<Transform>(new_id) {
                    t.position += offset;
                }
            }
        }
    }

    /// Cambia el sprite de una entidad (escenario o personaje) a un frame de animación.
    /// - `pivot_x/pivot_y`: punto ancla en píxeles dentro del frame (0,0 = esquina superior-izq).
    /// - `logical_w/logical_h`: bounding box lógico fijo de la animación (en píxeles).
    ///
    /// La entidad mantiene su posición de ancla en el mundo.  El quad se redimensiona y
    /// desplaza para que el píxel (pivot_x, pivot_y) quede exactamente sobre dicha posición.
    pub(crate) fn play_animation_frame(
        &mut self,
        id: u32,
        path: &str,
        pivot_x: f32,
        pivot_y: f32,
        logical_w: u32,
        logical_h: u32,
    ) {
        // Verificar que la entidad existe y obtener su tipo
        let is_scenario  = self.scenario_entities.contains(&id);
        let is_character = self.character_entities.contains(&id);
        if !is_scenario && !is_character {
            log::warn!("[play_animation_frame] entidad {id} no es escenario ni personaje");
            return;
        }

        // Obtener (o crear) el bind group + dimensiones desde la caché.
        // Solo en el primer uso de cada ruta se hace disk I/O + decode + upload a GPU.
        // Las llamadas siguientes son un simple lookup de HashMap → sin trabajo de GPU.
        let (arc_bg, img_width, img_height) =
            if let Some((cached_bg, w, h)) = self.anim_texture_cache.get(path) {
                (std::sync::Arc::clone(cached_bg), *w, *h)
            } else {
                // Cache miss: cargar, decodificar y subir a GPU UNA sola vez
                let bytes = match fs::read(path) {
                    Ok(b)  => b,
                    Err(e) => {
                        log::error!("[play_animation_frame] error leyendo {path}: {e}");
                        send_event(&EngineEvent::Error { message: format!("No se pudo leer el frame: {e}") });
                        return;
                    }
                };
                use image::ImageReader;
                use std::io::Cursor;
                let img = match ImageReader::new(Cursor::new(&bytes))
                    .with_guessed_format()
                    .map_err(|e| e.to_string())
                    .and_then(|r| r.decode().map_err(|e| e.to_string()))
                {
                    Ok(i)  => i.to_rgba8(),
                    Err(e) => {
                        log::error!("[play_animation_frame] error decodificando PNG {path}: {e}");
                        send_event(&EngineEvent::Error { message: format!("Error al decodificar frame: {e}") });
                        return;
                    }
                };
                let (w, h) = img.dimensions();
                let gpu_tex = GpuTexture::from_rgba(&self.device, &self.queue, &img, w, h, "anim-frame");
                let bg = std::sync::Arc::new(gpu_tex.create_bind_group(&self.device, &self.texture_bgl));
                self.anim_texture_cache.insert(path.to_string(), (std::sync::Arc::clone(&bg), w, h));
                log::info!("[play_animation_frame] frame cargado a GPU (caché miss): {path}");
                (bg, w, h)
            };

        // Obtener tex_position para el override
        let tex_position = match self.world.get::<MeshComponent>(id) {
            Some(m) => m.mesh_idx,
            None => {
                log::warn!("[play_animation_frame] entidad {id} sin MeshComponent");
                return;
            }
        };
        if tex_position >= self.textures.len() {
            log::warn!("[play_animation_frame] indice invalido: {tex_position}");
            return;
        }

        // Escribir el override — el render loop lo lee con prioridad sobre textures[]
        self.anim_overrides.insert(tex_position, arc_bg);

        // ── Aplicar pivot ────────────────────────────────────────────────────
        if logical_w > 0 && logical_h > 0 {
            if let Some(transform) = self.world.get::<Transform>(id).cloned() {
                let (orig_pos, orig_scale) = *self.anim_saved_transforms
                    .entry(id)
                    .or_insert((transform.position, transform.scale));

                let world_per_px = orig_scale.y / img_height as f32;
                let new_scale_x  = img_width  as f32 * world_per_px;
                let new_scale_y  = img_height as f32 * world_per_px;
                let offset_x     =  (pivot_x - img_width  as f32 * 0.5) * world_per_px;
                let offset_y     = -(pivot_y - img_height as f32 * 0.5) * world_per_px;

                if let Some(t) = self.world.get_mut::<Transform>(id) {
                    t.scale    = GlamVec3::new(new_scale_x, new_scale_y, 1.0);
                    t.position = orig_pos - GlamVec3::new(offset_x, offset_y, 0.0);
                }
            }
        }

        log::info!("[play_animation_frame] frame actualizado para entidad {id} (tex_idx={tex_position}, pivot=({pivot_x},{pivot_y}))");
    }

    /// Restaura el sprite original de una entidad después de una animación.
    pub(crate) fn restore_animation_frame(&mut self, id: u32) {
        let is_scenario  = self.scenario_entities.contains(&id);
        let is_character = self.character_entities.contains(&id);
        if !is_scenario && !is_character {
            log::warn!("[restore_animation_frame] entidad {id} no es escenario ni personaje");
            return;
        }

        // Obtener tex_position del MeshComponent
        let tex_position = match self.world.get::<MeshComponent>(id) {
            Some(m) => m.mesh_idx,
            None => {
                log::warn!("[restore_animation_frame] entidad {id} sin MeshComponent");
                return;
            }
        };

        // Eliminar el override: el render loop vuelve a usar textures[tex_position]
        // que nunca fue modificado. No hay que recargar nada de disco.
        self.anim_overrides.remove(&tex_position);

        // Restaurar el transform original si fue modificado por play_animation_frame
        if let Some((orig_pos, orig_scale)) = self.anim_saved_transforms.remove(&id) {
            if let Some(t) = self.world.get_mut::<Transform>(id) {
                t.position = orig_pos;
                t.scale    = orig_scale;
            }
        }

        log::info!("[restore_animation_frame] sprite restaurado para entidad {id}");
    }

    // ── Modo edición de pivot ─────────────────────────────────────────────────

    /// Activa el modo edición de pivot para una entidad:
    /// - Muestra el frame como textura temporal (sin modificar la escala).
    /// - Dibuja un borde cyan alrededor de la entidad en el overlay.
    /// - El siguiente click calculará el pivot y emitirá PivotSelected.
    pub(crate) fn enter_pivot_edit_mode(&mut self, id: u32, frame_path: &str, pivot_x: f32, pivot_y: f32) {
        let bytes = match fs::read(frame_path) {
            Ok(b)  => b,
            Err(e) => { log::error!("[enter_pivot_edit_mode] error leyendo {frame_path}: {e}"); return; }
        };
        use image::ImageReader;
        use std::io::Cursor;
        let img = match ImageReader::new(Cursor::new(&bytes))
            .with_guessed_format()
            .map_err(|e| e.to_string())
            .and_then(|r| r.decode().map_err(|e| e.to_string()))
        {
            Ok(i)  => i.to_rgba8(),
            Err(e) => { log::error!("[enter_pivot_edit_mode] error decodificando {frame_path}: {e}"); return; }
        };
        let (img_w, img_h) = img.dimensions();

        // 1. Guardar transform original (si no estaba ya guardado por una animación previa)
        //    y calcular la escala ajustada para que el frame no aparezca deformado en pantalla.
        let (new_pos, new_scale_x, new_scale_y) = {
            let t = match self.world.get::<Transform>(id) {
                Some(t) => t.clone(),
                None    => { log::error!("[enter_pivot_edit_mode] entidad {id} sin Transform"); return; }
            };
            let (_, orig_scale) = *self.anim_saved_transforms.entry(id).or_insert((t.position, t.scale));
            // Escala ajustada: altura = orig_scale.y, ancho proporcional al ratio píxel del frame.
            // Esto asegura que el frame se vea sin deformar al hacer click para asignar el pivot.
            let aspect   = img_w as f32 / img_h as f32;
            let scale_y  = orig_scale.y;
            let scale_x  = scale_y * aspect;
            (t.position, scale_x, scale_y)
        };

        // 2. Aplicar la escala corregida al transform de la entidad
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.scale = GlamVec3::new(new_scale_x, new_scale_y, 1.0);
        }

        // 3. Swap de textura con el frame a editar
        if let Some(m) = self.world.get::<MeshComponent>(id) {
            let tex_pos = m.mesh_idx;
            if tex_pos < self.textures.len() {
                let gpu_tex = GpuTexture::from_rgba(&self.device, &self.queue, &img, img_w, img_h, "pivot-edit");
                self.textures[tex_pos] = gpu_tex.create_bind_group(&self.device, &self.texture_bgl);
            }
        }

        // 4. Overlay combinado: borde cyan + cruceta amarilla en el pivot actual
        self.tool_overlay_buffer = build_pivot_edit_overlay_with_cross(
            &self.device,
            new_pos,
            GlamVec3::new(new_scale_x, new_scale_y, 1.0),
            pivot_x, pivot_y,
            img_w, img_h,
        );

        self.pivot_edit_mode = Some((id, frame_path.to_string(), img_w, img_h));
        log::info!("[enter_pivot_edit_mode] activo para entidad {id} ({img_w}×{img_h}) escala=({new_scale_x:.3},{new_scale_y:.3}): {frame_path}");
    }

    /// Cancela el modo edición de pivot y restaura el sprite original.
    pub(crate) fn cancel_pivot_edit_mode(&mut self) {
        if let Some((entity_id, _, _, _)) = self.pivot_edit_mode.take() {
            self.restore_animation_frame(entity_id);
            self.tool_overlay_buffer = gizmo::build_from_vertices(&self.device, &[]);
            log::info!("[cancel_pivot_edit_mode] modo cancelado para entidad {entity_id}");
        }
    }

    // ── Modo visualización del Área Lógica ────────────────────────────────────

    /// Muestra un borde naranja en el viewport indicando las dimensiones del área
    /// lógica (bounding box de referencia para la animación). El usuario puede
    /// actualizar w/h y re-enviar este comando para ver los cambios en tiempo real.
    pub(crate) fn enter_logical_area_mode(&mut self, id: u32, w: u32, h: u32) {
        let transform = match self.world.get::<Transform>(id) {
            Some(t) => t.clone(),
            None    => { log::warn!("[enter_logical_area_mode] entidad {id} sin Transform"); return; }
        };
        // Usar escala original si hay animación en curso, si no la actual
        let orig_scale_y = self.anim_saved_transforms
            .get(&id)
            .map(|(_, s)| s.y)
            .unwrap_or(transform.scale.y);

        self.tool_overlay_buffer = build_logical_area_overlay(
            &self.device, transform.position, orig_scale_y, w, h,
        );
        self.logical_area_mode = Some(id);
        log::info!("[enter_logical_area_mode] área {w}×{h} para entidad {id}");
    }

    /// Oculta el overlay de área lógica.
    pub(crate) fn cancel_logical_area_mode(&mut self) {
        self.logical_area_mode = None;
        self.tool_overlay_buffer = gizmo::build_from_vertices(&self.device, &[]);
        log::info!("[cancel_logical_area_mode] overlay ocultado");
    }

    /// Procesa un click del usuario cuando el modo edición de pivot está activo.
    /// Convierte las coordenadas de pantalla a coordenadas de píxel dentro del frame
    /// y emite el evento PivotSelected. Devuelve true si el click fue consumido.
    pub(crate) fn handle_pivot_click_2d(&mut self, pixel_x: f32, pixel_y: f32) -> bool {
        let (entity_id, frame_path, img_w, img_h) = match self.pivot_edit_mode.clone() {
            Some(m) => m,
            None    => return false,
        };
        let cam = match &self.camera_2d {
            Some(c) => Camera2D { x: c.x, y: c.y, half_h: c.half_h, near: c.near, far: c.far },
            None    => return false,
        };

        // Pantalla → mundo
        let w      = self.size.width  as f32;
        let h      = self.size.height as f32;
        let half_w = cam.half_h * (w / h);
        let wx     = cam.x + ((pixel_x / w) * 2.0 - 1.0) * half_w;
        let wy     = cam.y + (1.0 - (pixel_y / h) * 2.0) * cam.half_h;

        // Mundo → [0,1] dentro del quad de la entidad
        let transform = match self.world.get::<Transform>(entity_id) {
            Some(t) => t.clone(),
            None    => return false,
        };
        let nx       = ((wx - transform.position.x) / transform.scale.x + 0.5).clamp(0.0, 1.0);
        let ny_world = ((wy - transform.position.y) / transform.scale.y + 0.5).clamp(0.0, 1.0);
        let ny       = 1.0 - ny_world; // imagen: Y = arriba→abajo

        let pivot_x = nx * img_w as f32;
        let pivot_y = ny * img_h as f32;

        send_event(&EngineEvent::PivotSelected { frame_path: frame_path.clone(), pivot_x, pivot_y });

        // Restaurar sprite original y limpiar modo
        self.pivot_edit_mode = None;
        self.restore_animation_frame(entity_id);
        self.tool_overlay_buffer = gizmo::build_from_vertices(&self.device, &[]);

        log::info!("[handle_pivot_click_2d] pivot ({pivot_x:.1}, {pivot_y:.1}) para {frame_path}");
        true
    }

    // ── Proyeccion 2D a pantalla ──────────────────────────────────────────────

    /// Proyecta un punto de mundo XY a coordenadas de pantalla en píxeles.
    pub(crate) fn project_to_screen_2d(&self, cam: &Camera2D, p: GlamVec3) -> Option<(f32, f32)> {
        let w  = self.size.width  as f32;
        let h  = self.size.height as f32;
        let vp = cam.view_proj(w / h);
        let c  = vp * glam::Vec4::new(p.x, p.y, p.z, 1.0);
        if c.w.abs() < 1e-6 { return None; }
        Some(((c.x / c.w + 1.0) * 0.5 * w, (1.0 - c.y / c.w) * 0.5 * h))
    }

    // ── Picking 2D ────────────────────────────────────────────────────────────

    /// Selecciona la entidad bajo el cursor usando AABB en el plano XY.
    /// Cuando varios AABBs se solapan (p.ej. escenario + player) se elige
    /// la entidad con mayor Z (más cercana a la cámara).
    pub fn pick_entity_2d(&mut self, pixel_x: f32, pixel_y: f32) {
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

        // Recoge todos los hits y elige el de mayor Z (más cercano a la cámara).
        let mut best: Option<(EntityId, f32)> = None;
        for &entity in self.world.entities() {
            if self.world.get::<crate::ecs::NonSelectable>(entity).is_some() { continue; }
            if let Some(transform) = self.world.get::<Transform>(entity) {
                let p  = transform.position;
                let sx = transform.scale.x * 0.5;
                let sy = transform.scale.y * 0.5;
                if wx >= p.x - sx && wx <= p.x + sx && wy >= p.y - sy && wy <= p.y + sy {
                    if best.map_or(true, |(_, bz)| p.z > bz) {
                        best = Some((entity, p.z));
                    }
                }
            }
        }
        let hit = best.map(|(id, _)| id);
        match hit {
            Some(entity) => {
                if self.selected_entity == Some(entity) { return; }
                self.selected_entity = Some(entity);
                let name      = self.world.name(entity).unwrap_or("Entity").to_string();
                let transform = self.world.get::<Transform>(entity).cloned().unwrap_or_default();
                let pos = transform.position.to_array();
                let rot = [transform.rotation.x, transform.rotation.y,
                           transform.rotation.z, transform.rotation.w];
                let scl             = transform.scale.to_array();
                let physics_enabled = self.physics_2d.has_physics(entity);
                let physics_type    = self.physics_2d.get_body_type(entity).to_string();
                send_event(&EngineEvent::EntitySelected {
                    id: entity, name, position: pos, rotation: rot, scale: scl,
                    physics_enabled,
                    physics_type,
                });
            }
            None => {
                if self.selected_entity.is_some() {
                    self.selected_entity = None;
                    send_event(&EngineEvent::EntityDeselected);
                }
            }
        }
    }

    // ── Picking de eje del gizmo 2D ───────────────────────────────────────────

    /// Devuelve el índice del eje del gizmo 2D más cercano al cursor (0=X, 1=Y).
    pub fn pick_gizmo_axis_2d(&self, pixel_x: f32, pixel_y: f32) -> Option<usize> {
        let sel_id = self.selected_entity?;
        let origin = self.world.get::<Transform>(sel_id)?.position;
        let cam    = self.camera_2d.as_ref()?;
        let so     = self.project_to_screen_2d(cam, origin)?;

        const LEN:    f32 = 1.2;
        const THRESH: f32 = 16.0;
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

    // ── Drag de gizmo 2D ──────────────────────────────────────────────────────

    /// Arrastra la entidad seleccionada sobre el eje X (0) o Y (1) en modo 2D.
    pub fn drag_gizmo_2d(&mut self, pixel_x: f32, pixel_y: f32, last_x: f32, last_y: f32, axis_idx: usize, snap: bool) {
        let sel_id = match self.selected_entity { Some(id) => id, None => return };
        let cam = match &self.camera_2d {
            Some(c) => Camera2D { x: c.x, y: c.y, half_h: c.half_h, near: c.near, far: c.far },
            None    => return,
        };
        let origin = match self.world.get::<Transform>(sel_id) {
            Some(t) => t.position,
            None    => return,
        };
        let axis_world = if axis_idx == 0 { GlamVec3::X } else { GlamVec3::Y };
        let so = match self.project_to_screen_2d(&cam, origin)               { Some(p) => p, None => return };
        let se = match self.project_to_screen_2d(&cam, origin + axis_world)  { Some(p) => p, None => return };
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
            // Snap a cuadrícula: alinea el borde más cercano a la línea de
            // cuadrícula más próxima. Se activa si snap=true (Ctrl desde
            // cualquier fuente: winit o IPC).
            let cell = self.grid_config.cell_size;
            if snap && cell > 1e-6 {
                if axis_idx == 0 {
                    let hw = t.scale.x * 0.5;
                    let left  = t.position.x - hw;
                    let right = t.position.x + hw;
                    let left_snap  = (left  / cell).round() * cell;
                    let right_snap = (right / cell).round() * cell;
                    if (left - left_snap).abs() <= (right - right_snap).abs() {
                        t.position.x = left_snap + hw;
                    } else {
                        t.position.x = right_snap - hw;
                    }
                } else {
                    let hh = t.scale.y * 0.5;
                    let bottom = t.position.y - hh;
                    let top    = t.position.y + hh;
                    let bottom_snap = (bottom / cell).round() * cell;
                    let top_snap    = (top    / cell).round() * cell;
                    if (bottom - bottom_snap).abs() <= (top - top_snap).abs() {
                        t.position.y = bottom_snap + hh;
                    } else {
                        t.position.y = top_snap - hh;
                    }
                }
            }
            let pos = t.position.to_array();
            let rot = [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w];
            let scl             = t.scale.to_array();
            let physics_enabled = self.physics_2d.has_physics(sel_id);
            let physics_type    = self.physics_2d.get_body_type(sel_id).to_string();
            send_event(&EngineEvent::EntitySelected {
                id: sel_id, name, position: pos, rotation: rot, scale: scl,
                physics_enabled,
                physics_type,
            });
        }
    }

    // ── Hover 2D ─────────────────────────────────────────────────────────────

    /// Actualiza `hovered_entity` y `hovered_gizmo_axis` en modo 2D.
    pub fn update_hover_2d(&mut self, pixel_x: f32, pixel_y: f32) {
        let prev_hover = self.hovered_entity;
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
        let mut best_hover: Option<(EntityId, f32)> = None;
        for &entity in self.world.entities() {
            if self.world.get::<crate::ecs::NonSelectable>(entity).is_some() { continue; }
            if let Some(t) = self.world.get::<Transform>(entity) {
                let sx = t.scale.x * 0.5;
                let sy = t.scale.y * 0.5;
                if wx >= t.position.x - sx && wx <= t.position.x + sx
                && wy >= t.position.y - sy && wy <= t.position.y + sy {
                    if best_hover.map_or(true, |(_, bz)| t.position.z > bz) {
                        best_hover = Some((entity, t.position.z));
                    }
                }
            }
        }
        self.hovered_entity    = best_hover.map(|(id, _)| id);
        self.hovered_gizmo_axis = self.pick_gizmo_axis_2d(pixel_x, pixel_y);
        // Emitir evento solo si el hover cambió para no saturar el IPC
        match (prev_hover, self.hovered_entity) {
            (None, Some(id))              => send_event(&EngineEvent::EntityHovered { id }),
            (Some(_), None)               => send_event(&EngineEvent::EntityUnhovered),
            (Some(a), Some(b)) if a != b  => send_event(&EngineEvent::EntityHovered { id: b }),
            _                             => {}
        }
    }

    // ── Herramienta de dibujo: cuadro de colisiones ───────────────────────────

    /// Intenta procesar un click del cursor como evento de la herramienta activa.
    /// Devuelve `true` si la herramienta consumió el click (no debe disparar picking).
    pub(crate) fn handle_tool_click_2d(&mut self, pixel_x: f32, pixel_y: f32) -> bool {
        let cam = match &self.camera_2d {
            Some(c) => Camera2D { x: c.x, y: c.y, half_h: c.half_h, near: c.near, far: c.far },
            None    => return false,
        };
        if !self.active_tool.is_active() { return false; }

        let w      = self.size.width  as f32;
        let h      = self.size.height as f32;
        let aspect = w / h;
        let half_w = cam.half_h * aspect;
        let wx = cam.x + ((pixel_x / w) * 2.0 - 1.0) * half_w;
        let wy = cam.y + (1.0 - (pixel_y / h) * 2.0) * cam.half_h;

        match &mut self.active_tool {
            ActiveTool::DrawCollider { points_world } => {
                points_world.push([wx, wy]);
                let count = points_world.len() as u32;

                if count >= 4 {
                    let pts: [[f32; 2]; 4] = [
                        points_world[0], points_world[1],
                        points_world[2], points_world[3],
                    ];
                    self.active_tool = ActiveTool::None;
                    self.tool_overlay_buffer = gizmo::build_from_vertices(&self.device, &[]);
                    self.create_collision_box_from_points(&pts);
                } else {
                    let pts_clone: Vec<[f32; 2]> = points_world.clone();
                    self.tool_overlay_buffer = build_tool_overlay(&self.device, &pts_clone);
                    send_event(&EngineEvent::DrawingProgress { count });
                }
                true
            }
            ActiveTool::None => false,
        }
    }

    /// Crea una entidad ECS de colisionador a partir de 4 puntos en espacio de mundo.
    pub(crate) fn create_collision_box_from_points(&mut self, pts: &[[f32; 2]; 4]) {
        let (mesh, pos, scale) = create_mesh_from_4_points(pts, &self.device);
        let mesh_idx = self.meshes.len();
        self.meshes.push(mesh);

        // Textura semitransparente cyan para indicar área de colisión
        let tex = GpuTexture::from_rgba(&self.device, &self.queue, &[60, 220, 200, 110], 1, 1, "collider");
        self.textures.push(tex.create_bind_group(&self.device, &self.texture_bgl));
        let (buf, bg) = self.alloc_entity_uniform();
        self.entity_buffers.push(buf);
        self.entity_bind_groups.push(bg);

        let entity = self.world.spawn(Some("Colisionador"));
        self.world.insert(entity, MeshComponent { mesh_idx });
        self.world.insert(entity, Transform {
            position: GlamVec3::from(pos),
            scale:    GlamVec3::from(scale),
            ..Default::default()
        });
        self.world.insert(entity, ColliderMarker { points_world: *pts });
        // Usamos cuboid estático (AABB del bounding box) en lugar de hull convexo 3D,
        // ya que rapier3d puede rechazar hulls de puntos coplanares (z=0).
        // El result es idéntico en precisión al toggle manual que confirma el usuario.
        self.physics_2d.set_entity_physics(
            entity, true, "static",
            pos,
            [scale[0] * 0.5, scale[1] * 0.5, 0.01],
        );
        self.collider_entities.push(entity);

        send_event(&EngineEvent::ColliderCreated { id: entity, points: *pts });
        log::info!("[tool] colisionador creado: entidad {entity} en {:?}", pts);
    }
}

// ── Primitivas de malla para el modo 2D ───────────────────────────────────────

/// Quad en el plano XY (normal +Z).
/// `cx`, `cy` = centro en mundo  |  `w`, `h` = ancho y alto  |  UVs: 0..1
fn create_quad_xy(device: &wgpu::Device, cx: f32, cy: f32, w: f32, h: f32, label: &str) -> Mesh {
    let hw = w / 2.0;
    let hh = h / 2.0;
    let vertices = vec![
        Vertex { position: [cx - hw, cy - hh, 0.0], normal: [0.0, 0.0, 1.0], uv: [0.0, 1.0] },
        Vertex { position: [cx + hw, cy - hh, 0.0], normal: [0.0, 0.0, 1.0], uv: [1.0, 1.0] },
        Vertex { position: [cx + hw, cy + hh, 0.0], normal: [0.0, 0.0, 1.0], uv: [1.0, 0.0] },
        Vertex { position: [cx - hw, cy + hh, 0.0], normal: [0.0, 0.0, 1.0], uv: [0.0, 0.0] },
    ];
    let indices = vec![0u32, 1, 2, 2, 3, 0];
    upload(device, &vertices, &indices, label)
}

/// Crea un mesh a partir de 4 puntos arbitrarios en espacio de mundo (plano XY).
/// Los vértices se normalizan respecto al bounding box del centroide para que
/// `Transform.position = centroide` y `Transform.scale = (bbox_w, bbox_h, 1)`
/// sean coherentes con el renderizador y el picking por AABB.
/// Devuelve (Mesh, posición[3], escala[3]).
fn create_mesh_from_4_points(pts: &[[f32; 2]; 4], device: &wgpu::Device) -> (Mesh, [f32; 3], [f32; 3]) {
    let cx = pts.iter().map(|p| p[0]).sum::<f32>() / 4.0;
    let cy = pts.iter().map(|p| p[1]).sum::<f32>() / 4.0;
    let min_x = pts.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min);
    let max_x = pts.iter().map(|p| p[0]).fold(f32::NEG_INFINITY, f32::max);
    let min_y = pts.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
    let max_y = pts.iter().map(|p| p[1]).fold(f32::NEG_INFINITY, f32::max);
    let bw = (max_x - min_x).max(0.01);
    let bh = (max_y - min_y).max(0.01);

    // Normalize to [-0.5, 0.5] space so the model matrix (scale = bbox) remaps correctly.
    let uvs = [[0.0_f32, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
    let vertices: Vec<Vertex> = pts.iter().enumerate().map(|(i, p)| Vertex {
        position: [(p[0] - cx) / bw, (p[1] - cy) / bh, 0.0],
        normal:   [0.0, 0.0, 1.0],
        uv:       uvs[i],
    }).collect();
    let indices = vec![0u32, 1, 2, 2, 3, 0];

    // Z = -0.5: entre los escenarios (Z=-1) y los personajes (Z=0).
    let position = [cx, cy, -0.5];
    let scale    = [bw, bh, 1.0];
    (upload(device, &vertices, &indices, "collider-quad"), position, scale)
}

/// Construye el GizmoBuffer (LineList) de overlay para la herramienta de dibujo.
/// `pts`: puntos acumulados (1-4) en espacio de mundo.
/// Dibuja una cruz en cada punto y líneas de conexión consecutivas.
fn build_tool_overlay(device: &wgpu::Device, pts: &[[f32; 2]]) -> gizmo::GizmoBuffer {
    const ARM:         f32 = 0.15;
    const Z:           f32 = 0.1;
    let cross_color        = [1.0_f32, 1.0,  1.0,  1.0]; // blanco
    let line_color         = [1.0_f32, 0.75, 0.0,  1.0]; // naranja

    let mut verts: Vec<GizmoVertex> = Vec::new();

    // Cruz en cada punto acumulado
    for p in pts {
        let [x, y] = *p;
        verts.push(GizmoVertex { position: [x - ARM, y,       Z], color: cross_color });
        verts.push(GizmoVertex { position: [x + ARM, y,       Z], color: cross_color });
        verts.push(GizmoVertex { position: [x,       y - ARM, Z], color: cross_color });
        verts.push(GizmoVertex { position: [x,       y + ARM, Z], color: cross_color });
    }

    // Líneas entre puntos consecutivos
    for i in 0..pts.len().saturating_sub(1) {
        let [ax, ay] = pts[i];
        let [bx, by] = pts[i + 1];
        verts.push(GizmoVertex { position: [ax, ay, Z], color: line_color });
        verts.push(GizmoVertex { position: [bx, by, Z], color: line_color });
    }

    gizmo::build_from_vertices(device, &verts)
}

/// Construye el GizmoBuffer de overlay para el modo edición de pivot.
/// Dibuja un rectángulo cyan (LineList, 4 segmentos = 8 vértices) alrededor
/// del quad de la entidad para que el usuario sepa sobre qué área debe clickear.
fn build_pivot_edit_overlay(device: &wgpu::Device, pos: GlamVec3, scale: GlamVec3) -> gizmo::GizmoBuffer {
    let left   = pos.x - scale.x * 0.5;
    let right  = pos.x + scale.x * 0.5;
    let bottom = pos.y - scale.y * 0.5;
    let top    = pos.y + scale.y * 0.5;
    const Z: f32 = 0.2;
    let color = [0.2_f32, 0.9, 1.0, 1.0]; // cyan

    let verts = vec![
        // Borde inferior
        GizmoVertex { position: [left,  bottom, Z], color },
        GizmoVertex { position: [right, bottom, Z], color },
        // Borde derecho
        GizmoVertex { position: [right, bottom, Z], color },
        GizmoVertex { position: [right, top,    Z], color },
        // Borde superior
        GizmoVertex { position: [right, top,    Z], color },
        GizmoVertex { position: [left,  top,    Z], color },
        // Borde izquierdo
        GizmoVertex { position: [left,  top,    Z], color },
        GizmoVertex { position: [left,  bottom, Z], color },
    ];

    gizmo::build_from_vertices(device, &verts)
}

/// Borde cyan + cruceta amarilla en el pivot actual del frame.
/// pivot_x, pivot_y: coordenadas en píxeles dentro del frame (0,0 = esquina superior-izquierda).
fn build_pivot_edit_overlay_with_cross(
    device:   &wgpu::Device,
    pos:      GlamVec3,
    scale:    GlamVec3,
    pivot_x:  f32,
    pivot_y:  f32,
    img_w:    u32,
    img_h:    u32,
) -> gizmo::GizmoBuffer {
    let left   = pos.x - scale.x * 0.5;
    let right  = pos.x + scale.x * 0.5;
    let bottom = pos.y - scale.y * 0.5;
    let top    = pos.y + scale.y * 0.5;
    const Z: f32 = 0.2;
    let border_color = [0.2_f32, 0.9, 1.0, 1.0]; // cyan

    let mut verts = vec![
        GizmoVertex { position: [left,  bottom, Z], color: border_color },
        GizmoVertex { position: [right, bottom, Z], color: border_color },
        GizmoVertex { position: [right, bottom, Z], color: border_color },
        GizmoVertex { position: [right, top,    Z], color: border_color },
        GizmoVertex { position: [right, top,    Z], color: border_color },
        GizmoVertex { position: [left,  top,    Z], color: border_color },
        GizmoVertex { position: [left,  top,    Z], color: border_color },
        GizmoVertex { position: [left,  bottom, Z], color: border_color },
    ];

    // Cruceta en el pivot actual (solo si el pivot tiene coordenadas válidas)
    if img_w > 0 && img_h > 0 {
        let px = left + (pivot_x / img_w as f32) * scale.x;
        let py = top  - (pivot_y / img_h as f32) * scale.y;
        let s  = (scale.x.min(scale.y) * 0.07).max(0.005);
        let cross_color = [1.0_f32, 1.0, 0.0, 1.0]; // amarillo

        verts.extend_from_slice(&[
            GizmoVertex { position: [px - s, py,     Z], color: cross_color },
            GizmoVertex { position: [px + s, py,     Z], color: cross_color },
            GizmoVertex { position: [px,     py - s, Z], color: cross_color },
            GizmoVertex { position: [px,     py + s, Z], color: cross_color },
        ]);
    }

    gizmo::build_from_vertices(device, &verts)
}

/// Overlay naranja para el área lógica: rectángulo centrado en la entidad
/// con las dimensiones del bounding box lógico (w×h píxeles → mundo).
fn build_logical_area_overlay(
    device:       &wgpu::Device,
    pos:          GlamVec3,
    orig_scale_y: f32,
    w:            u32,
    h:            u32,
) -> gizmo::GizmoBuffer {
    if h == 0 { return gizmo::build_from_vertices(device, &[]); }
    let aspect  = w as f32 / h as f32;
    let world_h = orig_scale_y;
    let world_w = world_h * aspect;
    let left   = pos.x - world_w * 0.5;
    let right  = pos.x + world_w * 0.5;
    let bottom = pos.y - world_h * 0.5;
    let top    = pos.y + world_h * 0.5;
    const Z: f32 = 0.15;
    let color = [1.0_f32, 0.55, 0.0, 1.0]; // naranja

    let verts = vec![
        GizmoVertex { position: [left,  bottom, Z], color },
        GizmoVertex { position: [right, bottom, Z], color },
        GizmoVertex { position: [right, bottom, Z], color },
        GizmoVertex { position: [right, top,    Z], color },
        GizmoVertex { position: [right, top,    Z], color },
        GizmoVertex { position: [left,  top,    Z], color },
        GizmoVertex { position: [left,  top,    Z], color },
        GizmoVertex { position: [left,  bottom, Z], color },
    ];

    gizmo::build_from_vertices(device, &verts)
}
