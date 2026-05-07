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

mod herramienta_de_dibujo;

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
    DrawCollider { points_world: Vec<[f32; 2]>, cursor_world: Option<[f32; 2]> },
    DrawExecutionArea { points_world: Vec<[f32; 2]>, cursor_world: Option<[f32; 2]> },
    QuickBuildPlace { cursor_world: Option<[f32; 2]> },
}

impl Default for ActiveTool {
    fn default() -> Self { ActiveTool::None }
}

impl ActiveTool {
    pub(crate) fn is_active(&self) -> bool { !matches!(self, ActiveTool::None) }
}

/// Marca una entidad ECS como colisionador creado con la herramienta de dibujo.
#[derive(Debug, Clone)]
pub(crate) struct ColliderMarker {}

/// Marca una entidad ECS como área de ejecución (trigger sin física).
#[derive(Debug, Clone)]
pub(crate) struct ExecutionAreaMarker {}
impl State {
    fn screen_to_world_2d(&self, pixel_x: f32, pixel_y: f32) -> Option<(f32, f32)> {
        let cam = self.camera_2d.as_ref()?;
        let w = self.size.width as f32;
        let h = self.size.height as f32;
        let aspect = w / h;
        let half_w = cam.half_h * aspect;
        let wx = cam.x + ((pixel_x / w) * 2.0 - 1.0) * half_w;
        let wy = cam.y + (1.0 - (pixel_y / h) * 2.0) * cam.half_h;
        Some((wx, wy))
    }

    fn quick_build_snap_position_2d(&self, wx: f32, wy: f32) -> Option<(f32, f32)> {
        if !self.ctrl_held {
            return None;
        }
        let ghost_id = self.quick_build_ghost_id?;
        let preview_path = self.quick_build_preview_path.as_deref()?;
        let preview_kind = self.quick_build_preview_kind.as_deref()?;
        let ghost_t = self.world.get::<Transform>(ghost_id)?;
        let ghost_half_w = ghost_t.scale.x.abs() * 0.5;
        let ghost_half_h = ghost_t.scale.y.abs() * 0.5;

        let candidates = if preview_kind == "scenario" {
            &self.scenario_entities
        } else {
            &self.character_entities
        };

        let mut best: Option<(f32, (f32, f32))> = None;
        for &id in candidates {
            if id == ghost_id { continue; }

            let same_blueprint = if preview_kind == "scenario" {
                self.world
                    .get::<ScenarioMarker>(id)
                    .map(|m| m.path.as_str() == preview_path)
                    .unwrap_or(false)
            } else {
                self.world
                    .get::<CharacterMarker>(id)
                    .map(|m| m.path.as_str() == preview_path)
                    .unwrap_or(false)
            };
            if !same_blueprint { continue; }

            let Some(t) = self.world.get::<Transform>(id) else { continue; };
            let entity_half_w = t.scale.x.abs() * 0.5;
            let entity_half_h = t.scale.y.abs() * 0.5;

            let dx = wx - t.position.x;
            let dy = wy - t.position.y;

            let snap_x = t.position.x + if dx >= 0.0 {
                entity_half_w + ghost_half_w
            } else {
                -(entity_half_w + ghost_half_w)
            };
            let snap_y = t.position.y;

            let snap_x_v = t.position.x;
            let snap_y_v = t.position.y + if dy >= 0.0 {
                entity_half_h + ghost_half_h
            } else {
                -(entity_half_h + ghost_half_h)
            };

            let dist_h = ((wx - snap_x).powi(2) + (wy - snap_y).powi(2)).sqrt();
            let dist_v = ((wx - snap_x_v).powi(2) + (wy - snap_y_v).powi(2)).sqrt();

            let (dist, candidate) = if dist_h <= dist_v {
                (dist_h, (snap_x, snap_y))
            } else {
                (dist_v, (snap_x_v, snap_y_v))
            };

            let threshold = (entity_half_w + ghost_half_w)
                .max(entity_half_h + ghost_half_h)
                .max(0.25)
                * 1.35;
            if dist > threshold {
                continue;
            }

            if best.map_or(true, |(best_dist, _)| dist < best_dist) {
                best = Some((dist, candidate));
            }
        }

        if let Some((_, p)) = best {
            return Some(p);
        }

        let cell = self.grid_config.cell_size.max(0.05);
        let gx = (wx / cell).floor() * cell + cell * 0.5;
        let gy = (wy / cell).floor() * cell + cell * 0.5;
        Some((gx, gy))
    }

