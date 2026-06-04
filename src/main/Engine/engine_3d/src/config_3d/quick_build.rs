// ── Construcción rápida 3D (ghost + raycast + colocación) ────────────────────

use glam::{Vec3, Vec4};

use crate::config_3d::{is_fbx_model_path, is_gltf_model_path};
use crate::config_compat::ActiveTool;
use crate::ecs::{EntityId, MeshComponent, NonSelectable, Transform};
use crate::engine::State;
use crate::entity_save_meta::EntitySaveMeta;
use crate::ipc::{
    send_event, BlueprintPlacementMeta, EngineEvent, SaveAnimationSnapshot, SaveScriptSnapshot,
};
use crate::engine::entity_restore::{
    apply_entity_animations_snapshots, apply_entity_scripts_snapshots,
};
use crate::mesh;
use rer_engine_shared::editor_defaults::entity_label_for_category;

const GHOST_OFFSCREEN: f32 = -99999.0;
const GHOST_ALPHA: f32 = 0.38;
const PLACEMENT_RAY_MAX: f32 = 10_000.0;
const DEFAULT_ROTATION: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// Metadatos del blueprint activo en construcción rápida (manifest `Blueprints` + instancia).
#[derive(Clone, Debug)]
pub(crate) struct QuickBuildBlueprint {
    pub template_name: String,
    pub model: String,
    pub rotation: [f32; 4],
    pub physics_enabled: bool,
    pub physics_type: String,
    pub colision: bool,
    /// `Entity3DCategory` del manifest (`environment`, `object`, `character`, …).
    pub category: Option<String>,
    pub entity_category: Option<String>,
    pub blueprint_id: Option<String>,
    pub scripts: Option<Vec<SaveScriptSnapshot>>,
    pub animations: Option<Vec<SaveAnimationSnapshot>>,
}

impl QuickBuildBlueprint {
    pub(crate) fn from_placement_meta(meta: &BlueprintPlacementMeta, preview_path: &str) -> Self {
        let template_name = meta
            .template_name
            .clone()
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| "Blueprint".to_string());
        let mut bp = Self {
            template_name: template_name.clone(),
            model: meta
                .model
                .clone()
                .filter(|m| !m.trim().is_empty())
                .unwrap_or_else(|| preview_path.to_string()),
            rotation: meta.rotation.unwrap_or(DEFAULT_ROTATION),
            physics_enabled: meta.physics_enabled.unwrap_or(false),
            physics_type: meta
                .physics_type
                .clone()
                .unwrap_or_else(|| "static".to_string()),
            colision: meta.colision.unwrap_or(true),
            category: meta.category.clone(),
            entity_category: meta
                .category
                .as_deref()
                .and_then(|c| crate::ipc::normalize_placement_entity_category(Some(c))),
            blueprint_id: meta.blueprint_id.clone(),
            scripts: meta.scripts.clone(),
            animations: meta.animations.clone(),
        };
        if bp.entity_category.is_none() {
            bp.entity_category = infer_category_from_template(&template_name, meta.category.as_deref());
        }
        let is_environment = bp.entity_category.as_deref() == Some("environment");
        if is_environment {
            bp.physics_enabled = true;
            bp.physics_type = "static".to_string();
            bp.colision = true;
        }
        bp
    }
}

fn infer_category_from_template(
    template_name: &str,
    category: Option<&str>,
) -> Option<String> {
    category
        .and_then(|c| crate::ipc::normalize_placement_entity_category(Some(c)))
        .or_else(|| {
            rer_engine_shared::editor_defaults::infer_entity_category_from_numbered_name(template_name)
                .and_then(|c| crate::ipc::normalize_placement_entity_category(Some(c)))
        })
}

fn is_model_file_path(path: &str) -> bool {
    is_gltf_model_path(path) || is_fbx_model_path(path)
}

/// Categoría IPC para nombrado/físicas (manifest `environment` | `object` | `character`).
fn resolve_quick_build_entity_category(bp: &QuickBuildBlueprint) -> Option<String> {
    bp.entity_category.clone().or_else(|| {
        infer_category_from_template(&bp.template_name, bp.category.as_deref())
    })
}

