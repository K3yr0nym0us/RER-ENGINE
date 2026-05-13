// ── Lógica exclusiva del modo 2D (plataformer, vista lateral) ────────────────
//
// Contiene:
//  · camera_2d            — Camera2D (ortográfica) con pan()
//  · grid_2d              — GridConfig, GridBuffer, build_grid
//  · setup_2d_platformer  — inicialización de la escena 2D
//  · load_scenario        — carga un PNG como fondo de escenario
//  · project_to_screen_2d — proyecta un punto de mundo a píxeles (cámara ortográfica)
//  · pick_entity_2d       — picking por AABB en el plano XY
//  · pick_gizmo_axis_2d   — eje del gizmo más cercano al cursor
//  · drag_gizmo_2d        — arrastre de entidad sobre eje X o Y
//  · update_hover_2d      — hover AABB + detección de eje de gizmo

#[path = "config_2d/camera_2d.rs"]
pub(crate) mod camera_2d;
pub(crate) use camera_2d::Camera2D;

#[path = "config_2d/grid_2d.rs"]
pub(crate) mod grid_2d;
pub(crate) use grid_2d::{GridBuffer, GridConfig, build_grid};

#[path = "config_2d/physics_2d/mod.rs"]
pub(crate) mod physics_2d;
pub(crate) use physics_2d::PhysicsWorld2D;

#[path = "config_2d/drawing_tool.rs"]
mod drawing_tool;
use std::fs;
use glam::Vec3 as GlamVec3;
use crate::ecs::{MeshComponent, Transform};
use crate::engine::State;
use crate::engine::AnimTextureCacheEntry;
use crate::ipc::{send_event, EngineEvent};
use crate::gizmo;

