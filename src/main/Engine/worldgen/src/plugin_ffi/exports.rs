//! Exports `#[unsafe(no_mangle)]` de la DLL del plugin.
//!
//! Cada export captura panics con `catch_unwind(AssertUnwindSafe(..))` y registra
//! el error en el handle para `wg_last_error`. Ninguna lógica unsafe vive aquí:
//! las exports son adaptadores finos sobre la API segura del crate, y los bloques
//! `unsafe` se limitan a la desreferenciación validada de punteros del contrato.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Mutex;

use super::abi::{ABI_VERSION, Status, WgMeshBuffers};
use super::buffers;
use crate::chunks::{CHUNK_SIZE, ChunkKey, active_y_chunks, sample_chunk_grid_lod};
use crate::config::WorldGenConfig;
use crate::field::DensityField;
use crate::mesher::mesh_chunk;
use crate::plugin_ffi::abi::WgChunkTextures;
use crate::scatter::{ScatterRules, blueprint_hash, scatter};
use crate::splat::{SplatContext, chunk_splat};

/// Estado opaco del mundo generado, expuesto como handle `*mut WgWorld`.
pub struct WgWorld {
    config: WorldGenConfig,
    field: DensityField,
    last_error: Mutex<String>,
}

impl WgWorld {
    fn set_error(&self, message: String) {
        if let Ok(mut slot) = self.last_error.lock() {
            *slot = message;
        }
    }
}

/// Versión de la ABI del plugin.
///
/// El engine DEBE llamarla primero y rechazar la DLL si no coincide.
#[unsafe(no_mangle)]
pub extern "C" fn wg_abi_version() -> u32 {
    ABI_VERSION
}

const MAX_CONFIG_BYTES: usize = 4 * 1024 * 1024;

/// Crea un mundo a partir de un JSON de configuración (`WorldGenConfig`, camelCase).
///
/// Devuelve nulo si el JSON es inválido o no pasa la validación; en ese caso el
/// motivo queda en `wg_last_error(null)` (error global del proceso).
///
/// # Safety
///
/// `config_json` debe apuntar a un buffer de solo lectura de exactamente
/// `config_len` bytes, válido durante la llamada.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wg_world_create(
    config_json: *const u8,
    config_len: usize,
) -> *mut WgWorld {
    let result = catch_unwind(AssertUnwindSafe(|| {
        // # Seguridad: buffer de solo lectura de `config_len` bytes según el contrato.
        let json = unsafe {
            if config_json.is_null() || config_len == 0 || config_len > MAX_CONFIG_BYTES {
                return Err(Status::NullArgument);
            }
            std::slice::from_raw_parts(config_json, config_len)
        };
        let json = std::str::from_utf8(json).map_err(|_| Status::InvalidConfig)?;
        let config =
            serde_json::from_str::<WorldGenConfig>(json).map_err(|_| Status::InvalidConfig)?;
        config.validate().map_err(|_| Status::InvalidConfig)?;
        let field = DensityField::new(&config);
        let world = WgWorld {
            config,
            field,
            last_error: Mutex::new(String::new()),
        };
        Ok(Box::into_raw(Box::new(world)))
    }));

    match result {
        Ok(Ok(handle)) => handle,
        Ok(Err(status)) => {
            set_global_error(format!("{status:?}: config invalida o no legible"));
            std::ptr::null_mut()
        }
        Err(panic) => {
            set_global_error(format!(
                "panic en wg_world_create: {}",
                panic_message(&panic)
            ));
            std::ptr::null_mut()
        }
    }
}

/// Libera un mundo creado por `wg_world_create`. Nulo es válido (no-op).
///
/// # Safety
///
/// `world` debe provenir de `wg_world_create` y no usarse después.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wg_world_free(world: *mut WgWorld) {
    if !world.is_null() {
        // # Seguridad: handle opaco de esta DLL, no usado después del free.
        drop(unsafe { Box::from_raw(world) });
    }
}

