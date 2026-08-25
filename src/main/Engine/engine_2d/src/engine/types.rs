use std::sync::Arc;
use std::time::{Duration, Instant};

use bytemuck::{Pod, Zeroable};

use crate::config_2d::{CharacterMarker, ScenarioMarker};
use crate::ecs::MeshComponent;
use crate::ipc::{AnimScriptData, AnimationFrameData};

use super::audio::DecodedAudio;

pub(crate) const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
pub(crate) const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// Undo/gizmo snapshot: `(entity_id, position, rotation_quat, scale)`.
pub(crate) type EntityTransformSnapshot = (u32, [f32; 3], [f32; 4], [f32; 3]);

pub(crate) enum UndoAction {
    RestoreTransform {
        id: u32,
        position: [f32; 3],
        rotation: [f32; 4],
        scale: [f32; 3],
    },
    RestoreTransforms {
        items: Vec<EntityTransformSnapshot>,
    },
    /// Undo de creación (quick build, etc.): deshacer = eliminar la entidad.
    RemoveEntity {
        snapshot: EntityUndoSnapshot,
    },
    /// Redo tras eliminar una entidad recién creada: volver a insertarla con el mismo id.
    RestoreEntity {
        snapshot: EntityUndoSnapshot,
    },
    RestorePlayerUiHud {
        snapshot: crate::config_2d::player_ui::hud_undo::PlayerUiHudUndoSnapshot,
    },
}

#[derive(Clone)]
pub(crate) struct EntityUndoSnapshot {
    pub id: u32,
    pub name: String,
    pub transform_position: [f32; 3],
    pub transform_rotation: [f32; 4],
    pub transform_scale: [f32; 3],
    pub mesh: MeshComponent,
    pub kind: UndoEntityKind,
}

#[derive(Clone)]
pub(crate) enum UndoEntityKind {
    Character(CharacterMarker),
    Scenario(ScenarioMarker),
    Collider { points: [[f32; 2]; 4] },
    ExecutionArea { points: [[f32; 2]; 4] },
}

#[derive(Clone)]
pub struct AnimationState {
    pub frames: Vec<AnimationFrameData>,
    pub fps: u32,
    pub loop_: bool,
    pub flip_horizontal: bool,
    /// Audio pre-decodificado a muestras PCM durante SetAnimation.
    /// `None` si la animación no tiene audio o falló la decodificación.
    pub audio_decoded: Option<Arc<DecodedAudio>>,
    pub logical_w: u32,
    pub logical_h: u32,
    /// Scripts Rhai que se ejecutan solo mientras esta animación está activa.
    pub scripts: Vec<AnimScriptData>,
    /// Si false, ningún `PlayAnimation` puede interrumpirla hasta que termine.
    pub is_cancelable: bool,
}

#[derive(Clone, Copy)]
pub struct AnimTextureCacheEntry {
    pub uv_rect: [f32; 4],
    pub img_width: u32,
    pub img_height: u32,
    pub tight_bounds: Option<[u32; 4]>,
}

pub struct ActiveAnimation {
    pub animation_name: String,
    pub current_frame: usize,
    pub last_frame_time: Instant,
    pub fps: u32,
    pub finished: bool,
}

// ── Uniform compartido por frame (group 0) ───────────────────────────────────
// Solo view_proj + cam_pos; el model matrix va en el instance buffer.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct SceneUniforms {
    pub(crate) view_proj: [[f32; 4]; 4],
    pub(crate) cam_pos: [f32; 4], // xyz = posición cámara, w = sin uso
}

/// Uniformes del pass `screen_hud_pipeline` (layout completo de `shader_screen_hud.wgsl`).
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct ScreenHudSceneUniforms {
    pub view_proj: [[f32; 4]; 4],
    pub view_proj_stable: [[f32; 4]; 4],
    pub prev_view_proj: [[f32; 4]; 4],
    pub inv_view_proj: [[f32; 4]; 4],
    pub cam_pos: [f32; 4],
    pub light_dir: [f32; 4],
    pub light_color: [f32; 4],
    pub light_view_proj: [[f32; 4]; 4],
    pub light_params: [f32; 4],
    pub jitter: [f32; 4],
    pub shadow_bias: [f32; 4],
}

impl ScreenHudSceneUniforms {
    pub(crate) fn ndc_identity() -> Self {
        let id: [[f32; 4]; 4] = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        Self {
            view_proj: id,
            view_proj_stable: id,
            prev_view_proj: id,
            inv_view_proj: id,
            cam_pos: [0.0, 0.0, 5.0, 0.0],
            light_dir: [0.0, 1.0, 0.0, 1.0],
            light_color: [1.0, 1.0, 1.0, 0.0],
            light_view_proj: id,
            light_params: [0.0; 4],
            jitter: [0.0; 4],
            shadow_bias: [0.0; 4],
        }
    }
}

/// Estado de un deslizamiento suave iniciado desde `on_press`.
/// La entidad se mueve hacia (target_x, target_y) a `speed` u/s
/// usando el shape-cast kinematic de Rapier, con detección de colisiones.
#[derive(Clone, Copy)]
pub(crate) struct PendingSlide {
    pub target_x: f32,
    pub target_y: f32,
    pub speed: f32,
    /// Si es true, el slide solo corrige X y nunca intenta volver a target_y.
    /// Evita cancelar saltos cuando se aplica deriva horizontal en on_press.
    pub keep_current_y: bool,
}
