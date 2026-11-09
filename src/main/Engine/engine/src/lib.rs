//! Motores y editor de RER-ENGINE sobre Ash/Vulkan.
//!
//! Los binarios standalone entran por [`engine3d::host::run`] y
//! [`engine2d::host::run`]. El editor reutiliza el 3D en proceso y hospedará el
//! 2D cuando la ventana única esté montada.
//!
//! Cada motor vive tras su feature (`engine3d` / `engine2d`) y el editor tras
//! `editor`: los binarios se compilan con `--no-default-features --features
//! <motor>` para no enlazar el motor ajeno.

#[cfg(feature = "engine3d")]
pub mod engine3d;

#[cfg(feature = "engine2d")]
pub mod engine2d;

#[cfg(feature = "editor")]
pub mod editor;
