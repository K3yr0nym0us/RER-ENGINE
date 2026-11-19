//! Editor egui de RER-ENGINE.
//!
//! Comparte crate y `SharedGpu` con los motores (Ash/Vulkan). Electron actúa como
//! host headless de las modales y recibe los eventos del motor por stdout.

mod app;
mod assets;
mod bridge;
mod cli;
mod context;
pub(crate) mod control_bindings;
mod entity_properties;
pub(crate) mod hud_editor;
mod i18n;
mod modal_window;
mod particle_window;
pub(crate) mod plugin_runtime;
pub(crate) mod plugins;
mod project_state;
mod render;
mod save;
mod scenes;
mod socket_config;
mod theme;
mod ui;
mod ui_screens;
mod user_event;
pub(crate) mod visual_script;
mod world;
pub(crate) mod worldgen;

pub mod host;
