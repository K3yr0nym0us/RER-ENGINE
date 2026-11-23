//! Scan y meshing CPU del plugin en un hilo aparte del render.
//!
//! El hilo del engine no llama a `wg_chunk_status` / `wg_mesh_chunk` /
//! `wg_chunk_textures` mientras este worker está en un job. Un `generation`
//! nuevo invalida el job; el worker termina la llamada FFI en curso y sale.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;

use glam::Vec3;

use crate::engine3d::config_3d::worldgen::abi::{Status, WgChunkTextures, WgMeshBuffers};
use crate::engine3d::config_3d::worldgen::loader::WorldgenLibrary;
use crate::engine3d::config_3d::worldgen::types::{
    ChunkKey, PendingChunk, SCAN_COLUMNS_PER_FRAME, ScanCursor,
};
use crate::engine3d::mesh::Vertex;

/// Tope de mallas CPU en vuelo: el worker se bloquea si el tick no drena.
pub(crate) const WORKER_RESULT_BOUND: usize = 8;

/// Punteros ABI copiados. Válidos mientras viva la `WorldgenLibrary` del engine.
#[derive(Clone, Copy)]
pub(crate) struct WorldgenFfi {
    pub world_create: unsafe extern "C" fn(*const u8, usize) -> *mut std::ffi::c_void,
    pub world_free: unsafe extern "C" fn(*mut std::ffi::c_void),
    pub chunk_status: unsafe extern "C" fn(*const std::ffi::c_void, i32, i32, i32) -> i32,
    pub mesh_chunk:
        unsafe extern "C" fn(*mut std::ffi::c_void, i32, i32, i32, u32, *mut WgMeshBuffers) -> i32,
    pub last_error: unsafe extern "C" fn(*const std::ffi::c_void, *mut u8, usize) -> usize,
    pub free_mesh_buffers: unsafe extern "C" fn(*const WgMeshBuffers),
    pub chunk_textures:
        unsafe extern "C" fn(*mut std::ffi::c_void, i32, i32, u32, *mut WgChunkTextures) -> i32,
    pub free_chunk_textures: unsafe extern "C" fn(*const WgChunkTextures),
}

impl WorldgenLibrary {
    pub(crate) fn ffi(&self) -> WorldgenFfi {
        WorldgenFfi {
            world_create: self.world_create,
            world_free: self.world_free,
            chunk_status: self.chunk_status,
            mesh_chunk: self.mesh_chunk,
            last_error: self.last_error,
            free_mesh_buffers: self.free_mesh_buffers,
            chunk_textures: self.chunk_textures,
            free_chunk_textures: self.free_chunk_textures,
        }
    }
}

pub(crate) struct WorldgenRunJob {
    pub generation: u64,
    pub handle: usize,
    pub create_json: Option<String>,
    pub ffi: WorldgenFfi,
    pub scan: Option<ScanCursor>,
    pub pending: Vec<PendingChunk>,
    pub origin: [f32; 3],
}

pub(crate) enum WorldgenJob {
    Run(Box<WorldgenRunJob>),
    Shutdown,
}

type ColumnSplat = Option<(Vec<u8>, u32)>;
type ColumnSplatCache = std::collections::HashMap<(i32, i32), ColumnSplat>;

pub(crate) enum WorldgenWorkerMsg {
    WorldCreated {
        generation: u64,
        handle: usize,
    },
    ScanProgress {
        generation: u64,
        completed: u32,
        total: u32,
    },
    MeshPhase {
        generation: u64,
        total: u32,
    },
    ChunkReady {
        generation: u64,
        chunk: PendingCpuChunk,
    },
    Failed {
        generation: u64,
        message: String,
    },
    Cancelled {
        generation: u64,
    },
    Finished {
        generation: u64,
    },
}

#[derive(Debug)]
pub(crate) struct PendingCpuChunk {
    pub key: ChunkKey,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub splat: Option<(Vec<u8>, u32)>,
}

pub(crate) fn spawn_worldgen_worker(
    live: Arc<AtomicU64>,
) -> (
    Sender<WorldgenJob>,
    Receiver<WorldgenWorkerMsg>,
    JoinHandle<()>,
) {
    let (job_tx, job_rx) = mpsc::channel::<WorldgenJob>();
    let (msg_tx, msg_rx) = mpsc::sync_channel::<WorldgenWorkerMsg>(WORKER_RESULT_BOUND);
    let join = std::thread::Builder::new()
        .name("rer-worldgen".into())
        .spawn(move || worker_loop(live, job_rx, msg_tx))
        .expect("no se pudo crear el hilo rer-worldgen");
    (job_tx, msg_rx, join)
}

