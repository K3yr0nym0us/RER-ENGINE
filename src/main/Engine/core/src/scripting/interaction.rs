//! Estado de interacción compartido entre el motor 3D y los scripts Rhai.
//!
//! El motor lo actualiza cada frame en play (objetivo bajo la mira, distancia y
//! flanco de pulsación) y los scripts lo leen a través de la API `engine.*`.

/// Estado de la interacción en play.
#[derive(Debug, Clone, Default)]
pub struct InteractionState {
    /// Entidad interactuable actualmente bajo la mira (`None` si no hay).
    pub target: Option<u32>,
    /// Entidad que interactúa (jugador), si hay una.
    pub actor: Option<u32>,
    /// `true` solo el frame en que se pulsa la tecla de interacción.
    pub pressed: bool,
    /// Distancia en metros hasta el objetivo (0 si no hay).
    pub distance: f32,
}
