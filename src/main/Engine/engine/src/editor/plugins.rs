//! Estado de la ventana nativa del catálogo de plugins.
//!
//! Sustituye al `PluginsModalBody` y a `usePlugins` del renderer. El catálogo y
//! la UI viven en Rust; las operaciones de instalación/conexión se piden a
//! Electron main, que es quien posee el estado en disco y los runtime.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::editor::app::EditorApp;
use crate::editor::plugin_runtime as runtime;

/// Entrada del catálogo embebido (espejo de `PLUGIN_CATALOG`).
#[derive(Debug, Clone)]
pub struct PluginCatalogEntry {
    pub id: &'static str,
    pub name: &'static str,
    #[allow(dead_code)]
    pub description: &'static str,
    pub version: &'static str,
    pub size_label: &'static str,
}

pub const CATALOG: [PluginCatalogEntry; 2] = [
    PluginCatalogEntry {
        id: "local-ai-assistant",
        name: "RER-AI (OpenCode)",
        description: "Assistant powered by OpenCode. Reads only the docs/ai editor guides and acts on the UI with dedicated tools. Free models by default; OpenCode Go token enables paid models.",
        version: "2.0.0",
        size_label: "~125 MB (OpenCode runtime)",
    },
    PluginCatalogEntry {
        id: "world-gen",
        name: "World Generator",
        description: "Generación procedural de mundos 3D: Marching Cubes con superficies suaves, montañas, lagos, ríos, cuevas y biomas configurables con semilla determinista.",
        version: "1.0.0",
        size_label: "~0.2 MB (rer_worldgen_plugin.zip)",
    },
];

#[derive(Debug, Clone, Default)]
pub struct PluginsState {
    pub installed: BTreeMap<String, Value>,
    pub enabled: Vec<String>,
}

