//! Logs compactos del motor: `[módulo] mensaje` en stderr.
//!
//! El filtro es dinámico por capas: la consola inferior del editor puede
//! activar/desactivar `3D`, `2D`, `física`, `worldgen`, etc. en caliente. Los
//! avisos y errores (`warn`/`error`) se emiten siempre, aunque su capa esté
//! apagada.
//!
//! Para el motor sin editor se puede fijar el filtro inicial con
//! `RER_LOG_LAYERS` (p. ej. `engine,world3d,physics` o `all`).

use std::io::Write;
use std::sync::Once;
use std::sync::atomic::{AtomicU32, Ordering};

use log::{Level, LevelFilter, Log, Metadata, Record};

/// Capas que el editor puede activar o desactivar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum LogLayer {
    /// Arranque, parada y errores del motor.
    Engine = 0,
    /// Escenas, entidades y acciones del editor.
    Editor = 1,
    /// Carga de proyecto/escena.
    Loading = 2,
    /// Render, modelos, luces y ajustes 3D.
    World3d = 3,
    /// Sprites, fondos, cámara y dibujo 2D.
    World2d = 4,
    /// Cuerpos, colisiones y triggers.
    Physics = 5,
    /// Generación de mundo y plugin worldgen.
    Worldgen = 6,
    /// Carga de assets y listas internas (ruidoso).
    Assets = 7,
    /// Métricas, selección, play y eventos internos (ruidoso).
    Runtime = 8,
}

impl LogLayer {
    pub const ALL: [Self; 9] = [
        Self::Engine,
        Self::Editor,
        Self::Loading,
        Self::World3d,
        Self::World2d,
        Self::Physics,
        Self::Worldgen,
        Self::Assets,
        Self::Runtime,
    ];

    #[must_use]
    pub const fn bit(self) -> u32 {
        1 << self as u8
    }

    /// Clave estable para `RER_LOG_LAYERS`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Engine => "engine",
            Self::Editor => "editor",
            Self::Loading => "loading",
            Self::World3d => "3d",
            Self::World2d => "2d",
            Self::Physics => "physics",
            Self::Worldgen => "worldgen",
            Self::Assets => "assets",
            Self::Runtime => "runtime",
        }
    }

    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim().to_ascii_lowercase().as_str() {
            "engine" | "motor" => Some(Self::Engine),
            "editor" => Some(Self::Editor),
            "loading" | "carga" => Some(Self::Loading),
            "3d" | "world3d" | "engine3d" => Some(Self::World3d),
            "2d" | "world2d" | "engine2d" => Some(Self::World2d),
            "physics" | "fisica" | "física" => Some(Self::Physics),
            "worldgen" | "world-gen" => Some(Self::Worldgen),
            "assets" => Some(Self::Assets),
            "runtime" | "ruido" => Some(Self::Runtime),
            _ => None,
        }
    }
}

/// Máscara de capas activas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LogLayers(u32);

impl LogLayers {
    const ALL_MASK: u32 = (1 << LogLayer::ALL.len()) - 1;
    /// Arranque por defecto de los motores: las capas ruidosas quedan apagadas.
    pub const DEFAULT_MASK: u32 = LogLayer::Engine.bit()
        | LogLayer::Editor.bit()
        | LogLayer::Loading.bit()
        | LogLayer::World3d.bit()
        | LogLayer::World2d.bit()
        | LogLayer::Physics.bit()
        | LogLayer::Worldgen.bit();

    #[must_use]
    pub const fn none() -> Self {
        Self(0)
    }

    #[must_use]
    pub const fn all() -> Self {
        Self(Self::ALL_MASK)
    }

    #[must_use]
    pub const fn mask(self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn contains(self, layer: LogLayer) -> bool {
        self.0 & layer.bit() != 0
    }

    pub fn set(&mut self, layer: LogLayer, enabled: bool) {
        if enabled {
            self.0 |= layer.bit();
        } else {
            self.0 &= !layer.bit();
        }
    }

    #[must_use]
    pub fn with(mut self, layer: LogLayer, enabled: bool) -> Self {
        self.set(layer, enabled);
        self
    }

    /// `all`, `none`, o lista separada por comas (`3d,physics,worldgen`).
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let mut layers = Self::none();
        let mut recognized = false;
        for token in value.split(',') {
            let token = token.trim();
            if token.is_empty() {
                continue;
            }
            match token.to_ascii_lowercase().as_str() {
                "all" | "*" => return Some(Self::all()),
                "none" | "off" => return Some(Self::none()),
                _ => {
                    if let Some(layer) = LogLayer::from_key(token) {
                        layers.set(layer, true);
                        recognized = true;
                    }
                }
            }
        }
        recognized.then_some(layers)
    }

    /// Lee `RER_LOG_LAYERS`; `None` si no está definido o no contiene claves.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        std::env::var("RER_LOG_LAYERS")
            .ok()
            .and_then(|value| Self::parse(&value))
    }
}

