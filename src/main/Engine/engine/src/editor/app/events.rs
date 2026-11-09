use super::*;

/// Entidad seleccionada en el viewport 3D (la mantiene el motor).
#[derive(Debug, Clone, PartialEq)]
pub struct SelectedEntity {
    pub id: u64,
    pub name: String,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    pub physics_enabled: bool,
    pub physics_type: String,
}

impl SelectedEntity {
    fn from_event(event: &Value) -> Option<Self> {
        Some(Self {
            id: event.get("id")?.as_u64()?,
            name: event
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            position: array3(event.get("position")?)?,
            rotation: array4(event.get("rotation")?)?,
            scale: array3(event.get("scale")?)?,
            physics_enabled: event
                .get("physics_enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            physics_type: event
                .get("physics_type")
                .and_then(Value::as_str)
                .unwrap_or("static")
                .to_string(),
        })
    }
}

/// Vista del jugador/cámara 3D confirmada por el motor (`play_character_view_changed`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayView {
    pub player_id: Option<u64>,
    pub eye: [f32; 3],
    pub yaw: f32,
    pub fov_y: f32,
    pub frustum_distance: f32,
}

impl PlayView {
    fn from_event(event: &Value) -> Option<Self> {
        let eye = array3(event.get("camera_eye_position")?)?;
        let player_id = event.get("player_id").and_then(Value::as_u64);
        Some(Self {
            player_id,
            eye,
            yaw: event
                .get("fps_camera_yaw")
                .and_then(Value::as_f64)
                .unwrap_or_default() as f32,
            fov_y: event
                .get("fov_y")
                .and_then(Value::as_f64)
                .unwrap_or_default() as f32,
            frustum_distance: event
                .get("frustum_distance")
                .and_then(Value::as_f64)
                .unwrap_or_default() as f32,
        })
    }
}

/// Métricas de rendimiento que emite el motor (`debug_metrics`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DebugMetrics {
    pub fps: f32,
    pub frame_time_ms: f32,
    pub draw_calls: u64,
    pub physics_bodies: u64,
    pub cpu_percent: Option<f32>,
    pub gpu_percent: Option<f32>,
}

impl DebugMetrics {
    pub fn from_event(event: &Value) -> Self {
        Self {
            fps: number_field(event, "fps").unwrap_or(0.0) as f32,
            frame_time_ms: number_field(event, "frame_time_ms").unwrap_or(0.0) as f32,
            draw_calls: event.get("draw_calls").and_then(Value::as_u64).unwrap_or(0),
            physics_bodies: event
                .get("physics_bodies")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            cpu_percent: event
                .get("cpu_percent")
                .and_then(Value::as_f64)
                .map(|value| value as f32),
            gpu_percent: event
                .get("gpu_percent")
                .and_then(Value::as_f64)
                .map(|value| value as f32),
        }
    }
}

/// Resumen de los bindings de control del jugador (lectura).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ControlSummary {
    pub player_name: Option<String>,
    pub keyboard: Vec<String>,
    pub gamepad: Vec<String>,
}

fn array3(value: &Value) -> Option<[f32; 3]> {
    let array = value.as_array()?;
    Some([
        array.first()?.as_f64()? as f32,
        array.get(1)?.as_f64()? as f32,
        array.get(2)?.as_f64()? as f32,
    ])
}

fn array4(value: &Value) -> Option<[f32; 4]> {
    let array = value.as_array()?;
    Some([
        array.first()?.as_f64()? as f32,
        array.get(1)?.as_f64()? as f32,
        array.get(2)?.as_f64()? as f32,
        array.get(3)?.as_f64()? as f32,
    ])
}

fn number_field(event: &Value, key: &str) -> Option<f64> {
    event.get(key).and_then(Value::as_f64)
}

fn control_summary(player: &Value) -> Option<ControlSummary> {
    let bindings = player
        .get("controls")
        .or_else(|| player.get("control_bindings"));
    let keys = |key: &str| {
        bindings
            .and_then(|bindings| bindings.get(key))
            .and_then(Value::as_object)
            .map(|map| map.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default()
    };
    Some(ControlSummary {
        player_name: player
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string),
        keyboard: keys("keyboard_mouse"),
        gamepad: keys("gamepad"),
    })
}

