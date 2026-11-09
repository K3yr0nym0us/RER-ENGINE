//! Motor 3D de RER-ENGINE sobre Ash/Vulkan.
//!
//! El binario `rer_engine_3d` entra por [`engine3d::host::run`]. El editor podrá
//! reutilizar el mismo crate como librería cuando la ventana única esté montada.
//!
//! La feature `engine3d` mantiene el módulo aislado del resto de motores que se
//! irán sumando a este paquete (2D) y del editor.

#[cfg(feature = "engine3d")]
pub mod engine3d;

#[cfg(feature = "editor")]
pub mod editor;
