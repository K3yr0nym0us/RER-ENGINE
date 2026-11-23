//! Consumidor del plugin de generación de terreno en el `State` del engine.
//!
//! División de responsabilidades (contrato del plan):
//! - **La DLL (`rer-worldgen`) produce**: densidad, Marching Cubes, chunks,
//!   splats. No hay ninguna lógica de generación en el engine.
//! - **El engine consume**: subida a GPU con presupuesto por frame (el scan y
//!   el mesh CPU van en el hilo `rer-worldgen`), entidades ECS, eventos de
//!   progreso y limpieza.
//!
//! El flujo es: `world_gen_set_library_path` (ruta de la DLL instalada por el
//! editor) → `world_gen_set_config` (JSON camelCase, sin crear el mundo) →
//! `world_gen_start` → el hilo `rer-worldgen` crea el handle (`wg_world_create`),
//! barre columnas y meshea; `worldgen_tick` drena CPU, sube GPU
//! (`CHUNKS_PER_FRAME`) y coloca scatter (`PLACEMENTS_PER_FRAME`) → eventos
//! `world_gen_*` hacia el editor.

mod abi;
mod loader;
mod types;
mod worker;

pub(crate) use types::CHUNKS_PER_FRAME;

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use glam::Vec3;
use serde::Deserialize;

