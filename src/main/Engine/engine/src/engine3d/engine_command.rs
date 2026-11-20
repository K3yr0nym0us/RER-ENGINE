//! Ensamblaje del contrato IPC 3D: comandos comunes + extensión exclusiva.

use serde::Deserialize;

pub use rer_engine_core::ipc::EngineCommandCommon;

use crate::engine3d::ipc::{BlueprintPlacementMeta, PlayCameraFollowMode};

fn default_unit_quat() -> [f32; 4] {
    [0.0, 0.0, 0.0, 1.0]
}

fn default_static_physics_type() -> String {
    "static".to_string()
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum EngineCommand {
    Common(EngineCommandCommon),
    Only3d(EngineCommand3dOnly),
}

/// Comandos solo en `rer_engine_3d` (rechazados en main si projectType es 2D).
#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum EngineCommand3dOnly {
    SpawnCachedModel {
        path: String,
        #[serde(default)]
        name: Option<String>,
        position: [f32; 3],
        #[serde(default = "default_unit_quat")]
        rotation: [f32; 4],
        scale: [f32; 3],
        #[serde(default)]
        entity_category: Option<String>,
        #[serde(default)]
        blueprint_id: Option<String>,
        #[serde(default)]
        physics_enabled: bool,
        #[serde(default = "default_static_physics_type")]
        physics_type: String,
    },
    SpawnQuickBuildInstance {
        position: [f32; 3],
        rotation: [f32; 4],
        scale: [f32; 3],
    },
    PlaceQuickBuildAtCursor {
        #[serde(default)]
        pixel_x: Option<f32>,
        #[serde(default)]
        pixel_y: Option<f32>,
    },
    RegisterBlueprint {
        blueprint: BlueprintPlacementMeta,
    },
    LoadModelAsset {
        path: String,
        name: String,
        #[serde(default)]
        category: Option<String>,
    },
    RemoveModelAsset {
        path: String,
    },
    GetModelsList,
    /// Convierte un modelo de la escena actual en recurso global: deja de estar
    /// atado a su escena y se mantiene disponible en todas.
    GlobalizeModel {
        path: String,
    },
    /// «Limpiar escena»: elimina el contenido por defecto conservando jugador y cámaras.
    ClearSceneContent,
    /// Caras del mundo (m): norte (-Z), sur (+Z), este (+X), oeste (-X).
    SetWorldBounds {
        north: f32,
        south: f32,
        east: f32,
        west: f32,
        #[serde(default)]
        height: Option<f32>,
    },
    SpawnEditorBox {
        name: String,
        position: [f32; 3],
        scale: [f32; 3],
    },
    SpawnSun {
        name: String,
        position: [f32; 3],
        scale: [f32; 3],
    },
    SpawnGround {
        position: [f32; 3],
        scale: [f32; 3],
    },
    /// Plugin worldgen: ruta de `rer_worldgen_plugin.dll` instalado por el editor.
    WorldGenSetLibraryPath {
        path: String,
    },
    /// Plugin worldgen: configuración del mundo (JSON camelCase de `WorldGenConfig`).
    WorldGenSetConfig {
        config_json: String,
    },
    /// Plugin worldgen: planifica y encola la generación de todos los chunks activos.
    /// `replace_world` quita el suelo checker al terminar, no al empezar.
    WorldGenStart {
        #[serde(default)]
        replace_world: bool,
    },
    /// Plugin worldgen: cancela la cola de chunks pendientes.
    WorldGenStop,
    /// Plugin worldgen: elimina los chunks en escena y libera el handle de la DLL.
    WorldGenReset,
    /// Plugin worldgen: suelta escena, handle y DLL (desactivar / desinstalar).
    WorldGenUnload,
    /// Plugin worldgen: distribuye un blueprint según reglas (JSON camelCase de
    /// `ScatterRules` con `blueprintId`).
    WorldGenScatter {
        rules_json: String,
    },
    SetDirectionalLight {
        #[serde(default)]
        ambient: Option<f32>,
        #[serde(default)]
        intensity: Option<f32>,
        #[serde(default)]
        shadow_darkness: Option<f32>,
        /// Ciclo día/noche activado.
        #[serde(default)]
        sun_cycle_enabled: Option<bool>,
        /// Cardinal de salida del sol: east / north / west / south.
        #[serde(default)]
        sun_rise_cardinal: Option<String>,
        /// Duración del día en unidades UI (1000 = 1 hora).
        #[serde(default)]
        sun_cycle_day: Option<f32>,
        /// Duración de la noche en unidades UI (1000 = 1 hora).
        #[serde(default)]
        sun_cycle_night: Option<f32>,
        /// Fase manual del ciclo [0,1).
        #[serde(default)]
        sun_cycle_phase: Option<f32>,
    },
    #[serde(rename = "set_play_character_spawn")]
    SetPlayCharacterSpawn {
        position: [f32; 3],
        yaw: f32,
        pitch: f32,
    },
    #[serde(rename = "set_play_character_view")]
    SetPlayCharacterView {
        #[serde(default)]
        position: Option<[f32; 3]>,
        #[serde(default)]
        position_axis: Option<rer_engine_core::ipc::AxisValue>,
        #[serde(default)]
        yaw: Option<f32>,
        #[serde(default)]
        pitch: Option<f32>,
        #[serde(default)]
        fov_y: Option<f32>,
        #[serde(default)]
        frustum_distance: Option<f32>,
        /// Radio de exclusión del ojo en primera persona (m).
        #[serde(default)]
        camera_clip_radius: Option<f32>,
        #[serde(default)]
        camera_only: Option<bool>,
        #[serde(default)]
        camera_follow_mode: Option<PlayCameraFollowMode>,
        #[serde(default)]
        body_rotation: Option<[f32; 4]>,
        #[serde(default)]
        body_scale: Option<[f32; 3]>,
        #[serde(default)]
        camera_eye_position: Option<[f32; 3]>,
        #[serde(default)]
        fps_camera_yaw: Option<f32>,
        #[serde(default)]
        fps_camera_pitch: Option<f32>,
    },
    SetGraphicsTextureTier {
        tier: String,
    },
    SetTextureDetailDistance {
        distance_m: f32,
    },
    SetReflectionTier {
        tier: String,
    },
    /// Sistema activo: off | ssr | rt (mutuamente excluyentes).
    SetReflectionMode {
        mode: String,
    },
    /// Tier de RTX con sus parámetros reales.
    SetRtTier {
        tier: String,
    },
    SetReflectionRaytracing {
        enabled: bool,
    },
    SetReflectionProbes {
        enabled: bool,
    },
    SpawnReflectionProbe {
        #[serde(default)]
        position: Option<[f32; 3]>,
    },
    SetReflectionDebugView {
        view: String,
    },
    SetSsrDebugMode {
        enabled: bool,
    },
    SetShadowTier {
        tier: String,
    },
    SetMsaaTier {
        tier: String,
    },
    SetSsaaTier {
        tier: String,
    },
    SetWorldRadius {
        radius: f32,
    },
    SetTaa {
        enabled: bool,
        #[serde(default)]
        blend: Option<f32>,
        #[serde(default)]
        jitter_scale: Option<f32>,
    },
    SetModelLod {
        enabled: bool,
        #[serde(default)]
        lod1_distance_m: Option<f32>,
        #[serde(default)]
        lod2_distance_m: Option<f32>,
    },
    SetCameraFov {
        fov_y: f32,
    },
    #[serde(rename = "set_play_editor_frustum_distance")]
    SetPlayEditorFrustumDistance {
        distance: f32,
    },
    SetEntityColision {
        id: u32,
        colision: bool,
    },
    LoadCharacter {
        path: String,
        #[serde(default)]
        track_undo: Option<bool>,
    },
    CreateEditorScene {
        name: String,
    },
    SwitchEditorScene {
        scene_id: u32,
    },
    DeleteEditorScene {
        scene_id: u32,
    },
    NotifyProjectSaved {
        extract_dir: String,
    },
    ClearEditorUndoRedo,
    MergeEntities {
        ids: Vec<u32>,
    },
    ListEntityBones {
        entity_id: u32,
    },
    ListEntitySockets {
        entity_id: u32,
    },
    UpsertEntitySocket {
        entity_id: u32,
        name: String,
        bone_name: String,
        #[serde(default)]
        local_position: [f32; 3],
        #[serde(default = "default_unit_quat")]
        local_rotation: [f32; 4],
    },
    RemoveEntitySocket {
        entity_id: u32,
        name: String,
    },
    AttachToSocket {
        child_ids: Vec<u32>,
        host_id: u32,
        socket_name: String,
    },
    DetachFromSocket {
        child_id: u32,
    },
    SetSocketBonePickMode {
        entity_id: u32,
        active: bool,
    },
    SetBonePhysicsEditorEntity {
        entity_id: u32,
        active: bool,
    },
    SetBonePhysicsPickMode {
        entity_id: u32,
        active: bool,
    },
    SetBonePhysics {
        entity_id: u32,
        bone_name: String,
        mode: String,
    },
    RemoveBonePhysics {
        entity_id: u32,
        bone_name: String,
    },
    ListEntityBonePhysics {
        entity_id: u32,
    },
    SetProjectileConfig {
        id: u32,
        speed: f32,
        lifetime_s: f32,
        #[serde(default)]
        affected_by_gravity: Option<bool>,
        #[serde(default)]
        gravity_scale: Option<f32>,
        #[serde(default)]
        align_to_velocity: Option<bool>,
        #[serde(default)]
        muzzle_socket: Option<String>,
        #[serde(default)]
        bounceable: Option<bool>,
        #[serde(default)]
        max_bounces: Option<u32>,
        #[serde(default)]
        bounce_speed_loss: Option<f32>,
        #[serde(default)]
        hardness: Option<u32>,
    },
    FireProjectile {
        template_id: u32,
        #[serde(default)]
        from_id: Option<u32>,
        dir_x: f32,
        dir_y: f32,
        dir_z: f32,
    },
    SetEntityHardness {
        id: u32,
        hardness: u32,
    },
    /// Configura el prompt/tooltip de interacción de una entidad.
    SetInteractable {
        id: u32,
        #[serde(default)]
        enabled: Option<bool>,
        #[serde(default)]
        key: Option<String>,
        #[serde(default)]
        once: Option<bool>,
        #[serde(default)]
        max_distance: Option<f32>,
        #[serde(default)]
        tooltip_id: Option<String>,
        #[serde(default)]
        tooltip_distance: Option<f32>,
        #[serde(default)]
        trigger: Option<String>,
        #[serde(default)]
        action: Option<String>,
        #[serde(default)]
        action_name: Option<String>,
        #[serde(default)]
        highlight: Option<bool>,
        #[serde(default)]
        highlight_color: Option<[u8; 3]>,
    },
    /// Registro compartido de tooltips: cada entidad resuelve el suyo por id.
    SyncInteractionTooltips {
        tooltips: Vec<serde_json::Value>,
    },
    /// Partículas GPU-driven: alta de un emisor con preset del material + overrides.
    AddParticleEmitter {
        /// `fire | smoke | fog | rain | snow | sand | water | water_cube`.
        #[serde(default = "default_particle_material")]
        material: String,
        #[serde(default)]
        position: [f32; 3],
        #[serde(default)]
        radius: Option<f32>,
        #[serde(default)]
        rate: Option<f32>,
        #[serde(default)]
        max_particles: Option<u32>,
        #[serde(default)]
        velocity: Option<[f32; 3]>,
        #[serde(default)]
        size: Option<f32>,
        #[serde(default)]
        life: Option<f32>,
        #[serde(default)]
        gravity: Option<f32>,
        #[serde(default)]
        drag: Option<f32>,
        #[serde(default)]
        turbulence: Option<f32>,
        #[serde(default)]
        ground_y: Option<f32>,
        /// 0 = billboard a cámara, 1 = orientado a velocidad.
        #[serde(default)]
        billboard: Option<u32>,
        /// F4: cohesión de flujo (0–1).
        #[serde(default)]
        cohesion: Option<f32>,
        /// F4: viento horizontal (m/s).
        #[serde(default)]
        wind: Option<[f32; 2]>,
        /// F4: flags granulares.
        #[serde(default)]
        ground_collision: Option<bool>,
        #[serde(default)]
        accumulate: Option<bool>,
        #[serde(default)]
        splash: Option<bool>,
        /// Color RGB base de los líquidos (solo `water`); el motor conserva el
        /// gradiente alfa del preset.
        #[serde(default)]
        color: Option<[f32; 3]>,
        /// F5: id asignado por la UI (ids altos) para poder borrar el emisor después.
        #[serde(default)]
        id: Option<u64>,
    },
    RemoveParticleEmitter {
        id: u64,
    },
    ClearParticleEmitters,
    /// Cubo de líquidos: añade una fuente que se expande por
    /// la rejilla de agua. `cell_size` en metros (0.5 por defecto en la UI).
    AddWaterSource {
        #[serde(default)]
        position: [f32; 3],
        #[serde(default = "default_water_cell_size")]
        cell_size: f32,
        #[serde(default = "default_water_color")]
        color: [f32; 3],
        #[serde(default = "default_water_viscosity")]
        viscosity: f32,
        #[serde(default = "default_water_cohesion")]
        cohesion: f32,
        #[serde(default)]
        id: Option<u64>,
    },
    RemoveWaterSource {
        id: u64,
    },
    /// Clima global (F2/F6-bis): lluvia, nieve o niebla con seguimiento de cámara.
    /// "none" lo retira. `fall_speed` (multiplicador 0.25–3) y `wind_x` (m/s,
    /// negativo = hacia la izquierda) solo afectan a lluvia/nieve.
    SetParticleEnvironment {
        kind: String,
        #[serde(default)]
        intensity: Option<f32>,
        #[serde(default)]
        fall_speed: Option<f32>,
        #[serde(default)]
        wind_x: Option<f32>,
    },
}