/// Indica si el chunk necesita malla: `1` = hay contenido, `0` = chunk vacío
/// (aire), `-1` = handle nulo o chunk fuera de rango.
///
/// El engine lo usa para el Y-layer pruning antes de pedir mallas.
///
/// # Safety
///
/// `world` debe ser un handle válido de `wg_world_create` (o nulo).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wg_chunk_status(world: *const WgWorld, cx: i32, cy: i32, cz: i32) -> i32 {
    // # Seguridad: handle opaco de esta DLL.
    let world = unsafe { world.as_ref() };
    let Some(world) = world else {
        return -1;
    };
    let result = catch_unwind(AssertUnwindSafe(|| {
        if !chunk_in_range(&world.config, cx, cy, cz) {
            return -1;
        }
        let active = active_y_chunks(&world.field, &world.config, cx, cz);
        if active.contains(&cy) { 1 } else { 0 }
    }));
    result.unwrap_or(-1)
}

/// Genera la malla del chunk y entrega los buffers en `out`.
///
/// El chunk vacío es un `Status::Ok` con contadores a 0. El consumidor copia los
/// buffers a su memoria/GPU y libera con `wg_free_mesh_buffers` (allocator único).
///
/// Devuelve `0` ([`Status::Ok`]) o el código del [`Status`] de error.
///
/// # Safety
///
/// `world` debe ser un handle válido de `wg_world_create`; `out` debe apuntar a
/// un `WgMeshBuffers` escribible (o ser nulo para invalidar la llamada).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wg_mesh_chunk(
    world: *mut WgWorld,
    cx: i32,
    cy: i32,
    cz: i32,
    lod: u32,
    out: *mut WgMeshBuffers,
) -> i32 {
    let result = catch_unwind(AssertUnwindSafe(|| {
        // # Seguridad: handle opaco de esta DLL; `out` es un struct del contrato.
        let world = unsafe { world.as_mut() }.ok_or(Status::NullArgument)?;
        let out = unsafe { out.as_mut() }.ok_or(Status::NullArgument)?;
        *out = WgMeshBuffers::empty();

        if !chunk_in_range(&world.config, cx, cy, cz) {
            return Err(Status::ChunkOutOfRange);
        }

        let chunk = ChunkKey {
            x: cx,
            y: cy,
            z: cz,
        };
        let grid = sample_chunk_grid_lod(&world.field, &world.config, chunk, lod);
        *out = buffers::mesh_into_raw(mesh_chunk(grid));
        Ok(())
    }));

    match result {
        Ok(Ok(())) => Status::Ok as i32,
        Ok(Err(status)) => {
            // # Seguridad: `world` ya se validó dentro del closure; aquí solo
            // reconsultamos el handle para registrar el error.
            if let Some(world) = unsafe { world.as_ref() } {
                world.set_error(format!("{status:?}"));
            }
            status as i32
        }
        Err(panic) => {
            if let Some(world) = unsafe { world.as_ref() } {
                world.set_error(format!("panic en wg_mesh_chunk: {}", panic_message(&panic)));
            }
            Status::InternalPanic as i32
        }
    }
}

/// Copia el último error del handle a `buffer` (UTF-8, truncado a `cap` bytes y
/// terminado en NUL). Devuelve el número de bytes escritos (sin NUL). Con handle
/// nulo usa el error global del proceso.
///
/// # Safety
///
/// `buffer` debe apuntar a memoria escribible de al menos `cap` bytes, o ser nulo
/// con `cap == 0` para consultar el largo necesario.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wg_last_error(
    world: *const WgWorld,
    buffer: *mut u8,
    cap: usize,
) -> usize {
    let message = if world.is_null() {
        GLOBAL_ERROR
            .lock()
            .map(|slot| slot.clone())
            .unwrap_or_default()
    } else {
        // # Seguridad: handle opaco de esta DLL.
        let world = unsafe { world.as_ref() };
        world
            .and_then(|w| w.last_error.lock().ok().map(|slot| slot.clone()))
            .unwrap_or_default()
    };
    if buffer.is_null() || cap == 0 {
        return 0;
    }
    let bytes = message.as_bytes();
    let n = bytes.len().min(cap - 1);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, n);
        // # Seguridad: buffer tiene al menos `cap` bytes según el contrato.
        *buffer.add(n) = 0;
    }
    n
}

