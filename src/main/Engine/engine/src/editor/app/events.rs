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

/// Texto del hint flotante sobre el viewport mientras se coloca una entidad.
fn placement_hint_text(locale: crate::editor::cli::Locale) -> &'static str {
    crate::editor::i18n::tr(
        locale,
        "Mueve el cursor sobre el mundo y haz click para colocar la entidad",
        "Move the cursor over the world and click to place the entity",
    )
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
                self.placement_hint = None;
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
            "background_loaded" => {
                self.background_path = event
                    .get("path")
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }
            "project_loaded_3d" => {
                if let Some(world) = event.get("world") {
                    self.world.apply_loaded_world(world);
                }
                self.selected_entity = None;
                self.play_view = None;
                self.pending_properties = None;
            }
            "editor_scene_switched" | "project_loaded_2d" | "project_load_3d_complete" => {
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
                self.placement_hint = None;
            }
            "quick_build_ghost_ready" => {
                self.placement_hint = Some(placement_hint_text(self.cli.locale).to_string());
            }
            "particle_emitter_placed" => {
                let number =
                    |key: &str| event.get(key).and_then(Value::as_f64).unwrap_or(0.0) as f32;
                let position = [number("x"), number("y"), number("z")];
                let Some(mut draft) = self.right_sidebar.particles.pending_placement.take() else {
                    return;
                };
                draft.set_position(position);
                // El click del ghost CREA el elemento (comando al motor + fila
                // del sidebar) y DESPUÉS abre su modal de propiedades, donde
                // «Aplicar» hace upsert sin cerrar la ventana.
                let command = self.right_sidebar.particles.emitter_command(&draft);
                self.send_engine(command);
                let emitters = &mut self.right_sidebar.particles.emitters;
                match emitters.iter_mut().find(|existing| existing.id == draft.id) {
                    Some(existing) => *existing = draft.clone(),
                    None => emitters.push(draft.clone()),
                }
                self.right_sidebar.particles.awaiting_config = false;
                draft.pin_inapplicable_fields();
                self.right_sidebar.particles.modal = Some(draft);
                self.right_sidebar.particles.modal_is_new = false;
                self.right_sidebar.particles.modal_configure = true;
                self.push_log(
                    format!(
                        "[Partículas] Elemento colocado en ({:.2}, {:.2}, {:.2}). Ajusta sus propiedades y pulsa Aplicar",
                        position[0], position[1], position[2]
                    ),
                    false,
                );
            }
            "emitter_picked" => {
                // Click sobre un emisor en el viewport: abrir sus propiedades
                // directamente (sin buscarlo en la lista del sidebar).
                let Some(id) = event.get("id").and_then(Value::as_u64) else {
                    return;
                };
                let Some(row) = self
                    .right_sidebar
                    .particles
                    .emitters
                    .iter()
                    .find(|existing| existing.id == id)
                    .cloned()
                else {
                    return;
                };
                self.right_sidebar.particles.modal = Some(row);
                self.right_sidebar.particles.modal_is_new = false;
                self.right_sidebar.particles.modal_configure = false;
                self.push_log(
                    "[Partículas] Propiedades del emisor abiertas".to_string(),
                    false,
                );
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
        // Partículas (F5): el dropdown del sidebar se sincroniza con el estado real
        // del motor (clima + emisores de escena).
        if let Some(particles) = scene.get("world").and_then(|world| world.get("particles")) {
            self.right_sidebar.particles.apply_snapshot(particles);
        }
        self.background_path = scene
            .get("background_path")
            .and_then(Value::as_str)
            .map(str::to_string);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::ui::sidebar_right::particles::EmitterRow;

    #[test]
    fn placement_hint_follows_the_ghost_lifecycle() {
        let mut app = EditorApp::new(crate::editor::cli::Cli::default());
        assert!(app.placement_hint.is_none());

        app.handle_engine_event(&json!({
            "event": "quick_build_ghost_ready",
            "path": "model_hero",
            "name": "Hero",
        }));
        assert!(app.placement_hint.is_some());

        app.handle_engine_event(&json!({ "event": "tool_cancelled" }));
        assert!(app.placement_hint.is_none());
    }

    #[test]
    fn placed_after_select_only_creates_the_element_then_opens_properties() {
        // Flujo corregido (alta nueva en 3D): el click del ghost CREA el elemento
        // (comando + fila) y DESPUÉS abre la modal de propiedades.
        let mut app = EditorApp::new(crate::editor::cli::Cli::default());
        app.right_sidebar.particles.pending_placement = Some(EmitterRow::new_draft("fire"));
        app.right_sidebar.particles.awaiting_config = true;

        app.handle_engine_event(&json!({
            "event": "particle_emitter_placed",
            "x": 1.0,
            "y": 0.5,
            "z": 2.0,
        }));

        assert!(app.right_sidebar.particles.pending_placement.is_none());
        assert!(!app.right_sidebar.particles.awaiting_config);
        let modal = app
            .right_sidebar
            .particles
            .modal
            .as_ref()
            .expect("la modal de configuración debe abrirse");
        assert!(!app.right_sidebar.particles.modal_is_new);
        assert!(app.right_sidebar.particles.modal_configure);
        // La posición del ghost ya está en el borrador.
        assert_eq!(modal.position[0].parsed(), Some(1.0));
        assert_eq!(modal.position[1].parsed(), Some(0.5));
        assert_eq!(modal.position[2].parsed(), Some(2.0));
        // El elemento ya existe en el motor y en la lista; la modal es solo
        // para ajustar sus propiedades con «Aplicar».
        assert_eq!(app.right_sidebar.particles.emitters.len(), 1);
        assert_eq!(app.pending_engine.borrow().len(), 1);
    }

    #[test]
    fn placed_without_awaiting_config_creates_the_emitter_immediately() {
        // Re-colocación de un emisor existente (edición en 3D): conserva el
        // comportamiento directo — aplica al motor y actualiza la fila.
        let mut app = EditorApp::new(crate::editor::cli::Cli::default());
        let mut draft = EmitterRow::new_draft("smoke");
        draft.id = 7;
        app.right_sidebar.particles.pending_placement = Some(draft);
        app.right_sidebar.particles.awaiting_config = false;

        app.handle_engine_event(&json!({
            "event": "particle_emitter_placed",
            "x": -1.0,
            "y": 0.2,
            "z": 3.0,
        }));

        // Igual que en el alta nueva: colocar crea y abre las propiedades.
        assert!(app.right_sidebar.particles.modal.is_some());
        assert_eq!(app.right_sidebar.particles.emitters.len(), 1);
        assert_eq!(app.right_sidebar.particles.emitters[0].id, 7);
        assert_eq!(
            app.right_sidebar.particles.emitters[0].position[0].parsed(),
            Some(-1.0)
        );
        assert_eq!(app.pending_engine.borrow().len(), 1);
    }

    #[test]
    fn emitter_picked_opens_the_emitter_properties() {
        // Click sobre un emisor en el viewport: abre sus propiedades sin
        // buscarlo en la lista; un id desconocido no abre nada.
        let mut app = EditorApp::new(crate::editor::cli::Cli::default());
        let mut row = EmitterRow::new_draft("water");
        row.id = 42;
        app.right_sidebar.particles.emitters.push(row.clone());

        app.handle_engine_event(&json!({ "event": "emitter_picked", "id": 42 }));
        let modal = app
            .right_sidebar
            .particles
            .modal
            .as_ref()
            .expect("la modal debe abrirse con el emisor picado");
        assert_eq!(modal.id, 42);
        assert!(!app.right_sidebar.particles.modal_is_new);
        assert!(!app.right_sidebar.particles.modal_configure);

        app.right_sidebar.particles.modal = None;
        app.handle_engine_event(&json!({ "event": "emitter_picked", "id": 999 }));
        assert!(app.right_sidebar.particles.modal.is_none());
    }
}
