//! Gestión de plugins 100 % Rust: estado en disco, descargas, instalación y
//! servidor OpenCode. Sustituye a los handlers `plugins:*` de Electron main.
//!
//! Las operaciones largas se ejecutan en un hilo y publican su avance y result
//! through `UserEvent::PluginProgress` / `UserEvent::PluginFinished`.

use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc, LazyLock, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use base64::Engine as _;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::user_event::UserEvent;

const WORLDGEN_ID: &str = "world-gen";
const ASSISTANT_ID: &str = "local-ai-assistant";
const WORLDGEN_URL: &str = "https://github.com/K3yr0nym0us/RER-ENGINE/releases/download/worldgen-v1.0.0/rer-worldgen-1.0.0-win-x64.zip";
const WORLDGEN_SHA256: &str = "9a9819609643995d5433db9e124f7556ba9bdf604648c26a7d7db20a26516249";
const OPENCODE_URL: &str =
    "https://opencode.ai/update/api/latest/cli/opencode/opencode-windows-x64.zip";
const OPENCODE_SIZE: u64 = 124_800_929;

#[derive(Default)]
struct RuntimeState {
    child: Option<Child>,
    endpoint: Option<(String, String)>,
    status: String,
    error: Option<String>,
    block_new_server: bool,
    cancel: Arc<AtomicBool>,
}

static RUNTIME: LazyLock<Mutex<RuntimeState>> = LazyLock::new(|| {
    Mutex::new(RuntimeState {
        status: "idle".to_string(),
        ..RuntimeState::default()
    })
});

// ---------------------------------------------------------------------------
// Rutas
// ---------------------------------------------------------------------------

/// `app.getPath('userData')`: `%APPDATA%/RER-ENGINE` (o `rer-engine` en dev).
pub fn user_data_dir() -> PathBuf {
    let appdata = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    for name in ["RER-ENGINE", "rer-engine"] {
        let dir = appdata.join(name);
        if dir.exists() {
            return dir;
        }
    }
    appdata.join("RER-ENGINE")
}

pub fn plugins_dir() -> PathBuf {
    user_data_dir().join("plugins")
}

pub fn plugin_dir(id: &str) -> PathBuf {
    plugins_dir().join(id)
}

fn state_path() -> PathBuf {
    plugins_dir().join("state.json")
}

fn assistant_bin_dir() -> PathBuf {
    plugin_dir(ASSISTANT_ID).join("bin")
}

fn opencode_exe() -> PathBuf {
    assistant_bin_dir().join("opencode.exe")
}

fn workspace_dir() -> PathBuf {
    plugin_dir(ASSISTANT_ID).join("workspace")
}

fn xdg_dir(name: &str) -> PathBuf {
    plugin_dir(ASSISTANT_ID).join("xdg").join(name)
}

fn service_json() -> PathBuf {
    xdg_dir("state").join("opencode").join("service.json")
}

fn docs_ai_source() -> Option<PathBuf> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../docs/ai");
    [repo, PathBuf::from("docs/ai")]
        .into_iter()
        .find(|dir| dir.is_dir())
}

// ---------------------------------------------------------------------------
// Estado en disco
// ---------------------------------------------------------------------------

pub fn read_state() -> Value {
    fs::read_to_string(state_path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| json!({ "installed": {}, "enabled": [] }))
}

