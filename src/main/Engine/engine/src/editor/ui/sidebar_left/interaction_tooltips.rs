//! Accordion «Tooltips de interacción» del SidebarLeft.
//!
//! Aquí se crean, listan y editan las definiciones de tooltip que luego se
//! asignan a las entidades desde la modal de propiedades.

use serde_json::{Value, json};

use egui_phosphor::regular as icons;

use super::{LeftAction, SidebarLeftState, widgets};
use crate::editor::cli::Locale;
use crate::editor::i18n::tr;
use crate::editor::theme;

#[derive(Debug, Clone)]
pub(crate) struct TooltipDraft {
    pub id: String,
    pub name: String,
    pub key: String,
    /// `press` o `hold`: modo de activación del tooltip.
    pub trigger: String,
    pub action: String,
    /// Script/nodo asignado: nombre para mostrar.
    pub action_ref_name: String,
    /// Id de la animación cuando action = animation.
    pub animation_id: String,
    /// Fuente Rhai del script (o del grafo compilado).
    pub action_ref_source: String,
    /// Grafo de nodos serializado (cuando action = node).
    pub action_ref_graph: Value,
    pub max_distance: String,
    pub tooltip_distance: String,
    pub once: bool,
    pub highlight: bool,
    pub highlight_color: [u8; 3],
}

impl Default for TooltipDraft {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            key: "E".to_string(),
            trigger: "press".to_string(),
            action: "script".to_string(),
            action_ref_name: String::new(),
            animation_id: String::new(),
            action_ref_source: String::new(),
            action_ref_graph: Value::Null,
            max_distance: "3.5".to_string(),
            tooltip_distance: "5.0".to_string(),
            once: false,
            highlight: false,
            highlight_color: [31, 153, 224],
        }
    }
}

impl TooltipDraft {
    pub(crate) fn from_value(value: &Value) -> Self {
        let text = |key: &str| {
            value
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        Self {
            id: text("id"),
            name: text("name"),
            key: {
                let key = text("key");
                if key.is_empty() { "E".to_string() } else { key }
            },
            trigger: {
                let trigger = text("trigger");
                if trigger.eq_ignore_ascii_case("hold") {
                    "hold".to_string()
                } else {
                    "press".to_string()
                }
            },
            action: {
                let action = text("action");
                if action.is_empty() {
                    "script".to_string()
                } else {
                    action
                }
            },
            action_ref_name: text("action_ref_name"),
            animation_id: text("animation_id"),
            action_ref_source: text("action_ref_source"),
            action_ref_graph: value
                .get("action_ref_graph")
                .cloned()
                .unwrap_or(Value::Null),
            max_distance: {
                let value = text("max_distance");
                if value.is_empty() {
                    "3.5".to_string()
                } else {
                    value
                }
            },
            tooltip_distance: {
                let value = text("tooltip_distance");
                if value.is_empty() {
                    "5.0".to_string()
                } else {
                    value
                }
            },
            once: value.get("once").and_then(Value::as_bool).unwrap_or(false),
            highlight: value
                .get("highlight")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            highlight_color: crate::engine3d::config_3d::interaction::tooltip_color(value),
        }
    }

    pub(crate) fn to_value(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "key": self.key,
            "trigger": self.trigger,
            "action": self.action,
            "action_ref_name": self.action_ref_name,
            "animation_id": self.animation_id,
            "action_ref_source": self.action_ref_source,
            "action_ref_graph": self.action_ref_graph,
            "max_distance": self.max_distance,
            "tooltip_distance": self.tooltip_distance,
            "once": self.once,
            "highlight": self.highlight,
            "highlight_color": self.highlight_color,
        })
    }
}

/// Confirmación pendiente de borrado de un tooltip.
#[derive(Debug, Clone)]
pub(crate) struct DeleteInteractionTooltipDraft {
    pub id: String,
    pub name: String,
}

/// Comando que sincroniza el registro de tooltips del motor con el proyecto.
pub(crate) fn sync_command(app: &crate::editor::app::EditorApp) -> Value {
    json!({
        "cmd": "sync_interaction_tooltips",
        "tooltips": app.project_state.interaction_tooltips,
    })
}

/// IDs de los tooltips sembrados con la fila demo de interacción. «Limpiar
/// escena» los retira junto a las entidades `[InteractDemo]`.
pub(crate) const DEMO_TOOLTIP_IDS: &[&str] =
    &["demo_move", "demo_vanish", "demo_drop", "demo_hold"];

/// `true` si el tooltip pertenece a la fila demo de interacción.
pub(crate) fn is_demo_tooltip(tooltip: &Value) -> bool {
    tooltip
        .get("id")
        .and_then(Value::as_str)
        .is_some_and(|id| DEMO_TOOLTIP_IDS.contains(&id))
}

