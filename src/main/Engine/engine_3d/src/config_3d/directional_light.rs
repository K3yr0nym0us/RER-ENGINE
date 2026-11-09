//! Luz direccional (sol) para iluminaciÃ³n PBR compatible con pipelines RTX.
//! La posiciÃ³n del sol en el mundo define la direcciÃ³n de la luz.

use glam::Vec3;

use crate::config_3d::shadow_cascades::{
    CascadeCamera, CascadeSet, MAX_SHADOW_CASCADES, cascade_set,
};

use crate::ecs::{EntityId, MeshComponent, Transform};
use crate::engine::State;
use crate::entity_save_meta::EntitySaveMeta;
use crate::mesh;

/// DirecciÃ³n por defecto (hacia la luz, mismo sentido que el shader histÃ³rico).
pub const DEFAULT_LIGHT_DIR: Vec3 = Vec3::new(0.6, 1.0, 0.4);
/// Color de la luz direccional (intensidad ~1; evita saturar el framebuffer sRGB).
pub const DEFAULT_LIGHT_COLOR: Vec3 = Vec3::new(1.0, 0.96, 0.88);
/// Factor de ambiente (0â€“1). Bajo = caras no iluminadas mÃ¡s oscuras.
pub const DEFAULT_LIGHT_AMBIENT: f32 = 0.06;
/// Multiplicador de color de luz direccional.
pub const DEFAULT_LIGHT_INTENSITY: f32 = 1.0;
/// Factor mÃ­nimo en zona de sombra (0â€“1). Bajo = sombras mÃ¡s oscuras.
pub const DEFAULT_SHADOW_DARKNESS: f32 = 0.22;
/// Desplazamiento mÃ­nimo hacia la luz al comparar sombras (metros).
pub const SHADOW_NORMAL_BIAS_MIN: f32 = 0.0;
/// Desplazamiento mÃ¡ximo en superficies casi paralelas a los rayos del sol.
pub const SHADOW_NORMAL_BIAS_MAX: f32 = 0.0004;
/// Offset constante en profundidad del comparador de sombras (NDC).
pub const SHADOW_DEPTH_BIAS_CONST: f32 = 0.0;
/// Escala de bias por pendiente (1 - NÂ·L).
pub const SHADOW_DEPTH_BIAS_SLOPE: f32 = 0.00003;
/// Distancia tÃ­pica del icono del sol respecto al origen.
pub const SUN_DISTANCE: f32 = 42.0;

impl State {
    pub(crate) fn default_sun_position(&self) -> Vec3 {
        let center = self.directional_light_scene_center();
        center + DEFAULT_LIGHT_DIR.normalize() * SUN_DISTANCE
    }

    /// Centro de la escena para calcular la direcciÃ³n del sol (luz direccional).
    pub(crate) fn directional_light_scene_center(&self) -> Vec3 {
        let min = self.world_bounds_3d.min_corner();
        let max = self.world_bounds_3d.max_corner();
        (min + max) * 0.5
    }

    /// Actualiza la direcciÃ³n de luz: del centro de la escena hacia el icono del sol.
    pub(crate) fn sync_directional_light_from_sun(&mut self) {
        let Some(sun_id) = self.sun_entity else {
            return;
        };
        let Some(t) = self.world.get::<Transform>(sun_id) else {
            return;
        };
        let to_sun = t.position - self.directional_light_scene_center();
        if to_sun.length_squared() > 1e-8 {
            self.directional_light_dir = to_sun.normalize();
        }
    }

    /// Misma posiciÃ³n que `ensure_default_sun` / plantilla FP al arrancar sin `.save`.
    pub(crate) fn align_editor_sun_to_default_position(&mut self) {
        let pos = self.default_sun_position();
        if let Some(id) = self.sun_entity {
            self.apply_sun_icon_visual(id);
            if let Some(t) = self.world.get_mut::<Transform>(id) {
                t.position = pos;
                t.scale = Vec3::splat(1.0);
            }
            self.sync_directional_light_from_sun();
            let label = self.entity_display_name(id).unwrap_or_else(|| {
                rer_engine_shared::editor_defaults::entity_label::SUN.to_string()
            });
            self.send_model_loaded_event(id, &label);
        } else {
            self.spawn_sun("", pos.to_array(), [1.0, 1.0, 1.0]);
        }
    }

    pub(crate) fn scene_light_dir(&self) -> [f32; 4] {
        [
            self.directional_light_dir.x,
            self.directional_light_dir.y,
            self.directional_light_dir.z,
            self.directional_light_ambient,
        ]
    }