pub fn write_state(state: &Value) -> Result<(), String> {
    let path = state_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(
        &path,
        serde_json::to_string_pretty(state).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

pub fn worldgen_library_path() -> Option<String> {
    let state = read_state();
    let record = state.get("installed")?.get(WORLDGEN_ID)?;
    let path = record.get("libraryPath")?.as_str()?;
    Path::new(path).is_file().then(|| path.to_string())
}

pub fn preferences(plugin_id: &str) -> Value {
    read_state()
        .get("preferences")
        .and_then(|preferences| preferences.get(plugin_id))
        .cloned()
        .unwrap_or_else(|| json!({}))
}

pub fn update_preferences(plugin_id: &str, patch: Value) {
    let mut state = read_state();
    let preferences = state
        .as_object_mut()
        .expect("estado es objeto")
        .entry("preferences")
        .or_insert_with(|| json!({}));
    let current = preferences
        .as_object_mut()
        .expect("preferences es objeto")
        .entry(plugin_id)
        .or_insert_with(|| json!({}));
    if let Some(map) = current.as_object_mut() {
        for (key, value) in patch.as_object().expect("patch es objeto") {
            map.insert(key.clone(), value.clone());
        }
    }
    let _ = write_state(&state);
}

// ---------------------------------------------------------------------------
// Progreso / resultados al hilo de la UI
// ---------------------------------------------------------------------------

fn progress(
    proxy: &winit::event_loop::EventLoopProxy<UserEvent>,
    plugin_id: &str,
    phase: &str,
    percent: u32,
    received: u64,
    total: u64,
) {
    let _ = proxy.send_event(UserEvent::PluginProgress(json!({
        "pluginId": plugin_id,
        "phase": phase,
        "overallPercent": percent,
        "bytesOverallReceived": received,
        "bytesOverallTotal": total,
    })));
}

fn finish(proxy: &winit::event_loop::EventLoopProxy<UserEvent>, action: &str, result: Value) {
    let _ = proxy.send_event(UserEvent::PluginFinished(json!({
        "action": action,
        "result": result,
    })));
}

/// Aplica un resultado de instalación/acción al estado de la UI (en el hilo principal).
pub fn apply_finished(app: &mut super::app::EditorApp, payload: &Value) {
    let action = payload.get("action").and_then(Value::as_str).unwrap_or("");
    let result = payload.get("result").cloned().unwrap_or(Value::Null);
    super::plugins::apply_result(app, 0, action, &result);
}

pub fn apply_progress(app: &mut super::app::EditorApp, payload: &Value) {
    app.plugins_modal.progress = Some(super::plugins::DownloadProgress::from_value(payload));
}

// ---------------------------------------------------------------------------
// Descarga e instalación
// ---------------------------------------------------------------------------

fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(hex::encode(hasher.finalize()))
}

fn download(
    url: &str,
    destination: &Path,
    plugin_id: &str,
    phase: &str,
    expected: u64,
    cancel: &Arc<AtomicBool>,
    proxy: &winit::event_loop::EventLoopProxy<UserEvent>,
) -> Result<(), String> {
    let response = ureq::get(url)
        .call()
        .map_err(|error| format!("Download failed: {error}"))?;
    let total = response
        .header("Content-Length")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(expected);
    let mut reader = response.into_reader();
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut file = fs::File::create(destination).map_err(|error| error.to_string())?;
    let mut buffer = [0u8; 64 * 1024];
    let mut received = 0u64;
    let mut last_report = Instant::now();
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("__CANCELLED__".to_string());
        }
        let read = reader
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])
            .map_err(|error| error.to_string())?;
        received += read as u64;
        if last_report.elapsed() >= Duration::from_millis(120) {
            last_report = Instant::now();
            let percent = if total > 0 {
                ((received * 100) / total).min(99) as u32
            } else {
                0
            };
            progress(
                proxy,
                plugin_id,
                phase,
                percent,
                received,
                total.max(expected),
            );
        }
    }
    progress(
        proxy,
        plugin_id,
        phase,
        100,
        received.max(expected),
        total.max(expected),
    );
    Ok(())
}