/// Libera los buffers de una malla entregada por `wg_mesh_chunk`.
///
/// Debe llamarse una única vez por `WgMeshBuffers` recibido, con el mismo struct
/// (los contadores definen la longitud exacta de cada allocation). Nulo es válido.
///
/// # Safety
///
/// `buffers` debe provenir de esta misma DLL (allocator único). Tras la llamada,
/// los punteros del struct quedan invalidados y el struct no debe liberarse de nuevo.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wg_free_mesh_buffers(buffers: *const WgMeshBuffers) {
    // # Seguridad: puntero a struct del contrato entregado por `wg_mesh_chunk`.
    let buffers = unsafe { buffers.as_ref() };
    if let Some(buffers) = buffers {
        unsafe { buffers::free_mesh_buffers(buffers) };
    }
}

/// Genera los buffers de texturas de una columna de chunks (splat + albedo).
///
/// El splat es por columna de superficie: todos los chunks Y de la misma columna
/// comparten texturas. El consumidor copia y libera con `wg_free_chunk_textures`.
///
/// # Safety
///
/// `world` debe ser un handle válido; `out` un `WgChunkTextures` escribible.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wg_chunk_textures(
    world: *mut WgWorld,
    cx: i32,
    cz: i32,
    lod: u32,
    out: *mut WgChunkTextures,
) -> i32 {
    let result = catch_unwind(AssertUnwindSafe(|| {
        // # Seguridad: handle opaco de esta DLL y struct del contrato.
        let world = unsafe { world.as_mut() }.ok_or(Status::NullArgument)?;
        let out = unsafe { out.as_mut() }.ok_or(Status::NullArgument)?;
        *out = WgChunkTextures::empty();

        if !chunk_in_range(&world.config, cx, 0, cz) {
            return Err(Status::ChunkOutOfRange);
        }

        let ctx = SplatContext::new(
            &world.field,
            &world.config,
            world.field.plan().sites.clone(),
        );
        let (splat, albedo, dim) = chunk_splat(&world.field, &ctx, cx, cz, lod);
        *out = buffers::textures_into_raw(splat, albedo, dim);
        Ok(())
    }));

    match result {
        Ok(Ok(())) => Status::Ok as i32,
        Ok(Err(status)) => {
            if let Some(world) = unsafe { world.as_ref() } {
                world.set_error(format!("{status:?}"));
            }
            status as i32
        }
        Err(panic) => {
            if let Some(world) = unsafe { world.as_ref() } {
                world.set_error(format!(
                    "panic en wg_chunk_textures: {}",
                    panic_message(&panic)
                ));
            }
            Status::InternalPanic as i32
        }
    }
}

/// Libera los buffers de texturas entregados por `wg_chunk_textures`.
///
/// # Safety
///
/// `textures` debe provenir de esta misma DLL (allocator único), liberado una
/// única vez. Nulo es válido.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wg_free_chunk_textures(textures: *const WgChunkTextures) {
    // # Seguridad: puntero a struct del contrato entregado por wg_chunk_textures.
    let textures = unsafe { textures.as_ref() };
    if let Some(textures) = textures {
        unsafe { buffers::free_chunk_textures(textures) };
    }
}

