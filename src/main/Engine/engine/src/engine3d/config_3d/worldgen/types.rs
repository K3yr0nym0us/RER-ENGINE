//! Estado de generación de terreno en el `State` del engine.
//!
//! El plugin produce la geometría (crate `rer-worldgen` compilado a DLL); este
//! módulo solo la consume: mallas → GPU, entidades ECS y eventos de progreso,
//! con presupuesto por frame (mismo patrón que la precarga de modelos).

use std::collections::HashMap;

use super::loader::WorldgenLibrary;

/// Subidas de chunk a GPU por frame. Dos chunks de 32³ consumen ~1-2 ms de
/// generación + subida en release; ajustable si el profiling lo pide.
pub(crate) const CHUNKS_PER_FRAME: usize = 2;

/// Handle opaco del mundo en la DLL.
///
/// `unsafe impl Send` justificado: el handle solo se crea, usa y libera desde el
/// hilo del engine (update/comandos); la DLL garantiza que no cruza hilos y su
/// estado interno es sincronizado (`Mutex` para errores, sin estado mutable por
/// llamada). El `Library` vive en [`WorldgenState`], no en el handle.
pub(crate) struct WgWorldHandle(pub(crate) *mut std::ffi::c_void);
// # Seguridad: el handle cruza hilos únicamente dentro del `State` embalado por el
// loader del editor; el contrato del plugin no permite usos concurrentes.
unsafe impl Send for WgWorldHandle {}

/// Clave de chunk (debe coincidir con `rer_worldgen::chunks::ChunkKey`).
pub(crate) type ChunkKey = (i32, i32, i32);

/// Chunk de terreno ya subido a GPU y presente en la escena.
pub(crate) struct WorldgenChunk {
    /// Entidad ECS del chunk (`MeshComponent`).
    pub entity_id: crate::engine3d::ecs::EntityId,
}

/// Chunk pendiente de generar/meshear/subir.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PendingChunk {
    /// Clave del chunk.
    pub key: ChunkKey,
    /// Nivel de LOD del chunk (0 = resolución completa).
    pub lod: u32,
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
    /// Cola de chunks pendientes (ordenada por distancia al origen).
    pub pending: Vec<PendingChunk>,
    /// Total de chunks planificados (para progreso).
    pub total_chunks: u32,
    /// Chunks completados.
    pub completed_chunks: u32,
    /// Chunks activos en escena.
    pub spawned: HashMap<ChunkKey, WorldgenChunk>,
    /// Textura de splat/albedo por columna XZ (tex_idx compartido por los
    /// chunks Y de la misma columna).
    pub columns: HashMap<(i32, i32), usize>,
    /// Capa de textura de fallback (checker) mientras no haya texturas.
    pub fallback_tex_idx: Option<usize>,
    /// Colocaciones de blueprints registradas por el scatter (persistencia F8).
    pub placements: Vec<WorldgenPlacement>,
    /// Reglas de scatter aplicadas por blueprint (persistencia F8).
    pub scatter_rules: Vec<(String, String)>,
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
            total_chunks: 0,
            completed_chunks: 0,
            spawned: HashMap::new(),
            columns: HashMap::new(),
            fallback_tex_idx: None,
            placements: Vec::new(),
            scatter_rules: Vec::new(),
        }
    }
}
