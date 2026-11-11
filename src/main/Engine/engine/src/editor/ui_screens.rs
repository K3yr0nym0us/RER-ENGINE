//! Catálogo de pantallas de UI gestionado por React y sincronizado al editor nativo.

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
    /// Aplica `{ player: [...], menu: [...] }` recibido desde Electron.
    pub fn apply_sync(&mut self, value: &Value) {
        self.player = parse_screens(value.get("player"));
        self.menu = parse_screens(value.get("menu"));
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
}