    pub(crate) fn update_tool_overlay_cursor_2d(&mut self, pixel_x: f32, pixel_y: f32) {
        let Some((wx, wy)) = self.screen_to_world_2d(pixel_x, pixel_y) else { return; };
        let quick_build_snap_target = self.quick_build_snap_position_2d(wx, wy);
        match &mut self.active_tool {
            ActiveTool::DrawCollider { points_world, cursor_world }
            | ActiveTool::DrawExecutionArea { points_world, cursor_world } => {
                *cursor_world = Some([wx, wy]);
                let pts_clone = points_world.clone();
                self.tool_overlay_buffer = build_tool_overlay(&self.device, &pts_clone, *cursor_world);
            }
            ActiveTool::QuickBuildPlace { cursor_world } => {
                let (target_x, target_y) = quick_build_snap_target.unwrap_or((wx, wy));
                *cursor_world = Some([target_x, target_y]);
                if let Some(ghost_id) = self.quick_build_ghost_id {
                    if let Some(t) = self.world.get_mut::<Transform>(ghost_id) {
                        if let Some(base_scale) = self.quick_build_preview_scale {
                            if self.ctrl_held {
                                let cell = self.grid_config.cell_size.max(0.05);
                                t.scale = GlamVec3::new(cell, cell, base_scale[2]);
                            } else {
                                t.scale = GlamVec3::new(base_scale[0], base_scale[1], base_scale[2]);
                            }
                        }
                        t.position = GlamVec3::new(target_x, target_y, 0.5);
                    }
                }
                self.tool_overlay_buffer = gizmo::build_from_vertices(&self.device, &[]);
                send_event(&EngineEvent::QuickBuildMove { x: target_x, y: target_y });
            }
            ActiveTool::None => {}
        }
    }

    pub(crate) fn undo_last_tool_step_2d(&mut self) -> bool {
        match &mut self.active_tool {
            ActiveTool::DrawCollider { points_world, cursor_world }
            | ActiveTool::DrawExecutionArea { points_world, cursor_world } => {
                if points_world.pop().is_some() {
                    let pts_clone = points_world.clone();
                    self.tool_overlay_buffer = build_tool_overlay(&self.device, &pts_clone, *cursor_world);
                    send_event(&EngineEvent::DrawingProgress { count: points_world.len() as u32 });
                    return true;
                }
                false
            }
            ActiveTool::None => false,
            // QuickBuildPlace no tiene pasos que deshacer aquí; Ctrl+Z en JS deshace la entidad colocada
            ActiveTool::QuickBuildPlace { .. } => false,
        }
    }

    // ── Inicialización ────────────────────────────────────────────────────────