    pub(crate) fn scene_light_color(&self) -> [f32; 4] {
        let shadows_enabled =
            if self.shadow_tier == crate::config_3d::shadow_graphics::ShadowTier::Off {
                0.0
            } else {
                1.0
            };
        [
            self.directional_light_color.x,
            self.directional_light_color.y,
            self.directional_light_color.z,
            shadows_enabled,
        ]
    }

    /// Lado real del shadow map según el tier, en px. Con `Off` no se renderiza ni se muestrea,
    /// pero la matriz de luz se sigue construyendo: se usa el tamaño base para que el snapping al
    /// texel no trabaje con un mapa de 1 px.
    pub(crate) fn active_shadow_map_size(&self) -> u32 {
        if self.shadow_tier == crate::config_3d::shadow_graphics::ShadowTier::Off {
            crate::engine::SHADOW_MAP_SIZE
        } else {
            self.shadow_map_size.max(crate::engine::SHADOW_MAP_SIZE / 2)
        }
    }

    /// x = intensidad; y = oscuridad de sombra (lo sobrescribe `render` según haya sondas activas).
    ///
    /// `z` y `w` se quedan a 0: desde CSM el texel y el radio PCF viajan en `shadow_params`, que es
    /// **por cascada**, así que un valor único del mapa ya no significa nada. Se mantienen los
    /// campos porque el bloque `Scene` de los shaders es un espejo byte a byte del struct.
    pub(crate) fn scene_light_params(&self) -> [f32; 4] {
        [self.light_intensity, self.shadow_darkness, 0.0, 0.0]
    }

    pub(crate) fn scene_shadow_bias(&self) -> [f32; 4] {
        [
            SHADOW_NORMAL_BIAS_MIN,
            SHADOW_NORMAL_BIAS_MAX,
            SHADOW_DEPTH_BIAS_CONST,
            SHADOW_DEPTH_BIAS_SLOPE,
        ]
    }

    /// Cascadas de sombra del frame: una matriz por capa del array, con sus cortes y extents.
    ///
    /// El trabajo duro está en `shadow_cascades::cascade_set` (matemática pura, testeada en host);
    /// aquí solo se aporta la base de cámara y el alcance del tier. La base se lee de la propia matriz
    /// de vista, que es la autoridad tanto en órbita de editor como en FPS: sus filas son los ejes de
    /// vista expresados en mundo, así que hacia delante es `-fila(2)`.
    pub(crate) fn build_shadow_cascades(&self, aspect: f32) -> CascadeSet {
        let count = self.shadow_tier.cascades().min(MAX_SHADOW_CASCADES);
        let view = self.camera_view_matrix().to_cols_array_2d();
        let row = |index: usize| {
            Vec3::new(view[0][index], view[1][index], view[2][index]).normalize_or_zero()
        };
        let (right, up, backward) = (row(0), row(1), row(2));
        let (forward, right, up) =
            if backward == Vec3::ZERO || right == Vec3::ZERO || up == Vec3::ZERO {
                // Vista degenerada (ojo justo sobre el blanco): cámara canónica hacia -Z.
                (Vec3::NEG_Z, Vec3::X, Vec3::Y)
            } else {
                (-backward, right, up)
            };

        let min = self.world_bounds_3d.min_corner();
        let max = self.world_bounds_3d.max_corner();
        // Radio de escena: con un mundo pequeño las cascadas no se estiran hasta `camera.far`
        // (lejos de ahí no hay nada que sombrear y se perdería toda la resolución).
        let scene_radius = ((max - min).length() * 0.5).max(24.0);
        let near = self.camera.near.max(0.05);
        let shadow_far = (scene_radius * self.shadow_tier.shadow_distance_scale())
            .clamp(near + 2.0, self.camera.far.max(near + 2.0));

        let camera = CascadeCamera {
            eye: self.camera_world_position(),
            forward,
            right,
            up,
            tan_half_fov_y: (self.camera.fov_y * 0.5).tan().max(1e-4),
            aspect: aspect.max(0.01),
            near,
            shadow_far,
        };
        // Con `Off` no se dibuja ni se muestrea ninguna cascada: aun así se calcula una de relleno
        // para que el array contenga matrices finitas, y luego se reporta el 0 del tier.
        let mut set = cascade_set(
            &camera,
            count.max(1),
            self.active_shadow_map_size(),
            self.directional_light_dir,
        );
        set.count = count;
        set
    }

