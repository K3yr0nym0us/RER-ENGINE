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
    ChunkKey, PendingChunk, ScanCursor, WgWorldHandle, WorldgenChunk, WorldgenPlacement,
    WorldgenStatus,
};
use crate::engine3d::ecs::{MeshComponent, MeshLocalAabb, NonSelectable, SurfacePbr, Transform};
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
    ///
    /// Si la ruta ya está cargada no se recarga (el handle seguiría vivo en la
    /// DLL anterior). Si cambia, se libera el mundo con la biblioteca vieja
    /// antes de soltarla.
    pub(crate) fn worldgen_set_library_path(&mut self, path: String) {
        if self.worldgen.library.is_some()
            && self.worldgen.library_path.as_deref() == Some(path.as_str())
        {
            return;
        }
        self.worldgen_despawn_scene();
        self.worldgen_release_world();
        self.worldgen.library = None;
        self.worldgen.pending.clear();
        self.worldgen.scan = None;
        self.worldgen.library_path = Some(path.clone());
        let result = WorldgenLibrary::load(std::path::Path::new(&path));
        match result {
            Ok(library) => {
                self.worldgen.library = Some(library);
                self.worldgen_set_status(WorldgenStatus::Idle);
                send_event(&EngineEvent::WorldGenStatus {
                    state: self.worldgen.status.as_str().to_string(),
                });
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

    /// `world_gen_start`: arranca el barrido de columnas (Y-layer pruning vía
    /// `wg_chunk_status`). El barrido y el orden de la cola avanzan en
    /// `worldgen_tick`, no en este comando.
    pub(crate) fn worldgen_start(&mut self, replace_ground: bool) {
        if self.worldgen.library.is_none() {
            send_event(&EngineEvent::WorldGenError {
                message: "world_gen_start sin plugin cargado".to_string(),
            });
            return;
        }
        if self.worldgen.world.is_none() {
            send_event(&EngineEvent::WorldGenError {
                message: "world_gen_start sin config aplicada".to_string(),
            });
            return;
        }

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

        // El disco del mundo pasa a ser el círculo inscrito en la huella real
        // (rejilla de chunks, no el ancho nominal). El suelo más bajo queda en y = 0.
        self.worldgen.replace_ground = replace_ground;
        self.ensure_terrain_materials();
        let (extent_x, extent_z) = generated_footprint(&config);
        self.worldgen.scene_offset = scene_offset(extent_x, extent_z);
        self.fit_world_to_generator(extent_x, extent_z, &config.bounds);

        let (chunks_x, chunks_z, max_y) = chunk_grid(&config);
        let cell = crate::engine3d::config_3d::worldgen::abi::CHUNK_SIZE as f32
            * config.resolution.voxel_size;
        let camera_pos = self.camera_world_position();

        self.worldgen.pending.clear();
        self.worldgen.total_chunks = 0;
        self.worldgen.completed_chunks = 0;
        self.worldgen.scan = Some(ScanCursor {
            cx: 0,
            cz: 0,
            chunks_x,
            chunks_z,
            max_y,
            cell,
            camera: camera_pos.to_array(),
            collected: Vec::new(),
        });
        self.worldgen_set_status(WorldgenStatus::Generating);
        send_event(&EngineEvent::WorldGenStatus {
            state: self.worldgen.status.as_str().to_string(),
        });
        send_event(&EngineEvent::WorldGenProgress {
            completed: 0,
            total: 0,
        });
    }

    /// `world_gen_stop`: vacía la cola de chunks. El handle de la DLL sigue
    /// vivo hasta `world_gen_reset` o `world_gen_unload`. Los chunks ya en
    /// escena se mantienen hasta el reset.
    pub(crate) fn worldgen_stop(&mut self) {
        self.worldgen.replace_ground = false;
        self.worldgen.pending.clear();
        self.worldgen.scan = None;
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

    /// `world_gen_reset`: elimina terreno y scatter de la escena y libera el
    /// handle de la DLL. La biblioteca sigue cargada.
    pub(crate) fn worldgen_reset(&mut self) {
        self.worldgen_stop();
        self.worldgen_despawn_scene();
        self.worldgen_release_world();
    }

    /// `world_gen_unload`: suelta escena, handle y DLL para poder borrar el
    /// archivo (desactivar o desinstalar el plugin).
    pub(crate) fn worldgen_unload(&mut self) {
        self.worldgen_stop();
        self.worldgen_despawn_scene();
        self.worldgen_release_world();
        self.worldgen.library = None;
        self.worldgen.library_path = None;
        self.worldgen_set_status(WorldgenStatus::MissingLibrary);
        send_event(&EngineEvent::WorldGenStatus {
            state: self.worldgen.status.as_str().to_string(),
        });
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
            let origin = self.worldgen.scene_offset;
            let position = [
                placement.position[0] + origin.x,
                placement.position[1] + origin.y,
                placement.position[2] + origin.z,
            ];
            let entity_id = self.spawn_cached_model_part_at(
                part.mesh_idx,
                part.tex_idx,
                &model_path,
                position,
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
                position,
                rotation,
                scale: placement.scale,
                entity_id,
            });
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
            let entity_id = self.spawn_cached_model_part_at(
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
                entity_id,
            });
        }

        // 4) Generación del terreno: automática si el plugin está cargado.
        // Restaurar no quita el suelo: ese check solo viaja con Generar mundo.
        if self.worldgen.library.is_some() && self.worldgen.world.is_some() {
            self.worldgen_start(false);
        }
        send_event(&EngineEvent::WorldGenStatus {
            state: self.worldgen.status.as_str().to_string(),
        });
    }

    // -- Tick por frame -------------------------------------------------------

    /// Avanza el barrido de columnas o la cola de chunks con presupuesto
    /// [`CHUNKS_PER_FRAME`]. Se llama desde `State::update`.
    pub(crate) fn worldgen_tick(&mut self) {
        if self.worldgen.status != WorldgenStatus::Generating {
            return;
        }
        if self.worldgen.scan.is_some() {
            self.worldgen_scan_columns();
            return;
        }
        for _ in 0..CHUNKS_PER_FRAME {
            let Some(pending) = self.worldgen.pending.pop() else {
                let replace_ground = self.worldgen.replace_ground;
                self.worldgen.replace_ground = false;
                self.worldgen_set_status(WorldgenStatus::Complete);
                if replace_ground {
                    self.remove_default_ground();
                }
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

    /// Libera el handle con la biblioteca que lo creó. No suelta la DLL.
    fn worldgen_release_world(&mut self) {
        if let Some(handle) = self.worldgen.world.take()
            && let Some(library) = self.worldgen.library.as_ref()
        {
            // # Seguridad: handle creado por esta DLL; no se reutiliza.
            unsafe { (library.world_free)(handle.0) };
        }
    }

    /// Quita de la escena los chunks y las instancias de scatter, y suelta
    /// las mallas GPU de los chunks sin desplazar el resto de `self.meshes`.
    fn worldgen_despawn_scene(&mut self) {
        let keys: Vec<ChunkKey> = self.worldgen.spawned.keys().copied().collect();
        for key in keys {
            if let Some(chunk) = self.worldgen.spawned.remove(&key) {
                self.physics.remove_entity_body(chunk.entity_id);
                self.entity_colision.remove(&chunk.entity_id);
                self.worldgen_retire_mesh(chunk.mesh_idx);
                self.world.despawn(chunk.entity_id);
            }
        }
        let placements = std::mem::take(&mut self.worldgen.placements);
        for placement in placements {
            if placement.entity_id != 0 {
                self.world.despawn(placement.entity_id);
            }
        }
    }

    /// Sustituye la malla del chunk por un triángulo degenerado para que
    /// `Drop` de `GpuBuffer` libere la VRAM y el índice no se mueva.
    fn worldgen_retire_mesh(&mut self, mesh_idx: Option<usize>) {
        let Some(mesh_idx) = mesh_idx else {
            return;
        };
        if mesh_idx >= self.meshes.len() {
            return;
        }
        let placeholder = crate::engine3d::mesh::upload(
            &self.vulkan,
            &[crate::engine3d::mesh::Vertex {
                position: [0.0, 0.0, 0.0],
                normal: [0.0, 1.0, 0.0],
                uv: [0.0, 0.0],
            }; 3],
            &[0, 0, 0],
            "worldgen-freed",
            None,
        );
        let _old = std::mem::replace(&mut self.meshes[mesh_idx], placeholder);
    }

    /// Colisión estática con la misma malla que se dibuja. El winding se alinea
    /// con la normal visual para que el personaje se apoye en la cara exterior.
    fn attach_terrain_collider(
        &mut self,
        entity_id: crate::engine3d::ecs::EntityId,
        vertices: &[crate::engine3d::mesh::Vertex],
        indices: &[u32],
    ) {
        if vertices.len() < 3 || indices.len() < 3 {
            return;
        }
        let points: Vec<rapier3d::prelude::Vector> = vertices
            .iter()
            .map(|vertex| {
                rapier3d::prelude::Vector::new(
                    vertex.position[0],
                    vertex.position[1],
                    vertex.position[2],
                )
            })
            .collect();
        let mut tris = Vec::with_capacity(indices.len() / 3);
        for triangle in indices.chunks_exact(3) {
            let (i0, i1, i2) = (
                triangle[0] as usize,
                triangle[1] as usize,
                triangle[2] as usize,
            );
            if i0 >= vertices.len() || i1 >= vertices.len() || i2 >= vertices.len() {
                continue;
            }
            let p0 = glam::Vec3::from_array(vertices[i0].position);
            let p1 = glam::Vec3::from_array(vertices[i1].position);
            let p2 = glam::Vec3::from_array(vertices[i2].position);
            let outward = glam::Vec3::from_array(vertices[i0].normal)
                + glam::Vec3::from_array(vertices[i1].normal)
                + glam::Vec3::from_array(vertices[i2].normal);
            let face = (p1 - p0).cross(p2 - p0);
            if face.dot(outward) < 0.0 {
                tris.push([triangle[0], triangle[2], triangle[1]]);
            } else {
                tris.push([triangle[0], triangle[1], triangle[2]]);
            }
        }
        if tris.is_empty() {
            return;
        }
        let position = self.worldgen.scene_offset.to_array();
        self.physics
            .set_entity_static_trimesh(entity_id, position, points, tris);
        self.entity_colision.insert(entity_id, true);
    }

    /// Avanza unas columnas del barrido. Al cubrir el mundo, ordena la cola
    /// para que `pop` saque primero la superficie cercana.
    fn worldgen_scan_columns(&mut self) {
        let Some(mut scan) = self.worldgen.scan.take() else {
            return;
        };
        let mut failed: Option<String> = None;
        {
            let Some(library) = self.worldgen.library.as_ref() else {
                self.worldgen.scan = Some(scan);
                return;
            };
            let Some(handle) = self.worldgen.world.as_ref() else {
                self.worldgen.scan = Some(scan);
                return;
            };
            for _ in 0..CHUNKS_PER_FRAME {
                if scan.cz >= scan.chunks_z {
                    break;
                }
                let cx = scan.cx;
                let cz = scan.cz;
                for cy in 0..=scan.max_y {
                    // # Seguridad: handle válido de esta DLL, hilo del engine.
                    let status = unsafe { (library.chunk_status)(handle.0, cx, cy, cz) };
                    if status == 1 {
                        scan.collected.push(PendingChunk {
                            key: (cx, cy, cz),
                            lod: 0,
                        });
                    } else if status < 0 {
                        failed = Some(format!("wg_chunk_status({cx}, {cy}, {cz}) inválido"));
                        break;
                    }
                }
                if failed.is_some() {
                    break;
                }
                scan.cx += 1;
                if scan.cx >= scan.chunks_x {
                    scan.cx = 0;
                    scan.cz += 1;
                }
            }
        }
        if let Some(message) = failed {
            self.worldgen.scan = None;
            self.worldgen.pending.clear();
            self.worldgen_set_status(WorldgenStatus::LibraryError);
            send_event(&EngineEvent::WorldGenError { message });
            return;
        }
        if scan.cz >= scan.chunks_z {
            let cell = scan.cell;
            let camera = Vec3::from_array(scan.camera);
            let mut pending = scan.collected;
            finalize_pending(&mut pending, cell, camera);
            self.worldgen.total_chunks = pending.len() as u32;
            self.worldgen.completed_chunks = 0;
            self.worldgen.pending = pending;
            self.worldgen.scan = None;
            send_event(&EngineEvent::WorldGenProgress {
                completed: 0,
                total: self.worldgen.total_chunks,
            });
            return;
        }
        self.worldgen.scan = Some(scan);
    }

    /// Crea (o recrea) el handle del mundo con la config actual.
    fn worldgen_apply_config(&mut self, config_json: &str) {
        self.worldgen_despawn_scene();
        self.worldgen_release_world();
        self.worldgen.pending.clear();
        self.worldgen.scan = None;
        self.worldgen.column_epoch = self.worldgen.column_epoch.wrapping_add(1);
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
            self.worldgen.replace_ground = false;
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

        let (vertices, indices) = clip_chunk_to_world_disk(
            &vertices,
            &indices,
            self.worldgen.scene_offset,
            self.world_bounds_3d.radius,
        );

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

        let column_epoch = self.worldgen.column_epoch;
        let use_splat = self.worldgen.material_base.is_some();
        let tex_idx = match worldgen_pack_column_texture(
            &mut self.texture_array,
            &self.vulkan,
            library,
            handle.0,
            &mut self.worldgen.columns,
            &mut self.tex_layers,
            column_epoch,
            cx,
            cz,
            use_splat,
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
                position: self.worldgen.scene_offset,
                rotation: glam::Quat::IDENTITY,
                scale: Vec3::ONE,
            },
        );
        let (min, max) = chunk_vertex_aabb(&vertices);
        self.world.insert(entity_id, MeshLocalAabb { min, max });
        // Terreno: no seleccionable. Los vértices vienen en el espacio de la DLL;
        // la traslación centra la huella y apoya el suelo del terreno en y = 0.
        // La AABB local evita que el culling use un cubo de 1 m en esa esquina.
        self.world.insert(entity_id, NonSelectable);
        self.attach_terrain_collider(entity_id, &vertices, &indices);
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
        self.worldgen.spawned.insert(
            key,
            WorldgenChunk {
                entity_id,
                mesh_idx: Some(mesh_idx),
            },
        );
        send_event(&EngineEvent::WorldGenChunkReady {
            cx,
            cy,
            cz,
            entity_id,
        });
        Ok(())
    }

    /// Sube Grass, Dirt, Clay, Sand, Stone y Snow a 1024 con mips, en ese orden.
    /// El 4K se descarta al empaquetar: de lejos el GPU usa un mip pequeño.
    fn ensure_terrain_materials(&mut self) {
        if self.worldgen.material_base.is_some() {
            return;
        }
        let Some(dir) = terrain_material_dir() else {
            log::warn!("[worldgen] no están las texturas de terreno en assets/terrain");
            return;
        };
        // Capa 0 = checker del editor. Grass es la primera foto: si queda al lado,
        // un sampler LINEAR mezcla ambas y la pradera se ve a cuadros.
        if !self.pack_terrain_isolation_layer() {
            return;
        }
        let mut layers = [0u32; 6];
        for (index, name) in TERRAIN_MATERIAL_FILES.iter().enumerate() {
            let path = dir.join(name);
            let Ok(image) = image::open(&path) else {
                log::warn!("[worldgen] no se pudo abrir {}", path.display());
                return;
            };
            let rgba = image.to_rgba8();
            let (width, height) = rgba.dimensions();
            let Ok(layer) = self
                .texture_array
                .pack(&self.vulkan, rgba.as_raw(), width, height)
            else {
                log::warn!("[worldgen] no se pudo subir {name}");
                return;
            };
            if index == 0 && layer == crate::engine3d::texture::TextureArray::fallback_layer() {
                log::warn!("[worldgen] Grass no puede usar la capa 0 (checker del editor)");
                return;
            }
            layers[index] = layer;
            log::info!(
                "[worldgen] {name} quedó en la capa {layer} (origen {width}×{height}, GPU 1024 + mips)"
            );
        }
        let consecutive = layers.windows(2).all(|pair| pair[1] == pair[0] + 1);
        if !consecutive {
            log::warn!("[worldgen] las texturas de terreno no quedaron en capas seguidas");
            return;
        }
        self.worldgen.material_base = Some(layers[0]);
    }

    /// Reserva una capa verde entre el checker y Grass para que no sean vecinas.
    fn pack_terrain_isolation_layer(&mut self) -> bool {
        const PAD: [u8; 4] = [79, 133, 61, 255];
        let fallback = crate::engine3d::texture::TextureArray::fallback_layer();
        for _ in 0..2 {
            match self.texture_array.pack(&self.vulkan, &PAD, 1, 1) {
                Ok(layer) if layer != fallback => return true,
                Ok(_) => continue,
                Err(error) => {
                    log::warn!(
                        "[worldgen] no se pudo reservar el aislamiento del terreno: {error}"
                    );
                    return false;
                }
            }
        }
        log::warn!("[worldgen] no quedó capa libre para aislar Grass del checker");
        false
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

/// Obtiene (y cachea) el splat de una columna vía la DLL.
///
/// Devuelve el índice en `tex_layers` (no la capa GPU). El renderer resuelve
/// la capa con `texture_layer_for`.
fn worldgen_pack_column_texture(
    texture_array: &mut crate::engine3d::texture::TextureArray,
    vulkan: &crate::engine3d::vulkan::SharedGpu,
    library: &WorldgenLibrary,
    handle: *mut std::ffi::c_void,
    columns: &mut HashMap<(i32, i32), (usize, u64)>,
    tex_layers: &mut Vec<crate::engine3d::texture::TextureLayer>,
    epoch: u64,
    cx: i32,
    cz: i32,
    use_splat: bool,
) -> Option<usize> {
    if let Some(&(tex_idx, cached_epoch)) = columns.get(&(cx, cz))
        && cached_epoch == epoch
    {
        return Some(tex_idx);
    }

    let mut buffers = crate::engine3d::config_3d::worldgen::abi::WgChunkTextures::empty();
    // # Seguridad: handle válido de esta DLL y struct del contrato ABI.
    let status = unsafe { (library.chunk_textures)(handle, cx, cz, 0, &mut buffers) };
    let source_ptr = if use_splat && !buffers.splat.is_null() {
        buffers.splat
    } else {
        buffers.albedo
    };
    if status == crate::engine3d::config_3d::worldgen::abi::Status::Ok as i32
        && !source_ptr.is_null()
    {
        // # Seguridad: buffers del contrato; se copian antes de liberar.
        let pixels = unsafe { std::slice::from_raw_parts(source_ptr, buffers.len) }.to_vec();
        let dim = buffers.dim;
        unsafe { (library.free_chunk_textures)(&buffers) };

        if let Some(&(tex_idx, _)) = columns.get(&(cx, cz))
            && let Some(&gpu_layer) = tex_layers.get(tex_idx)
        {
            match texture_array.replace_layer(vulkan, gpu_layer, &pixels, dim, dim) {
                Ok(()) => {
                    columns.insert((cx, cz), (tex_idx, epoch));
                    return Some(tex_idx);
                }
                Err(error) => {
                    log::warn!("[worldgen] no se pudo reescribir el splat de la columna: {error}")
                }
            }
            return Some(tex_idx);
        }

        match texture_array.pack(vulkan, &pixels, dim, dim) {
            Ok(gpu_layer) => {
                let tex_idx = tex_layers.len();
                tex_layers.push(gpu_layer);
                columns.insert((cx, cz), (tex_idx, epoch));
                return Some(tex_idx);
            }
            Err(error) => {
                log::warn!("[worldgen] no se pudo empaquetar el splat de la columna: {error}")
            }
        }
    } else if !buffers.albedo.is_null() || !buffers.splat.is_null() {
        // # Seguridad: liberar buffers parciales en error.
        unsafe { (library.free_chunk_textures)(&buffers) };
    }
    columns.get(&(cx, cz)).map(|(tex_idx, _)| *tex_idx)
}

/// Ordena por capa alta y cercanía, asigna LOD y deja el vector listo para
/// `pop` (el primero a generar queda al final).
fn finalize_pending(pending: &mut [PendingChunk], cell: f32, camera: Vec3) {
    pending.sort_by(|a, b| {
        let score = |c: &PendingChunk| -> (i32, f32) {
            let chunk_center = Vec3::new(
                (c.key.0 as f32 + 0.5) * cell,
                (c.key.1 as f32 + 0.5) * cell,
                (c.key.2 as f32 + 0.5) * cell,
            );
            (-c.key.1, chunk_center.distance(camera))
        };
        let (layer_a, dist_a) = score(a);
        let (layer_b, dist_b) = score(b);
        layer_a.cmp(&layer_b).then(dist_a.total_cmp(&dist_b))
    });
    for chunk in pending.iter_mut() {
        let chunk_center = Vec3::new(
            (chunk.key.0 as f32 + 0.5) * cell,
            (chunk.key.1 as f32 + 0.5) * cell,
            (chunk.key.2 as f32 + 0.5) * cell,
        );
        let d = chunk_center.distance(camera);
        chunk.lod = if d < 120.0 {
            0
        } else if d < 320.0 {
            1
        } else {
            2
        };
    }
    pending.reverse();
}

/// Caja local de los vértices de un chunk, en el espacio de la DLL.
fn chunk_vertex_aabb(vertices: &[crate::engine3d::mesh::Vertex]) -> (Vec3, Vec3) {
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    for vertex in vertices {
        let position = Vec3::from_array(vertex.position);
        min = min.min(position);
        max = max.max(position);
    }
    (min, max)
}

const TERRAIN_MATERIAL_FILES: [&str; 6] = [
    "Grass.jpg",
    "Dirt.jpg",
    "Clay.jpg",
    "Sand.jpg",
    "Stone.jpg",
    "Snow.jpg",
];

/// Carpeta `assets/terrain` del crate del engine (Grass, Dirt, Clay, Sand, Stone, Snow).
fn terrain_material_dir() -> Option<std::path::PathBuf> {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/terrain");
    dir.join(TERRAIN_MATERIAL_FILES[0]).is_file().then_some(dir)
}

#[cfg(test)]
mod terrain_material_tests {
    use super::{TERRAIN_MATERIAL_FILES, terrain_material_dir};

    #[test]
    fn terrain_photos_are_on_disk_with_grass_first() {
        assert_eq!(TERRAIN_MATERIAL_FILES[0], "Grass.jpg");
        let dir = terrain_material_dir().expect("assets/terrain/Grass.jpg");
        for name in TERRAIN_MATERIAL_FILES {
            assert!(
                dir.join(name).is_file(),
                "falta la foto de terreno {name} en {}",
                dir.display()
            );
        }
    }

    #[test]
    fn grass_jpeg_decodes_green_not_a_checker() {
        let path = terrain_material_dir()
            .expect("assets/terrain")
            .join("Grass.jpg");
        let image = image::open(&path).expect("Grass.jpg").to_rgba8();
        let (width, height) = image.dimensions();
        assert!(width >= 256 && height >= 256, "{width}x{height}");
        let pixels = image.as_raw();
        let mut r = 0u64;
        let mut g = 0u64;
        let mut b = 0u64;
        let mut n = 0u64;
        let step = (width / 32).max(1);
        let mut y = 0u32;
        while y < height {
            let mut x = 0u32;
            while x < width {
                let i = ((y * width + x) * 4) as usize;
                r += pixels[i] as u64;
                g += pixels[i + 1] as u64;
                b += pixels[i + 2] as u64;
                n += 1;
                x += step;
            }
            y += step;
        }
        let (ar, ag, ab) = (r / n, g / n, b / n);
        assert!(
            ag + 8 > ar && ag + 8 > ab,
            "Grass.jpg no decodifica a verde (avg {ar},{ag},{ab})"
        );
    }
}
/// Suelo más bajo del terreno, en metros. Debe coincidir con
/// `rer_worldgen::config::TERRAIN_FLOOR`.
const TERRAIN_FLOOR: f32 = 2.0;
/// Hueco bajo las llanuras. Debe coincidir con
/// `rer_worldgen::config::TERRAIN_PLAINS_CLEARANCE`.
const TERRAIN_PLAINS_CLEARANCE: f32 = 6.0;

/// Ancho y largo reales de la rejilla. `chunk_grid` redondea hacia arriba, así
/// que la huella puede pasar del ancho nominal; el centrado usa esta medida.
fn generated_footprint(config: &WgConfigLite) -> (f32, f32) {
    let (chunks_x, chunks_z, _) = chunk_grid(config);
    let cell = crate::engine3d::config_3d::worldgen::abi::CHUNK_SIZE as f32
        * config.resolution.voxel_size.max(0.001);
    (chunks_x as f32 * cell, chunks_z as f32 * cell)
}

/// Centra la huella generada en el origen y apoya el suelo más bajo en y = 0.
fn scene_offset(extent_x: f32, extent_z: f32) -> Vec3 {
    Vec3::new(
        -extent_x * 0.5,
        -(TERRAIN_FLOOR + TERRAIN_PLAINS_CLEARANCE),
        -extent_z * 0.5,
    )
}

/// Radio del disco que el rectángulo cubre entero. Las esquinas que se salen
/// las recorta el límite del mundo, así no queda anillo vacío ni terreno fuera.
fn generator_world_radius(extent_x: f32, extent_z: f32) -> f32 {
    extent_x.min(extent_z) * 0.5
}

impl super::super::engine::State {
    /// Ajusta el radio configurado al disco cubierto por el generador y escala el suelo.
    fn fit_world_to_generator(&mut self, extent_x: f32, extent_z: f32, bounds: &WgBoundsLite) {
        let requested = generator_world_radius(extent_x, extent_z);
        self.set_world_bounds_3d_radius(requested);
        let applied = self.world_bounds_3d.radius;
        log::info!(
            "[worldgen] radio del mundo ajustado a {applied:.1} u (huella {extent_x:.0}×{extent_z:.0}, generador {:.0}×{:.0}×{:.0})",
            bounds.width,
            bounds.length,
            bounds.height
        );
        if (applied - requested).abs() > 0.1 {
            log::warn!(
                "[worldgen] el radio pedido ({requested:.1}) supera el máximo y se recortó a {applied:.1}"
            );
        }
    }
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

/// Recorta al disco del ecuador lo que el cuadrado de chunks deja fuera.
/// Los chunks interiores no se tocan. Solo el anillo paga el corte.
fn clip_chunk_to_world_disk(
    vertices: &[crate::engine3d::mesh::Vertex],
    indices: &[u32],
    offset: Vec3,
    radius: f32,
) -> (Vec<crate::engine3d::mesh::Vertex>, Vec<u32>) {
    if vertices.is_empty() || indices.len() < 3 || radius <= 0.0 {
        return (vertices.to_vec(), indices.to_vec());
    }
    let r2 = radius * radius;
    let world_xz = |position: [f32; 3]| {
        let x = position[0] + offset.x;
        let z = position[2] + offset.z;
        (x, z)
    };
    let inside = |position: [f32; 3]| {
        let (x, z) = world_xz(position);
        x * x + z * z <= r2
    };
    let mut any_out = false;
    let mut min_x = f32::MAX;
    let mut max_x = f32::MIN;
    let mut min_z = f32::MAX;
    let mut max_z = f32::MIN;
    for vertex in vertices {
        let (x, z) = world_xz(vertex.position);
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_z = min_z.min(z);
        max_z = max_z.max(z);
        if !inside(vertex.position) {
            any_out = true;
        }
    }
    if !any_out {
        return (vertices.to_vec(), indices.to_vec());
    }
    let closest_x = 0.0_f32.clamp(min_x, max_x);
    let closest_z = 0.0_f32.clamp(min_z, max_z);
    if closest_x * closest_x + closest_z * closest_z > r2 {
        return (Vec::new(), Vec::new());
    }

    let mut out_vertices = Vec::new();
    let mut out_indices = Vec::new();
    let mut emit = |vertex: crate::engine3d::mesh::Vertex| -> u32 {
        let index = out_vertices.len() as u32;
        out_vertices.push(vertex);
        index
    };
    for tri in indices.chunks_exact(3) {
        let Some(a) = vertices.get(tri[0] as usize).copied() else {
            continue;
        };
        let Some(b) = vertices.get(tri[1] as usize).copied() else {
            continue;
        };
        let Some(c) = vertices.get(tri[2] as usize).copied() else {
            continue;
        };
        let corners = [a, b, c];
        let mask = [inside(a.position), inside(b.position), inside(c.position)];
        let inside_count = mask.iter().filter(|flag| **flag).count();
        if inside_count == 0 {
            continue;
        }
        if inside_count == 3 {
            out_indices.extend([emit(a), emit(b), emit(c)]);
            continue;
        }
        let mut polygon = Vec::with_capacity(4);
        for index in 0..3 {
            let current = corners[index];
            let next = corners[(index + 1) % 3];
            if mask[index] {
                polygon.push(current);
            }
            if mask[index] != mask[(index + 1) % 3] {
                polygon.push(edge_on_world_disk(current, next, offset, radius));
            }
        }
        if polygon.len() < 3 {
            continue;
        }
        let origin = emit(polygon[0]);
        for index in 1..polygon.len() - 1 {
            out_indices.extend([origin, emit(polygon[index]), emit(polygon[index + 1])]);
        }
    }
    (out_vertices, out_indices)
}

fn edge_on_world_disk(
    a: crate::engine3d::mesh::Vertex,
    b: crate::engine3d::mesh::Vertex,
    offset: Vec3,
    radius: f32,
) -> crate::engine3d::mesh::Vertex {
    let t = disk_edge_t(a.position, b.position, offset, radius);
    let mix = |from: f32, to: f32| from + (to - from) * t;
    crate::engine3d::mesh::Vertex {
        position: [
            mix(a.position[0], b.position[0]),
            mix(a.position[1], b.position[1]),
            mix(a.position[2], b.position[2]),
        ],
        normal: [
            mix(a.normal[0], b.normal[0]),
            mix(a.normal[1], b.normal[1]),
            mix(a.normal[2], b.normal[2]),
        ],
        uv: [mix(a.uv[0], b.uv[0]), mix(a.uv[1], b.uv[1])],
    }
}

fn disk_edge_t(a: [f32; 3], b: [f32; 3], offset: Vec3, radius: f32) -> f32 {
    let ax = a[0] + offset.x;
    let az = a[2] + offset.z;
    let bx = b[0] + offset.x;
    let bz = b[2] + offset.z;
    let dx = bx - ax;
    let dz = bz - az;
    let qa = dx * dx + dz * dz;
    if qa <= 1.0e-12 {
        return 0.0;
    }
    let qb = 2.0 * (ax * dx + az * dz);
    let qc = ax * ax + az * az - radius * radius;
    let root = (qb * qb - 4.0 * qa * qc).max(0.0).sqrt();
    let t0 = (-qb - root) / (2.0 * qa);
    let t1 = (-qb + root) / (2.0 * qa);
    if (0.0..=1.0).contains(&t0) {
        t0
    } else if (0.0..=1.0).contains(&t1) {
        t1
    } else {
        t0.clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    //! Conformancia real: carga la DLL construida (`cargo build -p rer-worldgen`)
    //! y ejecuta el ciclo completo a través de la frontera FFI. Si la DLL no
    //! existe, el test se salta (el CI puede generarla antes si lo desea).
    use super::abi::Status;
    use super::loader::WorldgenLibrary;
    use super::{
        WgBoundsLite, WgConfigLite, WgResolutionLite, clip_chunk_to_world_disk,
        generator_world_radius,
    };
    use crate::engine3d::mesh::Vertex;

    fn vert(x: f32, z: f32) -> Vertex {
        Vertex {
            position: [x, 0.0, z],
            normal: [0.0, 1.0, 0.0],
            uv: [0.0, 0.0],
        }
    }

    #[test]
    fn disk_clip_drops_the_corner_and_keeps_the_interior() {
        let inside = [vert(0.0, 0.0), vert(1.0, 0.0), vert(0.0, 1.0)];
        let (kept, indices) = clip_chunk_to_world_disk(&inside, &[0, 1, 2], glam::Vec3::ZERO, 10.0);
        assert_eq!(kept.len(), 3);
        assert_eq!(indices, vec![0, 1, 2]);

        let outside = [vert(20.0, 20.0), vert(21.0, 20.0), vert(20.0, 21.0)];
        let (cut, cut_idx) = clip_chunk_to_world_disk(&outside, &[0, 1, 2], glam::Vec3::ZERO, 10.0);
        assert!(cut.is_empty());
        assert!(cut_idx.is_empty());

        let straddling = [vert(0.0, 0.0), vert(20.0, 0.0), vert(0.0, 20.0)];
        let (ring, ring_idx) =
            clip_chunk_to_world_disk(&straddling, &[0, 1, 2], glam::Vec3::ZERO, 10.0);
        assert!(ring.len() >= 3);
        assert_eq!(ring_idx.len() % 3, 0);
        for vertex in &ring {
            let x = vertex.position[0];
            let z = vertex.position[2];
            assert!(
                x * x + z * z <= 10.0 * 10.0 + 0.05,
                "{x},{z} fuera del disco"
            );
        }
    }

    fn config(width: f32, length: f32, voxel: f32) -> WgConfigLite {
        WgConfigLite {
            bounds: WgBoundsLite {
                width,
                length,
                height: 128.0,
            },
            resolution: WgResolutionLite { voxel_size: voxel },
        }
    }

    #[test]
    fn el_radio_es_el_disco_cubierto_por_la_huella() {
        let square = config(512.0, 512.0, 1.0);
        let (extent_x, extent_z) = super::generated_footprint(&square);
        assert!((extent_x - 512.0).abs() < 1.0e-3);
        assert!((generator_world_radius(extent_x, extent_z) - 256.0).abs() < 1.0e-3);

        // 200 m no caben en un número entero de chunks de 32: la huella crece
        // y el disco usa el lado corto para no dejar el mundo vacío.
        let wide = config(200.0, 100.0, 1.0);
        let (wide_x, wide_z) = super::generated_footprint(&wide);
        assert!((wide_x - 224.0).abs() < 1.0e-3);
        assert!((wide_z - 128.0).abs() < 1.0e-3);
        assert!((generator_world_radius(wide_x, wide_z) - 64.0).abs() < 1.0e-3);
    }

    #[test]
    fn el_terreno_se_apoya_en_el_suelo_y_queda_centrado() {
        let (extent_x, extent_z) = super::generated_footprint(&config(200.0, 100.0, 1.0));
        let offset = super::scene_offset(extent_x, extent_z);
        assert_eq!(offset.x, -112.0);
        assert_eq!(offset.z, -64.0);
        assert!((offset.y + 8.0).abs() < 1.0e-4);
    }

    #[test]
    fn la_caja_del_chunk_cubre_sus_vertices() {
        let vertices = [
            crate::engine3d::mesh::Vertex {
                position: [0.0, 10.0, 4.0],
                normal: [0.0, 1.0, 0.0],
                uv: [0.0, 0.0],
            },
            crate::engine3d::mesh::Vertex {
                position: [32.0, 40.0, 32.0],
                normal: [0.0, 1.0, 0.0],
                uv: [1.0, 1.0],
            },
        ];
        let (min, max) = super::chunk_vertex_aabb(&vertices);
        assert_eq!(min, glam::Vec3::new(0.0, 10.0, 4.0));
        assert_eq!(max, glam::Vec3::new(32.0, 40.0, 32.0));
    }

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
            // El apoyo en el suelo (y = 0) lo aplica el engine al colocar el chunk.
            let x = *buffers.positions;
            let y = *buffers.positions.add(1);
            assert!((-1.0..=257.0).contains(&x), "x={x} fuera de los límites");
            assert!((-1.0..=97.0).contains(&y), "y={y} fuera de los límites");

            (library.free_mesh_buffers)(&buffers);
            (library.world_free)(handle);
        }
    }

    #[test]
    fn queue_pop_yields_highest_nearest_chunk_first() {
        let mut pending = vec![
            super::PendingChunk {
                key: (0, 0, 0),
                lod: 0,
            },
            super::PendingChunk {
                key: (4, 1, 0),
                lod: 0,
            },
            super::PendingChunk {
                key: (0, 3, 0),
                lod: 0,
            },
        ];
        super::finalize_pending(&mut pending, 32.0, glam::Vec3::ZERO);
        let first = pending.pop().expect("la cola no está vacía");
        assert_eq!(first.key.1, 3, "la capa más alta sale primero");
        assert_eq!(first.lod, 0);
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
