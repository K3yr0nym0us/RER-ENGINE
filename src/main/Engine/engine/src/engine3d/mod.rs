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
pub(crate) mod particles;
mod platform;
mod probe_registry;
mod save_entity_3d;
mod screen_hud_image;
mod scripting;
mod texture;
pub(crate) mod vulkan;
pub(crate) mod water;

#[path = "config_3d/mod.rs"]
mod config_3d;
#[cfg(feature = "editor")]
pub(crate) use config_3d::character_anchor::PLAY_CHARACTER_CAMERA_CLIP_RADIUS;
#[cfg(feature = "editor")]
pub(crate) use config_3d::directional_light::{SHADOW_DARKNESS_MAX, SHADOW_DARKNESS_MIN};
#[cfg(feature = "editor")]
pub(crate) use config_3d::model_lod::{
    DEFAULT_MODEL_LOD_ENABLED, DEFAULT_MODEL_LOD1_DISTANCE_M, DEFAULT_MODEL_LOD2_DISTANCE_M,
};
#[cfg(feature = "editor")]
pub(crate) use config_3d::world_bounds::{
    MAX_WORLD_FACE_3D, MAX_WORLD_RADIUS_3D, MIN_WORLD_FACE_3D, MIN_WORLD_RADIUS_3D,
};
pub(crate) use config_3d::world_bounds::{MAX_WORLD_HEIGHT_3D, MIN_WORLD_HEIGHT_3D};
mod config_base;
#[path = "config_compat/mod.rs"]
mod config_compat;
mod config_shared;
#[path = "engine/mod.rs"]
pub(crate) mod engine;

pub mod host;
