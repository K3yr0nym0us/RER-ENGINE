//! Editor defaults owned by the engine (scene names, entity labels, etc.).

/// English prefixes for numbered editor entity names (`Scenario_01`, …).
pub mod entity_label {
    pub const SCENARIO: &str = "Scenario";
    pub const CHARACTER: &str = "Character";
    pub const OBJECT: &str = "Object";
    pub const ENVIRONMENT: &str = "Environment";
    pub const COLLIDER: &str = "Collider";
    pub const EXECUTION_AREA: &str = "ExecutionArea";
    pub const BACKGROUND: &str = "Background";
    pub const SUN: &str = "Sun";
    pub const BOX: &str = "Box";
    pub const GROUND: &str = "Ground";
    pub const PLAYER: &str = "Player";
    pub const BALL: &str = "Ball";
    pub const REFLECTION_PROBE: &str = "RefProbe";
    pub const WEAPON: &str = "Weapon";
    pub const PROJECTILE: &str = "Projectile";
}

/// Player UI por defecto en plantillas 3D (cámara play character).
pub mod player_ui {
    pub const DEFAULT_3D_PLAYER_UI_SCREEN_ID: &str = "player-hud-01";
    pub const DEFAULT_3D_PLAYER_UI_SCREEN_NAME: &str = "Player UI 01";
    pub const DEFAULT_CROSSHAIR_H_OBJECT_ID: u32 = 1;
    pub const DEFAULT_CROSSHAIR_V_OBJECT_ID: u32 = 2;

    const CROSSHAIR_THICKNESS: f32 = 0.0018;
    const CROSSHAIR_HALF_W: f32 = 0.018;
    const CROSSHAIR_HALF_H: f32 = 0.028;

    /// Barra horizontal del crosshair (rectángulo delgado en NDC).
    pub fn default_crosshair_horizontal_vertices() -> Vec<[f32; 2]> {
        let t = CROSSHAIR_THICKNESS;
        let hw = CROSSHAIR_HALF_W;
        vec![[-hw, -t], [hw, -t], [hw, t], [-hw, t]]
    }

    /// Barra vertical del crosshair (rectángulo delgado en NDC).
    pub fn default_crosshair_vertical_vertices() -> Vec<[f32; 2]> {
        let t = CROSSHAIR_THICKNESS;
        let hh = CROSSHAIR_HALF_H;
        vec![[-t, -hh], [t, -hh], [t, hh], [-t, hh]]
    }

    pub const DEFAULT_CROSSHAIR_FILL: [f32; 4] = [1.0, 1.0, 1.0, 0.92];
}

/// Default name for a project scene (`Scene-01`, `Scene-02`, …).
pub fn default_scene_name(scene_id: u32) -> String {
    format!("Scene-{scene_id:02}")
}

/// Next numbered entity label: `{base}_01`, `{base}_02`, …
pub fn next_numbered_entity_label(
    base: &str,
    existing_names: impl IntoIterator<Item = impl AsRef<str>>,
) -> String {
    let clean_base = base.trim();
    let prefix = format!("{clean_base}_");
    let mut max_suffix: u32 = 0;

    for name in existing_names {
        let current = name.as_ref().trim();
        if let Some(rest) = current.strip_prefix(&prefix)
            && let Ok(n) = rest.parse::<u32>()
        {
            max_suffix = max_suffix.max(n);
        }
    }

    format!("{clean_base}_{:02}", max_suffix.saturating_add(1))
}

/// Infiere categoría manifest desde etiqueta numerada (`Environment_04` → `environment`).
pub fn infer_entity_category_from_numbered_name(name: &str) -> Option<&'static str> {
    let base = name.split('_').next()?.trim();
    match base {
        "Environment" | "Scenario" => Some("environment"),
        "Object" => Some("object"),
        "Character" => Some("character"),
        "Weapon" => Some("weapon"),
        "Projectile" => Some("projectile"),
        "Player" => Some("player"),
        "Sun" => Some("sun"),
        "Ground" => Some("ground"),
        "Ball" => Some("object"),
        _ => None,
    }
}

/// Prefijo de nombre numerado según categoría de entidad del editor.
pub fn entity_label_for_category(entity_category: Option<&str>) -> &'static str {
    match entity_category {
        Some("environment") => entity_label::ENVIRONMENT,
        Some("character") | Some("player") => entity_label::CHARACTER,
        Some("object") => entity_label::OBJECT,
        Some("weapon") => entity_label::WEAPON,
        Some("projectile") => entity_label::PROJECTILE,
        Some("sun") => entity_label::SUN,
        Some("ground") => entity_label::GROUND,
        _ => entity_label::OBJECT,
    }
}

/// Prefijo de nombre al instanciar: `entity_category` manda; si falta, usa `kind` IPC (`character` → Character_*).
pub fn entity_label_for_spawn(kind: &str, entity_category: Option<&str>) -> &'static str {
    if entity_category.is_some() {
        return entity_label_for_category(entity_category);
    }
    match kind {
        "character" => entity_label::CHARACTER,
        "scenario" => entity_label::SCENARIO,
        _ => entity_label::OBJECT,
    }
}

pub fn resolve_entity_display_name(
    requested: &str,
    default_base: &str,
    existing_names: impl IntoIterator<Item = impl AsRef<str>>,
) -> String {
    if requested.trim().is_empty() {
        next_numbered_entity_label(default_base, existing_names)
    } else {
        requested.trim().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        default_scene_name, entity_label, infer_entity_category_from_numbered_name,
        next_numbered_entity_label, resolve_entity_display_name,
    };

    #[test]
    fn default_scene_name_zero_padded() {
        assert_eq!(default_scene_name(1), "Scene-01");
        assert_eq!(default_scene_name(12), "Scene-12");
    }

    #[test]
    fn next_numbered_entity_label_increments() {
        let names = ["Scenario_01", "Scenario_03", "Other_01"];
        assert_eq!(
            next_numbered_entity_label(entity_label::SCENARIO, names),
            "Scenario_04"
        );
    }

    #[test]
    fn infer_entity_category_from_numbered_name_prefix() {
        assert_eq!(
            infer_entity_category_from_numbered_name("Environment_04"),
            Some("environment")
        );
        assert_eq!(
            infer_entity_category_from_numbered_name("Object_02"),
            Some("object")
        );
    }

    #[test]
    fn resolve_entity_display_name_uses_default_when_empty() {
        assert_eq!(
            resolve_entity_display_name("", entity_label::SUN, ["Sun_01"]),
            "Sun_02"
        );
        assert_eq!(
            resolve_entity_display_name(
                "Custom Sun",
                entity_label::SUN,
                std::iter::empty::<&str>()
            ),
            "Custom Sun"
        );
    }
}