fn default_particle_material() -> String {
    "fire".to_string()
}

fn default_water_cell_size() -> f32 {
    crate::engine3d::water::DEFAULT_CELL_SIZE
}

fn default_water_color() -> [f32; 3] {
    crate::engine3d::water::DEFAULT_LIQUID_COLOR
}

fn default_water_viscosity() -> f32 {
    crate::engine3d::water::DEFAULT_VISCOSITY
}

fn default_water_cohesion() -> f32 {
    crate::engine3d::water::DEFAULT_COHESION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_set_reflection_tier_command() {
        let json = r#"{"cmd":"set_reflection_tier","tier":"medium"}"#;
        let cmd: EngineCommand = serde_json::from_str(json).expect("set_reflection_tier IPC");
        match cmd {
            EngineCommand::Only3d(EngineCommand3dOnly::SetReflectionTier { tier }) => {
                assert_eq!(tier, "medium");
            }
            other => panic!("expected Only3d SetReflectionTier, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_set_taa_command() {
        let json = r#"{"cmd":"set_taa","enabled":true}"#;
        let cmd: EngineCommand = serde_json::from_str(json).expect("set_taa IPC");
        match cmd {
            EngineCommand::Only3d(EngineCommand3dOnly::SetTaa { enabled, .. }) => {
                assert!(enabled);
            }
            other => panic!("expected Only3d SetTaa, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_set_taa_command_with_params() {
        let json = r#"{"cmd":"set_taa","enabled":true,"blend":0.7,"jitter_scale":0.8}"#;
        let cmd: EngineCommand = serde_json::from_str(json).expect("set_taa IPC with params");
        match cmd {
            EngineCommand::Only3d(EngineCommand3dOnly::SetTaa {
                enabled,
                blend,
                jitter_scale,
            }) => {
                assert!(enabled);
                assert_eq!(blend, Some(0.7));
                assert_eq!(jitter_scale, Some(0.8));
            }
            other => panic!("expected Only3d SetTaa, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_set_model_lod_command() {
        let json = r#"{"cmd":"set_model_lod","enabled":true,"lod1_distance_m":35.0,"lod2_distance_m":90.0}"#;
        let cmd: EngineCommand = serde_json::from_str(json).expect("set_model_lod IPC");
        match cmd {
            EngineCommand::Only3d(EngineCommand3dOnly::SetModelLod {
                enabled,
                lod1_distance_m,
                lod2_distance_m,
            }) => {
                assert!(enabled);
                assert_eq!(lod1_distance_m, Some(35.0));
                assert_eq!(lod2_distance_m, Some(90.0));
            }
            other => panic!("expected Only3d SetModelLod, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_set_reflection_debug_view_command() {
        let json = r#"{"cmd":"set_reflection_debug_view","view":"ssr_debug"}"#;
        let cmd: EngineCommand = serde_json::from_str(json).expect("set_reflection_debug_view IPC");
        match cmd {
            EngineCommand::Only3d(EngineCommand3dOnly::SetReflectionDebugView { view }) => {
                assert_eq!(view, "ssr_debug");
            }
            other => panic!("expected Only3d SetReflectionDebugView, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_add_particle_emitter_water_cube() {
        let json =
            r#"{"cmd":"add_particle_emitter","material":"water_cube","position":[1.0,1.6,0.0]}"#;
        let cmd: EngineCommand = serde_json::from_str(json).expect("add_particle_emitter IPC");
        match cmd {
            EngineCommand::Only3d(EngineCommand3dOnly::AddParticleEmitter {
                material,
                position,
                ..
            }) => {
                assert_eq!(material, "water_cube");
                assert_eq!(position, [1.0, 1.6, 0.0]);
            }
            other => panic!("expected Only3d AddParticleEmitter, got {other:?}"),
        }
    }
}
