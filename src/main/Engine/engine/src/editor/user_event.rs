//! Eventos que llegan al event loop del editor desde otros hilos.

use serde_json::Value;

/// Eventos internos del editor (motor, Electron y guardado).
pub enum UserEvent {
    /// Línea JSON emitida por el motor.
    EngineEvent(Value),
    /// Mensaje JSON enviado por Electron (solo puente de la UI antigua).
    FromElectron(Value),
    /// El hilo de escritura del `.save` terminó.
    SaveFinished(Result<crate::editor::save::SaveSuccess, String>),
    /// Un guardado excedió el tiempo máximo esperando el estado/snapshot.
    SaveTimeout(u64),
    /// Pide sincronizar los ajustes de mundo desde el motor.
    WorldSync,
    /// El motor 3D terminó de construirse en el hilo de carga.
    EngineReady(Box<crate::engine3d::engine::State>),
    /// El motor 2D terminó de construirse en el hilo de carga.
    Engine2dReady(Box<crate::engine2d::engine::State>),
    /// El motor no pudo arrancar.
    EngineFailed(String),
}
