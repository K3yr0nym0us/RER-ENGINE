//! Lista de escenas del editor 3D. La mantiene el motor y la reflejamos aquí.

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneEntry {
    pub id: u64,
    pub name: String,
    pub dirty: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SceneList {
    pub scenes: Vec<SceneEntry>,
    pub active_scene_id: u64,
    pub loading: bool,
    pub switching_to: Option<u64>,
}

impl SceneList {
    pub fn apply_event(&mut self, event: &Value) {
        match event.get("event").and_then(Value::as_str).unwrap_or("") {
            "editor_scenes_updated" | "editor_scene_created" | "editor_scene_switched" => {
                self.apply_snapshot(event);
                self.loading = false;
                self.switching_to = None;
            }
            "editor_scene_switch_blocked" => {
                self.loading = false;
                self.switching_to = None;
            }
            "project_loaded_3d" | "project_load_3d_complete" => {
                self.loading = false;
                self.switching_to = None;
            }
            "stopped" => {
                self.scenes.clear();
                self.active_scene_id = 0;
                self.loading = false;
                self.switching_to = None;
            }
            _ => {}
        }
    }

    fn apply_snapshot(&mut self, event: &Value) {
        if let Some(active) = event.get("active_scene_id").and_then(Value::as_u64) {
            self.active_scene_id = active;
        }
        if let Some(list) = event.get("scenes").and_then(Value::as_array) {
            self.scenes = list.iter().filter_map(scene_from_value).collect();
            if self.active_scene_id == 0
                && let Some(first) = self.scenes.first()
            {
                self.active_scene_id = first.id;
            }
        }
    }

    pub fn begin_switch(&mut self, id: u64) {
        self.switching_to = Some(id);
        self.loading = true;
    }

    #[cfg(test)]
    pub fn active(&self) -> Option<&SceneEntry> {
        self.scenes
            .iter()
            .find(|scene| scene.id == self.active_scene_id)
    }

    pub fn can_delete(&self) -> bool {
        self.scenes.len() > 1
    }

    pub fn name_for(&self, id: u64) -> String {
        self.scenes
            .iter()
            .find(|scene| scene.id == id)
            .map(|scene| scene.name.clone())
            .unwrap_or_else(|| format!("#{id}"))
    }
}

fn scene_from_value(value: &Value) -> Option<SceneEntry> {
    Some(SceneEntry {
        id: value.get("id")?.as_u64()?,
        name: value.get("name")?.as_str()?.to_string(),
        dirty: value.get("dirty").and_then(Value::as_bool).unwrap_or(false),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_scene_updates() {
        let mut list = SceneList::default();
        list.apply_event(&json!({
            "event": "editor_scenes_updated",
            "active_scene_id": 2,
            "scenes": [
                { "id": 1, "name": "Scene-01", "dirty": false },
                { "id": 2, "name": "Nivel 2", "dirty": true }
            ]
        }));
        assert_eq!(list.scenes.len(), 2);
        assert_eq!(list.active_scene_id, 2);
        assert_eq!(
            list.active().map(|scene| scene.name.as_str()),
            Some("Nivel 2")
        );
        assert!(list.can_delete());
    }

    #[test]
    fn switch_starts_and_blocked_clears_loading() {
        let mut list = SceneList::default();
        list.begin_switch(3);
        assert!(list.loading);
        assert_eq!(list.switching_to, Some(3));
        list.apply_event(&json!({
            "event": "editor_scene_switch_blocked",
            "active_scene_id": 1,
            "target_scene_id": 3,
            "reason": "unsaved_changes"
        }));
        assert!(!list.loading);
        assert_eq!(list.switching_to, None);
    }

    #[test]
    fn cannot_delete_the_only_scene() {
        let mut list = SceneList::default();
        list.apply_event(&json!({
            "event": "editor_scenes_updated",
            "active_scene_id": 1,
            "scenes": [{ "id": 1, "name": "Scene-01" }]
        }));
        assert!(!list.can_delete());
    }
}
