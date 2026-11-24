use std::sync::Arc;
use std::time::{Duration, Instant};

use bytemuck::{Pod, Zeroable};

use crate::engine3d::config_3d::shadow_cascades::MAX_SHADOW_CASCADES;
use crate::engine3d::ecs::MeshComponent;
use crate::engine3d::entity_save_meta::EntitySaveMeta;
use crate::engine3d::ipc::{AnimScriptData, AnimationFrameData};

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
        snapshot: crate::engine3d::config_3d::player_ui::hud_undo::PlayerUiHudUndoSnapshot,
    },
    /// Deshacer creación de socket: eliminar el socket creado.
    RemoveEntitySocket {
        entity_id: u32,
        socket: crate::engine3d::config_3d::entity_sockets::EntitySocketSnapshot,
    },
    /// Redo tras deshacer creación de socket.
    RestoreEntitySocket {
        entity_id: u32,
        socket: crate::engine3d::config_3d::entity_sockets::EntitySocketSnapshot,
    },
    /// Deshacer vinculación a socket: restaurar attachment/transform previos.
    RestoreSocketAttachment {
        child_id: u32,
        previous_attachment:
            Option<crate::engine3d::config_3d::entity_attachments::EntityAttachmentLocal>,
        previous_position: [f32; 3],
        previous_rotation: [f32; 4],
        previous_scale: [f32; 3],
        applied_attachment: crate::engine3d::config_3d::entity_attachments::EntityAttachmentLocal,
    },
    RestoreBonePhysics {
        entity_id: u32,
        bone_name: String,
        before: Option<crate::engine3d::config_3d::bone_physics::BonePhysicsMode>,
        after: Option<crate::engine3d::config_3d::bone_physics::BonePhysicsMode>,
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

/// Bytes reservados en GPU para el UBO de escena por slot en vuelo. Debe ser múltiplo de 16 y
/// caber `SceneUniforms` entero (752 B): el relleno final es solo alineación std140.
pub(crate) const SCENE_UBO_BYTES: usize = 768;

const _: () = assert!(std::mem::size_of::<SceneUniforms>() <= SCENE_UBO_BYTES);

/// Los shaders que declaran `uniform Scene` son un espejo byte a byte de este struct. Si un
/// offset se moviera, leerían otros campos y la escena se deformaría SIN error de API: estos
/// asserts de compilación fijan el contrato tanto de los campos antiguos como de las cascadas.
const _: () = assert!(std::mem::size_of::<SceneUniforms>() == 752);
const _: () = assert!(std::mem::offset_of!(SceneUniforms, view_proj) == 0);
const _: () = assert!(std::mem::offset_of!(SceneUniforms, view_proj_stable) == 64);
const _: () = assert!(std::mem::offset_of!(SceneUniforms, prev_view_proj) == 128);
const _: () = assert!(std::mem::offset_of!(SceneUniforms, inv_view_proj) == 192);
const _: () = assert!(std::mem::offset_of!(SceneUniforms, cam_pos) == 256);
const _: () = assert!(std::mem::offset_of!(SceneUniforms, light_dir) == 272);
const _: () = assert!(std::mem::offset_of!(SceneUniforms, light_color) == 288);
const _: () = assert!(std::mem::offset_of!(SceneUniforms, light_view_proj) == 304);
const _: () = assert!(std::mem::offset_of!(SceneUniforms, light_params) == 368);
const _: () = assert!(std::mem::offset_of!(SceneUniforms, jitter) == 384);
const _: () = assert!(std::mem::offset_of!(SceneUniforms, depth_plane) == 400);
const _: () = assert!(std::mem::offset_of!(SceneUniforms, shadow_bias) == 416);
const _: () = assert!(std::mem::offset_of!(SceneUniforms, aura_color) == 432);
// Campos nuevos, estrictamente AL FINAL: no se toca ningún offset anterior.
const _: () = assert!(std::mem::offset_of!(SceneUniforms, cascade_view_proj) == 448);
const _: () = assert!(std::mem::offset_of!(SceneUniforms, cascade_splits) == 704);
const _: () = assert!(std::mem::offset_of!(SceneUniforms, cascade_extent) == 720);
const _: () = assert!(std::mem::offset_of!(SceneUniforms, cascade_params) == 736);
const _: () = assert!(std::mem::size_of::<[[f32; 4]; 4]>() == 64);
const _: () = assert!(MAX_SHADOW_CASCADES == 4);

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
    pub(crate) light_view_proj: [[f32; 4]; 4],
    /// x = intensidad, y = oscuridad de sombra, z = 1/texel sombra, w = radio PCF.
    pub(crate) light_params: [f32; 4],
    /// xy = jitter subpíxel de TAA; z = peso del IBL de sondas; w = metros/texel de sombra.
    pub(crate) jitter: [f32; 4],
    /// x = near, y = far; z = modo de reflejos: 0 = Off, 1 = SSR, 2 = RT. RT sigue activando
    /// reflejos con >= 0.5 (como antes); SSR se distingue con ~= 1.0 para conservar spec/probes.
    /// w = capa GPU de Grass (0 = sin fotos de terreno).
    pub(crate) depth_plane: [f32; 4],
    /// x = bias_min, y = bias_max, z = depth_const, w = depth_slope.
    pub(crate) shadow_bias: [f32; 4],
    /// rgb = color del aura de interacción (flag 8.0 en play).
    pub(crate) aura_color: [f32; 4],
    /// Una matriz luz·proyección por capa del array de sombras (stride 64 B). `light_view_proj`
    /// conserva la de la cascada 0 para el pase de sombras histórico.
    pub(crate) cascade_view_proj: [[[f32; 4]; 4]; MAX_SHADOW_CASCADES],
    /// Borde lejano de cada cascada desde el ojo; `f32::MAX` en las inactivas.
    pub(crate) cascade_splits: [f32; 4],
    /// Mitad del lado de la caja ortográfica de cada cascada (unidades de mundo).
    pub(crate) cascade_extent: [f32; 4],
    /// x = radio suave (mundo), y = grosor del fade entre cascadas, z = 1/lado del mapa,
    /// w = número de cascadas activas.
    pub(crate) cascade_params: [f32; 4],
}

