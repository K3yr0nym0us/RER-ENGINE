//! Módulos del motor 3D.
//!
//! Los `#[path]` conservan la estructura histórica (`config_3d/`, `engine/`,
//! `config_compat/`) sin duplicar ficheros de módulo.

mod assets;
mod bvh;
mod ecs;
mod engine_command;
mod entity_save_meta;
mod gizmo;
mod hud_image_asset;
pub(crate) mod ipc;
mod mesh;
mod platform;
mod probe_registry;
mod save_entity_3d;
mod screen_hud_image;
mod scripting;
mod texture;
pub(crate) mod vulkan;

#[path = "config_3d/mod.rs"]
mod config_3d;
#[cfg(feature = "editor")]
pub(crate) use config_3d::directional_light::{SHADOW_DARKNESS_MAX, SHADOW_DARKNESS_MIN};
mod config_base;
#[path = "config_compat/mod.rs"]
mod config_compat;
mod config_shared;
#[path = "engine/mod.rs"]
pub(crate) mod engine;

pub mod host;
