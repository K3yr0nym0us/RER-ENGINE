mod animations;
mod animation_frame_stubs;
mod audio;
mod commands;
pub(crate) mod entity_restore;
pub(crate) mod load_proyect;
pub(crate) mod editor_scenes;
mod undo_entity;
mod undo_sockets;
mod undo_bone_physics;
mod animation_play_state;
#[path = "../save_snapshot.rs"]
mod save_snapshot;
mod init;
mod render;
mod scripts;
mod scene_scripts;
mod tick;
mod types;
#[path = "../engine.rs"]
mod inner;

pub use audio::DecodedAudio;
pub use inner::State;
pub use types::{ActiveAnimation, AnimationState};

pub(crate) use audio::{start_audio_thread, AudioSlot};
pub(crate) use render::create_depth_texture;
pub(crate) use render::is_aabb_visible_3d;
pub(crate) use types::{
    SceneUniforms, UndoAction, AUTOSAVE_INTERVAL, DEPTH_FORMAT, SHADOW_MAP_SIZE,
};