fn apply_blueprint_instance_metadata(state: &mut State, id: EntityId, bp: &QuickBuildBlueprint) {
    if let Some(cat) = resolve_quick_build_entity_category(bp) {
        if let Some(meta) = state.save_registry.meta.get_mut(&id) {
            meta.entity_category = Some(cat.clone());
        } else {
            state.save_registry.register_meta(
                id,
                EntitySaveMeta {
                    kind: "model".to_string(),
                    path: state.model_path_key(&bp.model),
                    visual_model_path: None,
                    entity_category: Some(cat),
                },
            );
        }
    }
    state.entity_colision.insert(id, bp.colision);
    apply_entity_scripts_snapshots(state, id, bp.scripts.as_deref());
    apply_entity_animations_snapshots(state, id, bp.animations.as_deref());
    if bp.colision || bp.physics_enabled {
        state.reconcile_entity_physics_with_mesh(id);
    }
}

fn ray_intersect_y_plane(origin: Vec3, dir: Vec3, y: f32) -> Option<Vec3> {
    if dir.y.abs() < 1e-8 {
        return None;
    }
    let t = (y - origin.y) / dir.y;
    if t < 0.0 {
        return None;
    }
    Some(origin + dir * t)
}

/// Alinea el borde del AABB del objeto a la línea de cuadrícula más cercana (eje X o Z).
fn snap_axis_edges_to_grid(center: f32, half_extent: f32, cell: f32) -> f32 {
    if cell <= 1e-6 {
        return center;
    }
    let left = center - half_extent;
    let right = center + half_extent;
    let left_snap = (left / cell).round() * cell;
    let right_snap = (right / cell).round() * cell;
    if (left - left_snap).abs() <= (right - right_snap).abs() {
        left_snap + half_extent
    } else {
        right_snap - half_extent
    }
}

fn raycast_ground_point(origin: Vec3, dir: Vec3, ground_y: f32) -> Vec3 {
    if let Some(p) = ray_intersect_y_plane(origin, dir, ground_y) {
        return p;
    }
    let xz = glam::Vec2::new(dir.x, dir.z);
    if xz.length_squared() > 1e-8 {
        let xz_dir = Vec3::new(xz.x, 0.0, xz.y).normalize();
        let dist = 8.0_f32;
        return Vec3::new(
            origin.x + xz_dir.x * dist,
            ground_y,
            origin.z + xz_dir.z * dist,
        );
    }
    Vec3::new(origin.x, ground_y, origin.z)
}

impl State {
    pub(crate) fn viewport_ray(&self, pixel_x: f32, pixel_y: f32) -> Option<(Vec3, Vec3)> {
        let w = self.size.width as f32;
        let h = self.size.height as f32;
        if w <= 0.0 || h <= 0.0 {
            return None;
        }
        let aspect = w / h;

        let ndc_x = (2.0 * pixel_x / w) - 1.0;
        let ndc_y = -(2.0 * pixel_y / h) + 1.0;

        let inv_proj = self.camera.proj_matrix(aspect).inverse();
        let inv_view = self.camera_view_matrix().inverse();

        let clip_dir = Vec4::new(ndc_x, ndc_y, -1.0, 0.0);
        let view_dir = inv_proj * clip_dir;
        let view_dir = Vec4::new(view_dir.x, view_dir.y, -1.0, 0.0);
        let world_dir = (inv_view * view_dir).truncate().normalize();
        let ray_origin = self.camera_world_position();
        Some((ray_origin, world_dir))
    }

    pub(crate) fn raycast_placement_point(&mut self, pixel_x: f32, pixel_y: f32) -> Option<[f32; 3]> {
        let (origin, dir) = self.viewport_ray(pixel_x, pixel_y)?;

        if let Some(dist) = self
            .physics
            .raycast_first_hit_distance(origin, dir, PLACEMENT_RAY_MAX, None)
        {
            let hit = origin + dir * dist;
            return Some(hit.to_array());
        }

        Some(raycast_ground_point(origin, dir, 0.0).to_array())
    }

    fn quick_build_snap_position_3d(&self, pos: [f32; 3]) -> [f32; 3] {
        if !self.ctrl_held {
            return pos;
        }
        let cell = self.grid_config.cell_size.max(0.05);
        let scale = self
            .quick_build_effective_scale_3d()
            .unwrap_or([1.0, 1.0, 1.0]);
        let half_x = scale[0].abs() * 0.5;
        let half_z = scale[2].abs() * 0.5;
        [
            snap_axis_edges_to_grid(pos[0], half_x, cell),
            pos[1],
            snap_axis_edges_to_grid(pos[2], half_z, cell),
        ]
    }

    fn quick_build_effective_scale_3d(&self) -> Option<[f32; 3]> {
        self.quick_build_preview_scale
    }