impl Default for LogLayers {
    fn default() -> Self {
        Self(Self::DEFAULT_MASK)
    }
}

static LAYERS: AtomicU32 = AtomicU32::new(LogLayers::DEFAULT_MASK);
static INIT: Once = Once::new();
static LOGGER: EngineLogger = EngineLogger;

struct EngineLogger;

impl Log for EngineLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        // Nunca silenciar avisos/errores: las capas controlan info/debug/trace.
        if metadata.level() <= Level::Warn {
            return true;
        }
        layer_for_target(metadata.target()).is_some_and(|layer| layers().contains(layer))
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let stderr = std::io::stderr();
        let mut handle = stderr.lock();
        let _ = writeln!(
            handle,
            "[{}] {}",
            record.module_path().unwrap_or(record.target()),
            record.args(),
        );
    }

    fn flush(&self) {
        let _ = std::io::stderr().flush();
    }
}

/// Inicializa el logger una sola vez y aplica `RER_LOG_LAYERS` si existe.
pub fn init(default_layers: LogLayers) {
    let layers = LogLayers::from_env().unwrap_or(default_layers);
    LAYERS.store(layers.mask(), Ordering::Relaxed);
    INIT.call_once(|| {
        let _ = log::set_logger(&LOGGER);
        log::set_max_level(LevelFilter::Trace);
    });
}

#[must_use]
pub fn layers() -> LogLayers {
    LogLayers(LAYERS.load(Ordering::Relaxed))
}

pub fn set_layers(layers: LogLayers) {
    LAYERS.store(layers.mask(), Ordering::Relaxed);
}

pub fn set_layer(layer: LogLayer, enabled: bool) {
    let mut current = layers();
    current.set(layer, enabled);
    set_layers(current);
}

/// Mapea el `target` de `log` a la capa visible en la consola.
fn layer_for_target(target: &str) -> Option<LogLayer> {
    if target.contains("worldgen") {
        return Some(LogLayer::Worldgen);
    }
    if target.contains("physics") {
        return Some(LogLayer::Physics);
    }
    if target.contains("load_proyect") || target.contains("load_project") {
        return Some(LogLayer::Loading);
    }
    if target.contains("model_asset") || target.contains("static_model_cache") {
        return Some(LogLayer::Assets);
    }
    if target.contains("play_character")
        || target.contains("play_controller")
        || target.contains("fps_camera")
        || target.contains("animation")
        || target.contains("metrics")
        || target.contains("tick")
    {
        return Some(LogLayer::Runtime);
    }
    if target.starts_with("rer_engine::engine3d") || target.starts_with("rer_engine_core::vulkan") {
        return Some(LogLayer::World3d);
    }
    if target.starts_with("rer_engine::engine2d") {
        return Some(LogLayer::World2d);
    }
    if target.starts_with("rer_engine::editor") {
        return Some(LogLayer::Editor);
    }
    if target.starts_with("rer_engine_core") || target.starts_with("rer_engine::") {
        return Some(LogLayer::Engine);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_layers_include_engine_but_not_noise() {
        let layers = LogLayers::default();
        assert!(layers.contains(LogLayer::Engine));
        assert!(layers.contains(LogLayer::World3d));
        assert!(layers.contains(LogLayer::Worldgen));
        assert!(!layers.contains(LogLayer::Assets));
        assert!(!layers.contains(LogLayer::Runtime));
    }

    #[test]
    fn layer_list_parses_aliases_and_all() {
        let layers = LogLayers::parse("motor,3d,world-gen,ruido").unwrap();
        assert!(layers.contains(LogLayer::Engine));
        assert!(layers.contains(LogLayer::World3d));
        assert!(layers.contains(LogLayer::Worldgen));
        assert!(layers.contains(LogLayer::Runtime));
        assert!(!layers.contains(LogLayer::World2d));

        assert_eq!(LogLayers::parse("all"), Some(LogLayers::all()));
        assert_eq!(LogLayers::parse("none"), Some(LogLayers::none()));
        assert_eq!(LogLayers::parse("  "), None);
    }

    #[test]
    fn targets_map_to_the_right_layer() {
        assert_eq!(
            layer_for_target("rer_engine::engine3d::config_3d::physics_3d"),
            Some(LogLayer::Physics)
        );
        assert_eq!(
            layer_for_target("rer_engine::engine3d::config_3d::worldgen::mod"),
            Some(LogLayer::Worldgen)
        );
        assert_eq!(
            layer_for_target("rer_engine::engine2d::config_2d"),
            Some(LogLayer::World2d)
        );
        assert_eq!(
            layer_for_target("rer_engine::editor::ui"),
            Some(LogLayer::Editor)
        );
        assert_eq!(
            layer_for_target("rer_engine_core::vulkan"),
            Some(LogLayer::World3d)
        );
    }
}
