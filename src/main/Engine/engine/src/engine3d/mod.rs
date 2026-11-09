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
mod ipc;
mod mesh;
mod platform;
mod probe_registry;
mod save_entity_3d;
mod screen_hud_image;
mod scripting;
mod texture;
mod vulkan;

#[path = "config_3d/mod.rs"]
mod config_3d;
mod config_base;
#[path = "config_compat/mod.rs"]
mod config_compat;
mod config_shared;
#[path = "engine/mod.rs"]
mod engine;

pub mod host;