/// Calcula las colocaciones de un blueprint según las reglas (JSON camelCase de
/// `ScatterRules` + campo obligatorio `blueprintId`).
///
/// El consumidor copia el array y libera con `wg_free_scatter`.
///
/// # Safety
///
/// `world` debe ser un handle válido; `rules_json` un buffer de solo lectura de
/// `rules_len` bytes; `out` un `WgScatterPlacements` escribible.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wg_scatter(
    world: *mut WgWorld,
    rules_json: *const u8,
    rules_len: usize,
    out: *mut crate::plugin_ffi::abi::WgScatterPlacements,
) -> i32 {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let world = unsafe { world.as_mut() }.ok_or(Status::NullArgument)?;
        let out = unsafe { out.as_mut() }.ok_or(Status::NullArgument)?;
        *out = crate::plugin_ffi::abi::WgScatterPlacements::empty();

        if rules_json.is_null() || rules_len == 0 || rules_len > MAX_CONFIG_BYTES {
            return Err(Status::NullArgument);
        }
        // # Seguridad: buffer de solo lectura de `rules_len` bytes.
        let json = unsafe { std::slice::from_raw_parts(rules_json, rules_len) };
        let json = std::str::from_utf8(json).map_err(|_| Status::InvalidConfig)?;
        let value: serde_json::Value =
            serde_json::from_str(json).map_err(|_| Status::InvalidConfig)?;
        // El `blueprintId` es obligatorio para sembrar la secuencia del scatter.
        let blueprint_id = value
            .get("blueprintId")
            .and_then(|v| v.as_str())
            .ok_or(Status::InvalidConfig)?
            .to_string();
        let mut rules: ScatterRules =
            serde_json::from_value(value).map_err(|_| Status::InvalidConfig)?;
        rules.max_instances = rules.max_instances.min(MAX_SCATTER_INSTANCES);

        let placements = scatter(
            &world.field,
            &world.config,
            &rules,
            blueprint_hash(&blueprint_id),
        );
        let placement_values: Vec<crate::plugin_ffi::abi::WgPlacement> = placements
            .iter()
            .map(|p| crate::plugin_ffi::abi::WgPlacement {
                position: p.position,
                yaw: p.yaw,
                scale: p.scale,
            })
            .collect();
        *out = buffers::scatter_into_raw(placement_values);
        Ok(())
    }));

    match result {
        Ok(Ok(())) => Status::Ok as i32,
        Ok(Err(status)) => {
            if let Some(world) = unsafe { world.as_ref() } {
                world.set_error(format!("{status:?}"));
            }
            status as i32
        }
        Err(panic) => {
            if let Some(world) = unsafe { world.as_ref() } {
                world.set_error(format!("panic en wg_scatter: {}", panic_message(&panic)));
            }
            Status::InternalPanic as i32
        }
    }
}

/// Límite duro de instancias por llamada de scatter (protección de memoria).
const MAX_SCATTER_INSTANCES: u32 = 200_000;

/// Libera la lista de colocaciones entregada por `wg_scatter`.
///
/// # Safety
///
/// Debe provenir de esta misma DLL (allocator único), liberada una única vez.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wg_free_scatter(
    placements: *const crate::plugin_ffi::abi::WgScatterPlacements,
) {
    // # Seguridad: puntero a struct del contrato entregado por wg_scatter.
    let placements = unsafe { placements.as_ref() };
    if let Some(placements) = placements {
        unsafe { buffers::free_scatter_placements(placements) };
    }
}

static GLOBAL_ERROR: Mutex<String> = Mutex::new(String::new());

fn set_global_error(message: String) {
    if let Ok(mut slot) = GLOBAL_ERROR.lock() {
        *slot = message;
    }
}

/// Verifica que el chunk esté dentro de los límites del mundo configurado (X, Z
/// contra los límites de dimensiones; Y contra la altura máxima en chunks).
fn chunk_in_range(config: &WorldGenConfig, cx: i32, cy: i32, cz: i32) -> bool {
    let (chunks_x, chunks_z) = config.chunk_count_xz(CHUNK_SIZE);
    cx >= 0
        && cz >= 0
        && (cx as u32) < chunks_x
        && (cz as u32) < chunks_z
        && cy >= 0
        && (cy as u32) <= config.max_chunk_y(CHUNK_SIZE)
}

fn panic_message(panic: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = panic.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = panic.downcast_ref::<String>() {
        message.clone()
    } else {
        "panic desconocido".to_string()
    }
}
