//! Motor 2D: host winit, ECS, fisica XY, player UI y renderer Ash.
//!
//! El binario `rer_engine_2d` entra por [`host::run`]. El modulo solo se
//! compila con la feature `engine2d`.

mod bundled_demo;
#[path = "config_2d/mod.rs"]
mod config_2d;
mod config_base;
mod config_compat;
mod config_shared;
mod ecs;
mod engine;
mod engine_command;
mod entity_save_meta;
mod gizmo;
mod hud_image_asset;
mod ipc;
mod mesh;
mod on_keep;
mod on_press;
mod screen_hud_image;
mod scripting;
mod spatial;
mod texture;
mod vulkan;

pub mod host;
