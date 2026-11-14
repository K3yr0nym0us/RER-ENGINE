//! Carga del plugin `rer_worldgen_plugin.dll` y resolución de símbolos ABI.
//!
//! La DLL se descarga/instala desde el editor (sección Plugins) y su ruta llega
//! al engine por el comando `world_gen_set_library_path`. El engine la carga de
//! forma diferida y comprueba `wg_abi_version()` antes de usarla: un mismatch
//! invalida el plugin con un evento de error, nunca degrada el motor.

use std::path::Path;

use libloading::{Library, Symbol};

use super::abi::{ABI_VERSION, WgChunkTextures, WgMeshBuffers, WgScatterPlacements};
/// Símbolos ABI del plugin. Los punteros son válidos mientras viva [`WorldgenLibrary`].
pub(crate) struct WorldgenLibrary {
    _lib: Library,
    /// `wg_world_create(json, len) -> handle`
    pub world_create: unsafe extern "C" fn(*const u8, usize) -> *mut std::ffi::c_void,
    /// `wg_world_free(handle)`
    pub world_free: unsafe extern "C" fn(*mut std::ffi::c_void),
    /// `wg_chunk_status(handle, cx, cy, cz) -> i32` (`1` contenido, `0` vacío, `-1` inválido)
    pub chunk_status: unsafe extern "C" fn(*const std::ffi::c_void, i32, i32, i32) -> i32,
    /// `wg_mesh_chunk(handle, cx, cy, cz, lod, out) -> i32` (código [`Status`])
    pub mesh_chunk:
        unsafe extern "C" fn(*mut std::ffi::c_void, i32, i32, i32, u32, *mut WgMeshBuffers) -> i32,
    /// `wg_last_error(handle, buffer, cap) -> usize`
    pub last_error: unsafe extern "C" fn(*const std::ffi::c_void, *mut u8, usize) -> usize,
    /// `wg_free_mesh_buffers(buffers)`
    pub free_mesh_buffers: unsafe extern "C" fn(*const WgMeshBuffers),
    /// `wg_chunk_textures(handle, cx, cz, lod, out) -> i32`
    pub chunk_textures:
        unsafe extern "C" fn(*mut std::ffi::c_void, i32, i32, u32, *mut WgChunkTextures) -> i32,
    /// `wg_free_chunk_textures(textures)`
    pub free_chunk_textures: unsafe extern "C" fn(*const WgChunkTextures),
    /// `wg_scatter(handle, rules_json, len, out) -> i32`
    pub scatter: unsafe extern "C" fn(
        *mut std::ffi::c_void,
        *const u8,
        usize,
        *mut WgScatterPlacements,
    ) -> i32,
    /// `wg_free_scatter(placements)`
    pub free_scatter: unsafe extern "C" fn(*const WgScatterPlacements),
}