#[path = "config_2d/assets.rs"]
mod assets;
#[path = "config_2d/overlay.rs"]
mod overlay;
#[path = "config_2d/selection.rs"]
mod selection;
use overlay::{
    build_logical_area_overlay,
    build_pivot_edit_overlay_with_cross,
    build_tool_overlay,
    create_quad_xy,
};
pub(crate) use overlay::build_scenario_collision_overlay;

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
    /// Bounds opacos del PNG original, usados como collider de arranque.
    pub tight_bounds: Option<[u32; 4]>,
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

    fn snap_size_to_grid_2d(&self, size: f32) -> f32 {
        let cell = self.grid_config.cell_size.max(0.05);
        let sign = if size < 0.0 { -1.0 } else { 1.0 };
        let snapped_cells = (size.abs() / cell).round().max(1.0);
        sign * snapped_cells * cell
    }

    fn quick_build_effective_scale_2d(&self) -> Option<[f32; 3]> {
        let base_scale = self.quick_build_preview_scale?;
        let preview_kind = self.quick_build_preview_kind.as_deref()?;

        if self.ctrl_held && preview_kind == "scenario" {
            Some([
                self.snap_size_to_grid_2d(base_scale[0]),
                self.snap_size_to_grid_2d(base_scale[1]),
                base_scale[2],
            ])
        } else {
            Some(base_scale)
        }
    }

    fn quick_build_snap_position_2d(&self, wx: f32, wy: f32) -> Option<(f32, f32)> {
        if !self.ctrl_held {
            return None;
        }
        let ghost_id = self.quick_build_ghost_id?;
        let preview_path = self.quick_build_preview_path.as_deref()?;
        let preview_kind = self.quick_build_preview_kind.as_deref()?;
        let effective_scale = self.quick_build_effective_scale_2d()?;
        let ghost_half_w = effective_scale[0].abs() * 0.5;
        let ghost_half_h = effective_scale[1].abs() * 0.5;

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
            let entity_scale_x = if preview_kind == "scenario" {
                self.snap_size_to_grid_2d(t.scale.x)
            } else {
                t.scale.x
            };
            let entity_scale_y = if preview_kind == "scenario" {
                self.snap_size_to_grid_2d(t.scale.y)
            } else {
                t.scale.y
            };
            let entity_half_w = entity_scale_x.abs() * 0.5;
            let entity_half_h = entity_scale_y.abs() * 0.5;

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
                let effective_scale = self.quick_build_effective_scale_2d();
                if let Some(ghost_id) = self.quick_build_ghost_id {
                    if let Some(t) = self.world.get_mut::<Transform>(ghost_id) {
                        if let Some(scale) = effective_scale {
                            t.scale = GlamVec3::new(scale[0], scale[1], scale[2]);
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

        let cache_entry =
            if let Some(cached_entry) = self.anim_texture_cache.get(&cache_key) {
                *cached_entry
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
                let tight_bounds = compute_tight_bounds(&processed);
                let cache_entry = AnimTextureCacheEntry {
                    uv_rect: uv_packed,
                    img_width: w,
                    img_height: h,
                    tight_bounds,
                };
                self.anim_texture_cache.insert(cache_key, cache_entry);
                log::debug!("[play_animation_frame] frame cargado al atlas (cache miss): {path}");
                cache_entry
            };

        let uv_rect = cache_entry.uv_rect;
        let img_width = cache_entry.img_width;
        let img_height = cache_entry.img_height;

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

        // ── Aplicar pivot / escala ───────────────────────────────────────────
        // En escenarios con src_rect (sprite sheet), si no ajustamos escala al
        // recorte activo se conserva la proporción del PNG completo y el frame
        // queda estirado. Ajustamos solo escala para preservar la posición de
        // colocación (quick build/grid) y evitar saltos visuales.
        if is_scenario && logical_w > 0 && logical_h > 0 {
            if let Some(t) = self.world.get_mut::<Transform>(id) {
                let world_per_px = t.scale.y / logical_h.max(1) as f32;
                t.scale = GlamVec3::new(
                    img_width as f32 * world_per_px,
                    img_height as f32 * world_per_px,
                    1.0,
                );
            }
        }

        if is_character && logical_w > 0 && logical_h > 0 {
            if let Some(transform) = self.world.get::<Transform>(id).cloned() {
                let saved = self.anim_saved_transforms
                    .entry(id)
                    .or_insert((transform.position, transform.scale));

                let orig_pos = saved.0;
                let orig_scale = saved.1;

                // En personajes usamos un factor uniforme por alto lógico para que
                // frames más anchos (ej. ataque) expandan visualmente sin aplastarse.
                let ref_h_px     = logical_h.max(1) as f32;
                let world_per_px = orig_scale.y / ref_h_px;
                let new_scale_x  = img_width  as f32 * world_per_px;
                let new_scale_y  = img_height as f32 * world_per_px;
                let offset_x     =  (pivot_x - img_width  as f32 * 0.5) * world_per_px;
                let offset_y     = -(pivot_y - img_height as f32 * 0.5) * world_per_px;

                if let Some(t) = self.world.get_mut::<Transform>(id) {
                    t.scale = GlamVec3::new(new_scale_x, new_scale_y, 1.0);
                    // Entidades con física: t.position es el body position (pivot point),
                    // el offset visual se aplica al renderizar vía visual_offsets.
                    // Entidades sin física: ajuste directo de posición para backward compat.
                    if self.physics_2d.has_physics(id) {
                        let vis_offset = GlamVec3::new(-offset_x, -offset_y, 0.0);
                        self.visual_offsets.insert(id, vis_offset);
                    } else {
                        t.position = orig_pos - GlamVec3::new(offset_x, offset_y, 0.0);
                    }
                }

                // Actualizar collider por frame solo fuera de gameplay.
                // En preview/juego el collider del personaje debe ser estable
                // (estilo CharacterBody2D de Godot) para evitar atravesar
                // paredes cuando los frames cambian tight-bounds/offset.
                if self.physics_2d.has_physics(id) && !self.preview_playing {
                    let bounds = cache_entry.tight_bounds.unwrap_or([0, 0, cache_entry.img_width.max(1), cache_entry.img_height.max(1)]);
                    if let Some(transform) = self.world.get::<Transform>(id) {
                        let world_per_px = transform.scale.y / cache_entry.img_height.max(1) as f32;
                        let bx = bounds[0] as f32;
                        let by = bounds[1] as f32;
                        let bw = bounds[2] as f32;
                        let bh = bounds[3] as f32;
                        let half_ext = [
                            bw * 0.5 * world_per_px,
                            bh * 0.5 * world_per_px,
                            0.01,
                        ];
                        let col_off = [
                            (bx + bw * 0.5 - cache_entry.img_width as f32 * 0.5) * world_per_px,
                            (cache_entry.img_height as f32 * 0.5 - by - bh * 0.5) * world_per_px,
                            0.0,
                        ];
                        // En esta fase saneamos el nombre, no la semantica:
                        // seguimos preservando la forma del collider una vez creada
                        // y solo resincronizamos el offset visual/logico.
                        self.physics_2d
                            .sync_entity_collider_offset_preserving_shape(id, half_ext, col_off);
                    }
                }
            }
        }
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
        self.anim_texture_cache.insert(cache_key, AnimTextureCacheEntry {
            uv_rect: uv,
            img_width: w,
            img_height: h,
            // tight_bounds solo se calcula aquí (preload/edición), nunca en hot path.
            tight_bounds: compute_tight_bounds(&processed),
        });
        log::debug!("[preload] frame pre-empacado en atlas: {path}");
    }

    /// Restaura el sprite original de una entidad después de una animación.
pub(crate) fn restore_animation_frame(&mut self, id: u32) {
        let is_scenario = self.scenario_entities.contains(&id);
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
        // Limpiar el visual offset para que el render use t.position directamente.
        self.visual_offsets.remove(&id);
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
                let uv = if let Some(cached_entry) = self.anim_texture_cache.get(&cache_key) {
                    cached_entry.uv_rect
                } else {
                    let u = self.atlas.pack(&self.queue, &img, img_w, img_h);
                    self.anim_texture_cache.insert(cache_key, AnimTextureCacheEntry {
                        uv_rect: u,
                        img_width: img_w,
                        img_height: img_h,
                        // tight_bounds solo se calcula aquí (preload/edición), nunca en hot path.
                        tight_bounds: compute_tight_bounds(&img),
                    });
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
                let scale = self.quick_build_effective_scale_2d()
                    .or(self.quick_build_preview_scale)
                    .unwrap_or([1.0, 1.0, 1.0]);
                send_event(&EngineEvent::QuickBuildClick { x: cx, y: cy, fit_to_grid, scale });
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
        // Usamos cuboid estático (AABB) en lugar de hull convexo,
        // porque el backend físico puede rechazar hulls coplanares (z=0).
        // IMPORTANTE: forzar z=0 en la posición física. create_box_entity devuelve
        // z=-0.5 para el orden de render, pero la simulación física usa XYZ interno:
        // si colisionador y personaje tienen z distinto no detectan contacto.
        self.physics_2d.set_entity_physics(
            entity, true, "static",
            [pos[0], pos[1], 0.0],
            [scale[0] * 0.5, scale[1] * 0.5, 0.01],
            [0.0, 0.0, 0.0],
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
    ///
    /// Contrato actual: usa AABB basado en `Transform` crudo.
    /// No consulta `visual_offsets` ni la forma real de Rapier; cambiar eso puede
    /// modificar gameplay/scripts existentes y debe tratarse como una segunda fase.
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

}

pub(crate) fn compute_tight_bounds(img: &image::RgbaImage) -> Option<[u32; 4]> {
    let (width, height) = img.dimensions();
    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = 0;
    let mut max_y = 0;
    let mut found = false;

    for y in 0..height {
        for x in 0..width {
            if img.get_pixel(x, y).0[3] == 0 {
                continue;
            }
            found = true;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }

    if !found {
        return None;
    }

    Some([
        min_x,
        min_y,
        max_x.saturating_sub(min_x).saturating_add(1),
        max_y.saturating_sub(min_y).saturating_add(1),
    ])
}

pub(crate) fn current_character_cache_entry(state: &State, entity_id: u32) -> Option<AnimTextureCacheEntry> {
    let (anim_name, frame_index) = if let Some(active) = state.active_animations.get(&entity_id) {
        (active.animation_name.clone(), active.current_frame)
    } else {
        let default_name = state.default_animation_by_entity.get(&entity_id)?.clone();
        (default_name, 0)
    };

    let anim = state.animations.get(&entity_id)?.get(&anim_name)?;
    let frame = anim.frames.get(frame_index.min(anim.frames.len().saturating_sub(1)))?;
    let cache_key = if let Some((sx, sy, sw, sh)) = frame.src_x.zip(frame.src_y).zip(frame.src_w.zip(frame.src_h)).map(|((x, y), (w, h))| (x, y, w, h)) {
        format!("{}#{}:{}:{}:{}", frame.path, sx, sy, sw, sh)
    } else {
        frame.path.clone()
    };
    state.anim_texture_cache.get(&cache_key).copied()
}

pub(crate) fn character_collision_shape(state: &State, entity_id: u32) -> Option<([f32; 3], [f32; 3])> {
    let transform = state.world.get::<Transform>(entity_id)?;
    let marker = state.world.get::<CharacterMarker>(entity_id)?;

    let cache_entry = current_character_cache_entry(state, entity_id);
    let (img_width, img_height, bounds) = if let Some(entry) = cache_entry {
        (
            entry.img_width,
            entry.img_height,
            entry.tight_bounds.unwrap_or([0, 0, entry.img_width.max(1), entry.img_height.max(1)]),
        )
    } else {
        (
            marker.img_width,
            marker.img_height,
            marker.tight_bounds.unwrap_or([0, 0, marker.img_width.max(1), marker.img_height.max(1)]),
        )
    };

    let world_per_px = if img_height == 0 {
        0.0
    } else {
        transform.scale.y / img_height as f32
    };

    // El collider se centra en el tight_bounds dentro del quad visual:
    // offset = tight_bounds_center - image_center (en espacio de imagen → mundo)
    let bx = bounds[0] as f32;
    let by = bounds[1] as f32;
    let bw = bounds[2] as f32;
    let bh = bounds[3] as f32;

    let half_ext = [
        bw * 0.5 * world_per_px,
        bh * 0.5 * world_per_px,
        0.01,
    ];
    let collider_offset = [
        (bx + bw * 0.5 - img_width as f32 * 0.5) * world_per_px,
        (img_height as f32 * 0.5 - by - bh * 0.5) * world_per_px,
        0.0,
    ];

    Some((half_ext, collider_offset))
}
