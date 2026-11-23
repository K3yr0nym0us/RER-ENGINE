//! Estado de generación de terreno en el `State` del engine.
//!
//! El plugin produce la geometría (crate `rer-worldgen` compilado a DLL); este
//! módulo solo la consume: mallas → GPU, entidades ECS y eventos de progreso,
//! con presupuesto por frame (mismo patrón que la precarga de modelos).

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::mpsc::{Receiver, Sender};
use std::thread::JoinHandle;

use super::loader::WorldgenLibrary;
use super::worker::{PendingCpuChunk, WorldgenJob, WorldgenWorkerMsg};

/// Subidas de chunk a GPU por frame. El scan/mesh CPU va en el hilo
/// `rer-worldgen`; este tope solo cubre `Mesh::upload` + ECS.
pub(crate) const CHUNKS_PER_FRAME: usize = 2;
/// Instancias de scatter por frame. Colocar todas en un comando congelaba el
/// editor al terminar la generación.
pub(crate) const PLACEMENTS_PER_FRAME: usize = 8;
/// Columnas XZ entre eventos de progreso de scan (el worker ya no recorta el
/// hilo de render; el UI sigue recibiendo `completed/total`).
pub(crate) const SCAN_COLUMNS_PER_FRAME: usize = 512;

/// Lado del tile de splat en el atlas (debe coincidir con `SPLAT_DIM` lod 0).
pub(crate) const SPLAT_ATLAS_TILE_PX: u32 = 64;
/// Tiles por eje en una capa 1024²: 16×16 = 256 columnas por página.
pub(crate) const SPLAT_ATLAS_TILES: u32 = 16;

/// Páginas del atlas de splats (una capa GPU = 256 columnas).
#[derive(Debug, Default)]
pub(crate) struct SplatAtlas {
    /// Índices en `tex_layers` de cada página.
    pub pages: Vec<usize>,
    /// Siguiente tile global (página * 256 + local). Al regenerar se vuelve a 0
    /// y se pisan tiles en orden; no se reutiliza el tile de una columna viva.
    pub next_tile: u32,
}

impl SplatAtlas {
    /// Olvida las páginas (el array GPU se resetea aparte).
    pub fn reset(&mut self) {
        self.pages.clear();
        self.next_tile = 0;
    }
}

/// Handle opaco del mundo en la DLL.
///
/// `unsafe impl Send` justificado: el handle se usa en el hilo del engine o, durante
/// Generating, en el worker `rer-worldgen`. Nunca en ambos a la vez: un job nuevo
/// invalida el anterior y el engine espera `Cancelled`/`Finished` antes de
/// `world_free` / scatter. `wg_world_create` corre en el worker. El `Library`
/// vive en [`WorldgenState`].
pub(crate) struct WgWorldHandle(pub(crate) *mut std::ffi::c_void);
// # Seguridad: el contrato del plugin no permite usos concurrentes del handle.
unsafe impl Send for WgWorldHandle {}

/// Clave de chunk (debe coincidir con `rer_worldgen::chunks::ChunkKey`).
pub(crate) type ChunkKey = (i32, i32, i32);

/// Chunk de terreno ya subido a GPU y presente en la escena.
pub(crate) struct WorldgenChunk {
    /// Entidad ECS del chunk (`MeshComponent`).
    pub entity_id: crate::engine3d::ecs::EntityId,
    /// Índice en `State::meshes`. `None` si el chunk no subió geometría.
    pub mesh_idx: Option<usize>,
}

/// Chunk pendiente de generar/meshear/subir.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PendingChunk {
    /// Clave del chunk.
    pub key: ChunkKey,
    /// Nivel de LOD del chunk (0 = resolución completa).
    pub lod: u32,
}

/// Pose pendiente de spawn tras `wg_scatter` (presupuesto por frame).
#[derive(Debug, Clone)]
pub(crate) struct PendingScatterPose {
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    pub index: usize,
}

/// Trabajo de scatter: un blueprint, N instancias en cola.
#[derive(Debug, Clone)]
pub(crate) struct PendingScatterJob {
    pub blueprint_id: String,
    pub model_path: String,
    pub display: String,
    pub mesh_idx: usize,
    pub tex_idx: usize,
    pub category: Option<String>,
    pub physics_enabled: bool,
    pub physics_type: String,
    pub local_bounds: ([f32; 3], [f32; 3]),
    pub roughness: f32,
    pub metallic: f32,
    pub ior: f32,
    pub remaining: VecDeque<PendingScatterPose>,
    pub placed: u32,
}