fn install_worldgen(
    cancel: Arc<AtomicBool>,
    proxy: winit::event_loop::EventLoopProxy<UserEvent>,
) -> Result<(), String> {
    let dir = plugin_dir(WORLDGEN_ID);
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let zip_path = dir.join("world-gen-1.0.0.zip");
    download(
        WORLDGEN_URL,
        &zip_path,
        WORLDGEN_ID,
        "artifact",
        201_366,
        &cancel,
        &proxy,
    )?;
    if cancel.load(Ordering::Relaxed) {
        return Err("__CANCELLED__".to_string());
    }
    let actual = sha256_file(&zip_path)?;
    if actual != WORLDGEN_SHA256 {
        let _ = fs::remove_file(&zip_path);
        return Err(format!(
            "SHA-256 mismatch: expected {WORLDGEN_SHA256}, got {actual}"
        ));
    }
    progress(&proxy, WORLDGEN_ID, "extracting", 100, 201_366, 201_366);
    let archive = fs::File::open(&zip_path).map_err(|error| error.to_string())?;
    let mut zip = zip::ZipArchive::new(archive).map_err(|error| error.to_string())?;
    let mut dll = zip
        .by_name("rer_worldgen.dll")
        .map_err(|_| "rer_worldgen.dll not found inside the artifact archive".to_string())?;
    let library = dir.join("rer_worldgen.dll");
    let mut output = fs::File::create(&library).map_err(|error| error.to_string())?;
    std::io::copy(&mut dll, &mut output).map_err(|error| error.to_string())?;
    drop(dll);

    // Fotos de terreno (opcionales).
    let mut photos: Vec<(String, Vec<u8>)> = Vec::new();
    for index in 0..zip.len() {
        let mut entry = match zip.by_index(index) {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        if entry.is_dir() {
            continue;
        }
        let name = match entry.name() {
            Ok(name) => name.to_string(),
            Err(_) => continue,
        };
        if let Some(relative) = terrain_zip_rel_path(&name) {
            let mut data = Vec::new();
            if entry.read_to_end(&mut data).is_ok() {
                photos.push((relative, data));
            }
        }
    }
    if !photos.is_empty() {
        let terrain = dir.join("terrain");
        fs::create_dir_all(&terrain).map_err(|error| error.to_string())?;
        for (relative, data) in photos {
            fs::write(dir.join(relative), data).map_err(|error| error.to_string())?;
        }
    }
    drop(zip);
    let _ = fs::remove_file(&zip_path);

    let mut state = read_state();
    let installed = state
        .as_object_mut()
        .expect("estado es objeto")
        .entry("installed")
        .or_insert_with(|| json!({}));
    installed
        .as_object_mut()
        .expect("installed es objeto")
        .insert(
            WORLDGEN_ID.to_string(),
            json!({
                "version": "1.0.0",
                "libraryPath": library.to_string_lossy(),
                "installedAt": now_iso(),
            }),
        );
    let enabled = state
        .as_object_mut()
        .expect("estado es objeto")
        .entry("enabled")
        .or_insert_with(|| json!([]));
    if let Some(list) = enabled.as_array_mut()
        && !list.iter().any(|id| id.as_str() == Some(WORLDGEN_ID))
    {
        list.push(json!(WORLDGEN_ID));
    }
    write_state(&state)
}

fn terrain_zip_rel_path(name: &str) -> Option<String> {
    let normalized = name.replace('\\', "/");
    let file = normalized.rsplit('/').next()?;
    if file.to_ascii_lowercase().ends_with(".jpg") || file.to_ascii_lowercase().ends_with(".jpeg") {
        Some(format!("terrain/{file}"))
    } else {
        None
    }
}

fn install_assistant(
    cancel: Arc<AtomicBool>,
    proxy: winit::event_loop::EventLoopProxy<UserEvent>,
) -> Result<(), String> {
    if docs_ai_source().is_none() {
        return Err("docs/ai bundle not found; cannot prepare assistant workspace".to_string());
    }
    let bin_dir = assistant_bin_dir();
    fs::create_dir_all(&bin_dir).map_err(|error| error.to_string())?;
    let exe = opencode_exe();
    if !exe.is_file() {
        let zip_path = bin_dir.join("opencode.zip");
        download(
            OPENCODE_URL,
            &zip_path,
            ASSISTANT_ID,
            "opencode",
            OPENCODE_SIZE,
            &cancel,
            &proxy,
        )?;
        if cancel.load(Ordering::Relaxed) {
            return Err("__CANCELLED__".to_string());
        }
        progress(
            &proxy,
            ASSISTANT_ID,
            "extracting",
            100,
            OPENCODE_SIZE,
            OPENCODE_SIZE,
        );
        let archive = fs::File::open(&zip_path).map_err(|error| error.to_string())?;
        let mut zip = zip::ZipArchive::new(archive).map_err(|error| error.to_string())?;
        zip.extract(&bin_dir).map_err(|error| error.to_string())?;
        drop(zip);
        let _ = fs::remove_file(&zip_path);
        if !exe.is_file() {
            return Err("opencode.exe was not extracted from runtime archive".to_string());
        }
    }
    progress(
        &proxy,
        ASSISTANT_ID,
        "preparing",
        100,
        OPENCODE_SIZE,
        OPENCODE_SIZE,
    );
    prepare_workspace()?;

    // Preferencias obsoletas de otros proveedores.
    let prefs = preferences(ASSISTANT_ID);
    if prefs
        .get("selectedModel")
        .and_then(Value::as_str)
        .is_some_and(|model| !model.starts_with("opencode"))
    {
        update_preferences(ASSISTANT_ID, json!({ "selectedModel": null }));
    }

    let mut state = read_state();
    let installed = state
        .as_object_mut()
        .expect("estado es objeto")
        .entry("installed")
        .or_insert_with(|| json!({}));
    installed
        .as_object_mut()
        .expect("installed es objeto")
        .insert(
            ASSISTANT_ID.to_string(),
            json!({
                "version": "2.0.0",
                "opencodePath": exe.to_string_lossy(),
                "workspaceDir": workspace_dir().to_string_lossy(),
                "installedAt": now_iso(),
            }),
        );
    let enabled = state
        .as_object_mut()
        .expect("estado es objeto")
        .entry("enabled")
        .or_insert_with(|| json!([]));
    if let Some(list) = enabled.as_array_mut()
        && !list.iter().any(|id| id.as_str() == Some(ASSISTANT_ID))
    {
        list.push(json!(ASSISTANT_ID));
    }
    write_state(&state)
}

fn now_iso() -> String {
    // ISO-8601 UTC sin dependencias de fecha: segundos desde epoch + formateo simple.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

/// Ejecuta una acción HTTP en un hilo y publica el resultado en la UI.
pub fn run_action_async<F>(app: &mut super::app::EditorApp, action: &str, task: F)
where
    F: FnOnce() -> Result<Value, String> + Send + 'static,
{
    let Some(proxy) = app.proxy.clone() else {
        return;
    };
    let action_name = action.to_string();
    std::thread::Builder::new()
        .name(format!("plugin-{action_name}"))
        .spawn(move || {
            let result = match task() {
                Ok(value) => value,
                Err(error) => json!({ "ok": false, "error": error }),
            };
            finish(&proxy, &action_name, result);
        })
        .expect("hilo de acción de plugin");
}

/// Instala o desinstala un plugin en un hilo, publicando progreso y resultado.
pub fn install_async(app: &mut super::app::EditorApp, plugin_id: &str, uninstall: bool) {
    let Some(proxy) = app.proxy.clone() else {
        return;
    };
    let id = plugin_id.to_string();
    let cancel = RUNTIME.lock().expect("runtime").cancel.clone();
    cancel.store(false, Ordering::Relaxed);
    let action = if uninstall { "uninstall" } else { "install" };
    std::thread::Builder::new()
        .name(format!("plugin-{action}"))
        .spawn(move || {
            let outcome = if uninstall {
                uninstall_plugin(&id).map(|_| json!({}))
            } else {
                match id.as_str() {
                    WORLDGEN_ID => install_worldgen(cancel, proxy.clone()).map(|_| json!({})),
                    ASSISTANT_ID => install_assistant(cancel, proxy.clone()).map(|_| json!({})),
                    _ => Err("Unknown plugin".to_string()),
                }
            };
            let result = match outcome {
                Ok(_) => json!({
                    "ok": true,
                    "plugins": read_state(),
                    "llm": llm_snapshot_value(),
                }),
                Err(error) if error == "__CANCELLED__" => json!({ "ok": false, "cancelled": true }),
                Err(error) => json!({ "ok": false, "error": error }),
            };
            finish(&proxy, action, result);
        })
        .expect("hilo de instalación");
}

fn uninstall_plugin(id: &str) -> Result<(), String> {
    let dir = plugin_dir(id);
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|error| {
            if error.kind() == std::io::ErrorKind::PermissionDenied {
                "The plugin DLL is still loaded by the engine. Disable it and try again."
                    .to_string()
            } else {
                error.to_string()
            }
        })?;
    }
    let mut state = read_state();
    if let Some(installed) = state.get_mut("installed").and_then(Value::as_object_mut) {
        installed.remove(id);
    }
    if let Some(enabled) = state.get_mut("enabled").and_then(Value::as_array_mut) {
        enabled.retain(|item| item.as_str() != Some(id));
    }
    write_state(&state)
}

