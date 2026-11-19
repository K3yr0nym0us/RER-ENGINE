//! Catálogo de pantallas de UI gestionado por el editor nativo (fuente de verdad).

use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UiScreenEntry {
    pub id: String,
    pub name: String,
    pub active: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UiScreenLists {
    pub player: Vec<UiScreenEntry>,
    pub menu: Vec<UiScreenEntry>,
}

impl UiScreenLists {
    /// Carga inicial desde el `ProjectState` leído del `manifest.json`.
    pub fn from_state(state: &crate::editor::project_state::ProjectState) -> Self {
        Self {
            player: parse_screens(Some(&Value::Array(state.player_ui_screens.clone()))),
            menu: parse_screens(Some(&Value::Array(state.menu_ui_screens.clone()))),
        }
    }

    #[cfg(test)]
    pub fn apply_sync(&mut self, value: &Value) {
        self.player = parse_screens(value.get("player"));
        self.menu = parse_screens(value.get("menu"));
    }

    /// Id fresco `ui_N`, sin colisionar con las pantallas ya existentes.
    pub fn next_id(&self) -> String {
        let mut max = self
            .player
            .iter()
            .chain(self.menu.iter())
            .filter_map(|screen| screen.id.strip_prefix("ui_"))
            .filter_map(|number| number.parse::<u64>().ok())
            .max()
            .unwrap_or(0);
        max += 1;
        loop {
            let id = format!("ui_{max}");
            if !self.player.iter().any(|screen| screen.id == id)
                && !self.menu.iter().any(|screen| screen.id == id)
            {
                return id;
            }
            max += 1;
        }
    }
}

fn parse_screens(value: Option<&Value>) -> Vec<UiScreenEntry> {
    value
        .and_then(Value::as_array)
        .map(|screens| {
            screens
                .iter()
                .filter_map(|screen| {
                    let id = screen.get("id")?.as_str()?.to_string();
                    let name = screen
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or(&id)
                        .to_string();
                    Some(UiScreenEntry {
                        id,
                        name,
                        active: screen
                            .get("active")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn applies_player_and_menu_screens_from_sync() {
        let mut lists = UiScreenLists::default();
        lists.apply_sync(&json!({
            "player": [
                { "id": "hud-1", "name": "HUD principal", "active": true },
                { "id": "hud-2", "name": "HUD secundario" }
            ],
            "menu": [
                { "id": "menu-1", "name": "Menu pausa" }
            ]
        }));

        assert_eq!(lists.player.len(), 2);
        assert_eq!(lists.player[0].id, "hud-1");
        assert_eq!(lists.player[0].name, "HUD principal");
        assert!(lists.player[0].active);
        assert!(!lists.player[1].active);
        assert_eq!(lists.menu.len(), 1);
        assert_eq!(lists.menu[0].id, "menu-1");
    }

    #[test]
    fn next_id_skips_existing_screens() {
        let mut lists = UiScreenLists::default();
        lists.apply_sync(&json!({
            "player": [{ "id": "ui_1" }, { "id": "ui_3" }],
            "menu": [{ "id": "ui_2" }]
        }));
        assert_eq!(lists.next_id(), "ui_4");
    }
}
