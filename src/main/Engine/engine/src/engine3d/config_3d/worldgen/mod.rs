//! Consumidor del plugin de generación de terreno en el `State` del engine.
//!
//! División de responsabilidades (contrato del plan):
//! - **La DLL (`rer-worldgen`) produce**: densidad, Marching Cubes, chunks,
//!   splats. No hay ninguna lógica de generación en el engine.
//! - **El engine consume**: subida a GPU con presupuesto por frame, entidades
//!   ECS, eventos de progreso y limpieza.
//!
//! El flujo es: `world_gen_set_library_path` (ruta de la DLL instalada por el
//! editor) → `world_gen_set_config` (JSON camelCase) → `world_gen_start` →
//! `worldgen_tick` avanza la cola → eventos `world_gen_*` hacia el editor.

mod abi;
mod loader;
mod types;

pub(crate) use types::CHUNKS_PER_FRAME;

use std::collections::HashMap;

use glam::Vec3;
use serde::Deserialize;

use crate::engine3d::config_3d::worldgen::abi::{Status, WgMeshBuffers};
use crate::engine3d::config_3d::worldgen::loader::WorldgenLibrary;
use crate::engine3d::config_3d::worldgen::types::{
    ChunkKey, PendingChunk, WgWorldHandle, WorldgenChunk, WorldgenPlacement, WorldgenStatus,
};
use crate::engine3d::ecs::{MeshComponent, NonSelectable, SurfacePbr, Transform};
use crate::engine3d::ipc::{EngineEvent, send_event};

