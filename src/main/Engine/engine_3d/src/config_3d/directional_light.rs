//! Luz direccional (sol) para iluminación PBR compatible con pipelines RTX.
//! La posición del sol en el mundo define la dirección de la luz.

use glam::{Mat4, Vec3};

use crate::ecs::{EntityId, MeshComponent, Transform};
use crate::engine::State;
use crate::entity_save_meta::EntitySaveMeta;
use crate::mesh;

/// Dirección por defecto (hacia la luz, mismo sentido que el shader histórico).
pub const DEFAULT_LIGHT_DIR: Vec3 = Vec3::new(0.6, 1.0, 0.4);
/// Color de la luz direccional (intensidad ~1; evita saturar el framebuffer sRGB).
pub const DEFAULT_LIGHT_COLOR: Vec3 = Vec3::new(1.0, 0.96, 0.88);
/// Factor de ambiente (0–1). Bajo = caras no iluminadas más oscuras.
pub const DEFAULT_LIGHT_AMBIENT: f32 = 0.06;
/// Multiplicador de color de luz direccional.
pub const DEFAULT_LIGHT_INTENSITY: f32 = 1.0;
/// Factor mínimo en zona de sombra (0–1). Bajo = sombras más oscuras.
pub const DEFAULT_SHADOW_DARKNESS: f32 = 0.22;
/// Desplazamiento mínimo hacia la luz al comparar sombras (metros).
pub const SHADOW_NORMAL_BIAS_MIN: f32 = 0.0;
/// Desplazamiento máximo en superficies casi paralelas a los rayos del sol.
pub const SHADOW_NORMAL_BIAS_MAX: f32 = 0.0004;
/// Offset constante en profundidad del comparador de sombras (NDC).
pub const SHADOW_DEPTH_BIAS_CONST: f32 = 0.0;
/// Escala de bias por pendiente (1 - N·L).
pub const SHADOW_DEPTH_BIAS_SLOPE: f32 = 0.00003;
/// Distancia típica del icono del sol respecto al origen.
pub const SUN_DISTANCE: f32 = 42.0;

impl State {
    pub(crate) fn default_sun_position(&self) -> Vec3 {
        let center = self.directional_light_scene_center();
        center + DEFAULT_LIGHT_DIR.normalize() * SUN_DISTANCE
    }

    /// Centro de la escena para calcular la dirección del sol (luz direccional).
    pub(crate) fn directional_light_scene_center(&self) -> Vec3 {
        let min = self.world_bounds_3d.min_corner();
        let max = self.world_bounds_3d.max_corner();
        (min + max) * 0.5
    }

    /// Actualiza la dirección de luz: del centro de la escena hacia el icono del sol.
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