/// Colocación registrada por el scatter (para persistencia F8).
#[derive(Debug, Clone)]
pub(crate) struct WorldgenPlacement {
    /// Blueprint colocado.
    pub blueprint_id: String,
    /// Posición (metros).
    pub position: [f32; 3],
    /// Rotación (quat x,y,z,w).
    pub rotation: [f32; 4],
    /// Escala.
    pub scale: [f32; 3],
    /// Entidad ECS de esta instancia. `0` si se restauró sin spawn.
    /// No se persiste: el `.save` guarda posición, rotación y escala.
    pub entity_id: crate::engine3d::ecs::EntityId,
}

/// Barrido de columnas XZ repartido en varios frames antes de meshear.
pub(crate) struct ScanCursor {
    /// Próxima columna X.
    pub cx: i32,
    /// Próxima columna Z.
    pub cz: i32,
    /// Primera columna X del barrido (inclusive).
    pub cx_min: i32,
    /// Columna X final del barrido (exclusiva).
    pub cx_max: i32,
    /// Primera columna Z del barrido (inclusive).
    pub cz_min: i32,
    /// Columna Z final del barrido (exclusiva).
    pub cz_max: i32,
    /// Capa Y máxima inclusive.
    pub max_y: i32,
    /// Tamaño de chunk en metros.
    pub cell: f32,
    /// Cámara en el momento de `world_gen_start` (orden y LOD).
    pub camera: [f32; 3],
    /// Chunks con contenido hallados hasta ahora.
    pub collected: Vec<PendingChunk>,
    /// Columnas XZ explícitas (recorte o crecimiento); `None` = barrido
    /// rectangular `cx_min..cx_max × cz_min..cz_max`.
    pub columns: Option<Vec<(i32, i32)>>,
    /// Siguiente columna de `columns` a barrer.
    pub column_index: usize,
}

/// Rectángulo que cubre el mundo en la DLL. Puede ser mayor que los límites
/// visibles (`WorldBounds3D`) para crecer sin regenerar todo el terreno.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GenerationArea {
    /// Esquina mínima X en coordenadas de mundo.
    pub min_x: f32,
    /// Esquina mínima Z en coordenadas de mundo.
    pub min_z: f32,
    /// Ancho generado (m).
    pub width: f32,
    /// Largo generado (m).
    pub depth: f32,
}

impl GenerationArea {
    pub(crate) fn max_x(&self) -> f32 {
        self.min_x + self.width
    }

    pub(crate) fn max_z(&self) -> f32 {
        self.min_z + self.depth
    }

    /// `true` si los límites visibles caben dentro del área generada.
    pub(crate) fn contains(&self, bounds: &crate::engine3d::config_3d::WorldBounds3D) -> bool {
        const EPS: f32 = 0.01;
        bounds.min_x() >= self.min_x - EPS
            && bounds.max_x() <= self.max_x() + EPS
            && bounds.min_z() >= self.min_z - EPS
            && bounds.max_z() <= self.max_z() + EPS
    }
}

/// Rango inclusivo de columnas de chunk que se solapan con un rectángulo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ChunkColumns {
    pub cx_min: i32,
    pub cx_max: i32,
    pub cz_min: i32,
    pub cz_max: i32,
}

impl ChunkColumns {
    pub(crate) fn contains(self, cx: i32, cz: i32) -> bool {
        cx >= self.cx_min && cx <= self.cx_max && cz >= self.cz_min && cz <= self.cz_max
    }
}

/// Estados de alto nivel del plugin visibles por el editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorldgenStatus {
    /// Sin ruta de DLL configurada.
    MissingLibrary,
    /// Error de carga/ABI (ver `last_error`).
    LibraryError,
    /// DLL cargada, esperando config.
    Idle,
    /// Config aplicada; listo para generar.
    Ready,
    /// Generando (quedan chunks en la cola).
    Generating,
    /// Generación completa.
    Complete,
}

impl WorldgenStatus {
    /// Nombre para los eventos IPC (`world_gen_status`).
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MissingLibrary => "missing_library",
            Self::LibraryError => "library_error",
            Self::Idle => "idle",
            Self::Ready => "ready",
            Self::Generating => "generating",
            Self::Complete => "complete",
        }
    }
}

