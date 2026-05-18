use std::sync::Arc;
use std::time::{Duration, Instant};

use bytemuck::{Pod, Zeroable};

use crate::ipc::{AnimScriptData, AnimationFrameData};

use super::DecodedAudio;

pub(crate) enum UndoAction {
    RestoreTransform {
        id: u32,
        position: [f32; 3],
        rotation: [f32; 4],
        scale: [f32; 3],
    },
    RestoreTransforms {
        items: Vec<(u32, [f32; 3], [f32; 4], [f32; 3])>,
    },
    RemoveEntity { id: u32 },
}

#[derive(Clone)]
pub struct AnimationState {
    pub frames: Vec<AnimationFrameData>,
    pub fps: u32,
    pub loop_: bool,
    pub flip_horizontal: bool,
    pub audio_decoded: Option<Arc<DecodedAudio>>,
    pub logical_w: u32,
    pub logical_h: u32,
    pub scripts: Vec<AnimScriptData>,
    pub is_cancelable: bool,
}

pub struct ActiveAnimation {
    pub animation_name: String,
    pub current_frame: usize,
    pub last_frame_time: Instant,
    pub fps: u32,
    pub finished: bool,
}

pub(crate) const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
pub(crate) const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(5 * 60);

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct SceneUniforms {
    pub(crate) view_proj: [[f32; 4]; 4],
    pub(crate) cam_pos: [f32; 4],
    /// xyz = hacia el sol, w = ambiente 0–1.
    pub(crate) light_dir: [f32; 4],
    /// rgb = color de luz; w > 0.5 activa sombras proyectadas.
    pub(crate) light_color: [f32; 4],
    pub(crate) light_view_proj: [[f32; 4]; 4],
    /// x = intensidad, y = oscuridad en sombra (factor mínimo).
    pub(crate) light_params: [f32; 4],
}

pub(crate) const SHADOW_MAP_SIZE: u32 = 2048;