pub fn cancel_install() {
    if let Ok(runtime) = RUNTIME.lock() {
        runtime.cancel.store(true, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// Workspace del asistente
// ---------------------------------------------------------------------------

fn prepare_workspace() -> Result<(), String> {
    let workspace = workspace_dir();
    fs::create_dir_all(&workspace).map_err(|error| error.to_string())?;
    let files = [
        ("opencode.jsonc", PROJECT_CONFIG),
        (".opencode/agents/rer-assistant-es.md", AGENT_ES),
        (".opencode/agents/rer-assistant-en.md", AGENT_EN),
        (".opencode/plugins/rer-ui/index.ts", RER_UI_PLUGIN),
    ];
    for (relative, content) in files {
        let path = workspace.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(
            path,
            content.replace("__LOG__", &ui_actions_log().to_string_lossy()),
        )
        .map_err(|error| error.to_string())?;
    }
    fs::write(workspace.join("ui-actions.log"), "").map_err(|error| error.to_string())?;
    if let Some(source) = docs_ai_source() {
        let target = workspace.join("docs/ai");
        if target.exists() {
            let _ = fs::remove_dir_all(&target);
        }
        copy_dir(&source, &target)?;
    }
    let global = xdg_dir("config").join("opencode/opencode.json");
    if let Some(parent) = global.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(global, GLOBAL_CONFIG).map_err(|error| error.to_string())
}

fn ui_actions_log() -> PathBuf {
    workspace_dir().join("ui-actions.log")
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(from).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

const GLOBAL_CONFIG: &str = r#"{
  "$schema": "https://opencode.ai/config.json",
  "update": "disable",
  "snapshots": false,
  "formatter": false
}
"#;

const PROJECT_CONFIG: &str = r#"{
  "$schema": "https://opencode.ai/config.json",
  "default_agent": "rer-assistant-es",
  "update": "disable",
  "snapshots": false,
  "formatter": false,
  "plugins": ["./.opencode/plugins/rer-ui"],
  "permissions": [
    { "action": "*", "resource": "*", "effect": "deny" },
    { "action": "read", "resource": "docs/ai/**", "effect": "allow" },
    { "action": "glob", "resource": "docs/ai/**", "effect": "allow" },
    { "action": "grep", "resource": "docs/ai/**", "effect": "allow" },
    { "action": "rer_ui_*", "resource": "*", "effect": "allow" }
  ]
}
"#;

const AGENT_ES: &str = "---\ndescription: Asistente RER-AI del editor RER-ENGINE (solo docs/ai)\nmode: primary\nsteps: 10\n---\n\nEres RER-AI, el asistente oficial del editor RER-ENGINE. Responde siempre en español y usa solo la documentación en docs/ai.\n";
const AGENT_EN: &str = "---\ndescription: RER-AI assistant for the RER-ENGINE editor (docs/ai only)\nmode: primary\nsteps: 10\n---\n\nYou are RER-AI, the official assistant of the RER-ENGINE editor. Always reply in English and use only the documentation in docs/ai.\n";

// ---------------------------------------------------------------------------
// Servidor OpenCode
// ---------------------------------------------------------------------------

fn basic_auth(password: &str) -> String {
    let credentials = format!("opencode:{password}");
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(credentials)
    )
}

fn read_service_registration() -> Option<(String, String)> {
    let value: Value = serde_json::from_str(&fs::read_to_string(service_json()).ok()?).ok()?;
    let url = value.get("url")?.as_str()?.to_string();
    let password = value.get("password")?.as_str()?.to_string();
    Some((url, password))
}

fn probe(endpoint: &(String, String)) -> bool {
    let url = format!("{}/api/info", endpoint.0.trim_end_matches('/'));
    ureq::get(&url)
        .timeout(Duration::from_secs(2))
        .set("Authorization", &basic_auth(&endpoint.1))
        .call()
        .map(|response| response.status() < 500)
        .unwrap_or(false)
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .ok()
        .and_then(|listener| listener.local_addr().ok())
        .map(|address| address.port())
        .unwrap_or(4096)
}

/// Estado del servidor para la tarjeta del asistente.
pub fn llm_snapshot_value() -> Value {
    let (status, error, running) = {
        let runtime = RUNTIME.lock().expect("runtime");
        (
            runtime.status.clone(),
            runtime.error.clone(),
            runtime.endpoint.is_some(),
        )
    };
    let installing = false;
    json!({
        "status": if installing { "downloading" } else { status.as_str() },
        "error": error,
        "enabled": is_enabled(ASSISTANT_ID),
        "installed": is_installed(ASSISTANT_ID),
        "running": running,
    })
}

fn is_installed(id: &str) -> bool {
    let state = read_state();
    let Some(record) = state
        .get("installed")
        .and_then(|installed| installed.get(id))
    else {
        return false;
    };
    if id == WORLDGEN_ID {
        return record
            .get("libraryPath")
            .and_then(Value::as_str)
            .is_some_and(|path| Path::new(path).is_file());
    }
    record
        .get("opencodePath")
        .and_then(Value::as_str)
        .is_some_and(|path| Path::new(path).is_file())
        && record
            .get("workspaceDir")
            .and_then(Value::as_str)
            .is_some_and(|path| Path::new(path).is_dir())
}

fn is_enabled(id: &str) -> bool {
    is_installed(id)
        && (id == WORLDGEN_ID
            || read_state()
                .get("enabled")
                .and_then(Value::as_array)
                .is_some_and(|list| list.iter().any(|value| value.as_str() == Some(id))))
}

pub fn start_server() -> Result<(), String> {
    let mut runtime = RUNTIME.lock().expect("runtime");
    if let Some(endpoint) = runtime.endpoint.clone()
        && probe(&endpoint)
    {
        runtime.status = "running".to_string();
        return Ok(());
    }
    if runtime.block_new_server {
        return Err(runtime
            .error
            .clone()
            .unwrap_or_else(|| "OpenCode server did not exit in time".to_string()));
    }
    let exe = opencode_exe();
    if !exe.is_file() {
        return Err("OpenCode runtime not installed".to_string());
    }
    let port = free_port();
    let child = Command::new(&exe)
        .args([
            "serve",
            "--service",
            "--hostname",
            "127.0.0.1",
            "--port",
            &port.to_string(),
        ])
        .current_dir(assistant_bin_dir())
        .env("XDG_CONFIG_HOME", xdg_dir("config"))
        .env("XDG_DATA_HOME", xdg_dir("data"))
        .env("XDG_CACHE_HOME", xdg_dir("cache"))
        .env("XDG_STATE_HOME", xdg_dir("state"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    runtime.child = Some(child);
    runtime.status = "starting".to_string();
    runtime.error = None;
    drop(runtime);

    let deadline = Instant::now() + Duration::from_secs(90);
    while Instant::now() < deadline {
        if let Some(registration) = read_service_registration()
            && probe(&registration)
        {
            let mut runtime = RUNTIME.lock().expect("runtime");
            runtime.endpoint = Some(registration);
            runtime.status = "running".to_string();
            runtime.error = None;
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    let _ = stop_server();
    let mut runtime = RUNTIME.lock().expect("runtime");
    runtime.status = "error".to_string();
    runtime.error = Some("OpenCode server did not become reachable in time".to_string());
    Err("OpenCode server did not become reachable in time".to_string())
}

pub fn stop_server() -> Result<(), String> {
    let mut runtime = RUNTIME.lock().expect("runtime");
    if let Some(mut child) = runtime.child.take() {
        #[cfg(windows)]
        {
            let system_root =
                std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
            let taskkill = Path::new(&system_root).join("System32/taskkill.exe");
            let _ = Command::new(taskkill)
                .args(["/PID", &child.id().to_string(), "/T", "/F"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
        }
        #[cfg(not(windows))]
        {
            let _ = child.kill();
        }
        let _ = child.wait();
    }
    runtime.endpoint = None;
    runtime.status = "idle".to_string();
    runtime.error = None;
    Ok(())
}

pub fn server_running() -> bool {
    let runtime = RUNTIME.lock().expect("runtime");
    runtime.status == "running" || runtime.status == "starting"
}

fn api(
    endpoint: &(String, String),
    path: &str,
    method: &str,
    body: Option<Value>,
) -> Result<Value, String> {
    let url = format!("{}{path}", endpoint.0.trim_end_matches('/'));
    let mut request = match method {
        "POST" => ureq::post(&url),
        _ => ureq::get(&url),
    };
    request = request
        .timeout(Duration::from_secs(30))
        .set("Authorization", &basic_auth(&endpoint.1))
        .set("Content-Type", "application/json");
    let response = match body {
        Some(value) => request.send_string(&value.to_string()),
        None => request.call(),
    }
    .map_err(|error| format!("HTTP error: {error}"))?;
    let text = response.into_string().map_err(|error| error.to_string())?;
    serde_json::from_str(&text).map_err(|_| "Invalid JSON response".to_string())
}

fn endpoint() -> Option<(String, String)> {
    RUNTIME.lock().expect("runtime").endpoint.clone()
}

pub fn assistant_settings() -> Value {
    let prefs = preferences(ASSISTANT_ID);
    let go_connected = endpoint()
        .and_then(|endpoint| api(&endpoint, "/api/go", "GET", None).ok())
        .and_then(|value| value.get("connected").and_then(Value::as_bool))
        .unwrap_or_else(|| {
            prefs
                .get("goConnected")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        });
    json!({
        "selectedModel": prefs.get("selectedModel").cloned().unwrap_or(Value::Null),
        "goConnected": go_connected,
        "opencodeConnected": false,
    })
}

pub fn list_models() -> Result<Vec<Value>, String> {
    let endpoint = endpoint().ok_or_else(|| "OpenCode server is not running".to_string())?;
    let response = api(&endpoint, "/api/model", "GET", None)?;
    let models = response
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| "Model catalog unavailable".to_string())?;
    let mut options: Vec<Value> = models
        .iter()
        .filter(|model| model.get("enabled").and_then(Value::as_bool) != Some(false))
        .filter(|model| model.get("type").and_then(Value::as_str) == Some("chat"))
        .filter(|model| {
            model
                .get("capabilities")
                .and_then(|capabilities| capabilities.get("tools"))
                .and_then(Value::as_bool)
                == Some(true)
        })
        .filter_map(|model| {
            let provider = model.get("providerID")?.as_str()?;
            if provider != "opencode" && provider != "opencode-go" {
                return None;
            }
            let id = model.get("id")?.as_str()?;
            let free = model
                .get("cost")
                .and_then(Value::as_array)
                .and_then(|cost| cost.first())
                .and_then(|entry| entry.get("input"))
                .and_then(Value::as_f64)
                .is_some_and(|value| value == 0.0);
            Some(json!({
                "ref": format!("{provider}/{id}"),
                "name": model.get("name").and_then(Value::as_str).unwrap_or(id),
                "free": free,
            }))
        })
        .collect();
    options.sort_by(|a, b| {
        let free_a = a.get("free").and_then(Value::as_bool).unwrap_or(false);
        let free_b = b.get("free").and_then(Value::as_bool).unwrap_or(false);
        free_b.cmp(&free_a).then_with(|| {
            a.get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .cmp(b.get("name").and_then(Value::as_str).unwrap_or(""))
        })
    });
    Ok(options)
}

pub fn set_model(model: Option<String>) {
    update_preferences(ASSISTANT_ID, json!({ "selectedModel": model }));
}

pub fn connect_go(api_key: &str) -> Result<(), String> {
    let endpoint = endpoint().ok_or_else(|| "OpenCode server is not running".to_string())?;
    if api_key.trim().len() < 8 {
        return Err("Enter a valid OpenCode Go token".to_string());
    }
    let response = api(
        &endpoint,
        "/api/go/connect",
        "POST",
        Some(json!({ "apiKey": api_key.trim() })),
    )?;
    let ok = response.get("ok").and_then(Value::as_bool).unwrap_or(true);
    update_preferences(ASSISTANT_ID, json!({ "goConnected": ok }));
    if ok {
        Ok(())
    } else {
        Err(response
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("Connection failed")
            .to_string())
    }
}

pub fn disconnect_go() -> Result<(), String> {
    let endpoint = endpoint().ok_or_else(|| "OpenCode server is not running".to_string())?;
    api(&endpoint, "/api/go/disconnect", "POST", Some(json!({})))?;
    update_preferences(ASSISTANT_ID, json!({ "goConnected": false }));
    Ok(())
}

pub fn start_login() -> Result<(), String> {
    let endpoint = endpoint().ok_or_else(|| "OpenCode server is not running".to_string())?;
    let response = api(&endpoint, "/api/account/login", "POST", Some(json!({})))?;
    if let Some(url) = response.get("url").and_then(Value::as_str) {
        #[cfg(windows)]
        let _ = Command::new("cmd")
            .args(["/C", "start", "", url])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        #[cfg(not(windows))]
        let _ = Command::new("xdg-open").arg(url).spawn();
    }
    Ok(())
}

const RER_UI_PLUGIN: &str = r#"import { appendFile } from 'node:fs/promises'
import { Plugin } from '@opencode/plugin'

const LOG_FILE = '__LOG__'

async function record(entry) {
  await appendFile(LOG_FILE, JSON.stringify({ at: new Date().toISOString(), ...entry }) + '\n', 'utf8')
  return 'OK'
}

export default Plugin.define({
  id: 'rer-ui',
  async setup(ctx) {
    await ctx.tool.transform((editor) => {
      editor.namespace({ name: 'rer_ui', description: 'Acciones de UI del editor RER-ENGINE' })
      editor.add({
        name: 'open_accordion',
        description: 'Abre un acordeón del panel izquierdo del editor RER-ENGINE',
        input: { type: 'object', properties: { accordionKey: { type: 'string' } }, required: ['accordionKey'] },
        options: { namespace: 'rer_ui', codemode: false },
        execute: async (input) => ({ content: await record({ tool: 'open_accordion', accordionKey: input.accordionKey }) }),
      })
      editor.add({
        name: 'highlight',
        description: 'Resalta un control de la UI del editor RER-ENGINE',
        input: { type: 'object', properties: { targetId: { type: 'string' } }, required: ['targetId'] },
        options: { namespace: 'rer_ui', codemode: false },
        execute: async (input) => ({ content: await record({ tool: 'highlight', targetId: input.targetId }) }),
      })
    })
  },
})
"#;