    /// Configura la escena 2D de plataformas con un único rectángulo (player).
    pub(crate) fn setup_2d_platformer(&mut self) {
        // Limpiar escena previa y escenarios de fondo
        self.scenario_entities.clear();
        self.character_entities.clear();
        self.collider_entities.clear();
        self.execution_area_entities.clear();
        self.execution_overlaps.clear();
        self.background_entity = None;
        self.active_tool = ActiveTool::None;
        self.tool_overlay_buffer = gizmo::build_from_vertices(&self.device, &[]);
        self.physics_2d.clear();
        self.world.clear();
        self.meshes.clear();
        self.uv_rects.clear();
        self.static_tex_cache.clear();
        self.anim_texture_cache.clear();
        self.anim_overrides.clear();
        self.animations.clear();
        self.active_animations.clear();
        self.default_animation_by_entity.clear();
        self.anim_saved_transforms.clear();
        self.anim_flip_overrides.clear();
        self.entity_facing_right.clear();
        self.selected_entity = None;
        self.selected_entities.clear();
        self.hovered_entity  = None;

        // Quad unitario canónico — compartido por TODOS los sprites 2D de la escena.
        // El Transform de cada entidad lo escala y posiciona correctamente.
        let canonical_quad = create_quad_xy(&self.device, 0.0, 0.0, 1.0, 1.0, "canonical-quad");
        self.meshes.push(canonical_quad);
        self.canonical_quad_idx = 0;

        // -- Personaje por defecto (Player): quad skin 1.0 × 1.5 -------------------
        let player_rgba = [232u8, 220, 200, 255];
        let tex_idx     = self.uv_rects.len();
        self.uv_rects.push(self.atlas.pack(&self.queue, &player_rgba, 1, 1));
        let player_id = self.world.spawn(Some("Player"));
        self.world.insert(player_id, MeshComponent { mesh_idx: self.canonical_quad_idx, tex_idx });
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
                send_event(&EngineEvent::Error { message: format!("No se pudo leer el escenario (ruta: {path:?}): {e}") });
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
        // Deduplicar textura: si ya existe una con el mismo path, reutilizar su UV rect.
        let uv = if let Some(&cached_uv) = self.static_tex_cache.get(path) {
            cached_uv
        } else {
            let u = self.atlas.pack(&self.queue, &img, img_width, img_height);
            self.static_tex_cache.insert(path.to_owned(), u);
            u
        };
        drop(gpu_tex); // ya no necesitamos GpuTexture (datos ya en atlas)
        // Todos los escenarios comparten el quad canónico (geometría idéntica).
        let tex_idx  = self.uv_rects.len();
        self.uv_rects.push(uv);
        let scenario_name = self.next_numbered_entity_name("Escenario");
        let sc_id = self.world.spawn(Some(&scenario_name));
        self.world.insert(sc_id, MeshComponent { mesh_idx: self.canonical_quad_idx, tex_idx });
        self.world.insert(sc_id, Transform {
            position: GlamVec3::new(0.0, 0.0, -1.0),
            scale:    GlamVec3::new(base_world_w, base_world_h, 1.0),
            ..Default::default()
        });
        self.world.insert(sc_id, ScenarioMarker { img_width, img_height, base_world_h, path: path.to_owned() });
        self.scenario_entities.push(sc_id);

        send_event(&EngineEvent::ScenarioLoaded { id: sc_id, path: path.to_owned() });
        log::debug!("[load_scenario] entidad {sc_id} creada {img_width}×{img_height}: {path}");
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
        let uv       = self.atlas.pack(&self.queue, &img, img_w, img_h);
        drop(gpu_tex);
        let tex_idx  = self.uv_rects.len();
        self.uv_rects.push(uv);
        let background_name = self.next_numbered_entity_name("Background");
        let bg_id = self.world.spawn(Some(&background_name));
        self.world.insert(bg_id, MeshComponent { mesh_idx: self.canonical_quad_idx, tex_idx });
        self.world.insert(bg_id, Transform {
            position: GlamVec3::new(0.0, 0.0, -10.0),
            scale:    GlamVec3::new(world_w, world_h, 1.0),
            ..Default::default()
        });
        // No seleccionable para que no interfiera con el picking
        self.world.insert(bg_id, crate::ecs::NonSelectable);
        self.background_entity = Some(bg_id);

        send_event(&EngineEvent::BackgroundLoaded { path: path.to_owned() });
        log::debug!("[load_background] fondo cargado {img_w}×{img_h} escala {world_w}×{world_h}: {path}");
    }