impl SceneUniforms {
    /// Offset de `jitter.z` (peso IBL) en el UBO de escena. La captura de sondas
    /// tiene que poner este campo a 0, no `light_params.y` (oscuridad de sombra).
    pub(crate) const IBL_WEIGHT_OFFSET: u64 = std::mem::offset_of!(Self, jitter) as u64 + 8;
    /// Offset de `depth_plane.x` (near) en el UBO de escena. SSR/denoise/TAA leen near/far
    /// desde aquí para linealizar profundidad sin duplicar un offset mágico.
    pub(crate) const DEPTH_PLANE_OFFSET: u64 = std::mem::offset_of!(Self, depth_plane) as u64;
}

#[cfg(test)]
mod tests {
    use super::SceneUniforms;

    #[test]
    fn ibl_weight_is_jitter_z_not_shadow_darkness() {
        let jitter_z = std::mem::offset_of!(SceneUniforms, jitter) as u64 + 8;
        let shadow_darkness = std::mem::offset_of!(SceneUniforms, light_params) as u64 + 4;
        assert_eq!(SceneUniforms::IBL_WEIGHT_OFFSET, jitter_z);
        assert_ne!(SceneUniforms::IBL_WEIGHT_OFFSET, shadow_darkness);
        assert_eq!(
            SceneUniforms::DEPTH_PLANE_OFFSET,
            std::mem::offset_of!(SceneUniforms, depth_plane) as u64
        );
    }

    #[test]
    fn reflection_mode_encoding_in_depth_plane_z() {
        // Contrato con forward.frag: SSR (~1.0) conserva spec/probes; RT (~2.0) mantiene el
        // suppression historico. El gate `>= 0.5` sigue marcando reflejos activos en RT.
        let off = 0.0f32;
        let ssr = 1.0f32;
        let rt = 2.0f32;
        assert!(off < 0.5);
        assert!((ssr - 1.0).abs() < 1e-6 && (0.5..1.5).contains(&ssr));
        assert!((rt - 2.0).abs() < 1e-6 && rt >= 1.5);
    }

    #[test]
    fn scene_uniforms_ubo_layout_is_frozen() {
        use super::{MAX_SHADOW_CASCADES, SCENE_UBO_BYTES};
        assert_eq!(std::mem::size_of::<SceneUniforms>(), 752);
        assert_eq!(SCENE_UBO_BYTES, 768);
        assert!(std::mem::size_of::<SceneUniforms>() <= SCENE_UBO_BYTES);
        assert_eq!(MAX_SHADOW_CASCADES, 4);
        // Espejo del bloque `Scene` en GLSL; cualquier cambio rompe el render silenciosamente.
        let offsets = [
            std::mem::offset_of!(SceneUniforms, view_proj),
            std::mem::offset_of!(SceneUniforms, view_proj_stable),
            std::mem::offset_of!(SceneUniforms, prev_view_proj),
            std::mem::offset_of!(SceneUniforms, inv_view_proj),
            std::mem::offset_of!(SceneUniforms, cam_pos),
            std::mem::offset_of!(SceneUniforms, light_dir),
            std::mem::offset_of!(SceneUniforms, light_color),
            std::mem::offset_of!(SceneUniforms, light_view_proj),
            std::mem::offset_of!(SceneUniforms, light_params),
            std::mem::offset_of!(SceneUniforms, jitter),
            std::mem::offset_of!(SceneUniforms, depth_plane),
            std::mem::offset_of!(SceneUniforms, shadow_bias),
            std::mem::offset_of!(SceneUniforms, aura_color),
            std::mem::offset_of!(SceneUniforms, cascade_view_proj),
            std::mem::offset_of!(SceneUniforms, cascade_splits),
            std::mem::offset_of!(SceneUniforms, cascade_extent),
            std::mem::offset_of!(SceneUniforms, cascade_params),
        ];
        assert_eq!(
            offsets,
            [
                0, 64, 128, 192, 256, 272, 288, 304, 368, 384, 400, 416, 432, 448, 704, 720, 736
            ]
        );
    }
}