fn worker_loop(
    live: Arc<AtomicU64>,
    job_rx: Receiver<WorldgenJob>,
    msg_tx: mpsc::SyncSender<WorldgenWorkerMsg>,
) {
    while let Ok(job) = job_rx.recv() {
        match job {
            WorldgenJob::Shutdown => break,
            WorldgenJob::Run(job) => run_job(&live, &msg_tx, *job),
        }
    }
}

fn run_job(
    live: &AtomicU64,
    msg_tx: &mpsc::SyncSender<WorldgenWorkerMsg>,
    mut job: WorldgenRunJob,
) {
    let generation = job.generation;
    let ffi = job.ffi;
    let origin = Vec3::from_array(job.origin);
    let mut handle = job.handle as *mut std::ffi::c_void;
    let mut pending = job.pending;
    if !still_current(live, generation) {
        let _ = msg_tx.send(WorldgenWorkerMsg::Cancelled { generation });
        return;
    }

    if let Some(json) = job.create_json.take() {
        // # Seguridad: buffer de solo lectura de json.len() bytes.
        handle = unsafe { (ffi.world_create)(json.as_ptr(), json.len()) };
        if handle.is_null() {
            let _ = msg_tx.send(WorldgenWorkerMsg::Failed {
                generation,
                message: last_error_text(ffi, std::ptr::null()),
            });
            return;
        }
        if !still_current(live, generation) {
            // # Seguridad: handle recién creado por esta misma FFI; el engine no lo vio.
            unsafe { (ffi.world_free)(handle) };
            let _ = msg_tx.send(WorldgenWorkerMsg::Cancelled { generation });
            return;
        }
        if msg_tx
            .send(WorldgenWorkerMsg::WorldCreated {
                generation,
                handle: handle as usize,
            })
            .is_err()
        {
            // # Seguridad: el engine no recibió el handle; hay que liberarlo aquí.
            unsafe { (ffi.world_free)(handle) };
            return;
        }
    } else if handle.is_null() {
        let _ = msg_tx.send(WorldgenWorkerMsg::Failed {
            generation,
            message: "worldgen sin handle de mundo".into(),
        });
        return;
    }

    if let Some(mut scan) = job.scan {
        match scan_columns(live, generation, handle, ffi, &mut scan, msg_tx) {
            ScanOutcome::Cancelled => {
                let _ = msg_tx.send(WorldgenWorkerMsg::Cancelled { generation });
                return;
            }
            ScanOutcome::Failed(message) => {
                let _ = msg_tx.send(WorldgenWorkerMsg::Failed {
                    generation,
                    message,
                });
                return;
            }
            ScanOutcome::Done => {
                pending = scan.collected;
                super::dedupe_pending(&mut pending);
                super::finalize_pending(
                    &mut pending,
                    scan.cell,
                    Vec3::from_array(scan.camera),
                    origin,
                );
            }
        }
    }

    if !still_current(live, generation) {
        let _ = msg_tx.send(WorldgenWorkerMsg::Cancelled { generation });
        return;
    }

    let total = pending.len() as u32;
    if msg_tx
        .send(WorldgenWorkerMsg::MeshPhase { generation, total })
        .is_err()
    {
        return;
    }

    let mut splat_cache = ColumnSplatCache::new();
    while let Some(chunk) = pending.pop() {
        if !still_current(live, generation) {
            let _ = msg_tx.send(WorldgenWorkerMsg::Cancelled { generation });
            return;
        }
        match mesh_chunk_cpu(ffi, handle, chunk.key, chunk.lod, &mut splat_cache) {
            Ok(cpu) => {
                if msg_tx
                    .send(WorldgenWorkerMsg::ChunkReady {
                        generation,
                        chunk: cpu,
                    })
                    .is_err()
                {
                    return;
                }
            }
            Err(message) => {
                let _ = msg_tx.send(WorldgenWorkerMsg::Failed {
                    generation,
                    message,
                });
                return;
            }
        }
    }

    if still_current(live, generation) {
        let _ = msg_tx.send(WorldgenWorkerMsg::Finished { generation });
    } else {
        let _ = msg_tx.send(WorldgenWorkerMsg::Cancelled { generation });
    }
}

