//! Estado de la ventana nativa de propiedades de entidad.
//!
//! Espejo de `EntityPropertiesState` del renderer: el editor nativo ya tiene la
//! entidad seleccionada y los metadatos del proyecto, así que construye el estado
//! aquí mismo (sin snapshot IPC) y envía los mismos comandos al motor.

use serde_json::Value;

use crate::editor::ui::sidebar_right::particles::EmitterRow;

/// Pestaña activa del inspector ( mismo orden que `PropertiesTab` en React ).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum PropTab {
    #[default]
    Transform,
    Particles,
    Physics,
    Animations,
    Scripting,
    Bones,
    Projectile,
    Interaction,
}

/// Acción pendiente de una confirmación mostrada dentro de la ventana.
#[derive(Debug, Clone)]
pub(crate) enum ConfirmAction {
    RemoveEntity(u64),
    RemoveMultiple(Vec<u64>),
    DeleteAnimation(usize),
    DeleteScript(String),
    ConvertToBlueprint(u64),
}

/// Entrada del selector de modelos (Reemplazar modelo).
#[derive(Debug, Clone)]
pub(crate) struct ModelChoice {
    pub label: String,
    pub path: String,
    pub state: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ModelPicker {
    pub choices: Vec<ModelChoice>,
    pub selected: Option<String>,
    /// Datos para reconstruir la lista cada frame desde la biblioteca viva del
    /// motor (que se actualiza con `model_asset_loaded`).
    pub active_scene_id: u64,
    pub intent: String,
    /// Ruta/meta del modelo asignado, para reconstruir el picker cada frame.
    pub current: String,
    /// Nombre del modelo asignado, que se muestra arriba del select.
    pub current_label: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ConfirmDialog {
    pub title: String,
    pub message: String,
    pub confirm_label: String,
    pub action: ConfirmAction,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PropAnimation {
    pub name: String,
    pub loop_: bool,
    pub embedded_in_model: bool,
    pub frame_count: usize,
    /// El clip mueve al personaje según el hueso raíz (root motion).
    pub root_motion: bool,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PropScript {
    pub name: String,
    pub source: String,
}

/// Submodal nativo abierto desde propiedades de entidad.
#[derive(Debug, Clone)]
pub(crate) enum NativeNested {
    ScriptEditor {
        name: String,
        source: String,
    },
    Animation {
        index: Option<usize>,
        name: String,
        sprite: String,
        fps: u32,
        loop_: bool,
        columns: u32,
        rows: u32,
    },
    VisualScript,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PropEntity {
    pub id: u64,
    pub name: String,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    pub physics_enabled: bool,
    pub physics_type: String,
    pub animations: Vec<PropAnimation>,
}

/// Config de interacción (tooltip) leída del meta guardado.
#[derive(Debug, Clone, Default)]
pub(crate) struct PropInteractable {
    pub enabled: bool,
    pub tooltip_id: Option<String>,
}

impl PropInteractable {
    pub(crate) fn from_value(value: &Value) -> Self {
        Self {
            enabled: value
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            tooltip_id: value
                .get("tooltip_id")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ProjectileCfg {
    pub speed: f32,
    pub lifetime_s: f32,
    pub affected_by_gravity: bool,
    pub gravity_scale: f32,
    pub align_to_velocity: bool,
    pub muzzle_socket: String,
    pub bounceable: bool,
    pub max_bounces: u32,
    pub bounce_speed_loss: f32,
    pub hardness: u32,
}

impl Default for ProjectileCfg {
    fn default() -> Self {
        Self {
            speed: 20.0,
            lifetime_s: 3.0,
            affected_by_gravity: false,
            gravity_scale: 1.0,
            align_to_velocity: true,
            muzzle_socket: "muzzle".to_string(),
            bounceable: false,
            max_bounces: 3,
            bounce_speed_loss: 0.2,
            hardness: 0,
        }
    }
}

impl ProjectileCfg {
    pub(crate) fn from_value(value: &Value) -> Self {
        let base = Self::default();
        Self {
            speed: f32_field(value, "speed", base.speed),
            lifetime_s: f32_field(value, "lifetime_s", base.lifetime_s),
            affected_by_gravity: value
                .get("affected_by_gravity")
                .and_then(Value::as_bool)
                .unwrap_or(base.affected_by_gravity),
            gravity_scale: f32_field(value, "gravity_scale", base.gravity_scale),
            align_to_velocity: value
                .get("align_to_velocity")
                .and_then(Value::as_bool)
                .unwrap_or(base.align_to_velocity),
            muzzle_socket: value
                .get("muzzle_socket")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .unwrap_or(&base.muzzle_socket)
                .to_string(),
            bounceable: value
                .get("bounceable")
                .and_then(Value::as_bool)
                .unwrap_or(base.bounceable),
            max_bounces: value
                .get("max_bounces")
                .and_then(Value::as_u64)
                .map(|v| v as u32)
                .unwrap_or(base.max_bounces),
            bounce_speed_loss: f32_field(value, "bounce_speed_loss", base.bounce_speed_loss),
            hardness: value
                .get("hardness")
                .and_then(Value::as_u64)
                .map(|v| v as u32)
                .unwrap_or(base.hardness),
        }
    }
}

fn f32_field(value: &Value, key: &str, fallback: f32) -> f32 {
    value
        .get(key)
        .and_then(Value::as_f64)
        .map(|v| v as f32)
        .unwrap_or(fallback)
}

#[derive(Debug, Clone, Default)]
pub(crate) struct EntityPropertiesState {
    pub open: bool,
    /// `true` cuando el editor se inició en contexto 3D.
    pub is_3d: bool,
    /// Firma de los datos de los que dependen los campos calculados; evita
    /// sobrescribir lo que el usuario está escribiendo cada frame.
    signature: String,

    pub entity: Option<PropEntity>,
    /// Emisor de partículas editado en la pestaña condicional (`Some` = visible).
    pub particle_draft: Option<EmitterRow>,
    /// `true` tras pulsar Aplicar: sincroniza la fila del sidebar con el draft.
    pub particle_applied: bool,
    pub multi_ids: Vec<u64>,
    pub multi_already_merged: bool,

    pub is_character: bool,
    pub is_environment: bool,
    pub is_player: bool,
    pub is_editor_camera: bool,
    pub is_collider: bool,
    pub is_execution_area: bool,
    pub is_from_blueprint: bool,
    pub is_projectile: bool,
    /// Categoría 3D de la entidad (`environment`, `weapon`, `object`, …): la
    /// modal de propiedades filtra el selector de modelos con ella.
    pub entity_category: Option<String>,
    /// Ruta del modelo actual (para preseleccionarlo en el selector de reemplazo).
    pub current_model_path: Option<String>,
    /// El selector se abrió desde el inspector: al cancelar se reabre este,
    /// al confirmar el reemplazo no.
    pub reopen_after_model_picker: bool,
    pub linked_blueprint_name: Option<String>,

    pub scripts: Vec<PropScript>,
    pub playing_animation_name: Option<String>,
    pub can_have_bone_physics: bool,
    pub projectile_config: Option<ProjectileCfg>,
    pub entity_hardness: u32,
    pub interactable: Option<PropInteractable>,

    // --- Estados locales del componente (drafts) ---
    pub name_draft: String,
    pub editing_name: bool,
    pub copied_until: Option<std::time::Instant>,
    pub active_tab: PropTab,
    pub confirm: Option<ConfirmDialog>,
    pub model_picker: Option<ModelPicker>,
    pub nested: Option<NativeNested>,
    pub hardness_draft: String,

    pub speed_draft: String,
    pub lifetime_draft: String,
    pub affected_by_gravity_draft: bool,
    pub gravity_draft: String,
    pub align_draft: bool,
    pub muzzle_draft: String,
    pub bounceable_draft: bool,
    pub max_bounces_draft: String,
    pub bounce_loss_draft: String,
    pub projectile_hardness_draft: String,

    pub interaction_enabled_draft: bool,
    pub interaction_tooltip_id_draft: Option<String>,
    /// Altura real del contenido para que la modal se ajuste (se rellena en la UI).
    pub content_height: f32,
    /// Ancho real del contenido (pestañas sobre todo) para ajustar la ventana.
    pub content_width: f32,

    /// Posición/rotación/escala en texto, como los `input` number de React.
    pub pos_draft: [String; 3],
    pub rot_draft: [String; 4],
    pub scl_draft: [String; 3],
    pub lock_proportions: bool,
    pub rot_focus_deg: [f32; 3],
}

impl EntityPropertiesState {
    pub fn entity(&self) -> Option<&PropEntity> {
        self.entity.as_ref()
    }

    /// Reconstruye los datos derivados a partir de la selección y el estado del
    /// proyecto. Los drafts solo se reinician cuando cambia la entidad o el meta.
    pub fn refresh(&mut self, app: &crate::editor::app::EditorApp) {
        self.is_3d = app.cli.context.is_3d();
        let selected = app.selected_entity.clone();
        let meta = selected
            .as_ref()
            .and_then(|e| app.project_state.entity_meta_for(e.id))
            .cloned();
        let signature = format!(
            "{}|{:?}|{}",
            selected.as_ref().map(|e| e.id).unwrap_or(0),
            selected.as_ref().map(|e| (
                e.name.as_str(),
                e.position,
                e.rotation,
                e.scale,
                e.physics_enabled,
                e.physics_type.as_str()
            )),
            meta.as_ref().map(|m| m.to_string()).unwrap_or_default(),
        );
        if signature == self.signature {
            return;
        }
        let id_changed = self.entity.as_ref().map(|e| e.id) != selected.as_ref().map(|e| e.id);
        self.signature = signature;
        self.entity = selected.as_ref().map(|e| PropEntity {
            id: e.id,
            name: e.name.clone(),
            position: e.position,
            rotation: e.rotation,
            scale: e.scale,
            physics_enabled: e.physics_enabled,
            physics_type: e.physics_type.clone(),
            animations: meta.as_ref().map(animations_from).unwrap_or_default(),
        });

        let meta = meta.unwrap_or(Value::Null);
        let kind = str_field(&meta, "kind");
        let category = str_field(&meta, "entityCategory");
        let category3d = str_field(&meta, "entity3dCategory");
        let name = self.entity.as_ref().map(|e| e.name.as_str()).unwrap_or("");
        let is_projectile = kind == "projectile"
            || category == "projectile"
            || category3d == "projectile"
            || is_projectile_name(name);

        self.is_character = kind == "character" || category == "character";
        self.is_environment = kind == "scenario"
            || kind == "environment"
            || category == "environment"
            || category3d == "environment";
        self.is_player =
            kind == "player" || category == "player" || name.eq_ignore_ascii_case("player");
        self.is_editor_camera = kind == "editor_camera" || category == "editor_camera";
        self.is_collider = kind == "collider";
        self.is_execution_area = kind == "execution_area";
        let blueprint_id = str_field(&meta, "blueprintId");
        self.is_from_blueprint = !blueprint_id.is_empty();
        self.linked_blueprint_name = if self.is_from_blueprint {
            app.project_state
                .blueprints
                .iter()
                .find(|bp| str_field(bp, "id") == blueprint_id)
                .map(|bp| str_field(bp, "name"))
                .filter(|n| !n.is_empty())
        } else {
            None
        };
        self.is_projectile = is_projectile;
        self.entity_category = if category3d.is_empty() {
            (!category.is_empty()).then(|| category.clone())
        } else {
            Some(category3d.clone())
        };
        self.current_model_path = meta
            .get("visualModelPath")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| {
                meta.get("path")
                    .and_then(Value::as_str)
                    .filter(|s| !s.starts_with('[') && !s.is_empty())
                    .map(str::to_string)
            });
        self.projectile_config = if is_projectile {
            Some(
                meta.get("projectileConfig")
                    .filter(|v| v.is_object())
                    .map(ProjectileCfg::from_value)
                    .unwrap_or_default(),
            )
        } else {
            None
        };
        self.entity_hardness = meta
            .get("hardness")
            .and_then(Value::as_u64)
            .map(|v| v as u32)
            .or_else(|| {
                meta.get("projectileConfig")
                    .and_then(|c| c.get("hardness"))
                    .and_then(Value::as_u64)
                    .map(|v| v as u32)
            })
            .unwrap_or(0);
        self.interactable = meta
            .get("interactable")
            .filter(|v| v.is_object())
            .map(PropInteractable::from_value);
        self.scripts = meta
            .get("scripts")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .map(|s| PropScript {
                        name: str_field(s, "name"),
                        source: str_field(s, "source"),
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.playing_animation_name = meta
            .get("playingAnimationName")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        self.can_have_bone_physics = meta
            .get("canHaveBonePhysics")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        self.multi_already_merged = meta
            .get("mergedEntityIds")
            .and_then(Value::as_array)
            .map(|v| !v.is_empty())
            .unwrap_or(false);

        if id_changed {
            self.name_draft = name.to_string();
            self.editing_name = false;
            self.copied_until = None;
            self.confirm = None;
            self.model_picker = None;
            self.active_tab = if self.particle_draft.is_some() {
                PropTab::Particles
            } else {
                PropTab::Transform
            };
            self.hardness_draft = self.entity_hardness.to_string();
            let interaction = self.interactable.clone().unwrap_or_default();
            // El motor es la fuente de verdad: las entidades demo tienen el
            // Interactable solo en el motor, no en el entity_meta del proyecto.
            self.interaction_enabled_draft = selected
                .as_ref()
                .map(|entity| entity.interactable_enabled)
                .unwrap_or(interaction.enabled);
            self.interaction_tooltip_id_draft = selected
                .as_ref()
                .and_then(|entity| entity.tooltip_id.clone())
                .or(interaction.tooltip_id);

            if let Some(cfg) = self.projectile_config.as_ref() {
                self.speed_draft = cfg.speed.to_string();
                self.lifetime_draft = cfg.lifetime_s.to_string();
                self.affected_by_gravity_draft = cfg.affected_by_gravity;
                self.gravity_draft = cfg.gravity_scale.to_string();
                self.align_draft = cfg.align_to_velocity;
                self.muzzle_draft = cfg.muzzle_socket.clone();
                self.bounceable_draft = cfg.bounceable;
                self.max_bounces_draft = cfg.max_bounces.to_string();
                self.bounce_loss_draft = cfg.bounce_speed_loss.to_string();
                self.projectile_hardness_draft = cfg.hardness.to_string();
            }
            if let Some(entity) = self.entity.as_ref() {
                self.pos_draft = entity.position.map(|v| format!("{v:.2}"));
                self.scl_draft = entity.scale.map(|v| format!("{v:.3}"));
                // En 3D el renderer muestra ángulos de Euler derivados del quaternion.
                let [x, y, z, w] = entity.rotation;
                let e = quat_to_euler_degrees([x, y, z, w]);
                self.rot_draft = [
                    format!("{:.1}", e[0]),
                    format!("{:.1}", e[1]),
                    format!("{:.1}", e[2]),
                    format!("{w:.2}"),
                ];
                self.rot_focus_deg = e;
            }
        }
    }
}

fn str_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn is_projectile_name(name: &str) -> bool {
    let trimmed = name.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("projectile") {
        return false;
    }
    let rest = &lower["projectile".len()..];
    rest.is_empty() || (rest.starts_with('_') && rest[1..].chars().all(|c| c.is_ascii_digit()))
}

fn animations_from(meta: &Value) -> Vec<PropAnimation> {
    meta.get("animations")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .map(|a| PropAnimation {
                    name: str_field(a, "name"),
                    loop_: a
                        .get("loop")
                        .or_else(|| a.get("loop_"))
                        .and_then(Value::as_bool)
                        .unwrap_or(true),
                    embedded_in_model: a
                        .get("embedded_in_model")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    frame_count: a
                        .get("frames")
                        .and_then(Value::as_array)
                        .map(|f| f.len())
                        .unwrap_or(0),
                    root_motion: a
                        .get("root_motion")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// YXZ -> grados (equivalente a `quatToEulerYxzDegrees` del renderer).
fn quat_to_euler_degrees(q: [f32; 4]) -> [f32; 3] {
    let [x, y, z, w] = q;
    let sinr_cosp = 2.0 * (w * x + y * z);
    let cosr_cosp = 1.0 - 2.0 * (x * x + y * y);
    let roll = sinr_cosp.atan2(cosr_cosp);
    let sinp = (2.0 * (w * y - z * x)).clamp(-1.0, 1.0);
    let pitch = sinp.asin();
    let siny_cosp = 2.0 * (w * z + x * y);
    let cosy_cosp = 1.0 - 2.0 * (y * y + z * z);
    let yaw = siny_cosp.atan2(cosy_cosp);
    let rad = 180.0 / std::f32::consts::PI;
    [roll * rad, pitch * rad, yaw * rad]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn projectile_name_matches_expected_pattern() {
        assert!(is_projectile_name("Projectile"));
        assert!(is_projectile_name(" projectile_12 "));
        assert!(!is_projectile_name("ProjectileX"));
        assert!(!is_projectile_name("Fireball"));
    }

    #[test]
    fn identity_quaternion_has_zero_euler() {
        let [x, y, z] = quat_to_euler_degrees([0.0, 0.0, 0.0, 1.0]);
        assert!(x.abs() < 1e-4 && y.abs() < 1e-4 && z.abs() < 1e-4);
    }

    #[test]
    fn projectile_config_uses_defaults_for_missing_fields() {
        let cfg = ProjectileCfg::from_value(&json!({ "speed": 30.0, "max_bounces": 5 }));
        assert_eq!(cfg.speed, 30.0);
        assert_eq!(cfg.max_bounces, 5);
        assert_eq!(cfg.lifetime_s, 3.0);
        assert_eq!(cfg.muzzle_socket, "muzzle");
        assert!(cfg.align_to_velocity);
    }
}