impl EditorApp {
    /// Actualiza el estado de la barra superior a partir de un evento del motor.
    pub fn handle_engine_event(&mut self, event: &Value) {
        let Some(name) = event.get("event").and_then(Value::as_str) else {
            return;
        };

        self.world.apply_event(event);
        self.scenes.apply_event(event);
        self.assets.apply_event(event);

        match name {
            "ready" => {
                self.status = EditorStatus::Ready;
                self.preview_playing = false;
            }
            "error" => {
                self.status = EditorStatus::Error;
            }
            "stopped" => {
                self.status = EditorStatus::Starting;
                self.preview_playing = false;
                self.left_sidebar.active_tool = None;
                self.selected_entity = None;
                self.play_view = None;
                self.pending_properties = None;
            }
            "entity_selected" => {
                self.selected_entity = SelectedEntity::from_event(event);
                if let Some(entity) = self.selected_entity.as_ref()
                    && self.pending_properties == Some(entity.id)
                {
                    self.pending_properties = None;
                    crate::editor::bridge::request_legacy_window("entity_properties", json!({}));
                }
            }
            "entity_properties_open" => {
                let id = event.get("id").and_then(Value::as_u64);
                match (id, self.selected_entity.as_ref()) {
                    (Some(id), Some(entity)) if entity.id == id => {
                        self.pending_properties = None;
                        crate::editor::bridge::request_legacy_window(
                            "entity_properties",
                            json!({}),
                        );
                    }
                    (Some(id), _) => self.pending_properties = Some(id),
                    _ => {}
                }
            }
            "entity_deselected" => {
                self.selected_entity = None;
                self.pending_properties = None;
            }
            "play_character_view_changed" => {
                self.play_view = PlayView::from_event(event);
            }
            "editor_scene_switched"
            | "project_loaded_3d"
            | "project_loaded_2d"
            | "project_load_3d_complete" => {
                self.selected_entity = None;
                self.play_view = None;
                self.pending_properties = None;
            }
            "preview_playing_changed" => {
                self.preview_playing = event
                    .get("playing")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
            }
            "debug_metrics" => {
                self.metrics = Some(DebugMetrics::from_event(event));
            }
            "tool_cancelled" | "collider_created" | "execution_area_created" => {
                self.left_sidebar.active_tool = None;
            }
            "player_ui_active_screen_changed" => {
                self.left_sidebar.active_ui_screen = event
                    .get("screen_id")
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }
            _ => {}
        }
    }

    /// Pide al motor un snapshot para sincronizar los ajustes de mundo.
    pub fn request_world_sync(&mut self) {
        if self.status != EditorStatus::Ready || self.world_sync_pending || self.saving {
            return;
        }
        self.world_sync_pending = true;
        self.send_engine(serde_json::json!({ "cmd": "export_save_snapshot" }));
    }

    /// Pide al motor las listas de recursos del proyecto.
    pub fn request_asset_sync(&mut self) {
        if self.status != EditorStatus::Ready {
            return;
        }
        for command in AssetLists::request_commands(self.cli.context.is_3d()) {
            self.send_engine(command);
        }
    }

    /// Aplica `scene.world` de un `save_snapshot_ready`.
    pub fn apply_world_snapshot(&mut self, scene: &Value) {
        self.world.apply_snapshot(scene);
        self.player_controls = scene
            .get("player")
            .filter(|player| player.is_object())
            .and_then(control_summary);
        self.world_sync_pending = false;
        log::info!(
            "[editor] mundo sincronizado (gravedad {:.2}, radio {:.1}, rejilla {}, tier sombras {:?})",
            self.world.gravity,
            self.world.world_radius,
            self.world.grid_visible,
            self.world.shadow_tier
        );
    }

    /// Aplica un comando de React para no desincronizar la barra derecha.
    pub fn apply_engine_command(&mut self, command: &Value) {
        self.world.apply_command(command);
    }
}
