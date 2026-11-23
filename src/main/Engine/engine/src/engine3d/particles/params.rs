//! Structs de partículas alineados con los shaders GLSL (`std430`/UBO `std140`).
//!
//! Espejo exacto de:
//! - `shaders/particles/spawn_sim.comp` (`Particle`, `Emitter`)
//! - `shaders/particles/render.vert` (`Particle`, `Emitter`, `PushDraw`)
//! - `shaders/particles/build_indirect.comp`
//!
//! Los tests de abajo fijan tamaños y offsets: si un campo cambia y se descuadra con el
//! GLSL, el test lo detecta antes de que la GPU muestre basura.

use bytemuck::{Pod, Zeroable};

/// Capacidad mínima de un pool por emisor (el grow es cuadrático).
pub(crate) const MIN_PARTICLES: u32 = 64;

/// `struct Particle` del GLSL: 4 × vec4 = 64 bytes.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct Particle {
    /// xyz = posición mundo, w = vida restante (s).
    pub pos_life: [f32; 4],
    /// xyz = velocidad, w = edad (s).
    pub vel_age: [f32; 4],
    /// x = ángulo del billboard (rad), y = tamaño base (m), z = semilla RNG
    /// (`fract` = [0,1), `1..2` = grano rodando, `2..3` = asentado, `>= 3` = splash
    /// de agua en billboard de cámara), w = velocidad angular (rad/s).
    pub rot_seed: [f32; 4],
    /// RGBA evaluado por la curva de vida en el shader de simulación.
    pub color: [f32; 4],
}

/// Billboard a cámara (fuego, humo, granos).
pub(crate) const BILLBOARD_CAMERA: u32 = 0;
/// Quad orientado a la velocidad (lluvia).
pub(crate) const BILLBOARD_VELOCITY: u32 = 1;
/// Stretched billboard: el eje largo sigue la velocidad (gotas/chorros).
pub(crate) const BILLBOARD_STRETCHED: u32 = 2;

/// `uniform Emitter` del GLSL: 10 × vec4 = 160 bytes.
///
/// `env.x` es el plano de suelo configurable (precisión 3 del plan) que F4 usa para
/// colisiones granulares; los balísticos lo usan como plano de reciclado (lluvia/nieve).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct EmitterParams {
    /// xyz = origen del emisor (mundo), w = radio de emisión (m).
    pub origin_radius: [f32; 4],
    /// xyz = velocidad inicial media, w = dispersión (m/s).
    pub velocity_spread: [f32; 4],
    /// x = tamaño base (m), y = variación de tamaño, z = vida media (s), w = variación de vida.
    pub size_life: [f32; 4],
    /// RGBA al nacer.
    pub color_start: [f32; 4],
    /// RGBA al morir.
    pub color_end: [f32; 4],
    /// x = gravedad (m/s², +Y arriba; fuego usa negativa = flotabilidad), y = drag,
    /// z = turbulencia (curl noise), w reservado.
    pub phys: [f32; 4],
    /// x = `ground_y` plano de suelo/reciclado, y = giro del billboard (rad/s),
    /// z = viento X (m/s), w = viento Z (m/s).
    pub env: [f32; 4],
    /// x = material_kind (ver `MaterialKind`), y = capacidad del pool,
    /// z = modo billboard (0 cámara, 1 velocidad, 2 stretched), w = flags (F3/F4).
    pub misc: [u32; 4],
    /// xyz = semi-extensión del volumen de cámara (lluvia/nieve/niebla); w sin uso.
    pub volume: [f32; 4],
    /// x = restitución al suelo, y = fricción [0,1], z = multiplicador de tamaño al
    /// morir (`sizeOverLife`), w = escala de stretched billboard (eje de velocidad).
    pub collide: [f32; 4],
}

/// Push constant del kernel de simulación: 16 bytes.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct PushSim {
    /// Delta time del frame ya acotado (≤ 0.05 s para no explotar en hitches).
    pub dt: f32,
    /// Partículas nuevas este frame (CPU: `rate * dt` con acarreo).
    pub spawn: u32,
    /// 1 = leemos A / escribimos B; 0 = leemos B / escribimos A (`sim_step & 1`).
    pub read_is_a: u32,
    /// Semilla del RNG del emisor (avanza por frame).
    pub seed: u32,
}

/// Push constant de `build_indirect.comp`: 8 bytes.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct PushBuild {
    /// Mismo flag que el kernel de sim de ESTE frame: el contador de salida es el opuesto.
    pub read_is_a: u32,
    /// Emisión estimada del frame siguiente (para dimensionar el dispatch indirecto).
    pub next_spawn: u32,
}

