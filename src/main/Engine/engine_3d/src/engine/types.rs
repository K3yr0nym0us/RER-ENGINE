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

pub(crate) const SHADOW_CASCADE_COUNT: u32 = 4;
pub(crate) const SHADOW_CASCADE_SIZE: u32 = 1024;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct SceneUniforms {
    pub(crate) view_proj: [[f32; 4]; 4],
    pub(crate) prev_view_proj: [[f32; 4]; 4],
    pub(crate) inv_view_proj: [[f32; 4]; 4],
    pub(crate) cam_pos: [f32; 4],
    /// xyz = hacia el sol, w = ambiente 0–1.
    pub(crate) light_dir: [f32; 4],
    /// rgb = color de luz; w > 0.5 activa sombras proyectadas.
    pub(crate) light_color: [f32; 4],
    pub(crate) light_view_proj: [[[f32; 4]; 4]; SHADOW_CASCADE_COUNT as usize],
    pub(crate) cascade_splits: [f32; 4],
    /// x = intensidad, z = 1/texel sombra, w = radio PCF (y sin uso en GPU).
    pub(crate) light_params: [f32; 4],
    /// xy = jitter subpíxel en espacio de proyección.
    pub(crate) jitter: [f32; 4],
    /// x = bias_min, y = bias_max, z = depth_const, w = depth_slope.
    pub(crate) shadow_bias: [f32; 4],
}
