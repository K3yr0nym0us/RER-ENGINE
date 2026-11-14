//! `rer_worldgen` — generación procedural de mundos 3D para RER-ENGINE.
//!
//! Crate independiente del motor (lógica pura, sin Vulkan/egui/winit):
//!
//! - [`config`]: esquema de configuración con límites de dimensiones, resolución,
//!   features activables y cantidades máximas por elemento.
//! - [`seed`]: seeds deterministas por dominio y RNG determinista.
//! - [`noise`]: puerto propio de FastNoiseLite (MIT).
//! - `field` (F2+): heightfield + SDF + carving (terreno, agua, cuevas).
//! - `mesher` (F2+): Marching Cubes con normales por gradiente.
//! - `chunks` (F2+): pipeline de chunks con Y-layer pruning.
//! - `plugin_ffi` (F2+): fachada C ABI para el plugin instalable (`rer_worldgen_plugin.dll`).
//!
//! Contrato de determinismo: toda la generación es función pura de
//! `(seed, config)`. No hay estado global, relojes ni entropía externa.
//! Solo se permiten operaciones de punto flotante de orden fijo (sin
//! reducciones paralelas no deterministas).

pub mod biomes;
pub mod chunks;
pub mod config;
pub mod field;
pub mod mesher;
pub mod noise;
pub mod planner;
pub mod plugin_ffi;
pub mod scatter;
pub mod seed;
pub mod splat;
