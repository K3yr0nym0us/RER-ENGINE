//! Estado del editor de sockets, alimentado por los eventos del motor.

use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct SocketInfo {
    pub name: String,
    pub bone_name: String,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SocketConfigState {
    pub open: bool,
    /// `false` = crear sockets, `true` = vincular sockets.
    pub links_tab: bool,
    pub target_id: Option<u64>,
    pub target_name: String,
    pub host_id: Option<u64>,
    pub sockets: Vec<SocketInfo>,
    pub bones: Vec<String>,
    pub picked_bone: Option<String>,
    pub bone_pick_active: bool,
    pub pending_link_socket: Option<String>,
    pub name_draft: String,
    pub selected_bone: Option<String>,
    /// El formulario de socket solo se muestra tras pulsar «Añadir socket».
    pub form_open: bool,
    /// Alto real del contenido que informa la UI a la ventana para que se ajuste.
    pub content_height: f32,
}

impl SocketConfigState {
    /// Aplica los eventos del motor que actualizan la ventana de sockets.
    pub(crate) fn apply_event(&mut self, event: &Value) {
        match event.get("event").and_then(Value::as_str).unwrap_or("") {
            "entity_sockets_list" | "entity_sockets_changed" => {
                let entity_id = event.get("entity_id").and_then(Value::as_u64);
                if entity_id != self.target_id && entity_id != self.host_id {
                    return;
                }
                self.sockets = event
                    .get("sockets")
                    .and_then(Value::as_array)
                    .map(|list| list.iter().map(socket_from_value).collect())
                    .unwrap_or_default();
            }
            "entity_bones_list" => {
                if event.get("entity_id").and_then(Value::as_u64) != self.target_id {
                    return;
                }
                self.bones = event
                    .get("bones")
                    .and_then(Value::as_array)
                    .map(|list| {
                        list.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
            }
            "socket_bone_picked" => {
                if event.get("entity_id").and_then(Value::as_u64) == self.target_id {
                    self.picked_bone = event
                        .get("bone_name")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    self.bone_pick_active = false;
                }
            }
            _ => {}
        }
    }
}

fn socket_from_value(value: &Value) -> SocketInfo {
    SocketInfo {
        name: value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        bone_name: value
            .get("bone_name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        position: value
            .get("local_position")
            .and_then(Value::as_array)
            .and_then(|a| {
                <[f32; 3]>::try_from(
                    a.iter()
                        .filter_map(|v| v.as_f64())
                        .map(|v| v as f32)
                        .collect::<Vec<_>>(),
                )
                .ok()
            })
            .unwrap_or([0.0; 3]),
        rotation: value
            .get("local_rotation")
            .and_then(Value::as_array)
            .and_then(|a| {
                <[f32; 4]>::try_from(
                    a.iter()
                        .filter_map(|v| v.as_f64())
                        .map(|v| v as f32)
                        .collect::<Vec<_>>(),
                )
                .ok()
            })
            .unwrap_or([0.0, 0.0, 0.0, 1.0]),
    }
}