enum ScanOutcome {
    Done,
    Cancelled,
    Failed(String),
}

fn scan_columns(
    live: &AtomicU64,
    generation: u64,
    handle: *mut std::ffi::c_void,
    ffi: WorldgenFfi,
    scan: &mut ScanCursor,
    msg_tx: &mpsc::SyncSender<WorldgenWorkerMsg>,
) -> ScanOutcome {
    let mut scanned = 0usize;
    while let Some((cx, cz)) = next_scan_column(scan) {
        if !still_current(live, generation) {
            return ScanOutcome::Cancelled;
        }
        if let Some(message) = scan_column(scan, ffi, handle, cx, cz) {
            return ScanOutcome::Failed(message);
        }
        scanned += 1;
        if scanned.is_multiple_of(SCAN_COLUMNS_PER_FRAME) {
            let (completed, total) = scan_progress(scan);
            if msg_tx
                .send(WorldgenWorkerMsg::ScanProgress {
                    generation,
                    completed,
                    total,
                })
                .is_err()
            {
                return ScanOutcome::Cancelled;
            }
        }
    }
    ScanOutcome::Done
}

fn scan_column(
    scan: &mut ScanCursor,
    ffi: WorldgenFfi,
    handle: *mut std::ffi::c_void,
    cx: i32,
    cz: i32,
) -> Option<String> {
    for cy in scan.min_y..=scan.max_y {
        // # Seguridad: handle vivo; el engine no llama a la DLL hasta Cancelled/Finished.
        let status = unsafe { (ffi.chunk_status)(handle, cx, cy, cz) };
        if status == 1 {
            scan.collected.push(PendingChunk {
                key: (cx, cy, cz),
                lod: 0,
            });
        } else if status < 0 {
            return Some(format!("wg_chunk_status({cx}, {cy}, {cz}) inválido"));
        }
    }
    None
}

pub(crate) fn next_scan_column(scan: &mut ScanCursor) -> Option<(i32, i32)> {
    if let Some(columns) = scan.columns.as_ref() {
        let pair = *columns.get(scan.column_index)?;
        scan.column_index += 1;
        return Some(pair);
    }
    if scan.cz >= scan.cz_max {
        return None;
    }
    let cx = scan.cx;
    let cz = scan.cz;
    scan.cx += 1;
    if scan.cx >= scan.cx_max {
        scan.cx = scan.cx_min;
        scan.cz += 1;
    }
    Some((cx, cz))
}

pub(crate) fn scan_progress(scan: &ScanCursor) -> (u32, u32) {
    match scan.columns.as_ref() {
        Some(columns) => (scan.column_index as u32, columns.len().max(1) as u32),
        None => {
            let width = (scan.cx_max - scan.cx_min).max(0);
            let height = (scan.cz_max - scan.cz_min).max(0);
            let scanned = (scan.cz - scan.cz_min).max(0) * width + (scan.cx - scan.cx_min).max(0);
            (scanned as u32, (width * height).max(1) as u32)
        }
    }
}

fn mesh_chunk_cpu(
    ffi: WorldgenFfi,
    handle: *mut std::ffi::c_void,
    key: ChunkKey,
    lod: u32,
    splat_cache: &mut ColumnSplatCache,
) -> Result<PendingCpuChunk, String> {
    let (cx, cy, cz) = key;
    let mut buffers = WgMeshBuffers::empty();
    // # Seguridad: handle vivo de esta generación; out es un struct del ABI.
    let status = unsafe { (ffi.mesh_chunk)(handle, cx, cy, cz, lod, &mut buffers) };
    if status != Status::Ok as i32 {
        return Err(format!(
            "wg_mesh_chunk({cx}, {cy}, {cz}) = {status:?}: {}",
            last_error_text(ffi, handle)
        ));
    }
    let vertices = copy_vertices(&buffers);
    let indices = copy_indices(&buffers);
    // # Seguridad: buffers de esta misma llamada.
    unsafe { (ffi.free_mesh_buffers)(&buffers) };

    let splat = match splat_cache.entry((cx, cz)) {
        std::collections::hash_map::Entry::Occupied(entry) => entry.get().clone(),
        std::collections::hash_map::Entry::Vacant(entry) => {
            let pixels = copy_column_splat(ffi, handle, cx, cz);
            entry.insert(pixels.clone());
            pixels
        }
    };

    Ok(PendingCpuChunk {
        key,
        vertices,
        indices,
        splat,
    })
}