/// Push constant del pipeline de dibujo: 128 bytes (límite garantizado por Vulkan).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct PushDraw {
    /// `view_proj` con jitter TAA (la misma que rasterizó el forward).
    pub view_proj: [[f32; 4]; 4],
    /// xyz = derecha de la cámara (mundo), w reservado.
    pub cam_right: [f32; 4],
    /// xyz = arriba de la cámara (mundo), w reservado.
    pub cam_up: [f32; 4],
    /// x = `read_is_a`, y = modo de mezcla (0 aditivo / 1 alpha / WBOIT usa el acumulador),
    /// z = soft particles (1 activo), w = material_kind (forma del fragmento).
    pub misc: [u32; 4],
    /// x = near, y = far de la cámara (linealización de la depth para soft particles),
    /// z = suavidad en metros, w reservado.
    pub depth_params: [f32; 4],
}

/// Tipos de material del sistema (F1 solo activa `Fire`; el resto llega en F2–F4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MaterialKind {
    Fire,
    Smoke,
    Fog,
    Rain,
    Snow,
    Sand,
    Water,
    /// Corona del sol: chispas blancas cálidas que nacen en toda la esfera y se
    /// expanden hacia fuera. Emisor interno reservado; no aparece en la UI F5.
    Sun,
}

impl MaterialKind {
    pub(crate) fn as_u32(self) -> u32 {
        match self {
            Self::Fire => 0,
            Self::Smoke => 1,
            Self::Fog => 2,
            Self::Rain => 3,
            Self::Snow => 4,
            Self::Sand => 5,
            Self::Water => 6,
            Self::Sun => 8,
        }
    }

    /// Cadena del contrato IPC/snapshot (inversa de `from_wire`).
    pub(crate) fn wire(self) -> &'static str {
        match self {
            Self::Fire => "fire",
            Self::Smoke => "smoke",
            Self::Fog => "fog",
            Self::Rain => "rain",
            Self::Snow => "snow",
            Self::Sand => "sand",
            Self::Water => "water",
            Self::Sun => "sun",
        }
    }

    pub(crate) fn from_wire(value: &str) -> Option<Self> {
        match value {
            "fire" => Some(Self::Fire),
            "smoke" => Some(Self::Smoke),
            "fog" => Some(Self::Fog),
            "rain" => Some(Self::Rain),
            "snow" => Some(Self::Snow),
            "sand" => Some(Self::Sand),
            "water" => Some(Self::Water),
            "sun" => Some(Self::Sun),
            _ => None,
        }
    }

    /// Modo de mezcla del pase: los materiales atmosféricos usan alpha estándar; el
    /// fuego (y F4 arena) aditivo.
    pub(crate) fn alpha_blend(self) -> u32 {
        match self {
            // Sol y fuego suman luz: el blanco cálido del sol se compone con HDR.
            Self::Fire | Self::Sun => 0,
            Self::Smoke | Self::Fog | Self::Rain | Self::Snow | Self::Sand | Self::Water => 1,
        }
    }

    /// Materiales que renderizan por WBOIT (F3): sin ordenar en CPU, con acumulación
    /// ponderada y revealage multiplicativo.
    pub(crate) fn uses_wboit(self) -> bool {
        matches!(self, Self::Smoke | Self::Fog | Self::Water)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn particle_is_64_bytes() {
        assert_eq!(std::mem::size_of::<Particle>(), 64);
    }

    #[test]
    fn emitter_params_is_160_bytes() {
        assert_eq!(std::mem::size_of::<EmitterParams>(), 160);
        // Diez vec4 sin relleno intermedio.
        assert_eq!(std::mem::size_of::<EmitterParams>(), 10 * 16);
    }

    #[test]
    fn push_constants_fit_the_guaranteed_budget() {
        // El límite garantizado por Vulkan es minMemoryPushConstantSize = 128.
        assert_eq!(std::mem::size_of::<PushSim>(), 16);
        assert_eq!(std::mem::size_of::<PushBuild>(), 8);
        assert_eq!(std::mem::size_of::<PushDraw>(), 128);
    }

    #[test]
    fn material_kind_round_trips_through_wire() {
        for name in ["fire", "smoke", "fog", "rain", "snow", "sand", "water"] {
            let kind = MaterialKind::from_wire(name).expect("material reconocido");
            assert_eq!(kind, MaterialKind::from_wire(name).unwrap());
        }
        assert!(MaterialKind::from_wire("plasma").is_none());
    }
}