/// Campos mínimos de `WorldGenConfig` que el engine necesita para planificar la
/// rejilla de chunks (el resto de la config solo lo interpreta la DLL).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WgConfigLite {
    bounds: WgBoundsLite,
    resolution: WgResolutionLite,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WgBoundsLite {
    width: f32,
    length: f32,
    height: f32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WgResolutionLite {
    voxel_size: f32,
}

impl super::super::engine::State {
    // -- Comandos IPC ---------------------------------------------------------

    /// `world_gen_set_library_path`: carga la DLL del plugin y aplica la config
    /// pendiente si ya se envió. Nunca degrada el motor: los fallos van a eventos.
    pub(crate) fn worldgen_set_library_path(&mut self, path: String) {
        self.worldgen.library_path = Some(path.clone());
        let result = WorldgenLibrary::load(std::path::Path::new(&path));
        match result {
            Ok(library) => {
                self.worldgen.library = Some(library);
                self.worldgen_set_status(WorldgenStatus::Idle);
                send_event(&EngineEvent::WorldGenStatus {
                    state: self.worldgen.status.as_str().to_string(),
                });
                // Con biblioteca y config disponibles, creamos el mundo ya.
                if let Some(config_json) = self.worldgen.config_json.clone() {
                    self.worldgen_apply_config(&config_json);
                }
            }
            Err(message) => {
                self.worldgen.library = None;
                self.worldgen.last_error = Some(message.clone());
                self.worldgen_set_status(WorldgenStatus::LibraryError);
                send_event(&EngineEvent::WorldGenError { message });
            }
        }
    }

    /// `world_gen_set_config`: aplica la config (JSON camelCase de `WorldGenConfig`)
    /// creando el handle del mundo en la DLL.
    pub(crate) fn worldgen_set_config(&mut self, config_json: String) {
        self.worldgen.config_json = Some(config_json.clone());
        if self.worldgen.library.is_none() {
            // La config se aplicará cuando llegue la ruta del plugin.
            self.worldgen_set_status(WorldgenStatus::MissingLibrary);
            return;
        }
        self.worldgen_apply_config(&config_json);
    }

    /// `world_gen_start`: planifica la cola de chunks activos (Y-layer pruning vía
    /// `wg_chunk_status`) y ordena por distancia al centro del mundo.
    pub(crate) fn worldgen_start(&mut self) {
        let Some(library) = self.worldgen.library.as_ref() else {
            send_event(&EngineEvent::WorldGenError {
                message: "world_gen_start sin plugin cargado".to_string(),
            });
            return;
        };
        let Some(handle) = self.worldgen.world.as_ref() else {
            send_event(&EngineEvent::WorldGenError {
                message: "world_gen_start sin config aplicada".to_string(),
            });
            return;
        };

        let Some(config) = self
            .worldgen
            .config_json
            .as_deref()
            .and_then(|json| serde_json::from_str::<WgConfigLite>(json).ok())
        else {
            send_event(&EngineEvent::WorldGenError {
                message: "world_gen_start: config ilegible".to_string(),
            });
            return;
        };

        let (chunks_x, chunks_z, max_y) = chunk_grid(&config);
        // Anclas de LOD: se ordena por proximidad a la cámara/viewport activo.
        let camera_pos = self.camera_world_position();

        let mut pending: Vec<PendingChunk> = Vec::new();
        for cz in 0..chunks_z {
            for cx in 0..chunks_x {
                for cy in 0..=max_y {
                    // # Seguridad: handle válido de esta DLL, hilo del engine.
                    let status = unsafe { (library.chunk_status)(handle.0, cx, cy, cz) };
                    if status == 1 {
                        pending.push(PendingChunk {
                            key: (cx, cy, cz),
                            lod: 0,
                        });
                    } else if status < 0 {
                        send_event(&EngineEvent::WorldGenError {
                            message: format!("wg_chunk_status({cx}, {cy}, {cz}) inválido"),
                        });
                        return;
                    }
                }
            }
        }

        // Orden: primero capas superiores (superficie visible), luego cercanía a
        // la cámara (el streaming avanza hacia fuera); LOD por distancia.
        let cell = crate::engine3d::config_3d::worldgen::abi::CHUNK_SIZE as f32
            * config.resolution.voxel_size;
        pending.sort_by(|a, b| {
            let score = |c: &PendingChunk| -> (i32, f32) {
                let chunk_center = Vec3::new(
                    (c.key.0 as f32 + 0.5) * cell,
                    (c.key.1 as f32 + 0.5) * cell,
                    (c.key.2 as f32 + 0.5) * cell,
                );
                (-c.key.1, chunk_center.distance(camera_pos))
            };
            let (layer_a, dist_a) = score(a);
            let (layer_b, dist_b) = score(b);
            layer_a.cmp(&layer_b).then(dist_a.total_cmp(&dist_b))
        });
        // LOD por anillos de distancia a la cámara (F9): cercano = completo.
        for chunk in &mut pending {
            let chunk_center = Vec3::new(
                (chunk.key.0 as f32 + 0.5) * cell,
                (chunk.key.1 as f32 + 0.5) * cell,
                (chunk.key.2 as f32 + 0.5) * cell,
            );
            let d = chunk_center.distance(camera_pos);
            chunk.lod = if d < 120.0 {
                0
            } else if d < 320.0 {
                1
            } else {
                2
            };
        }

        self.worldgen.total_chunks = pending.len() as u32;
        self.worldgen.completed_chunks = 0;
        self.worldgen.pending = pending;
        self.worldgen_set_status(WorldgenStatus::Generating);
        send_event(&EngineEvent::WorldGenStatus {
            state: self.worldgen.status.as_str().to_string(),
        });
        send_event(&EngineEvent::WorldGenProgress {
            completed: 0,
            total: self.worldgen.total_chunks,
        });
    }

    /// `world_gen_stop`: limpia la cola y libera el mundo de la DLL (los chunks
    /// ya en escena se mantienen hasta `world_gen_reset`).
    pub(crate) fn worldgen_stop(&mut self) {
        self.worldgen.pending.clear();
        self.worldgen.total_chunks = 0;
        self.worldgen.completed_chunks = 0;
        if self.worldgen.library.is_some() && self.worldgen.config_json.is_some() {
            self.worldgen_set_status(WorldgenStatus::Ready);
        } else if self.worldgen.library.is_some() {
            self.worldgen_set_status(WorldgenStatus::Idle);
        } else {
            self.worldgen_set_status(WorldgenStatus::MissingLibrary);
        }
        send_event(&EngineEvent::WorldGenStatus {
            state: self.worldgen.status.as_str().to_string(),
        });
    }

    /// `world_gen_reset`: elimina los chunks de terreno en escena y libera el
    /// handle de la DLL.
    pub(crate) fn worldgen_reset(&mut self) {
        self.worldgen_stop();
        let keys: Vec<ChunkKey> = self.worldgen.spawned.keys().copied().collect();
        for key in keys {
            if let Some(chunk) = self.worldgen.spawned.remove(&key) {
                self.world.despawn(chunk.entity_id);
            }
        }
        if let Some(handle) = self.worldgen.world.take()
            && let Some(library) = self.worldgen.library.as_ref()
        {
            // # Seguridad: handle creado por esta DLL; no se reutiliza.
            unsafe { (library.world_free)(handle.0) };
        }
        self.worldgen.world = None;
        self.worldgen.placements.clear();
    }

    /// `world_gen_scatter`: distribuye un blueprint según las reglas y coloca las
    /// instancias por la vía existente (`spawn_cached_model_part_at` con
    /// `blueprint_id`), compartiendo los recursos GPU del blueprint.
    pub(crate) fn worldgen_scatter(&mut self, rules_json: &str) {
        // 1) Validaciones previas (sin llamar a la DLL todavía).
        let blueprint_id = serde_json::from_str::<serde_json::Value>(rules_json)
            .ok()
            .and_then(|v| {
                v.get("blueprintId")
                    .and_then(|b| b.as_str())
                    .map(String::from)
            });
        let Some(blueprint_id) = blueprint_id else {
            send_event(&EngineEvent::WorldGenError {
                message: "world_gen_scatter: falta blueprintId en las reglas".to_string(),
            });
            return;
        };
        let Some(meta) = self.blueprint_registry.get(&blueprint_id).cloned() else {
            send_event(&EngineEvent::WorldGenError {
                message: format!("world_gen_scatter: blueprint «{blueprint_id}» no registrado"),
            });
            return;
        };
        let Some(model_path) = meta.model.clone() else {
            send_event(&EngineEvent::WorldGenError {
                message: format!("world_gen_scatter: blueprint «{blueprint_id}» sin modelo"),
            });
            return;
        };
        // Los recursos del blueprint se comparten: 1 caché → N instancias.
        if let Err(error) = self.ensure_static_model_cached(&model_path) {
            send_event(&EngineEvent::WorldGenError {
                message: format!("world_gen_scatter: {error}"),
            });
            return;
        }
        let Some(parts) = self.cached_static_model_parts(&model_path) else {
            send_event(&EngineEvent::WorldGenError {
                message: format!("world_gen_scatter: modelo sin partes: {model_path}"),
            });
            return;
        };
        let Some(part) = parts.first().copied() else {
            send_event(&EngineEvent::WorldGenError {
                message: format!("world_gen_scatter: modelo vacío: {model_path}"),
            });
            return;
        };

        // 2) Llamada a la DLL con préstamo de solo lectura (termina aquí).
        let Some(library) = self.worldgen.library.as_ref() else {
            send_event(&EngineEvent::WorldGenError {
                message: "world_gen_scatter sin plugin cargado".to_string(),
            });
            return;
        };
        let Some(handle) = self.worldgen.world.as_ref() else {
            send_event(&EngineEvent::WorldGenError {
                message: "world_gen_scatter sin config aplicada".to_string(),
            });
            return;
        };
        let mut out = crate::engine3d::config_3d::worldgen::abi::WgScatterPlacements::empty();
        // # Seguridad: handle válido y buffers del contrato ABI.
        let status =
            unsafe { (library.scatter)(handle.0, rules_json.as_ptr(), rules_json.len(), &mut out) };
        if status != crate::engine3d::config_3d::worldgen::abi::Status::Ok as i32 {
            let message = format!(
                "wg_scatter({blueprint_id}) = {status:?}: {}",
                library.last_error_text(handle.0)
            );
            self.worldgen.last_error = Some(message.clone());
            send_event(&EngineEvent::WorldGenError { message });
            return;
        }
        // # Seguridad: se copia antes de liberar (allocator único).
        let placements: Vec<crate::engine3d::config_3d::worldgen::abi::WgPlacement> =
            unsafe { std::slice::from_raw_parts(out.data, out.len) }.to_vec();
        unsafe { (library.free_scatter)(&out) };
        // El préstamo de `library` termina aquí; a partir de este punto se
        // puede mutar `self` para colocar las entidades.

        // 3) Colocación por la vía del blueprint (física, undo off, categoría).
        let display = meta
            .template_name
            .clone()
            .unwrap_or_else(|| blueprint_id.clone());
        // Registrar las reglas para la persistencia (F8): misma regla por id se
        // sustituye.
        if let Some(slot) = self
            .worldgen
            .scatter_rules
            .iter_mut()
            .find(|(id, _)| *id == blueprint_id)
        {
            slot.1 = rules_json.to_string();
        } else {
            self.worldgen
                .scatter_rules
                .push((blueprint_id.clone(), rules_json.to_string()));
        }
        for (i, placement) in placements.iter().enumerate() {
            let rotation = glam::Quat::from_rotation_y(placement.yaw).to_array();
            let entity_id = self.spawn_cached_model_part_at(
                part.mesh_idx,
                part.tex_idx,
                &model_path,
                placement.position,
                rotation,
                placement.scale,
                &format!("{display} #{i}"),
                "model",
                Some(blueprint_id.clone()),
                meta.category.clone(),
                meta.physics_enabled
                    .unwrap_or(meta.colision.unwrap_or(false)),
                meta.physics_type.as_deref().unwrap_or("dynamic"),
                part.local_bounds,
                None,
                part.roughness,
                part.metallic,
                part.ior,
                false,
            );
            self.worldgen.placements.push(WorldgenPlacement {
                blueprint_id: blueprint_id.clone(),
                position: placement.position,
                rotation,
                scale: placement.scale,
            });
            let _ = entity_id;
        }
        send_event(&EngineEvent::WorldGenScatterDone {
            blueprint_id,
            count: placements.len() as u32,
        });
    }

    /// Restaura la sección `worldGen3d` del `.save` (F8): config + colocaciones.
    ///
    /// La geometría NO se persiste: se regenera desde seed+config si el plugin
    /// está cargado (arranque automático de la cola con presupuesto por frame).
    /// Las colocaciones de blueprints se recolocan por el registro existente,
    /// sin depender de la DLL.
    pub(crate) fn restore_worldgen_3d(
        &mut self,
        project: &crate::engine3d::engine::load_proyect::ProjectSaveData,
    ) {
        let Some(section) = &project.worldGen3d else {
            return;
        };

        // 1) Config (se aplica si la DLL está cargada; si no, queda pendiente).
        if let Some(config) = section.get("config") {
            self.worldgen_set_config(config.to_string());
        }

        // 2) Reglas de scatter guardadas (para re-scatters futuros).
        if let Some(rules) = section.get("scatterRules").and_then(|r| r.as_array()) {
            for rule in rules {
                let Some(id) = rule.get("blueprintId").and_then(|v| v.as_str()) else {
                    continue;
                };
                self.worldgen
                    .scatter_rules
                    .push((id.to_string(), rule.to_string()));
            }
        }

        // 3) Colocaciones guardadas: recolocación por el registro de blueprints.
        let placements = section
            .get("placements")
            .and_then(|p| p.as_array())
            .cloned()
            .unwrap_or_default();
        for placement in &placements {
            let (Some(blueprint_id), Some(position)) = (
                placement.get("blueprintId").and_then(|v| v.as_str()),
                placement
                    .get("position")
                    .and_then(|v| serde_json::from_value::<[f32; 3]>(v.clone()).ok()),
            ) else {
                continue;
            };
            let rotation = placement
                .get("rotation")
                .and_then(|v| serde_json::from_value::<[f32; 4]>(v.clone()).ok())
                .unwrap_or([0.0, 0.0, 0.0, 1.0]);
            let scale = placement
                .get("scale")
                .and_then(|v| serde_json::from_value::<[f32; 3]>(v.clone()).ok())
                .unwrap_or([1.0, 1.0, 1.0]);

            let Some(meta) = self.blueprint_registry.get(blueprint_id).cloned() else {
                log::warn!(
                    "[worldgen] colocación omitida: blueprint «{blueprint_id}» no registrado"
                );
                continue;
            };
            let Some(model_path) = meta.model.clone() else {
                continue;
            };
            if let Err(error) = self.ensure_static_model_cached(&model_path) {
                log::warn!("[worldgen] colocación sin modelo («{blueprint_id}»): {error}");
                continue;
            }
            let Some(parts) = self.cached_static_model_parts(&model_path) else {
                continue;
            };
            let Some(part) = parts.first().copied() else {
                continue;
            };
            let display = meta
                .template_name
                .clone()
                .unwrap_or_else(|| blueprint_id.to_string());
            self.spawn_cached_model_part_at(
                part.mesh_idx,
                part.tex_idx,
                &model_path,
                position,
                rotation,
                scale,
                &format!("{display} (restaurado)"),
                "model",
                Some(blueprint_id.to_string()),
                meta.category.clone(),
                meta.physics_enabled
                    .unwrap_or(meta.colision.unwrap_or(false)),
                meta.physics_type.as_deref().unwrap_or("dynamic"),
                part.local_bounds,
                None,
                part.roughness,
                part.metallic,
                part.ior,
                false,
            );
            self.worldgen.placements.push(WorldgenPlacement {
                blueprint_id: blueprint_id.to_string(),
                position,
                rotation,
                scale,
            });
        }

        // 4) Generación del terreno: automática si el plugin está cargado.
        if self.worldgen.library.is_some() && self.worldgen.world.is_some() {
            self.worldgen_start();
        }
        send_event(&EngineEvent::WorldGenStatus {
            state: self.worldgen.status.as_str().to_string(),
        });
    }

    // -- Tick por frame -------------------------------------------------------

    /// Avanza la cola de chunks con presupuesto [`CHUNKS_PER_FRAME`]. Se llama
    /// desde `State::update`.
    pub(crate) fn worldgen_tick(&mut self) {
        if self.worldgen.status != WorldgenStatus::Generating {
            return;
        }
        for _ in 0..CHUNKS_PER_FRAME {
            let Some(pending) = self.worldgen.pending.pop() else {
                self.worldgen_set_status(WorldgenStatus::Complete);
                send_event(&EngineEvent::WorldGenStatus {
                    state: self.worldgen.status.as_str().to_string(),
                });
                send_event(&EngineEvent::WorldGenCompleted {
                    chunks: self.worldgen.completed_chunks,
                });
                return;
            };
            if self.worldgen_spawn_chunk(pending.key, pending.lod).is_err() {
                return;
            }
        }
        send_event(&EngineEvent::WorldGenProgress {
            completed: self.worldgen.completed_chunks,
            total: self.worldgen.total_chunks,
        });
    }

    // -- Internos -------------------------------------------------------------

    fn worldgen_set_status(&mut self, status: WorldgenStatus) {
        self.worldgen.status = status;
    }

    /// Crea (o recrea) el handle del mundo con la config actual.
    fn worldgen_apply_config(&mut self, config_json: &str) {
        // Libera el mundo anterior si lo hubiera (cambio de config).
        if let Some(old) = self.worldgen.world.take()
            && let Some(library) = self.worldgen.library.as_ref()
        {
            // # Seguridad: handle creado por esta DLL; no se reutiliza.
            unsafe { (library.world_free)(old.0) };
        }
        self.worldgen.pending.clear();
        self.worldgen.spawned.clear();
        self.worldgen.total_chunks = 0;
        self.worldgen.completed_chunks = 0;

        let Some(library) = self.worldgen.library.as_ref() else {
            return;
        };
        // # Seguridad: buffer de solo lectura de config_json.len() bytes.
        let handle = unsafe { (library.world_create)(config_json.as_ptr(), config_json.len()) };
        if handle.is_null() {
            let message = library.last_error_text(std::ptr::null());
            self.worldgen.last_error = Some(message.clone());
            self.worldgen_set_status(WorldgenStatus::LibraryError);
            send_event(&EngineEvent::WorldGenError { message });
            return;
        }
        self.worldgen.world = Some(WgWorldHandle(handle));
        self.worldgen_set_status(WorldgenStatus::Ready);
        send_event(&EngineEvent::WorldGenStatus {
            state: self.worldgen.status.as_str().to_string(),
        });
    }

    /// Genera, meshea y sube un chunk a la escena. Devuelve Err si hay que
    /// abortar la cola (error de la DLL).
    fn worldgen_spawn_chunk(&mut self, key: ChunkKey, lod: u32) -> Result<(), ()> {
        let (cx, cy, cz) = key;
        let pending_lod = lod;
        let Some(library) = self.worldgen.library.as_ref() else {
            return Err(());
        };
        let Some(handle) = self.worldgen.world.as_ref() else {
            return Err(());
        };

        let mut buffers = WgMeshBuffers::empty();
        // # Seguridad: handle válido de esta DLL y struct del contrato ABI.
        let status =
            unsafe { (library.mesh_chunk)(handle.0, cx, cy, cz, pending_lod, &mut buffers) };
        if status != Status::Ok as i32 {
            let message = format!(
                "wg_mesh_chunk({cx}, {cy}, {cz}) = {status:?}: {}",
                library.last_error_text(handle.0)
            );
            self.worldgen.last_error = Some(message.clone());
            self.worldgen_set_status(WorldgenStatus::LibraryError);
            send_event(&EngineEvent::WorldGenError { message });
            return Err(());
        }

        // Copiar los buffers a memoria propia ANTES de liberarlos (allocator único).
        let vertices: Vec<crate::engine3d::mesh::Vertex> = if buffers.vertex_count == 0 {
            Vec::new()
        } else {
            // # Seguridad: punteros válidos para `vertex_count`/`index_count` elementos.
            unsafe {
                let positions =
                    std::slice::from_raw_parts(buffers.positions, buffers.vertex_count * 3);
                let normals = std::slice::from_raw_parts(buffers.normals, buffers.vertex_count * 3);
                let uvs = std::slice::from_raw_parts(buffers.uvs, buffers.vertex_count * 2);
                (0..buffers.vertex_count)
                    .map(|i| crate::engine3d::mesh::Vertex {
                        position: [positions[i * 3], positions[i * 3 + 1], positions[i * 3 + 2]],
                        normal: [normals[i * 3], normals[i * 3 + 1], normals[i * 3 + 2]],
                        uv: [uvs[i * 2], uvs[i * 2 + 1]],
                    })
                    .collect()
            }
        };
        let indices: Vec<u32> = if buffers.index_count == 0 {
            Vec::new()
        } else {
            // # Seguridad: ver arriba.
            unsafe { std::slice::from_raw_parts(buffers.indices, buffers.index_count) }.to_vec()
        };
        // # Seguridad: buffers entregados por wg_mesh_chunk en esta misma DLL.
        unsafe { (library.free_mesh_buffers)(&buffers) };

        self.worldgen.completed_chunks += 1;
        if vertices.is_empty() || indices.is_empty() {
            // Chunk sin superficie: cuenta como completado sin entidad.
            send_event(&EngineEvent::WorldGenChunkReady {
                cx,
                cy,
                cz,
                entity_id: 0,
            });
            return Ok(());
        }

        let tex_idx = match worldgen_pack_column_texture(
            &mut self.texture_array,
            &self.vulkan,
            library,
            handle.0,
            &mut self.worldgen.columns,
            cx,
            cz,
        ) {
            Some(tex_idx) => tex_idx,
            None => self.worldgen_ensure_fallback_texture(),
        };
        let label = format!("terrain-{cx}-{cy}-{cz}");
        let mesh = crate::engine3d::mesh::upload(&self.vulkan, &vertices, &indices, &label, None);
        let mesh_idx = self.meshes.len();
        self.meshes.push(mesh);

        let entity_id = self.world.spawn(Some("TerrainChunk"));
        self.world
            .insert(entity_id, MeshComponent { mesh_idx, tex_idx });
        self.world.insert(
            entity_id,
            Transform {
                position: Vec3::ZERO,
                rotation: glam::Quat::IDENTITY,
                scale: Vec3::ONE,
            },
        );
        // Terreno: no seleccionable en el editor; geometría en coordenadas de mundo
        // (transform identidad).
        self.world.insert(entity_id, NonSelectable);
        self.world.insert(
            entity_id,
            SurfacePbr {
                roughness: 0.95,
                metallic: 0.0,
                ior: 0.0,
                opacity: 1.0,
            },
        );
        // Sin registro en save_registry: los chunks se regeneran desde seed+config
        // (F8); persistirlos duplicaría la geometría en el manifest.
        self.worldgen
            .spawned
            .insert(key, WorldgenChunk { entity_id });
        send_event(&EngineEvent::WorldGenChunkReady {
            cx,
            cy,
            cz,
            entity_id,
        });
        Ok(())
    }

    /// Textura de fallback compartida (checker) si la DLL no entrega albedo.
    fn worldgen_ensure_fallback_texture(&mut self) -> usize {
        if let Some(tex_idx) = self.worldgen.fallback_tex_idx {
            return tex_idx;
        }
        let tex_idx = self.pack_scene_checker_texture();
        self.worldgen.fallback_tex_idx = Some(tex_idx);
        tex_idx
    }
}

/// Obtiene (y cachea) la textura de splat/albedo de una columna vía la DLL.
///
/// Función libre para respetar el préstamo por campos de `State` (la DLL está
/// prestada de `self.worldgen.library` mientras `texture_array`/`columns` se
/// mutan, y son campos distintos).
fn worldgen_pack_column_texture(
    texture_array: &mut crate::engine3d::texture::TextureArray,
    vulkan: &crate::engine3d::vulkan::SharedGpu,
    library: &WorldgenLibrary,
    handle: *mut std::ffi::c_void,
    columns: &mut HashMap<(i32, i32), usize>,
    cx: i32,
    cz: i32,
) -> Option<usize> {
    if let Some(tex_idx) = columns.get(&(cx, cz)) {
        return Some(*tex_idx);
    }

    let mut buffers = crate::engine3d::config_3d::worldgen::abi::WgChunkTextures::empty();
    // # Seguridad: handle válido de esta DLL y struct del contrato ABI.
    let status = unsafe { (library.chunk_textures)(handle, cx, cz, 0, &mut buffers) };
    if status == crate::engine3d::config_3d::worldgen::abi::Status::Ok as i32
        && !buffers.albedo.is_null()
    {
        // # Seguridad: buffers del contrato; se copian antes de liberar.
        let albedo = unsafe { std::slice::from_raw_parts(buffers.albedo, buffers.len) }.to_vec();
        let dim = buffers.dim;
        unsafe { (library.free_chunk_textures)(&buffers) };

        match texture_array.pack(vulkan, &albedo, dim, dim) {
            Ok(layer) => {
                let tex_idx = layer as usize;
                columns.insert((cx, cz), tex_idx);
                return Some(tex_idx);
            }
            Err(error) => log::warn!("[worldgen] no se pudo empaquetar albedo de columna: {error}"),
        }
    } else if !buffers.albedo.is_null() || !buffers.splat.is_null() {
        // # Seguridad: liberar buffers parciales en error.
        unsafe { (library.free_chunk_textures)(&buffers) };
    }
    None
}

/// Rejilla de chunks que cubren los límites del mundo.
fn chunk_grid(config: &WgConfigLite) -> (i32, i32, i32) {
    let cell = crate::engine3d::config_3d::worldgen::abi::CHUNK_SIZE as f32
        * config.resolution.voxel_size.max(0.001);
    let chunks_x = (config.bounds.width / cell).ceil().max(1.0) as i32;
    let chunks_z = (config.bounds.length / cell).ceil().max(1.0) as i32;
    let max_y = (config.bounds.height / cell).ceil() as i32;
    (chunks_x, chunks_z, max_y)
}

// Reexport interno para que init.rs/engine.rs construyan el campo.
pub(crate) use types::WorldgenState as StateWorldgenState;

#[cfg(test)]
mod tests {
    //! Conformancia real: carga la DLL construida (`cargo build -p rer-worldgen`)
    //! y ejecuta el ciclo completo a través de la frontera FFI. Si la DLL no
    //! existe, el test se salta (el CI puede generarla antes si lo desea).
    use super::abi::Status;
    use super::loader::WorldgenLibrary;

    fn dll_path() -> Option<std::path::PathBuf> {
        if let Ok(path) = std::env::var("RER_WORLDGEN_DLL") {
            let path = std::path::PathBuf::from(path);
            return path.exists().then_some(path);
        }
        // Ruta por defecto del target de release del workspace.
        let candidates = [
            "target/release/rer_worldgen.dll",
            "target/debug/rer_worldgen.dll",
        ];
        for candidate in candidates {
            let path = std::path::PathBuf::from(candidate);
            if path.exists() {
                return Some(path);
            }
        }
        None
    }

    const CONFIG: &str = r#"{
        "seed": 7,
        "bounds": { "width": 256.0, "length": 256.0, "height": 96.0 },
        "resolution": { "voxelSize": 2.0, "lodEnabled": false }
    }"#;

    #[test]
    fn dll_conformance_full_cycle() {
        let Some(path) = dll_path() else {
            eprintln!(
                "DLL de rer-worldgen no encontrada: test omitido (genera con `cargo build -p rer-worldgen`)"
            );
            return;
        };
        let library = WorldgenLibrary::load(&path).expect("la DLL debe cargar y cumplir la ABI");

        // # Seguridad: punteros del contrato ABI, ciclo completo con liberación.
        unsafe {
            let handle = (library.world_create)(CONFIG.as_ptr(), CONFIG.len());
            assert!(
                !handle.is_null(),
                "wg_world_create falló: {}",
                library.last_error_text(std::ptr::null())
            );

            // Chunk (0,0,0) debe contener terreno (bounds.height 96, base ~33.6).
            assert_eq!((library.chunk_status)(handle, 0, 0, 0), 1);

            let mut buffers = super::abi::WgMeshBuffers::empty();
            let status = (library.mesh_chunk)(handle, 0, 0, 0, 0, &mut buffers);
            assert_eq!(
                status,
                Status::Ok as i32,
                "wg_mesh_chunk falló: {}",
                library.last_error_text(handle)
            );
            assert!(buffers.vertex_count > 0);
            assert_eq!(buffers.index_count % 3, 0);

            // La posición del primer vértice debe estar dentro de los límites.
            let x = *buffers.positions;
            let y = *buffers.positions.add(1);
            assert!((-1.0..=257.0).contains(&x), "x={x} fuera de los límites");
            assert!((-1.0..=97.0).contains(&y), "y={y} fuera de los límites");

            (library.free_mesh_buffers)(&buffers);
            (library.world_free)(handle);
        }
    }

    #[test]
    fn dll_rejects_invalid_config() {
        let Some(path) = dll_path() else {
            return;
        };
        let library = WorldgenLibrary::load(&path).expect("la DLL debe cargar");
        let bad = "{ no es json }";
        // # Seguridad: buffer de solo lectura de `bad.len()` bytes.
        let handle = unsafe { (library.world_create)(bad.as_ptr(), bad.len()) };
        assert!(handle.is_null(), "config inválida debe rechazarse");
    }
}