fn copy_vertices(buffers: &WgMeshBuffers) -> Vec<Vertex> {
    if buffers.vertex_count == 0 {
        return Vec::new();
    }
    // # Seguridad: punteros de wg_mesh_chunk, vertex_count elementos.
    unsafe {
        let positions = std::slice::from_raw_parts(buffers.positions, buffers.vertex_count * 3);
        let normals = std::slice::from_raw_parts(buffers.normals, buffers.vertex_count * 3);
        let uvs = std::slice::from_raw_parts(buffers.uvs, buffers.vertex_count * 2);
        (0..buffers.vertex_count)
            .map(|i| Vertex {
                position: [positions[i * 3], positions[i * 3 + 1], positions[i * 3 + 2]],
                normal: [normals[i * 3], normals[i * 3 + 1], normals[i * 3 + 2]],
                uv: [uvs[i * 2], uvs[i * 2 + 1]],
            })
            .collect()
    }
}

fn copy_indices(buffers: &WgMeshBuffers) -> Vec<u32> {
    if buffers.index_count == 0 {
        return Vec::new();
    }
    // # Seguridad: ver copy_vertices.
    unsafe { std::slice::from_raw_parts(buffers.indices, buffers.index_count) }.to_vec()
}

fn copy_column_splat(
    ffi: WorldgenFfi,
    handle: *mut std::ffi::c_void,
    cx: i32,
    cz: i32,
) -> Option<(Vec<u8>, u32)> {
    let mut buffers = WgChunkTextures::empty();
    // # Seguridad: handle vivo; struct del ABI.
    let status = unsafe { (ffi.chunk_textures)(handle, cx, cz, 0, &mut buffers) };
    let source_ptr = if !buffers.splat.is_null() {
        buffers.splat
    } else {
        buffers.albedo
    };
    if status == Status::Ok as i32 && !source_ptr.is_null() {
        // # Seguridad: len bytes del contrato; se copian antes de liberar.
        let pixels = unsafe { std::slice::from_raw_parts(source_ptr, buffers.len) }.to_vec();
        let dim = buffers.dim;
        unsafe { (ffi.free_chunk_textures)(&buffers) };
        Some((pixels, dim))
    } else {
        if !buffers.albedo.is_null() || !buffers.splat.is_null() {
            unsafe { (ffi.free_chunk_textures)(&buffers) };
        }
        None
    }
}

fn last_error_text(ffi: WorldgenFfi, world: *const std::ffi::c_void) -> String {
    let mut buffer = [0u8; 512];
    // # Seguridad: buffer local; firma del contrato.
    let n = unsafe { (ffi.last_error)(world, buffer.as_mut_ptr(), buffer.len()) };
    std::str::from_utf8(&buffer[..n.min(buffer.len() - 1)])
        .unwrap_or("error del plugin ilegible")
        .trim_end_matches('\0')
        .to_string()
}

fn still_current(live: &AtomicU64, generation: u64) -> bool {
    live.load(Ordering::Acquire) == generation
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_scan(cx_min: i32, cx_max: i32, cz_min: i32, cz_max: i32) -> ScanCursor {
        ScanCursor {
            cx: cx_min,
            cz: cz_min,
            cx_min,
            cx_max,
            cz_min,
            cz_max,
            min_y: 0,
            max_y: 0,
            cell: 32.0,
            camera: [0.0; 3],
            collected: Vec::new(),
            columns: None,
            column_index: 0,
        }
    }

    #[test]
    fn next_scan_column_recorre_el_rectangulo() {
        let mut scan = empty_scan(0, 2, 0, 2);
        let mut seen = Vec::new();
        while let Some(pair) = next_scan_column(&mut scan) {
            seen.push(pair);
        }
        assert_eq!(seen, vec![(0, 0), (1, 0), (0, 1), (1, 1)]);
    }

    #[test]
    fn next_scan_column_sigue_la_lista_explicita() {
        let mut scan = empty_scan(0, 0, 0, 0);
        scan.columns = Some(vec![(3, 1), (4, 1)]);
        assert_eq!(next_scan_column(&mut scan), Some((3, 1)));
        assert_eq!(next_scan_column(&mut scan), Some((4, 1)));
        assert_eq!(next_scan_column(&mut scan), None);
        let (completed, total) = scan_progress(&scan);
        assert_eq!((completed, total), (2, 2));
    }
}