impl PluginsState {
    pub fn from_value(value: &Value) -> Self {
        let installed = value
            .get("installed")
            .and_then(Value::as_object)
            .map(|map| {
                map.iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let enabled = value
            .get("enabled")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        Self { installed, enabled }
    }

    pub fn is_installed(&self, id: &str) -> bool {
        self.installed.contains_key(id)
    }

    pub fn is_enabled(&self, id: &str) -> bool {
        self.is_installed(id) && self.enabled.iter().any(|item| item == id)
    }
}

#[derive(Debug, Clone, Default)]
pub struct LlmStatus {
    pub status: String,
    pub error: Option<String>,
    #[allow(dead_code)]
    pub enabled: bool,
    #[allow(dead_code)]
    pub installed: bool,
}

impl LlmStatus {
    pub fn from_value(value: &Value) -> Self {
        Self {
            status: value
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("idle")
                .to_string(),
            error: value
                .get("error")
                .and_then(Value::as_str)
                .map(str::to_string),
            enabled: value
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            installed: value
                .get("installed")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }

    pub fn server_running(&self) -> bool {
        self.status == "running" || self.status == "starting"
    }
}

#[derive(Debug, Clone, Default)]
pub struct DownloadProgress {
    pub plugin_id: String,
    pub phase: String,
    pub overall_percent: u32,
    pub bytes_received: u64,
    pub bytes_total: u64,
}

impl DownloadProgress {
    pub fn from_value(value: &Value) -> Self {
        Self {
            plugin_id: value
                .get("pluginId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            phase: value
                .get("phase")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            overall_percent: value
                .get("overallPercent")
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
                .round()
                .max(0.0) as u32,
            bytes_received: value
                .get("bytesOverallReceived")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            bytes_total: value
                .get("bytesOverallTotal")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AssistantSettings {
    pub selected_model: Option<String>,
    pub go_connected: bool,
    #[allow(dead_code)]
    pub opencode_connected: bool,
}

impl AssistantSettings {
    pub fn from_value(value: &Value) -> Self {
        Self {
            selected_model: value
                .get("selectedModel")
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
                .map(str::to_string),
            go_connected: value
                .get("goConnected")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            opencode_connected: value
                .get("opencodeConnected")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AssistantModel {
    pub name: String,
    pub reference: String,
    pub free: bool,
}

#[derive(Clone, Default)]
pub struct PluginsModalState {
    pub open: bool,
    pub plugins: PluginsState,
    pub llm: LlmStatus,
    pub progress: Option<DownloadProgress>,
    pub busy_id: Option<String>,
    pub cancelling: bool,
    pub error: Option<String>,
    pub settings: AssistantSettings,
    pub models: Vec<AssistantModel>,
    pub models_error: Option<String>,
    pub token: String,
    pub assistant_busy: bool,
    pub assistant_error: Option<String>,
    pub assistant_bootstrap_requested: bool,
    /// Icono RER-AI cargado una sola vez por la ventana.
    pub icon: Option<egui::TextureHandle>,
}

impl PluginsModalState {
    pub fn has_assistant(&self) -> bool {
        self.plugins.is_installed("local-ai-assistant")
    }
}

/// Lee el estado de plugins y del servidor directamente desde Rust.
pub fn request_snapshot(app: &mut EditorApp) {
    app.plugins_modal.plugins = PluginsState::from_value(&runtime::read_state());
    app.plugins_modal.llm = LlmStatus::from_value(&runtime::llm_snapshot_value());
    if runtime::server_running() {
        app.plugins_modal.assistant_bootstrap_requested = false;
    }
}

/// Ejecuta una acción del modal 100 % en Rust (o en hilos propios del editor).
pub fn action(app: &mut EditorApp, action: &str, args: Value) {
    app.push_log(format!("[Plugins] acción {action}"), false);
    match action {
        "snapshot" => request_snapshot(app),
        "install" => {
            let id = args
                .get("plugin_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            runtime::install_async(app, &id, false);
        }
        "uninstall" => {
            let id = args
                .get("plugin_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            runtime::install_async(app, &id, true);
        }
        "cancel" => {
            runtime::cancel_install();
            app.plugins_modal.cancelling = true;
        }
        "set_enabled" => {
            let id = args
                .get("plugin_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let enabled = args
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let mut state = runtime::read_state();
            if let Some(list) = state.get_mut("enabled").and_then(Value::as_array_mut) {
                if enabled {
                    if !list.iter().any(|value| value.as_str() == Some(id.as_str())) {
                        list.push(json!(id));
                    }
                } else {
                    list.retain(|value| value.as_str() != Some(id.as_str()));
                }
            }
            let _ = runtime::write_state(&state);
            let result = if id == "local-ai-assistant" {
                if enabled {
                    runtime::start_server().map(|_| json!({}))
                } else {
                    runtime::stop_server().map(|_| json!({}))
                }
            } else {
                Ok(json!({}))
            };
            let payload = match result {
                Ok(_) => json!({ "ok": true }),
                Err(error) => json!({ "ok": false, "error": error }),
            };
            apply_result(
                app,
                0,
                "set_enabled",
                &json!({
                    "plugins": runtime::read_state(),
                    "llm": runtime::llm_snapshot_value(),
                    "ok": payload["ok"].clone(),
                }),
            );
        }
        "assistant_bootstrap" => {
            app.plugins_modal.assistant_busy = true;
            runtime::run_action_async(app, "assistant_bootstrap", || {
                let settings = runtime::assistant_settings();
                match runtime::list_models() {
                    Ok(models) => Ok(json!({ "ok": true, "settings": settings, "models": models })),
                    Err(error) => Ok(json!({ "ok": false, "settings": settings, "error": error })),
                }
            });
        }
        "set_model" => {
            let model = args
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or_default();
            runtime::set_model(if model.is_empty() {
                None
            } else {
                Some(model.to_string())
            });
        }
        "connect_go" => {
            app.plugins_modal.assistant_busy = true;
            let key = args
                .get("api_key")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            runtime::run_action_async(app, "connect_go", move || {
                runtime::connect_go(&key).map(|_| json!({ "ok": true }))
            });
        }
        "disconnect_go" => {
            app.plugins_modal.assistant_busy = true;
            runtime::run_action_async(app, "disconnect_go", || {
                runtime::disconnect_go().map(|_| json!({ "ok": true }))
            });
        }
        "start_login" => {
            runtime::run_action_async(app, "start_login", || {
                runtime::start_login().map(|_| json!({ "ok": true }))
            });
        }
        _ => {}
    }
}

/// Aplica un resultado de una operación de plugins a la UI.
pub fn apply_result(app: &mut EditorApp, id: u64, action: &str, result: &Value) {
    let state = &mut app.plugins_modal;
    match action {
        "snapshot" => {
            state.plugins = PluginsState::from_value(result.get("plugins").unwrap_or(&Value::Null));
            state.llm = LlmStatus::from_value(result.get("llm").unwrap_or(&Value::Null));
        }
        "install" | "uninstall" => {
            state.busy_id = None;
            state.cancelling = false;
            state.progress = None;
            if let Some(plugins) = result.get("plugins") {
                state.plugins = PluginsState::from_value(plugins);
            }
            if let Some(llm) = result.get("llm") {
                state.llm = LlmStatus::from_value(llm);
            }
            if result.get("ok").and_then(Value::as_bool) != Some(true)
                && result.get("cancelled").and_then(Value::as_bool) != Some(true)
            {
                state.error = result
                    .get("error")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or_else(|| Some("Install failed".to_string()));
            }
        }
        "set_enabled" => {
            if let Some(plugins) = result.get("plugins") {
                state.plugins = PluginsState::from_value(plugins);
            }
            if let Some(llm) = result.get("llm") {
                state.llm = LlmStatus::from_value(llm);
            }
        }
        "assistant_bootstrap" => {
            state.settings =
                AssistantSettings::from_value(result.get("settings").unwrap_or(&Value::Null));
            if result.get("ok").and_then(Value::as_bool) == Some(true) {
                state.models = result
                    .get("models")
                    .and_then(Value::as_array)
                    .map(|list| {
                        list.iter()
                            .map(|model| AssistantModel {
                                name: model
                                    .get("name")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_string(),
                                reference: model
                                    .get("ref")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_string(),
                                free: model.get("free").and_then(Value::as_bool).unwrap_or(false),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                state.models_error = None;
            } else {
                state.models.clear();
                state.models_error = result
                    .get("error")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or_else(|| Some("Model catalog unavailable".to_string()));
            }
            state.assistant_busy = false;
        }
        "set_model" => {
            if let Some(model) = result.get("model").and_then(Value::as_str) {
                state.settings.selected_model = if model.is_empty() {
                    None
                } else {
                    Some(model.to_string())
                };
            }
        }
        "connect_go" | "disconnect_go" => {
            state.assistant_busy = false;
            if result.get("ok").and_then(Value::as_bool) == Some(true) {
                state.token.clear();
                crate::editor::plugins::action(app, "assistant_bootstrap", json!({}));
            } else {
                state.assistant_error = result
                    .get("error")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or_else(|| Some("Connection failed".to_string()));
            }
        }
        "cancel" => {
            state.cancelling = true;
        }
        _ => {}
    }
    let _ = id;
}
