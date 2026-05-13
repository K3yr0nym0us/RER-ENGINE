use glam::Vec3 as GlamVec3;

use crate::ecs::{MeshComponent, Transform};
use crate::ipc::{send_event, EngineEvent};
use crate::texture::GpuTexture;

use super::{CharacterMarker, ScenarioMarker, compute_tight_bounds, create_quad_xy};
use crate::engine::State;

impl State {
    // ── Inicialización ────────────────────────────────────────────────────────

    /// Configura la escena 2D de plataformas con un único rectángulo (player).
    pub(crate) fn setup_2d_platformer(&mut self) {
        self.reset_runtime_scene_2d();

        // Quad unitario canónico — compartido por TODOS los sprites 2D de la escena.
        // El Transform de cada entidad lo escala y posiciona correctamente.
        let canonical_quad = create_quad_xy(&self.device, 0.0, 0.0, 1.0, 1.0, "canonical-quad");
        self.meshes.push(canonical_quad);
        self.canonical_quad_idx = 0;

        // -- Cámara ortográfica -----------------------------------------------
        self.camera_2d = Some(super::Camera2D {
            x:      0.0,
            y:      0.0,
            half_h: 3.5,
            near:  -100.0,
            far:    100.0,
        });

        // Fondo oscuro azulado (estilo Hollow Knight)
        self.clear_color = wgpu::Color { r: 0.04, g: 0.04, b: 0.10, a: 1.0 };

        log::info!("Escena 2D cargada: plataformer vista lateral");
    }

    // ── Escenario PNG de fondo ────────────────────────────────────────────────

    /// Carga una imagen PNG del disco y la registra como entidad ECS de escenario.
    /// La entidad se posiciona en Z=-1 (detrás de todo), mantiene las proporciones
    /// de la imagen y puede seleccionarse, arrastrarse y escalarse como cualquier entidad.
    pub(crate) fn load_scenario(&mut self, path: &str) {
        let bytes = match std::fs::read(path) {
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
        // tight_bounds solo se calcula aquí (preload/edición), nunca en hot path.
        let tight_bounds = compute_tight_bounds(&img);

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
        self.world.insert(ch_id, CharacterMarker { img_width, img_height, base_world_h, tight_bounds, path: path.to_owned() });
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
                // Mantener este comportamiento por ahora: la escala visual cambia,
                // pero el collider no se recompone automaticamente en esta fase.
                t.scale = GlamVec3::new(new_w, new_h, 1.0);
            }
        }
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
