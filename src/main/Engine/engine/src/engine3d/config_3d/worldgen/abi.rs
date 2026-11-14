//! Espejo del contrato ABI de `rer_worldgen_plugin.dll`.
//!
//! Este módulo DUPLICA deliberadamente los tipos de `rer_worldgen::plugin_ffi::abi`:
//! el engine **no depende** del crate worldgen (la lógica de generación viaja solo
//! en la DLL instalable). Cualquier cambio de contrato debe reflejarse en ambos
//! lados e incrementar `ABI_VERSION` en el crate `rer-worldgen`.

/// Versión de la ABI esperada por el engine. Debe coincidir con
/// `rer_worldgen::plugin_ffi::abi::ABI_VERSION`.
pub const ABI_VERSION: u32 = 2;

/// Voxels de chunk por eje (debe coincidir con `rer_worldgen::chunks::CHUNK_SIZE`).
pub const CHUNK_SIZE: u32 = 32;

/// Buffers de geometría de un chunk devueltos por la DLL (layout `#[repr(C)]`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct WgMeshBuffers {
    /// Posiciones: 3 f32 por vértice, coordenadas de mundo.
    pub positions: *mut f32,
    /// Normales: 3 f32 por vértice.
    pub normals: *mut f32,
    /// UVs: 2 f32 por vértice.
    pub uvs: *mut f32,
    /// Índices: 3 u32 por triángulo.
    pub indices: *mut u32,
    /// Número de vértices.
    pub vertex_count: usize,
    /// Número de índices (múltiplo de 3).
    pub index_count: usize,
}

impl WgMeshBuffers {
    /// Buffer vacío.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            positions: std::ptr::null_mut(),
            normals: std::ptr::null_mut(),
            uvs: std::ptr::null_mut(),
            indices: std::ptr::null_mut(),
            vertex_count: 0,
            index_count: 0,
        }
    }
}

/// Buffers de texturas de una columna de chunks (splat + albedo de preview).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct WgChunkTextures {
    /// Pesos de splat RGBA8 (R=hierba, G=roca, B=arena, A=nieve).
    pub splat: *mut u8,
    /// Albedo de preview RGBA8.
    pub albedo: *mut u8,
    /// Largo de cada buffer en bytes (`dim² * 4`).
    pub len: usize,
    /// Dimensión del mapa (texels por lado).
    pub dim: u32,
}

impl WgChunkTextures {
    /// Buffer vacío.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            splat: std::ptr::null_mut(),
            albedo: std::ptr::null_mut(),
            len: 0,
            dim: 0,
        }
    }
}

/// Colocación de una instancia de blueprint (layout C).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct WgPlacement {
    /// Posición apoyada en la superficie (metros).
    pub position: [f32; 3],
    /// Rotación yaw (radianes).
    pub yaw: f32,
    /// Escala uniforme.
    pub scale: [f32; 3],
}

/// Lista de colocaciones devueltas por `wg_scatter`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct WgScatterPlacements {
    /// Array de colocaciones.
    pub data: *mut WgPlacement,
    /// Número de colocaciones.
    pub len: usize,
}

impl WgScatterPlacements {
    /// Buffer vacío.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            data: std::ptr::null_mut(),
            len: 0,
        }
    }
}

/// Códigos de estado del contrato (deben coincidir con `plugin_ffi::abi::Status`).
/// Espejo del contrato: las variantes se *construyen* en la DLL y aquí solo se
/// comparan códigos numéricos, de ahí el `allow(dead_code)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
#[allow(dead_code)]
pub enum Status {
    /// Operación completada.
    Ok = 0,
    /// Argumento nulo o handle inválido.
    NullArgument = 1,
    /// Config JSON inválida.
    InvalidConfig = 2,
    /// Pánico interno capturado en la DLL.
    InternalPanic = 3,
    /// Fallo de memoria en la DLL.
    OutOfMemory = 4,
    /// Chunk fuera de rango.
    ChunkOutOfRange = 5,
}
