//! Editor egui de RER-ENGINE.
//!
//! Comparte crate y `SharedGpu` con los motores (Ash/Vulkan). Electron actúa como
//! host headless de las modales y recibe los eventos del motor por stdout.

mod app;
mod assets;
mod bridge;
mod cli;
mod context;
mod engine_host;
mod engine_viewport;
mod i18n;
mod project_state;
mod render;
mod save;
mod scenes;
mod theme;
mod ui;
mod world;

pub mod host;