    /// x = radio PCF en unidades de mundo, y = grosor del fade de costura, z = texel en UV
    /// (1 / lado del mapa, igual para todas las capas), w = cascadas activas.
    ///
    /// El `cascades` llega desde `ShadowCascadeSet::count`, no se vuelve a leer del tier: lo que ve
    /// el shader tiene que ser exactamente el número de cortes que se calcularon, si no elegiría una
    /// cascada con `split = FLT_MAX`.
    pub(crate) fn scene_shadow_params(&self, cascades: usize) -> [f32; 4] {
        [
            self.shadow_tier.shadow_soft_radius(),
            self.shadow_tier.cascade_fade(),
            1.0 / self.active_shadow_map_size() as f32,
            cascades.min(MAX_SHADOW_CASCADES) as f32,
        ]
    }

    pub(crate) fn apply_directional_light_settings(
        &mut self,
        ambient: Option<f32>,
        intensity: Option<f32>,
        shadow_darkness: Option<f32>,
    ) {
        if let Some(v) = ambient {
            self.directional_light_ambient = v.clamp(0.0, 1.0);
        }
        if let Some(v) = intensity {
            self.light_intensity = v.clamp(0.05, 4.0);
        }
        if let Some(v) = shadow_darkness {
            self.shadow_darkness = v.clamp(0.02, 1.0);
        }
    }

    pub(crate) fn sun_icon_mesh_idx(&mut self) -> usize {
        if let Some(idx) = self.sun_icon_mesh_idx {
            return idx;
        }
        let idx = self.meshes.len();
        self.meshes.push(mesh::create_uv_sphere(&self.vulkan, 20));
        self.sun_icon_mesh_idx = Some(idx);
        idx
    }

    fn sun_icon_texture_idx(&mut self) -> usize {
        if let Some(idx) = self.sun_icon_tex_idx {
            return idx;
        }
        const N: u32 = 32;
        let mut px: Vec<u8> = Vec::with_capacity((N * N * 4) as usize);
        for y in 0..N {
            for x in 0..N {
                let u = (x as f32 + 0.5) / N as f32 * 2.0 - 1.0;
                let v = (y as f32 + 0.5) / N as f32 * 2.0 - 1.0;
                let r = (u * u + v * v).sqrt().min(1.0);
                let t = (1.0 - r).powf(0.55);
                let r8 = (255.0 * (0.97 + 0.03 * t)) as u8;
                let g8 = (255.0 * (0.96 + 0.04 * t)) as u8;
                let b8 = (255.0 * (0.93 + 0.07 * t)) as u8;
                px.extend_from_slice(&[r8, g8, b8, 255]);
            }
        }
        let tex_idx = self.tex_layers.len();
        let layer = self.texture_array.pack(&self.vulkan, &px, N, N).unwrap();
        self.tex_layers.push(layer);
        self.sun_icon_tex_idx = Some(tex_idx);
        tex_idx
    }

    fn apply_sun_icon_visual(&mut self, id: crate::ecs::EntityId) {
        let mesh_idx = self.sun_icon_mesh_idx();
        let tex_idx = self.sun_icon_texture_idx();
        if let Some(mc) = self.world.get_mut::<MeshComponent>(id) {
            mc.mesh_idx = mesh_idx;
            mc.tex_idx = tex_idx;
        } else {
            self.world.insert(id, MeshComponent { mesh_idx, tex_idx });
        }
    }

    /// Icono del sol seleccionable con gizmo; sin fÃ­sica. Idempotente al recargar `.save`.
    pub(crate) fn spawn_sun(&mut self, name: &str, position: [f32; 3], scale: [f32; 3]) {
        let label = self.resolve_entity_display_name(
            name,
            rer_engine_shared::editor_defaults::entity_label::SUN,
        );
        if let Some(id) = self.sun_entity {
            self.apply_sun_icon_visual(id);
            if let Some(t) = self.world.get_mut::<Transform>(id) {
                t.position = Vec3::from_array(position);
                let s = scale.into_iter().fold(f32::INFINITY, f32::min).max(0.15);
                t.scale = Vec3::splat(s);
            }
            self.sync_directional_light_from_sun();
            self.send_model_loaded_event(id, &label);
            return;
        }

        let id = self.world.spawn(Some(&label));
        self.apply_sun_icon_visual(id);
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.position = Vec3::from_array(position);
            let s = scale.into_iter().fold(f32::INFINITY, f32::min).max(0.15);
            t.scale = Vec3::splat(s);
        }