use crate::engine3d::config_3d::WorldBounds3D;
use crate::engine3d::config_3d::worldgen::abi::CHUNK_SIZE;
use crate::engine3d::config_3d::worldgen::loader::WorldgenLibrary;
use crate::engine3d::config_3d::worldgen::types::{
    ChunkColumns, ChunkKey, GenerationArea, PLACEMENTS_PER_FRAME, PendingChunk, PendingScatterJob,
    PendingScatterPose, SPLAT_ATLAS_TILE_PX, SPLAT_ATLAS_TILES, ScanCursor, SplatAtlas,
    WgWorldHandle, WorldgenChunk, WorldgenPlacement, WorldgenStatus,
};
use crate::engine3d::config_3d::worldgen::worker::{
    PendingCpuChunk, WorldgenJob, WorldgenRunJob, WorldgenWorkerMsg, spawn_worldgen_worker,
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
    /// El engine sustituye ancho/largo por el área generada; se conservan para
    /// documentar el contrato JSON y para los tests.
    #[allow(dead_code)]
    width: f32,
    #[allow(dead_code)]
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
        self.worldgen_cancel_worker();
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
        self.worldgen.generation = None;
        self.worldgen.generated = false;
        self.worldgen.library_path = Some(path.clone());
        let result = WorldgenLibrary::load(std::path::Path::new(&path));
        match result {
            Ok(library) => {
                self.worldgen.library = Some(library);
                self.worldgen_set_status(WorldgenStatus::Idle);
                send_event(&EngineEvent::WorldGenStatus {
                    state: self.worldgen.status.as_str().to_string(),
                });
                if self.worldgen.config_json.is_some() {
                    self.worldgen_set_status(WorldgenStatus::Ready);
                    send_event(&EngineEvent::WorldGenStatus {
                        state: self.worldgen.status.as_str().to_string(),
                    });
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

    /// `world_gen_set_config`: guarda el JSON camelCase. El handle se crea en
    /// el worker al generar; no bloquear el editor aquí.
    pub(crate) fn worldgen_set_config(&mut self, config_json: String) {
        // Nueva configuración: el área generada y el terreno anterior dejan de
        // ser válidos hasta el próximo `world_gen_start`.
        self.worldgen.generation = None;
        self.worldgen.generated = false;
        self.worldgen.config_json = Some(config_json);
        if self.worldgen.library.is_none() {
            self.worldgen_set_status(WorldgenStatus::MissingLibrary);
            return;
        }
        self.worldgen_set_status(WorldgenStatus::Ready);
        send_event(&EngineEvent::WorldGenStatus {
            state: self.worldgen.status.as_str().to_string(),
        });
    }

    /// `world_gen_start`: genera el mundo desde cero con la config actual.
    ///
    /// El área que entrega el plugin es mayor que los límites visibles (margen
    /// de crecimiento); solo se meshean las columnas que solapan el mundo. Al
    /// mover una cara dentro del área, `worldgen_retrim` añade o recorta solo
    /// lo afectado.
    pub(crate) fn worldgen_start(&mut self, replace_ground: bool, resize_world: bool) {
        self.worldgen_cancel_worker();
        if self.worldgen.library.is_none() {
            send_event(&EngineEvent::WorldGenError {
                message: "world_gen_start sin plugin cargado".to_string(),
            });
            return;
        }

        let Some(config_json) = self.worldgen.config_json.clone() else {
            send_event(&EngineEvent::WorldGenError {
                message: "world_gen_start sin config aplicada".to_string(),
            });
            return;
        };
        let Some(config) = serde_json::from_str::<WgConfigLite>(&config_json).ok() else {
            send_event(&EngineEvent::WorldGenError {
                message: "world_gen_start: config ilegible".to_string(),
            });
            return;
        };

        // Generar desde el modal fija el mundo al tamaño pedido (el plugin es
        // la fuente del tamaño al crear el mundo). En restauraciones o
        // crecimientos automáticos se conservan las caras actuales.
        if resize_world {
            self.set_world_bounds_3d_faces(
                config.bounds.length * 0.5,
                config.bounds.length * 0.5,
                config.bounds.width * 0.5,
                config.bounds.width * 0.5,
            );
            self.clamp_play_character_camera_to_bounds();
        }

        // El mundo conserva sus caras: el plugin cubre un rectángulo con margen
        // y el terreno se apoya en y = 0 desde la esquina de ese rectángulo.
        let area = generation_area(&self.world_bounds_3d);
        self.worldgen.replace_ground = replace_ground;
        self.ensure_terrain_materials();
        self.worldgen.generation = Some(area);
        self.worldgen.scene_offset = scene_offset(area.min_x, area.min_z);

        let config_json = config_with_bounds(&config_json, &area, &self.world_bounds_3d);
        self.worldgen.config_json = Some(config_json);
        self.worldgen_reset_generation_state();
        self.worldgen.generated = true;

        let voxel = config.resolution.voxel_size.max(0.001);
        let cell = CHUNK_SIZE as f32 * voxel;
        let (chunks_x, chunks_z, max_y) =
            chunk_grid_size(area.width, area.depth, config.bounds.height, voxel);
        let Some(columns) =
            chunk_columns_for_bounds(&self.world_bounds_3d, &area, cell, chunks_x, chunks_z)
        else {
            // Mundo sin superficie visible: no hay chunks que generar.
            self.worldgen.pending.clear();
            self.worldgen.total_chunks = 0;
            self.worldgen.completed_chunks = 0;
            self.worldgen_set_status(WorldgenStatus::Complete);
            send_event(&EngineEvent::WorldGenStatus {
                state: self.worldgen.status.as_str().to_string(),
            });
            send_event(&EngineEvent::WorldGenCompleted { chunks: 0 });
            return;
        };
        let camera_pos = self.camera_world_position();

        self.worldgen.pending.clear();
        self.worldgen.total_chunks = 0;
        self.worldgen.completed_chunks = 0;
        self.worldgen.scan = Some(ScanCursor {
            cx: columns.cx_min,
            cz: columns.cz_min,
            cx_min: columns.cx_min,
            cx_max: columns.cx_max + 1,
            cz_min: columns.cz_min,
            cz_max: columns.cz_max + 1,
            max_y,
            cell,
            camera: camera_pos.to_array(),
            collected: Vec::new(),
            columns: None,
            column_index: 0,
        });
        self.worldgen_set_status(WorldgenStatus::Generating);
        send_event(&EngineEvent::WorldGenStatus {
            state: self.worldgen.status.as_str().to_string(),
        });
        let total_columns = (columns.cx_max - columns.cx_min + 1).max(0) as u32
            * (columns.cz_max - columns.cz_min + 1).max(0) as u32;
        send_event(&EngineEvent::WorldGenProgress {
            completed: 0,
            total: total_columns,
            phase: Some("scan".into()),
        });
    }

    /// Reacciona a un cambio de las caras del mundo cuando el plugin ya generó
    /// terreno: crece solo el área nueva (dentro del margen generado) o recorta
    /// la zona que quedó fuera. Sin DLL/mundo generado no hace nada: el motor
    /// normal nunca usa esta ruta.
    pub(crate) fn worldgen_world_bounds_changed(&mut self, old: WorldBounds3D) {
        if !self.worldgen.generated
            || self.worldgen.library.is_none()
            || self.worldgen.world.is_none()
        {
            return;
        }
        let new = self.world_bounds_3d;
        if same_world_xz(&old, &new) {
            return;
        }
        let Some(area) = self.worldgen.generation else {
            return;
        };
        // Una generación interrumpida (o parada a medias) no tiene garantías de
        // cubrir el mundo: se reconstruye entera. Mientras genera, también se
        // reinicia para no dejar huecos interiores en la cola cancelada.
        if self.worldgen.status != WorldgenStatus::Complete {
            let replace_ground = self.worldgen.replace_ground;
            self.worldgen_start(replace_ground, false);
            return;
        }
        if !area.contains(&new) {
            log::info!(
                "[worldgen] el mundo creció fuera del área generada ({:.0}×{:.0} m): regenerando",
                area.width,
                area.depth
            );
            self.worldgen_start(false, false);
            return;
        }
        self.worldgen_retrim(old);
    }

    /// Ajusta el terreno ya generado a los límites nuevos sin reconstruirlo:
    /// conserva los chunks interiores, re-meshea los que tocan un borde y
    /// elimina los que quedaron fuera.
    fn worldgen_retrim(&mut self, old: WorldBounds3D) {
        let Some(config) = self
            .worldgen
            .config_json
            .as_deref()
            .and_then(|json| serde_json::from_str::<WgConfigLite>(json).ok())
        else {
            return;
        };
        let Some(area) = self.worldgen.generation else {
            return;
        };

        let voxel = config.resolution.voxel_size.max(0.001);
        let cell = CHUNK_SIZE as f32 * voxel;
        let (chunks_x, chunks_z, max_y) =
            chunk_grid_size(area.width, area.depth, config.bounds.height, voxel);
        let new = self.world_bounds_3d;
        let old_columns = chunk_columns_for_bounds(&old, &area, cell, chunks_x, chunks_z);
        let Some(new_columns) = chunk_columns_for_bounds(&new, &area, cell, chunks_x, chunks_z)
        else {
            for key in self.worldgen.spawned.keys().copied().collect::<Vec<_>>() {
                self.worldgen_remove_spawned_chunk(key);
            }
            self.worldgen.pending.clear();
            self.worldgen.scan = None;
            self.worldgen.total_chunks = 0;
            self.worldgen.completed_chunks = 0;
            self.worldgen.replace_ground = false;
            self.worldgen_set_status(WorldgenStatus::Complete);
            send_event(&EngineEvent::WorldGenStatus {
                state: self.worldgen.status.as_str().to_string(),
            });
            send_event(&EngineEvent::WorldGenCompleted { chunks: 0 });
            return;
        };

        // 1) Chunks fuera del nuevo rectángulo: se borran con su collider y malla.
        let outside: Vec<ChunkKey> = self
            .worldgen
            .spawned
            .keys()
            .filter(|(cx, _, cz)| !new_columns.contains(*cx, *cz))
            .copied()
            .collect();
        for key in outside {
            self.worldgen_remove_spawned_chunk(key);
        }

        // 2) Chunks cortados por un borde que se movió: hay que re-meshearlos
        //    para que el recorte coincida con el rectángulo actual.
        let mut remesh = std::collections::HashSet::new();
        for key in self.worldgen.spawned.keys().copied() {
            if chunk_touches_changed_bounds(key, &old, &new, &area, cell) {
                remesh.insert(key);
            }
        }

        // 3) Columnas nuevas (zona añadida) y columnas del borde movido, donde
        //    un chunk pudo quedar vacío al recortarse y ahora vuelve a ser
        //    visible (o al revés).
        let mut columns = std::collections::HashSet::new();
        for cz in new_columns.cz_min..=new_columns.cz_max {
            for cx in new_columns.cx_min..=new_columns.cx_max {
                let newly_exposed = old_columns.is_none_or(|old| !old.contains(cx, cz));
                let boundary = column_touches_changed_bounds(cx, cz, &old, &new, &area, cell);
                if newly_exposed || boundary {
                    columns.insert((cx, cz));
                }
            }
        }

        // 4) Scatter: las instancias que quedaron fuera del mundo desaparecen.
        self.worldgen_drop_placements_outside(&new);

        let cell_camera = self.camera_world_position();
        let mut collected: Vec<PendingChunk> = remesh
            .into_iter()
            .map(|key| PendingChunk { key, lod: 0 })
            .collect();
        let mut columns: Vec<(i32, i32)> = columns.into_iter().collect();
        columns.sort_unstable();

        self.worldgen.pending.clear();
        self.worldgen.replace_ground = false;
        self.worldgen.total_chunks = 0;
        self.worldgen.completed_chunks = 0;

        if columns.is_empty() {
            if collected.is_empty() {
                self.worldgen_set_status(WorldgenStatus::Complete);
                send_event(&EngineEvent::WorldGenStatus {
                    state: self.worldgen.status.as_str().to_string(),
                });
                send_event(&EngineEvent::WorldGenCompleted { chunks: 0 });
                return;
            }
            dedupe_pending(&mut collected);
            let origin = self.worldgen.scene_offset;
            finalize_pending(&mut collected, cell, cell_camera, origin);
            self.worldgen.total_chunks = collected.len() as u32;
            self.worldgen.pending = collected;
            self.worldgen.scan = None;
            self.worldgen_set_status(WorldgenStatus::Generating);
            send_event(&EngineEvent::WorldGenStatus {
                state: self.worldgen.status.as_str().to_string(),
            });
            send_event(&EngineEvent::WorldGenProgress {
                completed: 0,
                total: self.worldgen.total_chunks,
                phase: Some("mesh".into()),
            });
            return;
        }

        let total_columns = columns.len() as u32;
        self.worldgen.scan = Some(ScanCursor {
            cx: 0,
            cz: 0,
            cx_min: 0,
            cx_max: 0,
            cz_min: 0,
            cz_max: 0,
            max_y,
            cell,
            camera: cell_camera.to_array(),
            collected,
            columns: Some(columns),
            column_index: 0,
        });
        self.worldgen_set_status(WorldgenStatus::Generating);
        send_event(&EngineEvent::WorldGenStatus {
            state: self.worldgen.status.as_str().to_string(),
        });
        send_event(&EngineEvent::WorldGenProgress {
            completed: 0,
            total: total_columns,
            phase: Some("scan".into()),
        });
    }

    /// `world_gen_stop`: vacía la cola de chunks. El handle de la DLL sigue
    /// vivo hasta `world_gen_reset` o `world_gen_unload`. Los chunks ya en
    /// escena se mantienen hasta el reset.
    pub(crate) fn worldgen_stop(&mut self) {
        self.worldgen_cancel_worker();
        self.worldgen.replace_ground = false;
        self.worldgen.scatter_jobs.clear();
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
        self.worldgen.generation = None;
        self.worldgen.generated = false;
    }

    /// `world_gen_unload`: suelta escena, handle y DLL para poder borrar el
    /// archivo (desactivar o desinstalar el plugin).
    pub(crate) fn worldgen_unload(&mut self) {
        self.worldgen_stop();
        self.worldgen_despawn_scene();
        self.worldgen_release_world();
        self.worldgen.library = None;
        self.worldgen.library_path = None;
        self.worldgen.generation = None;
        self.worldgen.generated = false;
        self.worldgen_set_status(WorldgenStatus::MissingLibrary);
        send_event(&EngineEvent::WorldGenStatus {
            state: self.worldgen.status.as_str().to_string(),
        });
    }

    /// `world_gen_scatter`: distribuye un blueprint según las reglas y coloca las
    /// instancias por la vía existente (`spawn_cached_model_part_at` con
    /// `blueprint_id`), compartiendo los recursos GPU del blueprint.
    pub(crate) fn worldgen_scatter(&mut self, rules_json: &str) {
        if self.worldgen.inflight.is_some() {
            send_event(&EngineEvent::WorldGenError {
                message: "world_gen_scatter: la generación sigue en curso".to_string(),
            });
            return;
        }
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

        // 3) Encola las colocaciones; el tick las spawnea con presupuesto.
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
        let origin = self.worldgen.scene_offset;
        let remaining: VecDeque<PendingScatterPose> = placements
            .iter()
            .enumerate()
            .filter_map(|(index, placement)| {
                let position = [
                    placement.position[0] + origin.x,
                    placement.position[1] + origin.y,
                    placement.position[2] + origin.z,
                ];
                if !self.world_bounds_3d.contains_xz(position[0], position[2]) {
                    return None;
                }
                Some(PendingScatterPose {
                    position,
                    rotation: glam::Quat::from_rotation_y(placement.yaw).to_array(),
                    scale: placement.scale,
                    index,
                })
            })
            .collect();
        if remaining.is_empty() {
            send_event(&EngineEvent::WorldGenScatterDone {
                blueprint_id,
                count: 0,
            });
            return;
        }
        self.worldgen.scatter_jobs.push_back(PendingScatterJob {
            blueprint_id,
            model_path,
            display,
            mesh_idx: part.mesh_idx,
            tex_idx: part.tex_idx,
            category: meta.category.clone(),
            physics_enabled: meta
                .physics_enabled
                .unwrap_or(meta.colision.unwrap_or(false)),
            physics_type: meta
                .physics_type
                .clone()
                .unwrap_or_else(|| "dynamic".to_string()),
            local_bounds: part.local_bounds,
            roughness: part.roughness,
            metallic: part.metallic,
            ior: part.ior,
            remaining,
            placed: 0,
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
        if self.worldgen.library.is_some() && self.worldgen.config_json.is_some() {
            self.worldgen_start(false, false);
        }
        send_event(&EngineEvent::WorldGenStatus {
            state: self.worldgen.status.as_str().to_string(),
        });
    }

    // -- Tick por frame -------------------------------------------------------

    /// Drena el worker y sube como máximo [`CHUNKS_PER_FRAME`] mallas a GPU.
    /// Scan y `wg_mesh_chunk` corren en `rer-worldgen`, no en este hilo.
    /// El scatter se drena siempre (también tras Complete) para no congelar.
    pub(crate) fn worldgen_tick(&mut self) {
        self.worldgen_tick_scatter();
        if self.worldgen.status != WorldgenStatus::Generating {
            return;
        }
        if self.worldgen.inflight.is_none()
            && (self.worldgen.scan.is_some() || !self.worldgen.pending.is_empty())
        {
            self.worldgen_dispatch_generation();
        }
        self.worldgen_drain_worker();
        let mut uploaded = 0usize;
        while uploaded < CHUNKS_PER_FRAME {
            let Some(chunk) = self.worldgen.gpu_queue.pop_front() else {
                break;
            };
            if self.worldgen_upload_cpu_chunk(chunk).is_err() {
                return;
            }
            uploaded += 1;
        }
        if uploaded > 0 {
            send_event(&EngineEvent::WorldGenProgress {
                completed: self.worldgen.completed_chunks,
                total: self.worldgen.total_chunks,
                phase: Some("mesh".into()),
            });
        }
        if self.worldgen.status == WorldgenStatus::Generating
            && self.worldgen.inflight.is_none()
            && self.worldgen.gpu_queue.is_empty()
            && self.worldgen.scan.is_none()
            && self.worldgen.pending.is_empty()
        {
            self.worldgen_finish_generation();
        }
    }

    fn worldgen_ensure_worker(&mut self) {
        if self.worldgen.job_tx.is_some() {
            return;
        }
        let live = Arc::clone(&self.worldgen.live_generation);
        let (job_tx, result_rx, join) = spawn_worldgen_worker(live);
        self.worldgen.job_tx = Some(job_tx);
        self.worldgen.result_rx = Some(result_rx);
        self.worldgen.worker = Some(join);
    }

    fn worldgen_cancel_worker(&mut self) {
        let Some(old) = self.worldgen.inflight else {
            self.worldgen.gpu_queue.clear();
            return;
        };
        self.worldgen.generation_id = self.worldgen.generation_id.wrapping_add(1);
        self.worldgen
            .live_generation
            .store(self.worldgen.generation_id, Ordering::Release);
        self.worldgen.gpu_queue.clear();
        let deadline = Instant::now() + Duration::from_secs(30);
        while self.worldgen.inflight == Some(old) && Instant::now() < deadline {
            if !self.worldgen_try_recv_worker() {
                std::thread::yield_now();
            }
        }
        self.worldgen.gpu_queue.clear();
    }

    fn worldgen_dispatch_generation(&mut self) {
        self.worldgen_ensure_worker();
        let Some(ffi) = self.worldgen.library.as_ref().map(WorldgenLibrary::ffi) else {
            return;
        };
        let handle = self
            .worldgen
            .world
            .as_ref()
            .map(|world| world.0 as usize)
            .unwrap_or(0);
        let create_json = if handle == 0 {
            self.worldgen.config_json.clone()
        } else {
            None
        };
        if create_json.is_none() && handle == 0 {
            return;
        }
        self.worldgen.generation_id = self.worldgen.generation_id.wrapping_add(1);
        let generation = self.worldgen.generation_id;
        self.worldgen
            .live_generation
            .store(generation, Ordering::Release);
        self.worldgen.gpu_queue.clear();
        let job = WorldgenJob::Run(Box::new(WorldgenRunJob {
            generation,
            handle,
            create_json,
            ffi,
            scan: self.worldgen.scan.take(),
            pending: std::mem::take(&mut self.worldgen.pending),
            origin: self.worldgen.scene_offset.to_array(),
        }));
        self.worldgen.inflight = Some(generation);
        if let Some(tx) = self.worldgen.job_tx.as_ref()
            && tx.send(job).is_err()
        {
            self.worldgen.inflight = None;
            self.worldgen_set_status(WorldgenStatus::LibraryError);
            send_event(&EngineEvent::WorldGenError {
                message: "hilo rer-worldgen cerrado".into(),
            });
        }
    }

    fn worldgen_drain_worker(&mut self) {
        while self.worldgen_try_recv_worker() {}
    }

    fn worldgen_try_recv_worker(&mut self) -> bool {
        let msg = {
            let Some(rx) = self.worldgen.result_rx.as_ref() else {
                return false;
            };
            match rx.try_recv() {
                Ok(msg) => msg,
                Err(_) => return false,
            }
        };
        let live = self.worldgen.generation_id;
        match msg {
            WorldgenWorkerMsg::WorldCreated { generation, handle } => {
                let ptr = handle as *mut std::ffi::c_void;
                if generation == live {
                    if let Some(old) = self.worldgen.world.take()
                        && let Some(library) = self.worldgen.library.as_ref()
                    {
                        // # Seguridad: handle previo de esta misma biblioteca.
                        unsafe { (library.world_free)(old.0) };
                    }
                    self.worldgen.world = Some(WgWorldHandle(ptr));
                } else if let Some(library) = self.worldgen.library.as_ref() {
                    // # Seguridad: job obsoleto; el worker ya no posee el handle.
                    unsafe { (library.world_free)(ptr) };
                }
            }
            WorldgenWorkerMsg::ScanProgress {
                generation,
                completed,
                total,
            } if generation == live => {
                send_event(&EngineEvent::WorldGenProgress {
                    completed,
                    total,
                    phase: Some("scan".into()),
                });
            }
            WorldgenWorkerMsg::MeshPhase { generation, total } if generation == live => {
                self.worldgen.total_chunks = total;
                self.worldgen.completed_chunks = 0;
                send_event(&EngineEvent::WorldGenProgress {
                    completed: 0,
                    total,
                    phase: Some("mesh".into()),
                });
            }
            WorldgenWorkerMsg::ChunkReady { generation, chunk } if generation == live => {
                self.worldgen.gpu_queue.push_back(chunk);
            }
            WorldgenWorkerMsg::Failed {
                generation,
                message,
            } => {
                if self.worldgen.inflight == Some(generation) {
                    self.worldgen.inflight = None;
                }
                if generation == live {
                    self.worldgen.gpu_queue.clear();
                    self.worldgen.replace_ground = false;
                    self.worldgen.last_error = Some(message.clone());
                    self.worldgen_set_status(WorldgenStatus::LibraryError);
                    send_event(&EngineEvent::WorldGenError { message });
                }
            }
            WorldgenWorkerMsg::Cancelled { generation }
            | WorldgenWorkerMsg::Finished { generation }
                if self.worldgen.inflight == Some(generation) =>
            {
                self.worldgen.inflight = None;
            }
            _ => {}
        }
        true
    }

    fn worldgen_finish_generation(&mut self) {
        let replace_ground = self.worldgen.replace_ground;
        self.worldgen.replace_ground = false;
        if self.worldgen.total_chunks > 0 {
            send_event(&EngineEvent::WorldGenProgress {
                completed: self.worldgen.completed_chunks,
                total: self.worldgen.total_chunks,
                phase: Some("mesh".into()),
            });
        }
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
    }

    // -- Internos -------------------------------------------------------------

    fn worldgen_set_status(&mut self, status: WorldgenStatus) {
        self.worldgen.status = status;
    }

    /// Libera el handle con la biblioteca que lo creó. No suelta la DLL.
    fn worldgen_release_world(&mut self) {
        self.worldgen_cancel_worker();
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
            self.worldgen_remove_spawned_chunk(key);
        }
        let placements = std::mem::take(&mut self.worldgen.placements);
        for placement in placements {
            if placement.entity_id != 0 {
                self.world.despawn(placement.entity_id);
            }
        }
    }

    /// Quita un chunk de la escena: collider, malla y entidad, sin tocar el
    /// resto de índices de `self.meshes`.
    fn worldgen_remove_spawned_chunk(&mut self, key: ChunkKey) {
        let Some(chunk) = self.worldgen.spawned.remove(&key) else {
            return;
        };
        self.physics.remove_entity_body(chunk.entity_id);
        self.entity_colision.remove(&chunk.entity_id);
        self.worldgen.entity_splat_tile.remove(&chunk.entity_id);
        self.worldgen_retire_mesh(chunk.mesh_idx);
        self.world.despawn(chunk.entity_id);
    }

    /// Despacha las instancias de scatter fuera del mundo visible (p. ej. tras
    /// encoger una cara) y deja solo las que siguen dentro.
    fn worldgen_drop_placements_outside(&mut self, bounds: &WorldBounds3D) {
        let placements = std::mem::take(&mut self.worldgen.placements);
        for placement in placements {
            if bounds.contains_xz(placement.position[0], placement.position[2]) {
                self.worldgen.placements.push(placement);
            } else if placement.entity_id != 0 {
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
        for triangle in indices.as_chunks::<3>().0 {
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

    /// Quita escena y handle previos. `wg_world_create` corre en el worker.
    fn worldgen_reset_generation_state(&mut self) {
        self.worldgen_cancel_worker();
        self.worldgen_despawn_scene();
        self.worldgen_release_world();
        self.worldgen.scatter_jobs.clear();
        self.worldgen.pending.clear();
        self.worldgen.scan = None;
        self.worldgen.column_epoch = self.worldgen.column_epoch.wrapping_add(1);
        self.worldgen.columns.clear();
        self.worldgen.entity_splat_tile.clear();
        self.worldgen.splat_atlas.next_tile = 0;
        self.worldgen.total_chunks = 0;
        self.worldgen.completed_chunks = 0;
    }

    /// Coloca como máximo [`PLACEMENTS_PER_FRAME`] instancias de scatter.
    fn worldgen_tick_scatter(&mut self) {
        let mut spawned = 0usize;
        while spawned < PLACEMENTS_PER_FRAME {
            if self
                .worldgen
                .scatter_jobs
                .front()
                .is_some_and(|job| job.remaining.is_empty())
            {
                if let Some(done) = self.worldgen.scatter_jobs.pop_front() {
                    send_event(&EngineEvent::WorldGenScatterDone {
                        blueprint_id: done.blueprint_id,
                        count: done.placed,
                    });
                }
                continue;
            }
            let Some(front) = self.worldgen.scatter_jobs.front() else {
                break;
            };
            let mesh_idx = front.mesh_idx;
            let tex_idx = front.tex_idx;
            let model_path = front.model_path.clone();
            let display = front.display.clone();
            let blueprint_id = front.blueprint_id.clone();
            let category = front.category.clone();
            let physics_enabled = front.physics_enabled;
            let physics_type = front.physics_type.clone();
            let local_bounds = front.local_bounds;
            let roughness = front.roughness;
            let metallic = front.metallic;
            let ior = front.ior;
            let Some(pose) = self
                .worldgen
                .scatter_jobs
                .front_mut()
                .and_then(|job| job.remaining.pop_front())
            else {
                break;
            };
            let name = format!("{} #{}", display, pose.index);
            let entity_id = self.spawn_cached_model_part_at(
                mesh_idx,
                tex_idx,
                &model_path,
                pose.position,
                pose.rotation,
                pose.scale,
                &name,
                "model",
                Some(blueprint_id.clone()),
                category,
                physics_enabled,
                &physics_type,
                local_bounds,
                None,
                roughness,
                metallic,
                ior,
                false,
            );
            self.worldgen.placements.push(WorldgenPlacement {
                blueprint_id,
                position: pose.position,
                rotation: pose.rotation,
                scale: pose.scale,
                entity_id,
            });
            if let Some(job) = self.worldgen.scatter_jobs.front_mut() {
                job.placed += 1;
            }
            spawned += 1;
        }
    }

    /// Sube a GPU un chunk ya mesheado en el worker. Recorta al mundo visible.
    fn worldgen_upload_cpu_chunk(&mut self, chunk: PendingCpuChunk) -> Result<(), ()> {
        let key = chunk.key;
        let (cx, cy, cz) = key;
        self.worldgen_remove_spawned_chunk(key);

        let (mut vertices, indices) = clip_chunk_to_world_bounds(
            &chunk.vertices,
            &chunk.indices,
            self.worldgen.scene_offset,
            self.world_bounds_3d.center().x,
            self.world_bounds_3d.center().z,
            self.world_bounds_3d.width() * 0.5,
            self.world_bounds_3d.depth() * 0.5,
        );
        clamp_chunk_uvs(&mut vertices);

        self.worldgen.completed_chunks += 1;
        if vertices.is_empty() || indices.is_empty() {
            send_event(&EngineEvent::WorldGenChunkReady {
                cx,
                cy,
                cz,
                entity_id: 0,
            });
            return Ok(());
        }

        let column_epoch = self.worldgen.column_epoch;
        let tex_idx = match worldgen_stamp_column_splat(
            &mut self.texture_array,
            &self.vulkan,
            &mut self.worldgen.columns,
            &mut self.worldgen.splat_atlas,
            &mut self.tex_layers,
            column_epoch,
            cx,
            cz,
            chunk
                .splat
                .as_ref()
                .map(|(pixels, dim)| (pixels.as_slice(), *dim)),
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
        if let Some(&(_, tile, _)) = self.worldgen.columns.get(&(cx, cz)) {
            self.worldgen.entity_splat_tile.insert(entity_id, tile);
        }
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

    /// Sube Grass, Dirt, Clay, Sand, Stone, Snow y Mud a 1024 con mips, en ese orden.
    /// El 4K se descarta al empaquetar: de lejos el GPU usa un mip pequeño.
    fn ensure_terrain_materials(&mut self) {
        if self.worldgen.material_base.is_some() {
            return;
        }
        let Some(dir) = terrain_material_dir(self.worldgen.library_path.as_deref()) else {
            log::warn!("[worldgen] no están las texturas de terreno junto al plugin");
            return;
        };
        // Capa 0 = checker del editor. Grass es la primera foto: si queda al lado,
        // un sampler LINEAR mezcla ambas y la pradera se ve a cuadros.
        if !self.pack_terrain_isolation_layer() {
            return;
        }
        let mut layers = [0u32; 7];
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

/// Sella en el atlas un splat ya copiado por el worker.
fn worldgen_stamp_column_splat(
    texture_array: &mut crate::engine3d::texture::TextureArray,
    vulkan: &crate::engine3d::vulkan::SharedGpu,
    columns: &mut HashMap<(i32, i32), (usize, u32, u64)>,
    atlas: &mut SplatAtlas,
    tex_layers: &mut Vec<crate::engine3d::texture::TextureLayer>,
    epoch: u64,
    cx: i32,
    cz: i32,
    splat: Option<(&[u8], u32)>,
) -> Option<usize> {
    if let Some(&(tex_idx, _, cached_epoch)) = columns.get(&(cx, cz))
        && cached_epoch == epoch
    {
        return Some(tex_idx);
    }
    let Some((pixels, dim)) = splat else {
        return columns.get(&(cx, cz)).map(|(tex_idx, _, _)| *tex_idx);
    };

    if let Some(&(tex_idx, tile, _)) = columns.get(&(cx, cz)) {
        if stamp_splat_tile(
            texture_array,
            vulkan,
            tex_layers,
            tex_idx,
            tile,
            pixels,
            dim,
        ) {
            columns.insert((cx, cz), (tex_idx, tile, epoch));
            return Some(tex_idx);
        }
        return Some(tex_idx);
    }

    if let Some((tex_idx, tile)) = alloc_splat_tile(texture_array, vulkan, atlas, tex_layers) {
        if stamp_splat_tile(
            texture_array,
            vulkan,
            tex_layers,
            tex_idx,
            tile,
            pixels,
            dim,
        ) {
            columns.insert((cx, cz), (tex_idx, tile, epoch));
            return Some(tex_idx);
        }
        log::warn!("[worldgen] no se pudo sellar el splat de la columna ({cx},{cz}) en el atlas");
    } else {
        log::warn!("[worldgen] atlas de splats lleno: la columna ({cx},{cz}) usa el fallback");
    }
    columns.get(&(cx, cz)).map(|(tex_idx, _, _)| *tex_idx)
}

/// Reserva un tile 64×64 en el atlas. Una página = 256 columnas; no pisa tiles
/// de columnas vivas. Si el array GPU está lleno, `None` (el caller usa fallback).
fn alloc_splat_tile(
    texture_array: &mut crate::engine3d::texture::TextureArray,
    vulkan: &crate::engine3d::vulkan::SharedGpu,
    atlas: &mut SplatAtlas,
    tex_layers: &mut Vec<crate::engine3d::texture::TextureLayer>,
) -> Option<(usize, u32)> {
    let per_page = SPLAT_ATLAS_TILES * SPLAT_ATLAS_TILES;
    let page = (atlas.next_tile / per_page) as usize;
    let tile = atlas.next_tile % per_page;
    while atlas.pages.len() <= page {
        let blank = vec![0u8; (SPLAT_ATLAS_TILE_PX * SPLAT_ATLAS_TILE_PX * 4) as usize];
        match texture_array.pack(vulkan, &blank, SPLAT_ATLAS_TILE_PX, SPLAT_ATLAS_TILE_PX) {
            Ok(gpu_layer)
                if gpu_layer != crate::engine3d::texture::TextureArray::fallback_layer() =>
            {
                let tex_idx = tex_layers.len();
                tex_layers.push(gpu_layer);
                atlas.pages.push(tex_idx);
            }
            Ok(_) => {
                log::warn!("[worldgen] no hay capa GPU libre para una página del atlas de splats");
                return None;
            }
            Err(error) => {
                log::warn!("[worldgen] no se pudo crear una página del atlas de splats: {error}");
                return None;
            }
        }
    }
    let tex_idx = *atlas.pages.get(page)?;
    atlas.next_tile = atlas.next_tile.saturating_add(1);
    Some((tex_idx, tile))
}

/// Copia el splat (lod 0, 64×64) en el tile del atlas, sin inflarlo a 1024².
fn stamp_splat_tile(
    texture_array: &crate::engine3d::texture::TextureArray,
    vulkan: &crate::engine3d::vulkan::SharedGpu,
    tex_layers: &[crate::engine3d::texture::TextureLayer],
    tex_idx: usize,
    tile: u32,
    pixels: &[u8],
    dim: u32,
) -> bool {
    let Some(&gpu_layer) = tex_layers.get(tex_idx) else {
        return false;
    };
    if dim != SPLAT_ATLAS_TILE_PX {
        log::warn!("[worldgen] splat {dim}×{dim}, el atlas espera {SPLAT_ATLAS_TILE_PX}");
        return false;
    }
    let (tx, tz) = splat_tile_xy(tile);
    let x = tx * SPLAT_ATLAS_TILE_PX;
    let y = tz * SPLAT_ATLAS_TILE_PX;
    match texture_array.blit_layer_tile(vulkan, gpu_layer, x, y, pixels, dim, dim) {
        Ok(()) => true,
        Err(error) => {
            log::warn!("[worldgen] no se pudo copiar el tile de splat: {error}");
            false
        }
    }
}

fn splat_tile_xy(tile: u32) -> (u32, u32) {
    (tile % SPLAT_ATLAS_TILES, tile / SPLAT_ATLAS_TILES)
}

/// El recorte al disco interpola UVs; un valor fuera de `[0, 1]` con sampler
/// REPEAT envuelve a la capa 0 (checker) y pinta damero en el terreno.
fn clamp_chunk_uvs(vertices: &mut [crate::engine3d::mesh::Vertex]) {
    for vertex in vertices {
        vertex.uv[0] = vertex.uv[0].clamp(0.0, 1.0);
        vertex.uv[1] = vertex.uv[1].clamp(0.0, 1.0);
    }
}

/// Ordena por capa alta y cercanía, asigna LOD y deja el vector listo para
/// `pop` (el primero a generar queda al final).
pub(super) fn finalize_pending(
    pending: &mut [PendingChunk],
    cell: f32,
    camera: Vec3,
    origin: Vec3,
) {
    pending.sort_by(|a, b| {
        let score = |c: &PendingChunk| -> (i32, f32) {
            let chunk_center = origin
                + Vec3::new(
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
        let chunk_center = origin
            + Vec3::new(
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

const TERRAIN_MATERIAL_FILES: [&str; 7] = [
    "Grass.jpg",
    "Dirt.jpg",
    "Clay.jpg",
    "Sand.jpg",
    "Stone.jpg",
    "Snow.jpg",
    "Mud.jpg",
];

/// Fotos de suelo del plugin: `terrain/` al lado de la DLL, o el checkout
/// `worldgen/assets/terrain` en desarrollo.
fn terrain_material_dir(library_path: Option<&str>) -> Option<std::path::PathBuf> {
    let mut dirs = Vec::new();
    if let Some(parent) = library_path.and_then(|path| std::path::Path::new(path).parent()) {
        dirs.push(parent.join("terrain"));
    }
    if let Ok(env_dll) = std::env::var("RER_WORLDGEN_DLL")
        && let Some(parent) = std::path::Path::new(&env_dll).parent()
    {
        dirs.push(parent.join("terrain"));
    }
    dirs.push(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../worldgen/assets/terrain"),
    );
    dirs.into_iter()
        .find(|dir| dir.join(TERRAIN_MATERIAL_FILES[0]).is_file())
}

#[cfg(test)]
mod terrain_material_tests {
    use super::{TERRAIN_MATERIAL_FILES, terrain_material_dir};

    #[test]
    fn terrain_photos_are_on_disk_with_grass_first() {
        assert_eq!(TERRAIN_MATERIAL_FILES[0], "Grass.jpg");
        let dir = terrain_material_dir(None).expect("worldgen/assets/terrain/Grass.jpg");
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
        let path = terrain_material_dir(None)
            .expect("worldgen/assets/terrain")
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
#[cfg(test)]
fn generated_footprint(config: &WgConfigLite) -> (f32, f32) {
    let (chunks_x, chunks_z, _) = chunk_grid(config);
    let cell = crate::engine3d::config_3d::worldgen::abi::CHUNK_SIZE as f32
        * config.resolution.voxel_size.max(0.001);
    (chunks_x as f32 * cell, chunks_z as f32 * cell)
}

/// Esquina mínima del mundo (XZ) y apoyo del terreno en y = 0.
fn scene_offset(min_x: f32, min_z: f32) -> Vec3 {
    Vec3::new(min_x, -(TERRAIN_FLOOR + TERRAIN_PLAINS_CLEARANCE), min_z)
}

/// Radio equivalente del generador (solo para escalas de features/tests).
#[cfg(test)]
fn generator_world_radius(extent_x: f32, extent_z: f32) -> f32 {
    extent_x.min(extent_z) * 0.5
}

/// Rejilla de chunks que cubren los límites del mundo.
#[cfg(test)]
fn chunk_grid(config: &WgConfigLite) -> (i32, i32, i32) {
    chunk_grid_size(
        config.bounds.width,
        config.bounds.length,
        config.bounds.height,
        config.resolution.voxel_size,
    )
}

/// Rejilla de chunks de un rectángulo de generación (ancho × largo).
fn chunk_grid_size(width: f32, length: f32, height: f32, voxel: f32) -> (i32, i32, i32) {
    let cell = CHUNK_SIZE as f32 * voxel.max(0.001);
    let chunks_x = (width / cell).ceil().max(1.0) as i32;
    let chunks_z = (length / cell).ceil().max(1.0) as i32;
    let max_y = (height / cell).ceil() as i32;
    (chunks_x, chunks_z, max_y)
}

/// Margen de crecimiento alrededor del mundo visible, por lado (100 %). El
/// área generada puede triplicar el mundo; el grade, los rasgos y su densidad
/// se escalan al tamaño visible (`visibleWidth/visibleLength`).
const GENERATION_MARGIN_FRACTION: f32 = 1.0;
/// Límites del rectángulo que acepta `WorldGenConfig` (`validate`).
const GENERATION_MIN_SPAN: f32 = 30.0;
const GENERATION_MAX_SPAN: f32 = 2000.0;

/// Rectángulo que entrega el plugin: el mundo visible más un margen, para que
/// crecer una cara solo genere la franja nueva.
fn generation_area(bounds: &WorldBounds3D) -> GenerationArea {
    let width = (bounds.width() * (1.0 + GENERATION_MARGIN_FRACTION * 2.0))
        .clamp(GENERATION_MIN_SPAN, GENERATION_MAX_SPAN);
    let depth = (bounds.depth() * (1.0 + GENERATION_MARGIN_FRACTION * 2.0))
        .clamp(GENERATION_MIN_SPAN, GENERATION_MAX_SPAN);
    let center = bounds.center();
    GenerationArea {
        min_x: center.x - width * 0.5,
        min_z: center.z - depth * 0.5,
        width,
        depth,
    }
}

/// Copia el JSON de config fijando el área generada y el rectángulo visible
/// (grade/rasgos) dentro de esa área.
fn config_with_bounds(config_json: &str, area: &GenerationArea, world: &WorldBounds3D) -> String {
    let mut value: serde_json::Value =
        serde_json::from_str(config_json).unwrap_or(serde_json::Value::Null);
    if let Some(bounds) = value.get_mut("bounds") {
        bounds["width"] = serde_json::json!(area.width);
        bounds["length"] = serde_json::json!(area.depth);
        bounds["visibleWidth"] = serde_json::json!(world.width());
        bounds["visibleLength"] = serde_json::json!(world.depth());
        bounds["visibleOffsetX"] = serde_json::json!(world.min_x() - area.min_x);
        bounds["visibleOffsetZ"] = serde_json::json!(world.min_z() - area.min_z);
    }
    if value.is_object() {
        value.to_string()
    } else {
        config_json.to_string()
    }
}

fn same_world_xz(a: &WorldBounds3D, b: &WorldBounds3D) -> bool {
    const EPS: f32 = 1.0e-3;
    (a.min_x() - b.min_x()).abs() < EPS
        && (a.max_x() - b.max_x()).abs() < EPS
        && (a.min_z() - b.min_z()).abs() < EPS
        && (a.max_z() - b.max_z()).abs() < EPS
}

/// Columnas de chunk (inclusivas) que se solapan con `bounds` dentro del área
/// generada. `None` si no hay solape.
fn chunk_columns_for_bounds(
    bounds: &WorldBounds3D,
    area: &GenerationArea,
    cell: f32,
    chunks_x: i32,
    chunks_z: i32,
) -> Option<ChunkColumns> {
    if cell <= 0.0 || chunks_x <= 0 || chunks_z <= 0 {
        return None;
    }
    let cx_min = ((bounds.min_x() - area.min_x) / cell).floor() as i32;
    let cx_max = (((bounds.max_x() - area.min_x) / cell).ceil() as i32) - 1;
    let cz_min = ((bounds.min_z() - area.min_z) / cell).floor() as i32;
    let cz_max = (((bounds.max_z() - area.min_z) / cell).ceil() as i32) - 1;
    if cx_max < 0 || cx_min >= chunks_x || cz_max < 0 || cz_min >= chunks_z {
        return None;
    }
    Some(ChunkColumns {
        cx_min: cx_min.clamp(0, chunks_x - 1),
        cx_max: cx_max.clamp(0, chunks_x - 1),
        cz_min: cz_min.clamp(0, chunks_z - 1),
        cz_max: cz_max.clamp(0, chunks_z - 1),
    })
}

/// `true` si un borde que cambió entre `old` y `new` (solo caras movidas)
/// corta la celda del chunk. Evita re-meshear los tres lados que no se tocaron.
fn chunk_touches_changed_bounds(
    key: ChunkKey,
    old: &WorldBounds3D,
    new: &WorldBounds3D,
    area: &GenerationArea,
    cell: f32,
) -> bool {
    let (min_x, max_x, min_z, max_z) = chunk_world_rect(key.0, key.2, area, cell);
    const EPS: f32 = 1.0e-3;
    let cuts = |value: f32| value >= min_x && value <= max_x;
    let cuts_z = |value: f32| value >= min_z && value <= max_z;
    let mut changed = false;
    if (old.min_x() - new.min_x()).abs() > EPS {
        changed |= cuts(old.min_x()) || cuts(new.min_x());
    }
    if (old.max_x() - new.max_x()).abs() > EPS {
        changed |= cuts(old.max_x()) || cuts(new.max_x());
    }
    if (old.min_z() - new.min_z()).abs() > EPS {
        changed |= cuts_z(old.min_z()) || cuts_z(new.min_z());
    }
    if (old.max_z() - new.max_z()).abs() > EPS {
        changed |= cuts_z(old.max_z()) || cuts_z(new.max_z());
    }
    changed
}

/// `true` si un borde movido corta la columna XZ del chunk.
fn column_touches_changed_bounds(
    cx: i32,
    cz: i32,
    old: &WorldBounds3D,
    new: &WorldBounds3D,
    area: &GenerationArea,
    cell: f32,
) -> bool {
    chunk_touches_changed_bounds((cx, 0, cz), old, new, area, cell)
}

fn chunk_world_rect(cx: i32, cz: i32, area: &GenerationArea, cell: f32) -> (f32, f32, f32, f32) {
    (
        area.min_x + cx as f32 * cell,
        area.min_x + (cx + 1) as f32 * cell,
        area.min_z + cz as f32 * cell,
        area.min_z + (cz + 1) as f32 * cell,
    )
}

pub(super) fn dedupe_pending(pending: &mut Vec<PendingChunk>) {
    let mut seen = std::collections::HashSet::with_capacity(pending.len());
    pending.retain(|chunk| seen.insert(chunk.key));
}

// Reexport interno para que init.rs/engine.rs construyan el campo.
pub(crate) use types::WorldgenState as StateWorldgenState;

/// Recorta al disco del ecuador lo que el cuadrado de chunks deja fuera.
/// Los chunks interiores no se tocan. Solo el anillo paga el corte.
fn clip_chunk_to_world_bounds(
    vertices: &[crate::engine3d::mesh::Vertex],
    indices: &[u32],
    offset: Vec3,
    center_x: f32,
    center_z: f32,
    half_x: f32,
    half_z: f32,
) -> (Vec<crate::engine3d::mesh::Vertex>, Vec<u32>) {
    if vertices.is_empty() || indices.len() < 3 || half_x <= 0.0 || half_z <= 0.0 {
        return (vertices.to_vec(), indices.to_vec());
    }
    let world_xz = |position: [f32; 3]| {
        let x = position[0] + offset.x;
        let z = position[2] + offset.z;
        (x, z)
    };
    let inside = |position: [f32; 3]| {
        let (x, z) = world_xz(position);
        (x - center_x).abs() <= half_x && (z - center_z).abs() <= half_z
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
    if max_x < center_x - half_x
        || min_x > center_x + half_x
        || max_z < center_z - half_z
        || min_z > center_z + half_z
    {
        return (Vec::new(), Vec::new());
    }

    let mut out_vertices = Vec::new();
    let mut out_indices = Vec::new();
    let mut emit = |vertex: crate::engine3d::mesh::Vertex| -> u32 {
        let index = out_vertices.len() as u32;
        out_vertices.push(vertex);
        index
    };
    for tri in indices.as_chunks::<3>().0 {
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
                polygon.push(edge_on_world_bounds(
                    current, next, offset, center_x, center_z, half_x, half_z,
                ));
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

fn edge_on_world_bounds(
    a: crate::engine3d::mesh::Vertex,
    b: crate::engine3d::mesh::Vertex,
    offset: Vec3,
    center_x: f32,
    center_z: f32,
    half_x: f32,
    half_z: f32,
) -> crate::engine3d::mesh::Vertex {
    let t = rect_edge_t(
        a.position, b.position, offset, center_x, center_z, half_x, half_z,
    );
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
        uv: [
            mix(a.uv[0], b.uv[0]).clamp(0.0, 1.0),
            mix(a.uv[1], b.uv[1]).clamp(0.0, 1.0),
        ],
    }
}

/// `t` de salida del rectángulo centrado en `(center_x, center_z)` (mundo).
fn rect_edge_t(
    a: [f32; 3],
    b: [f32; 3],
    offset: Vec3,
    center_x: f32,
    center_z: f32,
    half_x: f32,
    half_z: f32,
) -> f32 {
    let ax = a[0] + offset.x;
    let az = a[2] + offset.z;
    let bx = b[0] + offset.x;
    let bz = b[2] + offset.z;
    let dx = bx - ax;
    let dz = bz - az;
    let mut t = 1.0_f32;
    if dx.abs() > 1e-9 {
        let tx = if dx > 0.0 {
            (center_x + half_x - ax) / dx
        } else {
            (center_x - half_x - ax) / dx
        };
        if tx >= 0.0 && tx < t {
            t = tx;
        }
    }
    if dz.abs() > 1e-9 {
        let tz = if dz > 0.0 {
            (center_z + half_z - az) / dz
        } else {
            (center_z - half_z - az) / dz
        };
        if tz >= 0.0 && tz < t {
            t = tz;
        }
    }
    t.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    //! Conformancia real: carga la DLL construida (`cargo build -p rer-worldgen`)
    //! y ejecuta el ciclo completo a través de la frontera FFI. Si la DLL no
    //! existe, el test se salta (el CI puede generarla antes si lo desea).
    use super::abi::Status;
    use super::loader::WorldgenLibrary;
    use super::{
        WgBoundsLite, WgConfigLite, WgResolutionLite, clip_chunk_to_world_bounds,
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
    fn rect_clip_drops_the_outside_and_keeps_the_interior() {
        let inside = [vert(0.0, 0.0), vert(1.0, 0.0), vert(0.0, 1.0)];
        let (kept, indices) =
            clip_chunk_to_world_bounds(&inside, &[0, 1, 2], glam::Vec3::ZERO, 0.0, 0.0, 10.0, 10.0);
        assert_eq!(kept.len(), 3);
        assert_eq!(indices, vec![0, 1, 2]);

        let outside = [vert(20.0, 20.0), vert(21.0, 20.0), vert(20.0, 21.0)];
        let (cut, cut_idx) = clip_chunk_to_world_bounds(
            &outside,
            &[0, 1, 2],
            glam::Vec3::ZERO,
            0.0,
            0.0,
            10.0,
            10.0,
        );
        assert!(cut.is_empty());
        assert!(cut_idx.is_empty());

        let straddling = [vert(0.0, 0.0), vert(20.0, 0.0), vert(0.0, 20.0)];
        let (ring, ring_idx) = clip_chunk_to_world_bounds(
            &straddling,
            &[0, 1, 2],
            glam::Vec3::ZERO,
            0.0,
            0.0,
            10.0,
            10.0,
        );
        assert!(ring.len() >= 3);
        assert_eq!(ring_idx.len() % 3, 0);
        for vertex in &ring {
            let x = vertex.position[0];
            let z = vertex.position[2];
            assert!(x.abs() <= 10.0 + 0.05, "{x} fuera del rectángulo");
            assert!(z.abs() <= 10.0 + 0.05, "{z} fuera del rectángulo");
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
    fn el_terreno_se_apoya_en_el_suelo_desde_la_esquina_minima() {
        let offset = super::scene_offset(-112.0, -64.0);
        assert_eq!(offset.x, -112.0);
        assert_eq!(offset.z, -64.0);
        assert!((offset.y + 8.0).abs() < 1.0e-4);
    }

    fn vertex(position: [f32; 3], uv: [f32; 2]) -> crate::engine3d::mesh::Vertex {
        crate::engine3d::mesh::Vertex {
            position,
            normal: [0.0, 1.0, 0.0],
            uv,
        }
    }

    #[test]
    fn splat_tile_xy_covers_a_16_by_16_page() {
        assert_eq!(super::splat_tile_xy(0), (0, 0));
        assert_eq!(super::splat_tile_xy(15), (15, 0));
        assert_eq!(super::splat_tile_xy(16), (0, 1));
        assert_eq!(super::splat_tile_xy(255), (15, 15));
    }

    #[test]
    fn clamp_chunk_uvs_keeps_splat_inside_the_layer() {
        let mut vertices = [vertex([0.0, 0.0, 0.0], [-0.2, 1.4])];
        super::clamp_chunk_uvs(&mut vertices);
        assert_eq!(vertices[0].uv, [0.0, 1.0]);
    }

    #[test]
    fn clip_to_disk_keeps_interpolated_uvs_in_range() {
        let vertices = [
            vertex([0.0, 0.0, 0.0], [0.0, 0.0]),
            vertex([40.0, 0.0, 0.0], [1.2, 0.0]),
            vertex([0.0, 0.0, 40.0], [0.0, 1.2]),
        ];
        let (mut clipped, _) = super::clip_chunk_to_world_bounds(
            &vertices,
            &[0, 1, 2],
            glam::Vec3::ZERO,
            0.0,
            0.0,
            16.0,
            16.0,
        );
        super::clamp_chunk_uvs(&mut clipped);
        assert!(!clipped.is_empty());
        for vertex in &clipped {
            assert!((0.0..=1.0).contains(&vertex.uv[0]));
            assert!((0.0..=1.0).contains(&vertex.uv[1]));
        }
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

    #[test]
    fn el_area_de_generacion_cubre_el_mundo_con_margen() {
        let bounds =
            crate::engine3d::config_3d::WorldBounds3D::with_faces(80.0, 40.0, 60.0, 20.0, 100.0);
        let area = super::generation_area(&bounds);
        assert!(area.contains(&bounds));
        assert!(area.width > bounds.width());
        assert!(area.depth > bounds.depth());
        let center = bounds.center();
        assert!((area.min_x - (center.x - area.width * 0.5)).abs() < 1.0e-3);
        assert!((area.min_z - (center.z - area.depth * 0.5)).abs() < 1.0e-3);
    }

    #[test]
    fn el_area_respeta_los_limites_del_generador() {
        let tiny = crate::engine3d::config_3d::WorldBounds3D::with_faces(5.0, 5.0, 5.0, 5.0, 48.0);
        let area = super::generation_area(&tiny);
        assert!(area.width >= 30.0 && area.depth >= 30.0);
        assert!(area.contains(&tiny));

        let huge = crate::engine3d::config_3d::WorldBounds3D::with_faces(
            1000.0, 1000.0, 1000.0, 1000.0, 200.0,
        );
        let area = super::generation_area(&huge);
        assert!(area.width <= 2000.0 && area.depth <= 2000.0);
        assert!(area.contains(&huge));
    }

    #[test]
    fn las_columnas_del_mundo_se_alinean_con_el_area() {
        let area = super::GenerationArea {
            min_x: -100.0,
            min_z: -100.0,
            width: 300.0,
            depth: 300.0,
        };
        let bounds = crate::engine3d::config_3d::WorldBounds3D::with_faces(
            100.0, 100.0, 100.0, 100.0, 100.0,
        );
        let columns = super::chunk_columns_for_bounds(&bounds, &area, 32.0, 10, 10).unwrap();
        // [-100, 100] sobre un área que empieza en -100: 200 m / 32 → 7 chunks.
        assert_eq!(columns.cx_min, 0);
        assert_eq!(columns.cx_max, 6);
        assert_eq!(columns.cz_min, 0);
        assert_eq!(columns.cz_max, 6);
        assert!(columns.contains(0, 0));
        assert!(!columns.contains(7, 0));
    }

    #[test]
    fn un_chunk_cortado_por_el_borde_se_reconoce() {
        let area = super::GenerationArea {
            min_x: 0.0,
            min_z: -100.0,
            width: 640.0,
            depth: 640.0,
        };
        // Cara este de 40 → 80: el borde pasa por los chunks 1 y 2.
        let old =
            crate::engine3d::config_3d::WorldBounds3D::with_faces(100.0, 100.0, 40.0, 100.0, 100.0);
        let new =
            crate::engine3d::config_3d::WorldBounds3D::with_faces(100.0, 100.0, 80.0, 100.0, 100.0);
        assert!(super::chunk_touches_changed_bounds(
            (1, 0, 1),
            &old,
            &new,
            &area,
            32.0
        ));
        assert!(super::chunk_touches_changed_bounds(
            (2, 0, 1),
            &old,
            &new,
            &area,
            32.0
        ));
        assert!(!super::chunk_touches_changed_bounds(
            (5, 0, 1),
            &old,
            &new,
            &area,
            32.0
        ));
    }

    #[test]
    fn la_config_del_area_conserva_el_resto_del_json() {
        let config = r#"{"seed":7,"bounds":{"width":100.0,"length":80.0,"height":96.0},"resolution":{"voxelSize":2.0}}"#;
        let area = super::GenerationArea {
            min_x: -75.0,
            min_z: -60.0,
            width: 150.0,
            depth: 120.0,
        };
        let world =
            crate::engine3d::config_3d::WorldBounds3D::with_faces(40.0, 40.0, 50.0, 50.0, 96.0);
        let updated = super::config_with_bounds(config, &area, &world);
        let value: serde_json::Value = serde_json::from_str(&updated).unwrap();
        assert_eq!(value["bounds"]["width"], 150.0);
        assert_eq!(value["bounds"]["length"], 120.0);
        assert_eq!(value["bounds"]["visibleWidth"], 100.0);
        assert_eq!(value["bounds"]["visibleLength"], 80.0);
        assert_eq!(value["bounds"]["visibleOffsetX"], 25.0);
        assert_eq!(value["bounds"]["visibleOffsetZ"], 20.0);
        assert_eq!(value["bounds"]["height"], 96.0);
        assert_eq!(value["seed"], 7);
        assert_eq!(value["resolution"]["voxelSize"], 2.0);
    }

    #[test]
    fn el_area_con_margen_encaja_con_la_dll() {
        // El engine entrega a la DLL un área algo mayor que el mundo visible y
        // luego pide solo las columnas que lo solapan. Si el DLL recorta o
        // fuerza el área a otro tamaño, `wg_chunk_status`/`wg_mesh_chunk`
        // devuelven fuera de rango: este test detecta esa desincronización.
        let Some(path) = dll_path() else {
            eprintln!("DLL de rer-worldgen no encontrada: test omitido");
            return;
        };
        let library = WorldgenLibrary::load(&path).expect("la DLL debe cargar");
        let config = r#"{
            "seed": 11,
            "bounds": {
                "width": 384.0,
                "length": 288.0,
                "height": 96.0,
                "visibleWidth": 128.0,
                "visibleLength": 96.0,
                "visibleOffsetX": 128.0,
                "visibleOffsetZ": 96.0
            },
            "resolution": { "voxelSize": 1.0, "lodEnabled": false }
        }"#;
        let world =
            crate::engine3d::config_3d::WorldBounds3D::with_faces(48.0, 48.0, 64.0, 64.0, 96.0);
        let area = super::GenerationArea {
            min_x: -192.0,
            min_z: -144.0,
            width: 384.0,
            depth: 288.0,
        };
        let (chunks_x, chunks_z, max_y) = super::chunk_grid_size(384.0, 288.0, 96.0, 1.0);
        let columns =
            super::chunk_columns_for_bounds(&world, &area, 32.0, chunks_x, chunks_z).unwrap();

        // # Seguridad: contrato ABI; el handle se libera al final.
        unsafe {
            let handle = (library.world_create)(config.as_ptr(), config.len());
            assert!(
                !handle.is_null(),
                "wg_world_create falló: {}",
                library.last_error_text(std::ptr::null())
            );
            let mut meshed = 0;
            'columns: for cz in columns.cz_min..=columns.cz_max {
                for cx in columns.cx_min..=columns.cx_max {
                    for cy in 0..=max_y {
                        let status = (library.chunk_status)(handle, cx, cy, cz);
                        assert_ne!(
                            status, -1,
                            "columna ({cx},{cy},{cz}) fuera del área de la DLL"
                        );
                        if status != 1 {
                            continue;
                        }
                        let mut out = super::abi::WgMeshBuffers::empty();
                        let code = (library.mesh_chunk)(handle, cx, cy, cz, 0, &mut out);
                        assert_eq!(
                            code,
                            Status::Ok as i32,
                            "wg_mesh_chunk({cx},{cy},{cz}) falló: {}",
                            library.last_error_text(handle)
                        );
                        (library.free_mesh_buffers)(&mut out);
                        meshed += 1;
                        if meshed >= 4 {
                            break 'columns;
                        }
                    }
                }
            }
            assert!(meshed > 0, "el área de prueba no produjo geometría");
            (library.world_free)(handle);
        }
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
        super::finalize_pending(&mut pending, 32.0, glam::Vec3::ZERO, glam::Vec3::ZERO);
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