    /// Misma posición que `ensure_default_sun` / plantilla FP al arrancar sin `.save`.
    pub(crate) fn align_editor_sun_to_default_position(&mut self) {
        let pos = self.default_sun_position();
        if let Some(id) = self.sun_entity {
            self.apply_sun_icon_visual(id);
            if let Some(t) = self.world.get_mut::<Transform>(id) {
                t.position = pos;
                t.scale = Vec3::splat(1.0);
            }
            self.sync_directional_light_from_sun();
            let label = self
                .entity_display_name(id)
                .unwrap_or_else(|| rer_engine_shared::editor_defaults::entity_label::SUN.to_string());
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
        let shadows_enabled = if self.shadow_tier == crate::config_3d::shadow_graphics::ShadowTier::Off {
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

    /// x = intensidad; z = 1/texel sombra; w = radio PCF. Oscuridad → lit-composite (CPU).
    pub(crate) fn scene_light_params(&self) -> [f32; 4] {
        let dist = self.camera.distance;
        // Radio PCF algo mayor con shadow map 1024 para compensar resolución.
        let pcf_radius = if dist < 12.0 {
            0.85
        } else if dist < 28.0 {
            0.95
        } else {
            1.15
        };
        [
            self.light_intensity,
            0.0,
            1.0 / self.shadow_map_size as f32,
            pcf_radius,
        ]
    }

    pub(crate) fn scene_shadow_bias(&self) -> [f32; 4] {
        [
            SHADOW_NORMAL_BIAS_MIN,
            SHADOW_NORMAL_BIAS_MAX,
            SHADOW_DEPTH_BIAS_CONST,
            SHADOW_DEPTH_BIAS_SLOPE,
        ]
    }

    /// Proyección ortográfica de la luz que cubre los bounds del mundo.
    pub(crate) fn build_light_view_proj(&self) -> [[f32; 4]; 4] {
        let center = self.directional_light_scene_center();
        let dir = self.directional_light_dir.normalize();
        let min = self.world_bounds_3d.min_corner();
        let max = self.world_bounds_3d.max_corner();
        let extent = (max - min).length().max(48.0) * 0.55;
        let up = if dir.y.abs() > 0.95 {
            Vec3::X
        } else {
            Vec3::Y
        };
        let eye = center + dir * extent * 2.5;
        let view = Mat4::look_at_rh(eye, center, up);
        let center_ls = view.transform_point3(center);
        let texel = (2.0 * extent) / self.shadow_map_size as f32;
        let cx = (center_ls.x / texel).floor() * texel + texel * 0.5;
        let cy = (center_ls.y / texel).floor() * texel + texel * 0.5;
        let proj = Mat4::orthographic_rh(
            cx - extent,
            cx + extent,
            cy - extent,
            cy + extent,
            0.5,
            extent * 8.0,
        );
        (proj * view).to_cols_array_2d()
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

    fn sun_icon_mesh_idx(&mut self) -> usize {
        if let Some(idx) = self.sun_icon_mesh_idx {
            return idx;
        }
        let idx = self.meshes.len();
        self.meshes.push(mesh::create_uv_sphere(&self.device, 20));
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
        let layer = self.texture_array.pack(&self.queue, &px, N, N);
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
            self.world.insert(
                id,
                MeshComponent {
                    mesh_idx,
                    tex_idx,
                },
            );
        }
    }

    fn reflection_probe_texture_idx(&mut self, _roughness: f32) -> usize {
        // F0 único para todas las sondas: la rugosidad solo vive en SurfacePbr, no en el albedo.
        if let Some(idx) = self.reflection_probe_tex_idx[0] {
            return idx;
        }
        const N: u32 = 4;
        const BASE: f32 = 0.55;
        let rr = (BASE * 255.0) as u8;
        let gg = (BASE * 0.97 * 255.0) as u8;
        let bb = (BASE * 0.92 * 255.0) as u8;
        let mut px: Vec<u8> = Vec::with_capacity((N * N * 4) as usize);
        for _ in 0..(N * N) {
            px.extend_from_slice(&[rr, gg, bb, 255]);
        }
        let tex_idx = self.tex_layers.len();
        let layer = self.texture_array.pack(&self.queue, &px, N, N);
        self.tex_layers.push(layer);
        self.reflection_probe_tex_idx[0] = Some(tex_idx);
        tex_idx
    }

    pub(crate) fn apply_reflection_probe_visual(&mut self, id: crate::ecs::EntityId, roughness: f32) {
        let mesh_idx = self.sun_icon_mesh_idx();
        let tex_idx = self.reflection_probe_texture_idx(roughness);
        if let Some(mc) = self.world.get_mut::<MeshComponent>(id) {
            mc.mesh_idx = mesh_idx;
            mc.tex_idx = tex_idx;
        } else {
            self.world.insert(
                id,
                MeshComponent {
                    mesh_idx,
                    tex_idx,
                },
            );
        }
    }

    /// Icono del sol seleccionable con gizmo; sin física. Idempotente al recargar `.save`.
    pub(crate) fn spawn_sun(&mut self, name: &str, position: [f32; 3], scale: [f32; 3]) {
        let label = self.resolve_entity_display_name(
            name,
            rer_engine_shared::editor_defaults::entity_label::SUN,
        );
        if let Some(id) = self.sun_entity {
            self.apply_sun_icon_visual(id);
            if let Some(t) = self.world.get_mut::<Transform>(id) {
                t.position = Vec3::from_array(position);
                let s = scale
                    .into_iter()
                    .fold(f32::INFINITY, f32::min)
                    .max(0.15);
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
            let s = scale
                .into_iter()
                .fold(f32::INFINITY, f32::min)
                .max(0.15);
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

    /// Esfera visual como el sol (malla UV + textura) sin luz direccional; física esférica.
    pub(crate) fn spawn_physics_ball(
        &mut self,
        name: &str,
        position: [f32; 3],
        scale: [f32; 3],
        physics_type: &str,
    ) -> EntityId {
        use crate::ipc::send_event;
        use crate::ipc::EngineEvent;

        let label = self.resolve_entity_display_name(
            name,
            rer_engine_shared::editor_defaults::entity_label::BALL,
        );
        let id = self.world.spawn(Some(&label));
        self.apply_sun_icon_visual(id);
        let radius = scale
            .into_iter()
            .fold(f32::INFINITY, f32::min)
            .max(0.15)
            * 0.5;
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.position = Vec3::from_array(position);
            t.scale = Vec3::splat(radius * 2.0);
        }
        self.physics.set_entity_sphere_physics(
            id,
            true,
            physics_type,
            position,
            radius,
        );
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
        log::info!("Pelota física «{label}» en [{:.1}, {:.1}, {:.1}]", position[0], position[1], position[2]);
        id
    }

    /// Esfera estática (misma malla que la pelota) para probar reflejos / roughness PBR.
    pub(crate) fn spawn_static_reflection_probe_sphere(
        &mut self,
        name: &str,
        position: [f32; 3],
        radius: f32,
        roughness: f32,
    ) -> EntityId {
        use crate::ecs::SurfacePbr;
        use crate::ipc::send_event;
        use crate::ipc::EngineEvent;

        let label = self.resolve_entity_display_name(
            name,
            rer_engine_shared::editor_defaults::entity_label::REFLECTION_PROBE,
        );
        let id = self.world.spawn(Some(&label));
        self.apply_reflection_probe_visual(id, roughness);
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.position = Vec3::from_array(position);
            t.scale = Vec3::splat(radius * 2.0);
        }
        self.world.insert(
            id,
            SurfacePbr::metal_probe(roughness),
        );
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
            scale: Some([radius * 2.0; 3]),
            rotation: Some([0.0, 0.0, 0.0, 1.0]),
            path: Some("[ReflectionProbe]".to_string()),
            kind: Some("model".to_string()),
            blueprint_id: None,
            physics_enabled: Some(false),
            physics_type: None,
            entity_category: Some("object".to_string()),
        });
        log::info!(
            "Sonda de reflejo «{label}» roughness={:.2} en [{:.1}, {:.1}, {:.1}]",
            roughness,
            position[0],
            position[1],
            position[2]
        );
        id
    }
}
