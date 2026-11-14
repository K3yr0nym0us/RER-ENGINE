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
/// Tope de columnas XZ por frame. El tick también corta por
/// [`SCAN_BUDGET_MS`] para emitir progreso aunque una columna sea cara.
pub(crate) const SCAN_COLUMNS_PER_FRAME: usize = 512;
/// Presupuesto de barrido por frame: el UI recibe `completed/total` de columnas.
pub(crate) const SCAN_BUDGET_MS: u64 = 8;

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
    /// Columnas X del mundo (exclusivo).
    pub chunks_x: i32,
    /// Columnas Z del mundo (exclusivo).
    pub chunks_z: i32,
    /// Capa Y máxima inclusive.
    pub max_y: i32,
    /// Tamaño de chunk en metros.
    pub cell: f32,
    /// Cámara en el momento de `world_gen_start` (orden y LOD).
    pub camera: [f32; 3],
    /// Chunks con contenido hallados hasta ahora.
    pub collected: Vec<PendingChunk>,
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
    /// Capa GPU de `Grass.jpg`. Las cinco siguientes, en orden, son Dirt, Clay,
    /// Sand, Stone y Snow.
    pub material_base: Option<u32>,
    /// Capa de textura de fallback (checker) mientras no haya texturas.
    pub fallback_tex_idx: Option<usize>,
    /// Colocaciones de blueprints registradas por el scatter (persistencia F8).
    pub placements: Vec<WorldgenPlacement>,
    /// Reglas de scatter aplicadas por blueprint (persistencia F8).
    pub scatter_rules: Vec<(String, String)>,
    /// Traslación de la geometría de la DLL: centra la huella y apoya el suelo
    /// del terreno en y = 0. Se fija en `worldgen_start`.
    pub scene_offset: glam::Vec3,
    /// Si es true, al completar la cola se quita el suelo checker.
    pub replace_ground: bool,
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
            scatter_rules: Vec::new(),
            scene_offset: glam::Vec3::ZERO,
            replace_ground: false,
        }
    }
}

impl Drop for WorldgenState {
    fn drop(&mut self) {
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