impl WorldgenLibrary {
    /// Carga la DLL y resuelve todos los símbolos del contrato.
    ///
    /// # Errores
    ///
    /// Devuelve un mensaje legible si la DLL no se puede cargar, falta un símbolo
    /// o la versión de ABI no coincide.
    pub fn load(path: &Path) -> Result<Self, String> {
        // # Seguridad: Library::new ejecuta DllMain y los símbolos resueltos se
        // invocan con las firmas del contrato documentado en `rer-worldgen`.
        let lib = unsafe { Library::new(path) }
            .map_err(|e| format!("no se pudo cargar {}: {e}", path.display()))?;
        unsafe {
            let abi_version: Symbol<unsafe extern "C" fn() -> u32> = lib
                .get(b"wg_abi_version")
                .map_err(|e| format!("símbolo wg_abi_version ausente: {e}"))?;
            let found = abi_version();
            if found != ABI_VERSION {
                return Err(format!(
                    "ABI incompatible: engine espera v{ABI_VERSION}, el plugin es v{found} (reinstala el plugin)"
                ));
            }

            let world_create: Symbol<
                unsafe extern "C" fn(*const u8, usize) -> *mut std::ffi::c_void,
            > = lib
                .get(b"wg_world_create")
                .map_err(|e| format!("símbolo wg_world_create ausente: {e}"))?;
            let world_free: Symbol<unsafe extern "C" fn(*mut std::ffi::c_void)> = lib
                .get(b"wg_world_free")
                .map_err(|e| format!("símbolo wg_world_free ausente: {e}"))?;
            let chunk_status: Symbol<
                unsafe extern "C" fn(*const std::ffi::c_void, i32, i32, i32) -> i32,
            > = lib
                .get(b"wg_chunk_status")
                .map_err(|e| format!("símbolo wg_chunk_status ausente: {e}"))?;
            let mesh_chunk: Symbol<
                unsafe extern "C" fn(
                    *mut std::ffi::c_void,
                    i32,
                    i32,
                    i32,
                    u32,
                    *mut WgMeshBuffers,
                ) -> i32,
            > = lib
                .get(b"wg_mesh_chunk")
                .map_err(|e| format!("símbolo wg_mesh_chunk ausente: {e}"))?;
            let last_error: Symbol<
                unsafe extern "C" fn(*const std::ffi::c_void, *mut u8, usize) -> usize,
            > = lib
                .get(b"wg_last_error")
                .map_err(|e| format!("símbolo wg_last_error ausente: {e}"))?;
            let free_mesh_buffers: Symbol<unsafe extern "C" fn(*const WgMeshBuffers)> = lib
                .get(b"wg_free_mesh_buffers")
                .map_err(|e| format!("símbolo wg_free_mesh_buffers ausente: {e}"))?;
            let chunk_textures: Symbol<
                unsafe extern "C" fn(
                    *mut std::ffi::c_void,
                    i32,
                    i32,
                    u32,
                    *mut WgChunkTextures,
                ) -> i32,
            > = lib
                .get(b"wg_chunk_textures")
                .map_err(|e| format!("símbolo wg_chunk_textures ausente: {e}"))?;
            let free_chunk_textures: Symbol<unsafe extern "C" fn(*const WgChunkTextures)> = lib
                .get(b"wg_free_chunk_textures")
                .map_err(|e| format!("símbolo wg_free_chunk_textures ausente: {e}"))?;
            let scatter: Symbol<
                unsafe extern "C" fn(
                    *mut std::ffi::c_void,
                    *const u8,
                    usize,
                    *mut WgScatterPlacements,
                ) -> i32,
            > = lib
                .get(b"wg_scatter")
                .map_err(|e| format!("símbolo wg_scatter ausente: {e}"))?;
            let free_scatter: Symbol<unsafe extern "C" fn(*const WgScatterPlacements)> = lib
                .get(b"wg_free_scatter")
                .map_err(|e| format!("símbolo wg_free_scatter ausente: {e}"))?;

            // Copiar los punteros de función antes de mover `lib` al struct
            // (abi_version se consumió arriba al comprobar la versión).
            let (world_create, world_free) = (*world_create, *world_free);
            let (chunk_status, mesh_chunk) = (*chunk_status, *mesh_chunk);
            let (last_error, free_mesh_buffers) = (*last_error, *free_mesh_buffers);
            let (chunk_textures, free_chunk_textures) = (*chunk_textures, *free_chunk_textures);
            let (scatter, free_scatter) = (*scatter, *free_scatter);

            Ok(Self {
                _lib: lib,
                world_create,
                world_free,
                chunk_status,
                mesh_chunk,
                last_error,
                free_mesh_buffers,
                chunk_textures,
                free_chunk_textures,
                scatter,
                free_scatter,
            })
        }
    }

    /// Lee el último error del handle (o el global si es nulo) como `String`.
    pub(crate) fn last_error_text(&self, world: *const std::ffi::c_void) -> String {
        let mut buffer = [0u8; 512];
        // # Seguridad: símbolos con firma del contrato; buffer local del tamaño fijo.
        let n = unsafe { (self.last_error)(world, buffer.as_mut_ptr(), buffer.len()) };
        std::str::from_utf8(&buffer[..n.min(buffer.len() - 1)])
            .unwrap_or("error del plugin ilegible")
            .trim_end_matches('\0')
            .to_string()
    }
}