/// Tooltips de la fila demo, disponibles desde el primer arranque de un proyecto 3D.
/// Cada uno lleva su script Rhai en la definición, así que se puede editar o
/// quitar desde el formulario igual que cualquier otro tooltip (sin código en duro).
pub(crate) fn demo_tooltips() -> Vec<Value> {
    const MOVE: &str = r#"fn on_interact(target, actor) {
    let next = if target.y < 1.2 { 2.2 } else { 0.6 };
    engine.set_physics(target.id, false, "static");
    engine.move_to(target.id, target.x, next);
    engine.log("Demo · plataforma movida");
}
"#;
    const VANISH: &str = r#"fn on_interact(target, actor) {
    engine.set_interactable(target.id, false);
    engine.log("Demo · caja desaparecida");
    engine.despawn_entity(target.id);
}
"#;
    const DROP: &str = r#"fn on_interact(target, actor) {
    engine.set_physics(target.id, true, "dynamic");
    engine.set_interactable(target.id, false);
    engine.log("Demo · orbe liberado");
}
"#;
    const HOLD: &str = r#"fn on_interact(target, actor) {
    engine.toggle_demo_fire();
    engine.log("Demo · mantener completado");
}
"#;

    let base = |id: &str, name: &str, trigger: &str, color: [u8; 3], script: &str| {
        json!({
            "id": id,
            "name": name,
            "key": "E",
            "trigger": trigger,
            "action": "script",
            "action_ref_name": format!("{id}_script"),
            "action_ref_source": script,
            "max_distance": "5.0",
            "tooltip_distance": "6.0",
            "once": false,
            "highlight": true,
            "highlight_color": color,
        })
    };
    vec![
        base(
            "demo_move",
            "Mover plataforma",
            "press",
            [31, 153, 224],
            MOVE,
        ),
        base(
            "demo_vanish",
            "Desaparecer caja",
            "press",
            [242, 64, 89],
            VANISH,
        ),
        base("demo_drop", "Soltar orbe", "press", [199, 115, 255], DROP),
        base(
            "demo_hold",
            "Encender fuego (mantener)",
            "hold",
            [255, 115, 51],
            HOLD,
        ),
    ]
}

pub(crate) fn body(
    ui: &mut egui::Ui,
    _state: &mut SidebarLeftState,
    tooltips: &[Value],
    locale: Locale,
    actions: &mut Vec<LeftAction>,
) {
    let width = ui.available_width();
    if widgets::wide_button_click_sized(
        ui,
        width,
        icons::PLUS,
        tr(locale, "Crear tooltip", "Create tooltip"),
        tr(
            locale,
            "Abre el formulario del tooltip",
            "Open the tooltip form",
        ),
        theme::ACCENT,
    ) {
        actions.push(LeftAction::OpenTooltipModal(Box::new(None)));
    }
    ui.add_space(6.0);

    if tooltips.is_empty() {
        widgets::tiny(ui, tr(locale, "Sin tooltips", "No tooltips yet"));
        return;
    }

    for tooltip in tooltips {
        let id = tooltip
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let name = tooltip
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Tooltip")
            .to_string();
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(8, 6))
            .corner_radius(4)
            .stroke(egui::Stroke::new(1.0, theme::BORDER))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let name_width = (ui.available_width() - 46.0).max(0.0);
                    ui.add_sized(
                        [name_width, 18.0],
                        egui::Label::new(egui::RichText::new(&name).size(12.0).color(theme::TEXT))
                            .truncate(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        if widgets::icon_action(
                            ui,
                            icons::TRASH,
                            18.0,
                            theme::DANGER,
                            tr(locale, "Borrar tooltip", "Delete tooltip"),
                        ) {
                            actions.push(LeftAction::DeleteInteractionTooltip {
                                id: id.clone(),
                                name: name.clone(),
                            });
                        }
                        if widgets::icon_action(
                            ui,
                            icons::PENCIL,
                            18.0,
                            theme::INFO,
                            tr(locale, "Editar tooltip", "Edit tooltip"),
                        ) {
                            actions.push(LeftAction::OpenTooltipModal(Box::new(Some(
                                TooltipDraft::from_value(tooltip),
                            ))));
                        }
                    });
                });
            });
        ui.add_space(4.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_scene_identifies_demo_tooltips_only() {
        let demos = demo_tooltips();
        assert_eq!(demos.len(), DEMO_TOOLTIP_IDS.len());
        assert!(demos.iter().all(is_demo_tooltip));

        let custom = json!({ "id": "my_tooltip", "name": "Custom" });
        assert!(!is_demo_tooltip(&custom));
        assert!(!is_demo_tooltip(&json!({ "name": "sin id" })));
    }
}
