mod animations;
mod audio;
mod commands;
#[path = "../save_snapshot.rs"]
mod save_snapshot;
mod init;
mod render;
mod scripts;
mod tick;
mod types;
#[path = "../engine.rs"]
mod inner;

pub use audio::DecodedAudio;
pub use inner::State;
pub use types::{ActiveAnimation, AnimationState};

pub(crate) use audio::{start_audio_thread, AudioSlot};
pub(crate) use render::create_depth_texture;
pub(crate) use types::{
    SceneUniforms, UndoAction, AUTOSAVE_INTERVAL, DEPTH_FORMAT, SHADOW_MAP_SIZE,
};