    pub(crate) fn load_quick_build_ghost_3d(
        &mut self,
        path: &str,
        scale: [f32; 3],
    ) -> Option<EntityId> {
        if !is_model_file_path(path) {
            log::warn!("[quick_build] ruta no es modelo 3D: {path}");
            return None;
        }

        if self.ensure_static_model_cached(path).is_err() {
            log::warn!("[quick_build] no se pudo precargar ghost desde {path}");
            return None;
        }
        let part = *self.cached_static_model_parts(path)?.first()?;

        let ghost_id = self.world.spawn(Some("__qb_ghost__"));
        self.world.insert(
            ghost_id,
            MeshComponent {
                mesh_idx: part.mesh_idx,
                tex_idx: part.tex_idx,
            },
        );
        self.world.insert(
            ghost_id,
            Transform {
                position: Vec3::new(GHOST_OFFSCREEN, GHOST_OFFSCREEN, GHOST_OFFSCREEN),
                scale: Vec3::new(scale[0], scale[1], scale[2]),
                ..Default::default()
            },
        );
        self.world.insert(ghost_id, NonSelectable);
        log::info!("[quick_build] ghost 3D creado id={ghost_id} path={path}");
        Some(ghost_id)
    }

    pub(crate) fn update_tool_overlay_cursor_3d(&mut self, pixel_x: f32, pixel_y: f32) {
        if !matches!(self.active_tool, ActiveTool::QuickBuildPlace { .. }) {
            return;
        }

        let Some(raw) = self.raycast_placement_point(pixel_x, pixel_y) else {
            return;
        };
        let target = self.quick_build_snap_position_3d(raw);
        let scale = self.quick_build_effective_scale_3d();

        if let ActiveTool::QuickBuildPlace { cursor_world } = &mut self.active_tool {
            *cursor_world = Some(target);
        }

        if let Some(ghost_id) = self.quick_build_ghost_id {
            if let Some(t) = self.world.get_mut::<Transform>(ghost_id) {
                if let Some(scale) = scale {
                    t.scale = Vec3::new(scale[0], scale[1], scale[2]);
                }
                t.position = Vec3::new(target[0], target[1], target[2]);
            }
        }
    }

    pub(crate) fn handle_tool_click_3d(&mut self, pixel_x: f32, pixel_y: f32) -> bool {
        if !matches!(self.active_tool, ActiveTool::QuickBuildPlace { .. }) {
            return false;
        }

        let fit_to_grid = self.ctrl_held;
        let cursor_world = match &self.active_tool {
            ActiveTool::QuickBuildPlace { cursor_world } => *cursor_world,
            _ => None,
        };

        let position = if fit_to_grid {
            self.raycast_placement_point(pixel_x, pixel_y)
                .map(|p| self.quick_build_snap_position_3d(p))
                .or(cursor_world)
        } else {
            cursor_world.or_else(|| self.raycast_placement_point(pixel_x, pixel_y))
        };

        let Some([cx, cy, cz]) = position else {
            log::warn!(
                "[quick_build] click sin posición válida (px=({pixel_x:.1}, {pixel_y:.1}), cursor_world={cursor_world:?})"
            );
            return true;
        };

        let scale = self
            .quick_build_effective_scale_3d()
            .unwrap_or([1.0, 1.0, 1.0]);

        let kind = self.quick_build_preview_kind.as_deref().unwrap_or("model");

        match kind {
            "model" => {
                self.place_quick_build_model_at([cx, cy, cz], scale);
            }
            "character" => {
                if let Some(path) = self.quick_build_preview_path.clone() {
                    self.load_character(&path);
                }
            }
            "scenario" => {
                log::warn!("[quick_build] escenario 2D no soportado en viewport 3D");
            }
            other => {
                log::warn!("[quick_build] kind no soportado en 3D: {other}");
            }

        }
        true
    }

    pub(crate) fn build_quick_build_ghost_overlay(&self) -> Option<(usize, mesh::InstanceData)> {
        if !matches!(self.active_tool, ActiveTool::QuickBuildPlace { .. }) {
            return None;
        }
        let ghost_id = self.quick_build_ghost_id?;
        let mc = self.world.get::<MeshComponent>(ghost_id)?;
        let t = self.world.get::<Transform>(ghost_id)?;
        if t.position.x < GHOST_OFFSCREEN + 1.0 {
            return None;
        }
        let tex_idx = mc.tex_idx;
        let layer = self.texture_layer_for(tex_idx);
        let mut inst = mesh::InstanceData::new(t.to_matrix(), 0.0, layer);
        inst.flag_pad[1] = GHOST_ALPHA;
        Some((mc.mesh_idx, inst))
    }

