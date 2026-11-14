//! Fachada C ABI del plugin (`rer_worldgen_plugin.dll`).
//!
//! Este es el **único módulo del crate con código `unsafe`**, justificado como FFI
//! de plataforma (mismo criterio que las GPU FFI del engine; ver `clippy.toml`).
//!
//! Reglas del contrato (versión [`abi::ABI_VERSION`]):
//!
//! 1. Toda export es `#[no_mangle] pub extern "C"` y envuelve la lógica en
//!    `catch_unwind`: un pánico del generador **nunca cruza la frontera**.
//! 2. Regla de un solo allocator: la DLL allocó → la DLL libera. El consumidor
//!    copia los buffers y llama a `wg_free_buffer`. Nunca `free` de C ni `delete`.
//! 3. Los handles opacos (`WgWorld`) solo son válidos entre `create` y `free`.
//! 4. Errores: códigos de retorno de [`abi::Status`] + `wg_last_error` (UTF-8).
//! 5. El engine debe comprobar `wg_abi_version()` antes de cualquier otro uso;
//!    un mismatch invalida todo el contrato.

pub mod abi;
pub mod buffers;
pub mod exports;