    /// Elimina el fondo actual del mundo 2D, si existe.
    pub(crate) fn clear_background(&mut self) {
        if let Some(old_id) = self.background_entity.take() {
            self.world.despawn(old_id);
            log::info!("[clear_background] fondo eliminado");
        }
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
                send_event(&EngineEvent::Error { message: format!("No se pudo leer el personaje (ruta: {path:?}): {e}") });
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
        // Deduplicar textura: sprites del mismo PNG reutilizan la misma sub-región del atlas.
        let uv = if let Some(&cached_uv) = self.static_tex_cache.get(path) {
            cached_uv
        } else {
            let u = self.atlas.pack(&self.queue, &img, img_width, img_height);
            self.static_tex_cache.insert(path.to_owned(), u);
            u
        };
        drop(gpu_tex);
        let tex_idx  = self.uv_rects.len();
        self.uv_rects.push(uv);
        let character_name = self.next_numbered_entity_name("Personaje");
        let ch_id = self.world.spawn(Some(&character_name));
        self.world.insert(ch_id, MeshComponent { mesh_idx: self.canonical_quad_idx, tex_idx });
        self.world.insert(ch_id, Transform {
            position: GlamVec3::new(0.0, 0.0, 0.0),
            scale:    GlamVec3::new(base_world_w, base_world_h, 1.0),
            ..Default::default()
        });
        self.world.insert(ch_id, CharacterMarker { img_width, img_height, base_world_h, path: path.to_owned() });
        self.character_entities.push(ch_id);

        send_event(&EngineEvent::CharacterLoaded { id: ch_id, path: path.to_owned() });
        log::debug!("[load_character] entidad {ch_id} creada {img_width}×{img_height}: {path}");
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
            // Crear un nuevo quad blanco usando el quad canónico compartido
            let player_rgba = [232u8, 220, 200, 255];
            let tex_idx = self.uv_rects.len();
            self.uv_rects.push(self.atlas.pack(&self.queue, &player_rgba, 1, 1));
            let player_name = self.next_numbered_entity_name("Player");
            let new_id = self.world.spawn(Some(&player_name));
            self.world.insert(new_id, MeshComponent { mesh_idx: self.canonical_quad_idx, tex_idx });
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
        src_rect: Option<(u32, u32, u32, u32)>,
        flip_horizontal: bool,
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
        let cache_key = if let Some((sx, sy, sw, sh)) = src_rect {
            format!("{path}#{sx}:{sy}:{sw}:{sh}")
        } else {
            path.to_string()
        };

        let (uv_rect, img_width, img_height) =
            if let Some((cached_uv, w, h)) = self.anim_texture_cache.get(&cache_key) {
                (*cached_uv, *w, *h)
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
                let processed = if let Some((sx, sy, sw, sh)) = src_rect {
                    let (sheet_w, sheet_h) = img.dimensions();
                    if sx >= sheet_w || sy >= sheet_h {
                        log::error!("[play_animation_frame] recorte fuera de rango para {path}: x={sx} y={sy} sheet={sheet_w}x{sheet_h}");
                        send_event(&EngineEvent::Error { message: "Recorte de frame fuera del sprite sheet".to_string() });
                        return;
                    }

                    let max_w = sheet_w.saturating_sub(sx);
                    let max_h = sheet_h.saturating_sub(sy);
                    let crop_w = sw.min(max_w).max(1);
                    let crop_h = sh.min(max_h).max(1);

                    image::imageops::crop_imm(&img, sx, sy, crop_w, crop_h).to_image()
                } else {
                    img
                };

                let (w, h) = processed.dimensions();
                let uv_packed = self.atlas.pack(&self.queue, &processed, w, h);
                self.anim_texture_cache.insert(cache_key, (uv_packed, w, h));
                log::debug!("[play_animation_frame] frame cargado al atlas (cache miss): {path}");
                (uv_packed, w, h)
            };

        // Obtener tex_idx para el override (independiente del mesh geométrico)
        let tex_position = match self.world.get::<MeshComponent>(id) {
            Some(m) => m.tex_idx,
            None => {
                log::warn!("[play_animation_frame] entidad {id} sin MeshComponent");
                return;
            }
        };
        if tex_position >= self.uv_rects.len() {
            log::warn!("[play_animation_frame] indice invalido: {tex_position}");
            return;
        }

        // Escribir el override — el render loop lo lee con prioridad sobre uv_rects[].
        // Para flip horizontal invertimos u_min/u_max para espejar la muestra en el shader.
        let uv_rect_for_render = if flip_horizontal {
            [uv_rect[2], uv_rect[1], uv_rect[0], uv_rect[3]]
        } else {
            uv_rect
        };
        self.anim_overrides.insert(tex_position, uv_rect_for_render);

        // ── Aplicar pivot ────────────────────────────────────────────────────
        if logical_w > 0 && logical_h > 0 {
            if let Some(transform) = self.world.get::<Transform>(id).cloned() {
                let saved = self.anim_saved_transforms
                    .entry(id)
                    .or_insert((transform.position, transform.scale));

                let orig_pos = saved.0;
                let orig_scale = saved.1;

                // Escala de referencia estable: usar el alto lógico de la animación
                // para que cambios de tamaño entre frames no alteren el anclaje.
                let ref_h_px    = logical_h.max(1) as f32;
                let world_per_px = orig_scale.y / ref_h_px;
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

        log::debug!("[play_animation_frame] frame actualizado para entidad {id} (tex_idx={tex_position}, pivot=({pivot_x},{pivot_y}))");
    }

    pub(crate) fn preload_anim_frame_with_rect(&mut self, path: &str, src_rect: Option<(u32, u32, u32, u32)>) {
        let cache_key = if let Some((sx, sy, sw, sh)) = src_rect {
            format!("{path}#{sx}:{sy}:{sw}:{sh}")
        } else {
            path.to_string()
        };

        if self.anim_texture_cache.contains_key(&cache_key) {
            return; // ya cacheado
        }
        let bytes = match fs::read(path) {
            Ok(b) => b,
            Err(e) => { log::warn!("[preload] no se pudo leer {path}: {e}"); return; }
        };
        use image::ImageReader;
        use std::io::Cursor;
        let img = match ImageReader::new(Cursor::new(&bytes))
            .with_guessed_format()
            .map_err(|e| e.to_string())
            .and_then(|r| r.decode().map_err(|e| e.to_string()))
        {
            Ok(i) => i.to_rgba8(),
            Err(e) => { log::warn!("[preload] error decodificando {path}: {e}"); return; }
        };
        let processed = if let Some((sx, sy, sw, sh)) = src_rect {
            let (sheet_w, sheet_h) = img.dimensions();
            if sx >= sheet_w || sy >= sheet_h {
                log::warn!("[preload] recorte fuera de rango para {path}: x={sx} y={sy} sheet={sheet_w}x{sheet_h}");
                return;
            }
            let max_w = sheet_w.saturating_sub(sx);
            let max_h = sheet_h.saturating_sub(sy);
            let crop_w = sw.min(max_w).max(1);
            let crop_h = sh.min(max_h).max(1);
            image::imageops::crop_imm(&img, sx, sy, crop_w, crop_h).to_image()
        } else {
            img
        };

        let (w, h) = processed.dimensions();
        let uv = self.atlas.pack(&self.queue, &processed, w, h);
        self.anim_texture_cache.insert(cache_key, (uv, w, h));
        log::debug!("[preload] frame pre-empacado en atlas: {path}");
    }

    /// Restaura el sprite original de una entidad después de una animación.
    pub(crate) fn restore_animation_frame(&mut self, id: u32) {
        let is_scenario  = self.scenario_entities.contains(&id);
        let is_character = self.character_entities.contains(&id);
        if !is_scenario && !is_character {
            log::warn!("[restore_animation_frame] entidad {id} no es escenario ni personaje");
            return;
        }

        // Obtener tex_idx del MeshComponent
        let tex_position = match self.world.get::<MeshComponent>(id) {
            Some(m) => m.tex_idx,
            None => {
                log::warn!("[restore_animation_frame] entidad {id} sin MeshComponent");
                return;
            }
        };

        // Eliminar el override: el render loop vuelve a usar textures[tex_position]
        // que nunca fue modificado. No hay que recargar nada de disco.
        self.anim_overrides.remove(&tex_position);

        // Solo restaurar la escala original (elimina la distorsión del pivot calc).
        // La posición NO se toca: el personaje se queda donde llegó gracias a scripts/física.
        if let Some((_saved_pos, orig_scale)) = self.anim_saved_transforms.remove(&id) {
            if let Some(t) = self.world.get_mut::<Transform>(id) {
                t.scale = orig_scale;
            }
            log::info!("[restore_animation_frame] entidad {id} → escala restaurada, posición conservada");
        } else {
            log::warn!("[restore_animation_frame] entidad {id} sin anim_saved_transforms — escala NO modificada");
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
            let tex_pos = m.tex_idx;
            if tex_pos < self.uv_rects.len() {
                // Empacar el frame en el atlas (o recuperar de caché) 
                let cache_key = frame_path.to_string();
                let uv = if let Some((cached_uv, _, _)) = self.anim_texture_cache.get(&cache_key) {
                    *cached_uv
                } else {
                    let u = self.atlas.pack(&self.queue, &img, img_w, img_h);
                    self.anim_texture_cache.insert(cache_key, (u, img_w, img_h));
                    u
                };
                self.anim_overrides.insert(tex_pos, uv);
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
            if self.world.has::<crate::ecs::NonSelectable>(entity) { continue; }
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
                if self.ctrl_held {
                    if let Some(idx) = self.selected_entities.iter().position(|&e| e == entity) {
                        self.selected_entities.swap_remove(idx);
                        if self.selected_entity == Some(entity) {
                            self.selected_entity = self.selected_entities.last().copied();
                        }
                        if self.selected_entities.is_empty() {
                            self.selected_entity = None;
                            send_event(&EngineEvent::EntityDeselected);
                        } else if let Some(active_id) = self.selected_entity {
                            let active_name      = self.world.name(active_id).unwrap_or("Entity").to_string();
                            let active_transform = self.world.get::<Transform>(active_id).cloned().unwrap_or_default();
                            let active_pos = active_transform.position.to_array();
                            let active_rot = [active_transform.rotation.x, active_transform.rotation.y,
                                              active_transform.rotation.z, active_transform.rotation.w];
                            let active_scl             = active_transform.scale.to_array();
                            let physics_enabled = self.physics_2d.has_physics(active_id);
                            let physics_type    = self.physics_2d.get_body_type(active_id).to_string();
                            send_event(&EngineEvent::EntitySelected {
                                id: active_id, name: active_name, position: active_pos, rotation: active_rot, scale: active_scl,
                                physics_enabled,
                                physics_type,
                            });
                        }
                        return;
                    } else {
                        self.selected_entities.push(entity);
                        self.selected_entity = Some(entity);
                    }
                } else {
                    if self.selected_entity == Some(entity)
                        && self.selected_entities.len() == 1
                        && self.selected_entities[0] == entity {
                        return;
                    }
                    self.selected_entities.clear();
                    self.selected_entities.push(entity);
                    self.selected_entity = Some(entity);
                }
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
                if !self.ctrl_held && (self.selected_entity.is_some() || !self.selected_entities.is_empty()) {
                    self.selected_entity = None;
                    self.selected_entities.clear();
                    send_event(&EngineEvent::EntityDeselected);
                }
            }
        }
    }

    // ── Picking de eje del gizmo 2D ───────────────────────────────────────────

    /// Devuelve el índice del eje del gizmo 2D más cercano al cursor (0=X, 1=Y).
    pub fn pick_gizmo_axis_2d(&self, pixel_x: f32, pixel_y: f32) -> Option<usize> {
        let origin = self.selection_center()?;
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
        let selected_ids: Vec<EntityId> = if !self.selected_entities.is_empty() {
            self.selected_entities.clone()
        } else {
            self.selected_entity.into_iter().collect()
        };
        if selected_ids.is_empty() { return; }

        let cam = match &self.camera_2d {
            Some(c) => Camera2D { x: c.x, y: c.y, half_h: c.half_h, near: c.near, far: c.far },
            None    => return,
        };
        let mut sum = GlamVec3::ZERO;
        let mut count = 0usize;
        for &id in &selected_ids {
            if let Some(t) = self.world.get::<Transform>(id) {
                sum += t.position;
                count += 1;
            }
        }
        if count == 0 { return; }
        let origin = sum / count as f32;

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
        for &sel_id in &selected_ids {
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
            }
        }

        let lead_id = self.selected_entity.or_else(|| selected_ids.last().copied());
        if let Some(sel_id) = lead_id {
            let name = self.world.name(sel_id).unwrap_or("Entity").to_string();
            if let Some(t) = self.world.get::<Transform>(sel_id) {
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

        // Sincronizar el Rapier body con la nueva posición visual.
        // Sin esto, el cuerpo físico (y por tanto las colisiones) permanece
        // en la posición original aunque el cuadro visual se haya movido.
        for &sel_id in &selected_ids {
            let new_pos = self.world.get::<Transform>(sel_id)
                .map(|t| (t.position.x, t.position.y));
            if let Some((nx, ny)) = new_pos {
            // Si existe una base de animación guardada para la entidad,
            // mantenerla sincronizada con el drag del gizmo para que
            // play_animation_frame no ancle en una posición antigua.
            if let Some(saved) = self.anim_saved_transforms.get_mut(&sel_id) {
                saved.0.x = nx;
                saved.0.y = ny;
            }
            self.physics_2d.teleport_entity(sel_id, nx, ny);
            }
        }
    }

    // ── Hover 2D ─────────────────────────────────────────────────────────────

    /// Actualiza `hovered_entity` y `hovered_gizmo_axis` en modo 2D.
    /// Usa spatial grid para O(k) lookup en lugar de O(n) linear scan.
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
        
        // Query spatial grid para entidades cerca del cursor
        let candidates = self.spatial_grid.query_cell(wx, wy);
        for entity in candidates {
            if self.world.has::<crate::ecs::NonSelectable>(entity) { continue; }
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
        let _cam = match &self.camera_2d {
            Some(c) => Camera2D { x: c.x, y: c.y, half_h: c.half_h, near: c.near, far: c.far },
            None    => return false,
        };
        if !self.active_tool.is_active() { return false; }

        let Some((wx, wy)) = self.screen_to_world_2d(pixel_x, pixel_y) else { return false; };

        match &mut self.active_tool {
            ActiveTool::DrawCollider { points_world, cursor_world } => {
                points_world.push([wx, wy]);
                *cursor_world = Some([wx, wy]);
                let count = points_world.len() as u32;

                if count >= 4 {
                    let pts: [[f32; 2]; 4] = [
                        points_world[0], points_world[1],
                        points_world[2], points_world[3],
                    ];
                    self.active_tool = ActiveTool::None;
                    self.tool_overlay_buffer = gizmo::build_from_vertices(&self.device, &[]);
                    self.create_collision_box_from_points(&pts, true);
                } else {
                    let pts_clone: Vec<[f32; 2]> = points_world.clone();
                    self.tool_overlay_buffer = build_tool_overlay(&self.device, &pts_clone, *cursor_world);
                    send_event(&EngineEvent::DrawingProgress { count });
                }
                true
            }
            ActiveTool::DrawExecutionArea { points_world, cursor_world } => {
                points_world.push([wx, wy]);
                *cursor_world = Some([wx, wy]);
                let count = points_world.len() as u32;

                if count >= 4 {
                    let pts: [[f32; 2]; 4] = [
                        points_world[0], points_world[1],
                        points_world[2], points_world[3],
                    ];
                    self.active_tool = ActiveTool::None;
                    self.tool_overlay_buffer = gizmo::build_from_vertices(&self.device, &[]);
                    self.create_execution_area_from_points(&pts, true);
                } else {
                    let pts_clone: Vec<[f32; 2]> = points_world.clone();
                    self.tool_overlay_buffer = build_tool_overlay(&self.device, &pts_clone, *cursor_world);
                    send_event(&EngineEvent::DrawingProgress { count });
                }
                true
            }
            ActiveTool::QuickBuildPlace { cursor_world } => {
                let fit_to_grid = self.ctrl_held;
                let [cx, cy] = if fit_to_grid {
                    let (sx, sy) = self.quick_build_snap_position_2d(wx, wy).unwrap_or((wx, wy));
                    [sx, sy]
                } else {
                    cursor_world.unwrap_or([wx, wy])
                };
                send_event(&EngineEvent::QuickBuildClick { x: cx, y: cy, fit_to_grid });
                true
            }
            ActiveTool::None => false,
        }
    }

    /// Crea una entidad ECS de colisionador a partir de 4 puntos en espacio de mundo.
    pub(crate) fn create_collision_box_from_points(&mut self, pts: &[[f32; 2]; 4], track_undo: bool) {
        // Crea la entidad visual (quad + textura cyan) sin física.
        let collider_name = self.next_numbered_entity_name("Colisionador");
        let (entity, pos, scale) = self.create_box_entity(pts, &collider_name, [60, 220, 200, 235]);

        // Marca la entidad como colisionador y añade física estática.
        self.world.insert(entity, ColliderMarker {});
        // Usamos cuboid estático (AABB del bounding box) en lugar de hull convexo 3D,
        // ya que rapier3d puede rechazar hulls de puntos coplanares (z=0).
        // IMPORTANTE: forzar z=0 en la posición física. create_box_entity devuelve
        // z=-0.5 para el orden de render, pero Rapier trabaja en 3D real: si el
        // colisionador estático y el personaje dinámico tienen z distinto no colisionan.
        self.physics_2d.set_entity_physics(
            entity, true, "static",
            [pos[0], pos[1], 0.0],
            [scale[0] * 0.5, scale[1] * 0.5, 0.01],
        );
        self.collider_entities.push(entity);
        if track_undo {
            self.undo_stack.push(crate::engine::UndoAction::RemoveEntity { id: entity });
        }

        send_event(&EngineEvent::ColliderCreated { id: entity, points: *pts });
        log::info!("[tool] colisionador creado: entidad {entity} en {:?}", pts);
    }

    /// Crea una entidad ECS de área de ejecución (trigger) a partir de 4 puntos.
    /// No añade física para evitar colisiones con personajes.
    pub(crate) fn create_execution_area_from_points(&mut self, pts: &[[f32; 2]; 4], track_undo: bool) {
        let trigger_name = self.next_numbered_entity_name("ExecutionArea");
        let (entity, _pos, _scale) = self.create_box_entity(pts, &trigger_name, [220, 80, 80, 230]);

        self.world.insert(entity, ExecutionAreaMarker {});
        self.execution_area_entities.push(entity);
        if track_undo {
            self.undo_stack.push(crate::engine::UndoAction::RemoveEntity { id: entity });
        }

        send_event(&EngineEvent::ExecutionAreaCreated { id: entity, points: *pts });
        log::info!("[tool] área de ejecución creada: entidad {entity} en {:?}", pts);
    }

    /// Detecta entradas a áreas de ejecución en modo preview y dispara hooks de scripting.
    pub(crate) fn update_execution_areas_2d(&mut self) {
        if !self.preview_playing {
            self.execution_overlaps.clear();
            return;
        }

        let trigger_ids = self.execution_area_entities.clone();
        let actor_ids = self.character_entities.clone();
        let mut next_overlaps = std::collections::HashSet::new();

        for trigger_id in trigger_ids {
            let Some(trigger_t) = self.world.get::<Transform>(trigger_id).cloned() else { continue; };
            let trigger_hx = trigger_t.scale.x * 0.5;
            let trigger_hy = trigger_t.scale.y * 0.5;

            for actor_id in &actor_ids {
                let Some(actor_t) = self.world.get::<Transform>(*actor_id).cloned() else { continue; };
                let actor_hx = actor_t.scale.x * 0.5;
                let actor_hy = actor_t.scale.y * 0.5;

                let overlap_x = (trigger_t.position.x - actor_t.position.x).abs() <= (trigger_hx + actor_hx);
                let overlap_y = (trigger_t.position.y - actor_t.position.y).abs() <= (trigger_hy + actor_hy);
                if !overlap_x || !overlap_y {
                    continue;
                }

                next_overlaps.insert((trigger_id, *actor_id));
                if self.execution_overlaps.contains(&(trigger_id, *actor_id)) {
                    continue;
                }

                log::debug!("[trigger] entrada detectada: trigger={} actor={}", trigger_id, actor_id);
                crate::ipc::send_event(&crate::ipc::EngineEvent::TriggerEntered { trigger_id, actor_id: *actor_id });

                let trigger_snapshot = self.build_script_snapshot(trigger_id);
                let actor_snapshot = self.build_script_snapshot(*actor_id);
                match self.script_engine.run_trigger_enter_hook(
                    trigger_id,
                    *actor_id,
                    trigger_snapshot.as_ref(),
                    actor_snapshot.as_ref(),
                ) {
                    Ok(commands) => self.apply_script_commands(commands),
                    Err(e) => log::warn!("[trigger] error ejecutando script en área {trigger_id}: {e}"),
                }
            }
        }

        // Detectar salidas: pares que estaban pero ya no están
        let exited: Vec<_> = self.execution_overlaps
            .iter()
            .filter(|pair| !next_overlaps.contains(*pair))
            .cloned()
            .collect();
        for (trigger_id, actor_id) in exited {
            log::debug!("[trigger] salida detectada: trigger={} actor={}", trigger_id, actor_id);
            crate::ipc::send_event(&crate::ipc::EngineEvent::TriggerExited { trigger_id, actor_id });
        }

        self.execution_overlaps = next_overlaps;
    }

    /// Carga una entidad fantasma para previsualizar el blueprint en modo Quick Build.
    /// No se añade a `scenario_entities` ni `character_entities` y no emite eventos.
    /// Se posiciona fuera de pantalla hasta que el cursor se mueva.
    pub(crate) fn load_quick_build_ghost(&mut self, path: &str, kind: &str, scale: [f32; 3], src_rect: Option<[u32; 4]>) -> Option<u32> {
        let bytes = std::fs::read(path).ok()?;
        use image::ImageReader;
        use image::imageops;
        use std::io::Cursor;
        let mut img = ImageReader::new(Cursor::new(&bytes))
            .with_guessed_format().ok()?
            .decode().ok()?.to_rgba8();
        if let Some([sx, sy, sw, sh]) = src_rect {
            let iw = img.width();
            let ih = img.height();
            if sw > 0 && sh > 0 && sx < iw && sy < ih {
                let cw = sw.min(iw.saturating_sub(sx));
                let ch = sh.min(ih.saturating_sub(sy));
                if cw > 0 && ch > 0 {
                    img = imageops::crop_imm(&img, sx, sy, cw, ch).to_image();
                }
            }
        }
        let (img_w, img_h) = img.dimensions();
        let uv = if src_rect.is_none() {
            if let Some(&cached_uv) = self.static_tex_cache.get(path) {
                cached_uv
            } else {
                let u = self.atlas.pack(&self.queue, img.as_raw(), img_w, img_h);
                self.static_tex_cache.insert(path.to_owned(), u);
                u
            }
        } else {
            self.atlas.pack(&self.queue, img.as_raw(), img_w, img_h)
        };
        let tex_idx = self.uv_rects.len();
        self.uv_rects.push(uv);
        let ghost_id = self.world.spawn(Some("__qb_ghost__"));
        self.world.insert(ghost_id, MeshComponent { mesh_idx: self.canonical_quad_idx, tex_idx });
        let z = if kind == "scenario" { -0.5_f32 } else { 0.5_f32 };
        self.world.insert(ghost_id, Transform {
            position: GlamVec3::new(-99999.0, -99999.0, z),
            scale:    GlamVec3::new(scale[0], scale[1], scale[2]),
            ..Default::default()
        });
        self.world.insert(ghost_id, crate::ecs::NonSelectable);
        log::debug!("[quick_build] entidad fantasma {ghost_id} creada desde {path} ({kind})");
        Some(ghost_id)
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



/// Construye el GizmoBuffer (LineList) de overlay para la herramienta de dibujo.

fn build_tool_overlay(device: &wgpu::Device, pts: &[[f32; 2]], cursor: Option<[f32; 2]>) -> gizmo::GizmoBuffer {
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

    if let (Some(last), Some(cur)) = (pts.last().copied(), cursor) {
        verts.push(GizmoVertex { position: [last[0], last[1], Z], color: line_color });
        verts.push(GizmoVertex { position: [cur[0], cur[1], Z], color: line_color });

        // Preview de cierre del polígono: al definir el 4to punto, mostrar también
        // la línea desde el primer punto hasta el cursor para cuadrar mejor.
        if pts.len() >= 3 {
            let first = pts[0];
            verts.push(GizmoVertex { position: [first[0], first[1], Z], color: line_color });
            verts.push(GizmoVertex { position: [cur[0], cur[1], Z], color: line_color });
        }
    }

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
