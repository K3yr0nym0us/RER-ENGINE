//! Contrato de la ABI del plugin: versión, códigos de estado y estructuras C.
//!
//! Este módulo es la fuente canónica del contrato engine↔plugin. Cualquier cambio
//! de firma o de layout debe incrementar [`ABI_VERSION`] y actualizar el consumidor
//! del engine (`engine3d/world3d`).

/// Versión de la ABI. El engine la comprueba con `wg_abi_version()` al cargar la
/// DLL y rechaza el plugin si no coincide exactamente.
pub const ABI_VERSION: u32 = 2;

/// Buffers de texturas de una columna de chunks (splat + albedo de preview).
///
/// Propiedad: punteros válidos hasta `wg_free_chunk_textures`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct WgChunkTextures {
    /// Pesos de splat RGBA8 (R=hierba, G=roca, B=arena, A=nieve).
    pub splat: *mut u8,
    /// Albedo de preview RGBA8 (mezcla de paleta por pesos).
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

/// Códigos de retorno de las exports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Status {
    /// Operación completada (incluye chunks vacíos, con contadores a 0).
    Ok = 0,
    /// Argumento nulo o handle inválido.
    NullArgument = 1,
    /// Config JSON malformado o que no pasa la validación.
    InvalidConfig = 2,
    /// Pánico interno capturado en la frontera (ver `wg_last_error`).
    InternalPanic = 3,
    /// Fallo al reservar memoria para los buffers de salida.
    OutOfMemory = 4,
    /// Chunk fuera del rango del mundo configurado.
    ChunkOutOfRange = 5,
}

impl From<Status> for i32 {
    fn from(status: Status) -> Self {
        status as i32
    }
}

/// Buffers de geometría de un chunk devueltos por la DLL.
///
/// Propiedad: los punteros son válidos hasta la llamada a `wg_free_buffer(ptr)`
/// por cada buffer no nulo. El consumidor debe copiar (a staging GPU) antes de
/// liberar. Los contadores son elementos (no bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct WgMeshBuffers {
    /// Posiciones: 3 f32 por vértice, en coordenadas de mundo.
    pub positions: *mut f32,
    /// Normales: 3 f32 por vértice.
    pub normals: *mut f32,
    /// UVs: 2 f32 por vértice (placeholder hasta la fase de splat).
    pub uvs: *mut f32,
    /// Índices: 3 u32 por triángulo.
    pub indices: *mut u32,
    /// Número de vértices (largo de positions/3, normals/3 y uvs/2).
    pub vertex_count: usize,
    /// Número de índices (múltiplo de 3).
    pub index_count: usize,
}

impl WgMeshBuffers {
    /// Buffer vacío (todas las salidas a 0/nulo).
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

/// Estructura de error serializable para el consumidor.
#[derive(Debug, Clone, PartialEq)]
pub struct AbiError {
    /// Código de estado.
    pub status: Status,
    /// Mensaje UTF-8 legible (vacío si no hay error).
    pub message: String,
}

impl AbiError {
    /// Error sin mensaje.
    #[must_use]
    pub const fn new(status: Status) -> Self {
        Self {
            status,
            message: String::new(),
        }
    }
}

impl std::fmt::Display for AbiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.message.is_empty() {
            write!(f, "worldgen FFI: {:?}", self.status)
        } else {
            write!(f, "worldgen FFI {:?}: {}", self.status, self.message)
        }
    }
}

impl std::error::Error for AbiError {}
