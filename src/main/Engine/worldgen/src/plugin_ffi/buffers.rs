//! Gestión de buffers a través de la frontera FFI.
//!
//! Regla del allocator único: todo puntero devuelto por la DLL proviene de un
//! `Vec<T>` convertido a `Box<[T]>` (longitud exacta) y filtrado a raw pointer.
//! `free_boxed` reconstruye el `Box` con la misma longitud exacta y lo libera con
//! el allocator de la DLL. El consumidor jamás libera estos punteros por su cuenta.

use crate::mesher::MeshBuffer;
use crate::plugin_ffi::abi::WgMeshBuffers;

/// Convierte un `Vec<T>` en puntero crudo + longitud exacta para entregarlo por FFI.
///
/// `into_boxed_slice` garantiza `len == capacity`, lo que hace que la liberación
/// por [`free_boxed`] use exactamente el layout de la allocation original.
#[must_use]
pub fn vec_into_raw<T>(vec: Vec<T>) -> (*mut T, usize) {
    let boxed = vec.into_boxed_slice();
    let len = boxed.len();
    (Box::into_raw(boxed) as *mut T, len)
}

/// Libera un puntero creado por [`vec_into_raw`].
///
/// # Safety
///
/// Ver [`free_boxed`]: `ptr` debe provenir de `vec_into_raw` con el mismo `T` y
/// `len` del mismo origen. Solo la DLL debe llamar a esta función (allocator único).
///
/// # Panics
///
/// Nunca (el nulo es no-op).
pub unsafe fn free_boxed<T>(ptr: *mut T, len: usize) {
    if ptr.is_null() {
        return;
    }
    // # Seguridad: reconstruimos el Box<[T]> exacto creado por `vec_into_raw`.
    drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)) });
}

/// Aplana `Vec<[f32; N]>` a un `Vec<f32>` contiguo para entregarlo por FFI.
fn flatten<const N: usize>(planes: Vec<[f32; N]>) -> Vec<f32> {
    let mut flat = Vec::with_capacity(planes.len() * N);
    for plane in planes {
        flat.extend_from_slice(&plane);
    }
    flat
}

/// Empaqueta un [`MeshBuffer`] en la estructura `#[repr(C)]` para el consumidor.
#[must_use]
pub fn mesh_into_raw(mesh: MeshBuffer) -> WgMeshBuffers {
    let MeshBuffer {
        positions,
        normals,
        uvs,
        indices,
    } = mesh;
    let vertex_count = positions.len();
    let index_count = indices.len();
    let (positions, _) = vec_into_raw(flatten(positions));
    let (normals, _) = vec_into_raw(flatten(normals));
    let (uvs, _) = vec_into_raw(flatten(uvs));
    let (indices, _) = vec_into_raw(indices);
    WgMeshBuffers {
        positions,
        normals,
        uvs,
        indices,
        vertex_count,
        index_count,
    }
}

/// Empaqueta los buffers de texturas de una columna.
#[must_use]
pub fn textures_into_raw(
    splat: Vec<u8>,
    albedo: Vec<u8>,
    dim: u32,
) -> crate::plugin_ffi::abi::WgChunkTextures {
    debug_assert_eq!(splat.len(), albedo.len());
    let len = splat.len();
    let (splat, _) = vec_into_raw(splat);
    let (albedo, _) = vec_into_raw(albedo);
    crate::plugin_ffi::abi::WgChunkTextures {
        splat,
        albedo,
        len,
        dim,
    }
}

/// Libera los buffers de texturas creados por [`textures_into_raw`].
///
/// # Safety
///
/// Debe provenir de esta misma DLL y liberarse una única vez.
pub unsafe fn free_chunk_textures(textures: &crate::plugin_ffi::abi::WgChunkTextures) {
    unsafe {
        free_boxed(textures.splat, textures.len);
        free_boxed(textures.albedo, textures.len);
    }
}

/// Empaqueta una lista de colocaciones de scatter.
#[must_use]
pub fn scatter_into_raw(
    placements: Vec<crate::plugin_ffi::abi::WgPlacement>,
) -> crate::plugin_ffi::abi::WgScatterPlacements {
    let len = placements.len();
    let (data, _) = vec_into_raw(placements);
    crate::plugin_ffi::abi::WgScatterPlacements { data, len }
}

/// Libera la lista de colocaciones creada por [`scatter_into_raw`].
///
/// # Safety
///
/// Debe provenir de esta misma DLL y liberarse una única vez.
pub unsafe fn free_scatter_placements(placements: &crate::plugin_ffi::abi::WgScatterPlacements) {
    unsafe { free_boxed(placements.data, placements.len) };
}

/// Libera los cuatro buffers de un [`WgMeshBuffers`] creado por [`mesh_into_raw`].
///
/// # Safety
///
/// `buffers` debe apuntar a un struct devuelto por `wg_mesh_chunk` en esta misma
/// DLL (los contadores definen la longitud exacta de cada allocation). Los
/// punteros quedan invalidados; llamarlo dos veces es un error del consumidor.
pub unsafe fn free_mesh_buffers(buffers: &WgMeshBuffers) {
    unsafe {
        free_boxed(buffers.positions, buffers.vertex_count * 3);
        free_boxed(buffers.normals, buffers.vertex_count * 3);
        free_boxed(buffers.uvs, buffers.vertex_count * 2);
        free_boxed(buffers.indices, buffers.index_count);
    }
}
