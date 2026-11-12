//! Conformancia de la fachada FFI (F2): se llama a las exports como funciones
//! Rust para validar el contrato lógico (parse, códigos de estado, buffers y
//! ciclo de vida). La carga dinámica real de la DLL se valida en F6 con el engine.

use rer_worldgen::plugin_ffi::abi::{Status, WgMeshBuffers};
use rer_worldgen::plugin_ffi::exports::*;

const VALID_CONFIG: &str = r#"{
    "seed": 42,
    "bounds": { "width": 256.0, "length": 256.0, "height": 96.0 },
    "resolution": { "voxelSize": 2.0, "lodEnabled": false }
}"#;

#[test]
fn abi_version_is_stable() {
    assert_eq!(wg_abi_version(), 2);
}

#[test]
fn create_with_valid_config_returns_handle() {
    let handle = unsafe { wg_world_create(VALID_CONFIG.as_ptr(), VALID_CONFIG.len()) };
    assert!(!handle.is_null());
    unsafe { wg_world_free(handle) };
}

#[test]
fn create_with_invalid_json_returns_null_and_global_error() {
    let json = b"{ no es json }";
    let handle = unsafe { wg_world_create(json.as_ptr(), json.len()) };
    assert!(handle.is_null());
    let mut buffer = [0u8; 256];
    let written = unsafe { wg_last_error(std::ptr::null(), buffer.as_mut_ptr(), buffer.len()) };
    assert!(written > 0, "el error global debe tener mensaje");
    let message = std::str::from_utf8(&buffer[..written]).unwrap();
    assert!(
        message.contains("InvalidConfig"),
        "mensaje inesperado: {message}"
    );
}

#[test]
fn create_with_out_of_bounds_rejects_config() {
    let config = r#"{ "bounds": { "width": 1.0, "length": 256.0, "height": 96.0 } }"#;
    let handle = unsafe { wg_world_create(config.as_ptr(), config.len()) };
    assert!(handle.is_null(), "un width=1 debe rechazar la config");
}

#[test]
fn null_arguments_return_error_codes() {
    let handle = unsafe { wg_world_create(std::ptr::null(), 10) };
    assert!(handle.is_null());
    let mut out = WgMeshBuffers::empty();
    let code = unsafe { wg_mesh_chunk(std::ptr::null_mut(), 0, 0, 0, 0, &mut out) };
    assert_eq!(code, Status::NullArgument as i32);
    assert_eq!(unsafe { wg_chunk_status(std::ptr::null(), 0, 0, 0) }, -1);
    unsafe { wg_free_mesh_buffers(std::ptr::null()) };
    unsafe { wg_world_free(std::ptr::null_mut()) };
}

#[test]
fn mesh_chunk_produces_geometry_and_frees_cleanly() {
    let handle = unsafe { wg_world_create(VALID_CONFIG.as_ptr(), VALID_CONFIG.len()) };
    assert!(!handle.is_null());

    // Chunk con superficie: y=0 contiene terreno con bounds.height=96 (base ~33.6).
    let mut out = WgMeshBuffers::empty();
    let code = unsafe { wg_mesh_chunk(handle, 0, 0, 0, 0, &mut out) };
    assert_eq!(code, Status::Ok as i32, "wg_mesh_chunk debe tener éxito");
    assert!(
        out.vertex_count > 0,
        "el chunk (0,0,0) debe tener geometría"
    );
    assert_eq!(out.index_count % 3, 0);
    assert!(
        !out.positions.is_null()
            && !out.normals.is_null()
            && !out.uvs.is_null()
            && !out.indices.is_null()
    );

    // Copia de seguridad (lo que haría el engine antes de liberar).
    let positions =
        unsafe { std::slice::from_raw_parts(out.positions, out.vertex_count * 3) }.to_vec();
    let indices = unsafe { std::slice::from_raw_parts(out.indices, out.index_count) }.to_vec();
    assert_eq!(positions.len(), out.vertex_count * 3);
    assert!(!indices.is_empty());

    unsafe { wg_free_mesh_buffers(&out) };

    // Un chunk muy por encima del mundo: fuera de rango, nunca pánico.
    let mut out = WgMeshBuffers::empty();
    let code = unsafe { wg_mesh_chunk(handle, 0, 900, 0, 0, &mut out) };
    assert_eq!(code, Status::ChunkOutOfRange as i32);

    unsafe { wg_world_free(handle) };
}

#[test]
fn chunk_status_reflects_column_bands() {
    let handle = unsafe { wg_world_create(VALID_CONFIG.as_ptr(), VALID_CONFIG.len()) };
    assert!(!handle.is_null());
    // Con voxel 2.0 y height 96, hay 2 layers Y; la superficie está en y-chunk 0.
    assert_eq!(
        unsafe { wg_chunk_status(handle, 0, 0, 0) },
        1,
        "el chunk y=0 debe contener terreno"
    );
    assert_eq!(
        unsafe { wg_chunk_status(handle, 999, 0, 999) },
        -1,
        "chunk fuera de rango"
    );
    unsafe { wg_world_free(handle) };
}

#[test]
fn mesh_chunk_matches_direct_api() {
    // La vía FFI y la vía directa (Rust) deben producir la misma geometría.
    let config: rer_worldgen::config::WorldGenConfig = serde_json::from_str(VALID_CONFIG).unwrap();
    let field = rer_worldgen::field::DensityField::new(&config);
    let mesh = rer_worldgen::mesher::mesh_chunk(rer_worldgen::chunks::sample_chunk_grid(
        &field,
        &config,
        rer_worldgen::chunks::ChunkKey { x: 1, y: 0, z: 1 },
    ));

    let handle = unsafe { wg_world_create(VALID_CONFIG.as_ptr(), VALID_CONFIG.len()) };
    assert!(!handle.is_null());
    let mut out = WgMeshBuffers::empty();
    let code = unsafe { wg_mesh_chunk(handle, 1, 0, 1, 0, &mut out) };
    assert_eq!(code, Status::Ok as i32);

    assert_eq!(out.vertex_count, mesh.positions.len());
    assert_eq!(out.index_count, mesh.indices.len());
    let positions = unsafe { std::slice::from_raw_parts(out.positions, out.vertex_count * 3) };
    let flat: Vec<f32> = mesh
        .positions
        .iter()
        .flat_map(|p| p.iter())
        .copied()
        .collect();
    assert_eq!(positions, flat.as_slice(), "posiciones FFI != API directa");

    unsafe { wg_free_mesh_buffers(&out) };
    unsafe { wg_world_free(handle) };
}