        self.sun_entity = Some(id);
        self.sync_directional_light_from_sun();
        self.save_registry.register_meta(
            id,
            EntitySaveMeta {
                kind: "directional_light".to_string(),
                path: "[Sun]".to_string(),
                visual_model_path: None,
                entity_category: None,
            },
        );
        self.send_model_loaded_event(id, &label);
    }

    pub(crate) fn ensure_default_sun(&mut self) {
        if self.sun_entity.is_some() {
            return;
        }
        let pos = self.default_sun_position();
        self.spawn_sun("", pos.to_array(), [1.0, 1.0, 1.0]);
    }

    /// Esfera visual como el sol (malla UV + textura) sin luz direccional; fÃ­sica esfÃ©rica.
    pub(crate) fn spawn_physics_ball(
        &mut self,
        name: &str,
        position: [f32; 3],
        scale: [f32; 3],
        physics_type: &str,
    ) -> EntityId {
        use crate::ipc::EngineEvent;
        use crate::ipc::send_event;

        let label = self.resolve_entity_display_name(
            name,
            rer_engine_shared::editor_defaults::entity_label::BALL,
        );
        let id = self.world.spawn(Some(&label));
        self.apply_sun_icon_visual(id);
        let radius = scale.into_iter().fold(f32::INFINITY, f32::min).max(0.15) * 0.5;
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.position = Vec3::from_array(position);
            t.scale = Vec3::splat(radius * 2.0);
        }
        self.physics
            .set_entity_sphere_physics(id, true, physics_type, position, radius);
        self.entity_colision.insert(id, true);
        self.scenario_entities.push(id);
        self.save_registry.register_meta(
            id,
            EntitySaveMeta {
                kind: "model".to_string(),
                path: "[Ball]".to_string(),
                visual_model_path: None,
                entity_category: Some("object".to_string()),
            },
        );
        send_event(&EngineEvent::ModelLoaded {
            id,
            name: Some(label.clone()),
            position: Some(position),
            scale: Some([radius * 2.0; 3]),
            rotation: Some([0.0, 0.0, 0.0, 1.0]),
            path: Some("[Ball]".to_string()),
            kind: Some("model".to_string()),
            blueprint_id: None,
            physics_enabled: Some(true),
            physics_type: Some(physics_type.to_string()),
            entity_category: Some("object".to_string()),
        });
        log::info!(
            "Pelota fÃ­sica Â«{label}Â» en [{:.1}, {:.1}, {:.1}]",
            position[0],
            position[1],
            position[2]
        );
        id
    }

    /// Entidad lÃ³gica de reflection probe: sin malla en escena (solo gizmo en editor/debug).
    pub(crate) fn spawn_reflection_probe_entity(
        &mut self,
        name: &str,
        position: [f32; 3],
        influence_radius_m: f32,
    ) -> EntityId {
        use crate::ipc::EngineEvent;
        use crate::ipc::send_event;

        let label = self.resolve_entity_display_name(
            name,
            rer_engine_shared::editor_defaults::entity_label::REFLECTION_PROBE,
        );
        let id = self.world.spawn(Some(&label));
        let influence = influence_radius_m.max(0.15);
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.position = Vec3::from_array(position);
            // `reflection_probe_world_radius` usa scale.x * 0.5 como radio de influencia.
            t.scale = Vec3::splat(influence * 2.0);
        }
        self.entity_colision.insert(id, false);
        self.scenario_entities.push(id);
        self.save_registry.register_meta(
            id,
            EntitySaveMeta {
                kind: "model".to_string(),
                path: "[ReflectionProbe]".to_string(),
                visual_model_path: None,
                entity_category: Some("object".to_string()),
            },
        );
        self.allocate_probe_slot(id);
        send_event(&EngineEvent::ModelLoaded {
            id,
            name: Some(label.clone()),
            position: Some(position),
            scale: Some([influence * 2.0; 3]),
            rotation: Some([0.0, 0.0, 0.0, 1.0]),
            path: Some("[ReflectionProbe]".to_string()),
            kind: Some("model".to_string()),
            blueprint_id: None,
            physics_enabled: Some(false),
            physics_type: None,
            entity_category: Some("object".to_string()),
        });
        log::info!(
            "Sonda de reflejo Â«{label}Â» radio={influence:.1}m en [{:.1}, {:.1}, {:.1}]",
            position[0],
            position[1],
            position[2]
        );
        id
    }
}