    /// Coloca usando la misma vía que el acordeón Entidades (`spawn_cached_model_part_at`),
    /// en la posición del ghost (no clona el mesh del ghost).
    pub(crate) fn place_quick_build_model_at(
        &mut self,
        position: [f32; 3],
        scale: [f32; 3],
    ) -> Option<EntityId> {
        let preview_path = self.quick_build_preview_path.clone()?;
        if let Err(e) = self.ensure_static_model_cached(&preview_path) {
            log::error!("[quick_build] error cargando modelo: {e}");
            send_event(&EngineEvent::Error { message: e });
            return None;
        }
        let part = self
            .cached_static_model_parts(&preview_path)?
            .first()
            .copied()?;
        let bp = self.quick_build_blueprint.clone();
        let rotation = bp
            .as_ref()
            .map(|b| b.rotation)
            .unwrap_or(DEFAULT_ROTATION);
        let kind = "model".to_string();
        let entity_category = bp
            .as_ref()
            .and_then(resolve_quick_build_entity_category);
        let is_environment = entity_category.as_deref() == Some("environment");
        let default_label = entity_label_for_category(entity_category.as_deref());
        // Instancias: nombre nuevo por categoría (Environment_05, Object_03, …), no el de la plantilla.
        let entity_name = self.next_numbered_entity_name(default_label);
        let physics_enabled = if is_environment {
            true
        } else {
            bp.as_ref().map(|b| b.physics_enabled).unwrap_or(false)
        };
        let physics_type = if is_environment {
            "static"
        } else {
            bp.as_ref()
                .map(|b| b.physics_type.as_str())
                .unwrap_or("static")
        };
        let id = self.spawn_cached_model_part_at(
            part.mesh_idx,
            part.tex_idx,
            &preview_path,
            position,
            rotation,
            scale,
            &entity_name,
            &kind,
            bp.as_ref().and_then(|b| b.blueprint_id.clone()),
            entity_category.clone(),
            physics_enabled,
            physics_type,
            part.local_bounds,
        );
        if let Some(ref blueprint) = bp {
            apply_blueprint_instance_metadata(self, id, blueprint);
        } else {
            self.entity_colision.insert(id, physics_enabled);
            if physics_enabled {
                self.reconcile_entity_physics_with_mesh(id);
            }
        }
        log::info!("[quick_build] colocado {id} «{entity_name}» en {position:?}");
        Some(id)
    }

    pub(crate) fn place_quick_build_at_cursor(
        &mut self,
        pixels: Option<(f32, f32)>,
    ) -> bool {
        if !matches!(self.active_tool, ActiveTool::QuickBuildPlace { .. }) {
            return false;
        }
        let fit_to_grid = self.ctrl_held;
        let stored = match &self.active_tool {
            ActiveTool::QuickBuildPlace { cursor_world } => *cursor_world,
            _ => None,
        };
        let position = if let Some((px, py)) = pixels {
            if fit_to_grid {
                self.raycast_placement_point(px, py)
                    .map(|p| self.quick_build_snap_position_3d(p))
                    .or(stored)
            } else {
                stored.or_else(|| self.raycast_placement_point(px, py))
            }
        } else {
            stored
        };
        let Some(pos) = position else {
            send_event(&EngineEvent::Error {
                message: "[quick_build] sin posición (mueve el ghost sobre el suelo)".into(),
            });
            return false;
        };
        let scale = self
            .quick_build_effective_scale_3d()
            .unwrap_or([1.0, 1.0, 1.0]);
        self.place_quick_build_model_at(pos, scale).is_some()
    }

    pub(crate) fn spawn_quick_build_instance_at(
        &mut self,
        position: [f32; 3],
        rotation: [f32; 4],
        scale: [f32; 3],
    ) -> Option<EntityId> {
        let _ = rotation;
        self.place_quick_build_model_at(position, scale)
    }

    /// Instancia un modelo 3D como una sola entidad (primera malla del archivo).
    pub(crate) fn load_model_single(&mut self, path: &str, entity_category: Option<&str>) {
        if self.queue_load_model_if_preloading(path, entity_category, true) {
            return;
        }
        if let Err(e) = self.ensure_static_model_cached(path) {
            log::error!("Error cargando modelo: {e}");
            send_event(&EngineEvent::Error { message: e });
            return;
        }
        let Some(part) = self.cached_static_model_parts(path).and_then(|p| p.first()) else {
            send_event(&EngineEvent::Error {
                message: format!("Modelo vacío: {path}"),
            });
            return;
        };
        self.spawn_model_from_cached_part(*part, path, entity_category);
        log::info!("Modelo cargado (instancia única): {path}");
    }
}