/// Estado completo del generador dentro del `State` del engine.
pub(crate) struct WorldgenState {
    /// Ruta de la DLL configurada por el editor (plugin instalado).
    pub library_path: Option<String>,
    /// Último error legible (carga de DLL, ABI o generación).
    pub last_error: Option<String>,
    /// Biblioteca cargada (diferida hasta `world_gen_set_library_path`).
    pub library: Option<WorldgenLibrary>,
    /// Handle del mundo en la DLL (creado con la config aplicada).
    pub world: Option<WgWorldHandle>,
    /// Config JSON camelCase (se aplica cuando la biblioteca esté disponible).
    pub config_json: Option<String>,
    /// Estado de alto nivel.
    pub status: WorldgenStatus,
    /// Cola de chunks pendientes. Tras el barrido queda invertida para que
    /// `pop` saque primero la capa alta y la más cercana a la cámara.
    pub pending: Vec<PendingChunk>,
    /// Barrido de columnas en curso. `None` cuando ya se puede meshear.
    pub scan: Option<ScanCursor>,
    /// Generación de albedo: al cambiar la config se incrementa para reescribir
    /// las capas ya reservadas en vez de empaquetar otras nuevas.
    pub column_epoch: u64,
    /// Total de chunks planificados (para progreso).
    pub total_chunks: u32,
    /// Chunks completados.
    pub completed_chunks: u32,
    /// Chunks activos en escena.
    pub spawned: HashMap<ChunkKey, WorldgenChunk>,
    /// Splat de la columna XZ: `(tex_idx del atlas, tile local 0..255, epoch)`.
    pub columns: HashMap<(i32, i32), (usize, u32, u64)>,
    /// Atlas de splats: varias columnas en una capa 1024² (tiles 64×64).
    pub splat_atlas: SplatAtlas,
    /// Tile de splat por entidad de chunk (para el instance buffer).
    pub entity_splat_tile: HashMap<crate::engine3d::ecs::EntityId, u32>,
    /// Capa GPU de `Grass.jpg` del plugin. Las seis siguientes, en orden, son
    /// Dirt, Clay, Sand, Stone, Snow y Mud.
    pub material_base: Option<u32>,
    /// Capa de textura de fallback (checker) mientras no haya texturas.
    pub fallback_tex_idx: Option<usize>,
    /// Colocaciones de blueprints registradas por el scatter (persistencia F8).
    pub placements: Vec<WorldgenPlacement>,
    /// Instancias de scatter pendientes de spawn (presupuesto por frame).
    pub scatter_jobs: VecDeque<PendingScatterJob>,
    /// Reglas de scatter aplicadas por blueprint (persistencia F8).
    pub scatter_rules: Vec<(String, String)>,
    /// Traslación de la geometría de la DLL: esquina mínima del área generada.
    /// Se fija en `world_gen_start` y no cambia al mover las caras.
    pub scene_offset: glam::Vec3,
    /// Rectángulo de la DLL (mundo + margen de crecimiento).
    pub generation: Option<GenerationArea>,
    /// Ya se generó terreno al menos una vez (activa el seguimiento automático
    /// de los límites del mundo).
    pub generated: bool,
    /// Si es true, al completar la cola se quita el suelo checker.
    pub replace_ground: bool,
    /// Generación viva: el worker aborta si no coincide con el job.
    pub live_generation: Arc<AtomicU64>,
    /// Último id de job enviado al worker.
    pub generation_id: u64,
    /// Job cuyo Finished/Failed/Cancelled aún no llegó.
    pub inflight: Option<u64>,
    /// Mallas CPU listas para subir a GPU este / próximos frames.
    pub gpu_queue: VecDeque<PendingCpuChunk>,
    pub job_tx: Option<Sender<WorldgenJob>>,
    pub result_rx: Option<Receiver<WorldgenWorkerMsg>>,
    pub worker: Option<JoinHandle<()>>,
}

impl Default for WorldgenState {
    fn default() -> Self {
        Self {
            library_path: None,
            last_error: None,
            library: None,
            world: None,
            config_json: None,
            status: WorldgenStatus::MissingLibrary,
            pending: Vec::new(),
            scan: None,
            column_epoch: 0,
            total_chunks: 0,
            completed_chunks: 0,
            spawned: HashMap::new(),
            columns: HashMap::new(),
            splat_atlas: SplatAtlas::default(),
            entity_splat_tile: HashMap::new(),
            material_base: None,
            fallback_tex_idx: None,
            placements: Vec::new(),
            scatter_jobs: VecDeque::new(),
            scatter_rules: Vec::new(),
            scene_offset: glam::Vec3::ZERO,
            generation: None,
            generated: false,
            replace_ground: false,
            live_generation: Arc::new(AtomicU64::new(0)),
            generation_id: 0,
            inflight: None,
            gpu_queue: VecDeque::new(),
            job_tx: None,
            result_rx: None,
            worker: None,
        }
    }
}

impl Drop for WorldgenState {
    fn drop(&mut self) {
        self.live_generation
            .fetch_add(1, std::sync::atomic::Ordering::Release);
        if let Some(tx) = self.job_tx.take() {
            let _ = tx.send(WorldgenJob::Shutdown);
        }
        if let Some(rx) = self.result_rx.take() {
            while rx
                .recv_timeout(std::time::Duration::from_millis(50))
                .is_ok()
            {}
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        // El campo `library` se destruye después de este cuerpo. Liberar el
        // handle aquí evita descargar la DLL con el mundo todavía vivo.
        if let Some(handle) = self.world.take()
            && let Some(library) = self.library.as_ref()
        {
            // # Seguridad: handle creado por esta misma biblioteca, una sola vez.
            unsafe { (library.world_free)(handle.0) };
        }
    }
}
