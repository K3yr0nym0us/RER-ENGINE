use std::sync::Arc;
use std::time::{Duration, Instant};

use bytemuck::{Pod, Zeroable};

use crate::config_3d::shadow_cascades::MAX_SHADOW_CASCADES;
use crate::ecs::MeshComponent;
use crate::entity_save_meta::EntitySaveMeta;
use crate::ipc::{AnimScriptData, AnimationFrameData};

use super::DecodedAudio;

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
    /// Undo de creación: deshacer = eliminar la entidad (snapshot para redo).
    RemoveEntity {
        snapshot: EntityUndoSnapshot,
    },
    /// Redo tras Ctrl+Z en creación: volver a insertar con el mismo id.
    RestoreEntity {
        snapshot: EntityUndoSnapshot,
    },
    /// Deshacer cambios en el HUD de la pantalla UI en edición (texto, botones, imágenes, objetos, dibujo).
    RestorePlayerUiHud {
        snapshot: crate::config_3d::player_ui::hud_undo::PlayerUiHudUndoSnapshot,
    },
    /// Deshacer creación de socket: eliminar el socket creado.
    RemoveEntitySocket {
        entity_id: u32,
        socket: crate::config_3d::entity_sockets::EntitySocketSnapshot,
    },
    /// Redo tras deshacer creación de socket.
    RestoreEntitySocket {
        entity_id: u32,
        socket: crate::config_3d::entity_sockets::EntitySocketSnapshot,
    },
    /// Deshacer vinculación a socket: restaurar attachment/transform previos.
    RestoreSocketAttachment {
        child_id: u32,
        previous_attachment: Option<crate::config_3d::entity_attachments::EntityAttachmentLocal>,
        previous_position: [f32; 3],
        previous_rotation: [f32; 4],
        previous_scale: [f32; 3],
        applied_attachment: crate::config_3d::entity_attachments::EntityAttachmentLocal,
    },
    RestoreBonePhysics {
        entity_id: u32,
        bone_name: String,
        before: Option<crate::config_3d::bone_physics::BonePhysicsMode>,
        after: Option<crate::config_3d::bone_physics::BonePhysicsMode>,
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
    pub save_meta: EntitySaveMeta,
    pub physics_enabled: bool,
    pub physics_type: String,
    pub physics_half: [f32; 3],
    pub in_character_list: bool,
    pub in_scenario_list: bool,
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

pub(crate) const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// 1024: ~4× menos trabajo que 2048 en el shadow pass; PCF/texel usan esta constante.
pub(crate) const SHADOW_MAP_SIZE: u32 = 1024;

/// Bytes del UBO de escena. Debe sobrar sitio para `SceneUniforms` entero (con las 4 matrices de
/// cascada); `frame.rs` lo reserva fijo y copia el struct por delante.
pub(crate) const SCENE_UBO_BYTES: usize = 1024;

const _: () = assert!(std::mem::size_of::<SceneUniforms>() <= SCENE_UBO_BYTES);

/// Los 4 shaders que declaran `uniform Scene` (`forward.vert`, `forward.frag`, `skinned.vert`,
/// `shadow.vert`) son un espejo byte a byte de este struct. Si un offset se mueve, el shader lee
/// otros campos y la escena se deforma SIN ningún error de API: estos asserts fijan el contrato.
const _: () = assert!(std::mem::size_of::<SceneUniforms>() == 672);
const _: () = assert!(std::mem::offset_of!(SceneUniforms, light_view_proj) == 304);
const _: () = assert!(std::mem::offset_of!(SceneUniforms, cascade_splits) == 560);
const _: () = assert!(std::mem::offset_of!(SceneUniforms, shadow_bias) == 656);

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct SceneUniforms {
    /// Proyección con jitter Halton (raster).
    pub(crate) view_proj: [[f32; 4]; 4],
    /// Proyección sin jitter (velocity / `prev_view_proj`).
    pub(crate) view_proj_stable: [[f32; 4]; 4],
    pub(crate) prev_view_proj: [[f32; 4]; 4],
    pub(crate) inv_view_proj: [[f32; 4]; 4],
    pub(crate) cam_pos: [f32; 4],
    /// xyz = hacia el sol, w = ambiente 0–1.
    pub(crate) light_dir: [f32; 4],
    /// rgb = color de luz; w > 0.5 activa sombras proyectadas.
    pub(crate) light_color: [f32; 4],
    /// Una matriz `luz · proyección` por cascada (CSM). El pase de sombras reescribe la ranura 0
    /// con la cascada que está dibujando (`shadow.vert` siempre lee `[0]`); el forward elige
    /// cascada por profundidad contra `cascade_splits`, así que ahí la array se usa completa.
    pub(crate) light_view_proj: [[[f32; 4]; 4]; MAX_SHADOW_CASCADES],
    /// Borde lejano de cada cascada, en distancia desde el ojo. `f32::MAX` en las inactivas, para
    /// que el shader busque la primera que encaje sin ramificar por tier.
    pub(crate) cascade_splits: [f32; 4],
    /// Mitad del lado de la caja ortográfica por cascada (unidades de mundo). El radio PCF se
    /// escala con esto: una cascada lejana cubre más mundo por texel y necesita taps más separados.
    pub(crate) cascade_extent: [f32; 4],
    /// x = radio PCF en unidades de mundo, y = grosor del fade en la costura entre cascadas,
    /// z = 1 / lado del shadow map (texel en UV, idéntico para todas las capas), w = cascadas activas.
    pub(crate) shadow_params: [f32; 4],
    /// x = intensidad, z = 1/texel sombra, w = radio PCF (y sin uso en GPU).
    pub(crate) light_params: [f32; 4],
    /// xy = jitter subpíxel en espacio de proyección; zw sin uso (near/far en `depth_plane`).
    pub(crate) jitter: [f32; 4],
    /// x = near, y = far; z = 1 cuando SSR/reflejos activos (forward IBL cede especular).
    pub(crate) depth_plane: [f32; 4],
    /// x = bias_min, y = bias_max, z = depth_const, w = depth_slope.
    pub(crate) shadow_bias: [f32; 4],
}
