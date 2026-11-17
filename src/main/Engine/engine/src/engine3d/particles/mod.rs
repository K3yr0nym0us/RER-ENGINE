//! Estado del sistema de partículas del lado del motor (CPU).
//!
//! Este módulo no toca Vulkan: solo lleva la lista de emisores activos, su contador de
//! ping-pong propio (`sim_step`, que SOLO avanza cuando el emisor realmente ejecuta su
//! kernel de simulación ese frame) y el acarreo fraccional de la tasa de emisión. Los
//! buffers GPU por emisor viven en `vulkan::frame::particles` (`ParticlePool`).
//!
//! F1 (slice vertical): fuego. Los demás materiales llegan en F2–F4 reutilizando esta
//! misma estructura.

mod params;

pub(crate) use params::{
    BILLBOARD_CAMERA, BILLBOARD_STRETCHED, BILLBOARD_VELOCITY, EmitterParams, MIN_PARTICLES,
    MaterialKind, PushBuild, PushDraw, PushSim,
};

/// Configuración completa de un emisor. La UI del editor (F5) genera esto por comando.
#[derive(Debug, Clone)]
pub(crate) struct EmitterConfig {
    pub material: MaterialKind,
    pub origin: [f32; 3],
    /// Radio de emisión alrededor del origen (m) — emisores puntuales (fuego).
    pub radius: f32,
    /// Velocidad inicial media (m/s).
    pub velocity: [f32; 3],
    /// Dispersión de la velocidad (m/s).
    pub spread: f32,
    /// Tamaño base del billboard (m).
    pub size: f32,
    pub size_variation: f32,
    /// Vida media (s).
    pub life: f32,
    pub life_variation: f32,
    /// Emisión (partículas por segundo).
    pub rate: f32,
    /// Presupuesto del pool (capacidad máxima de partículas vivas).
    pub max_particles: u32,
    pub color_start: [f32; 4],
    pub color_end: [f32; 4],
    /// Gravedad en +Y (m/s²); el fuego usa valores negativos (flotabilidad).
    pub gravity: f32,
    /// Coeficiente de arrastre exponencial (s⁻¹).
    pub drag: f32,
    /// Amplitud del curl noise (fuego/humo).
    pub turbulence: f32,
    /// F4: cohesión de flujo (0–1); arrastra la velocidad hacia la del emisor
    /// (aproximación de tensión superficial sin búsqueda de vecinos).
    pub cohesion: f32,
    /// Plano de suelo para colisiones (F4) o reciclado (lluvia/nieve).
    pub ground_y: f32,
    /// Giro del billboard (rad/s) alrededor del eje de vista.
    pub spin: f32,
    /// Viento horizontal (m/s) — lluvia/nieve (F2).
    pub wind: [f32; 2],
    /// Semi-extensión del volumen alrededor del origen (m) — materiales que reciclan.
    pub volume: [f32; 3],
    /// `true`: el origen se re-centra en la cámara cada frame (clima).
    pub follow_camera: bool,
    /// 0 = billboard a cámara, 1 = orientado a velocidad (lluvia).
    pub billboard_mode: u32,
    /// Intensidad guardada por la UI (1.0 por defecto; los presets de clima la fijan).
    pub intensity: f32,
    /// Soft particles (F3): desvanecer contra la depth del forward en el borde.
    pub soft: bool,
    /// F4: colisión contra el plano `ground_y` (arena/agua).
    pub ground_collision: bool,
    /// F4: acumulación — al tocar el suelo la partícula se deposita (nieve).
    pub accumulate: bool,
    /// F4: salpicadura — aplastar/morir rápido (agua) o grano que se frena (arena).
    pub splash: bool,
    /// Restitución al colisionar con `ground_y` (0 = no rebota).
    pub bounciness: f32,
    /// Fricción horizontal al suelo [0,1] (1 = se detiene en XZ).
    pub friction: f32,
    /// Multiplicador de tamaño al morir (`sizeOverLife`: 1 = constante).
    pub size_end: f32,
    /// Escala extra del eje de velocidad en billboard estirado (modo 2).
    pub stretch: f32,
}

impl EmitterConfig {
    /// Preset de fuego para la F1: llama corta y turbulenta que sube.
    pub(crate) fn fire_demo() -> Self {
        Self {
            material: MaterialKind::Fire,
            origin: [0.0, 0.35, 0.0],
            radius: 0.18,
            velocity: [0.0, 2.4, 0.0],
            spread: 0.55,
            size: 0.42,
            size_variation: 0.55,
            life: 0.85,
            life_variation: 0.45,
            rate: 220.0,
            max_particles: 768,
            // Naranja (el kernel interpola blanco-amarillo → este → rojo oscuro).
            color_start: [1.0, 0.48, 0.08, 0.95],
            color_end: [0.28, 0.02, 0.0, 0.0],
            gravity: 3.2, // flotabilidad +Y (se suma a vel.y)
            drag: 1.15,
            turbulence: 3.4,
            cohesion: 0.0,
            ground_y: 0.0,
            spin: 5.5,
            wind: [0.0, 0.0],
            volume: [0.0, 0.0, 0.0],
            follow_camera: false,
            billboard_mode: BILLBOARD_CAMERA,
            intensity: 1.0,
            soft: true,
            ground_collision: false,
            accumulate: false,
            splash: false,
            bounciness: 0.0,
            friction: 0.0,
            size_end: 0.12,
            stretch: 0.0,
        }
    }

    /// Preset de humo (F3): gris oscuro, sube lento, crece con la edad y se desvanece.
    /// Renderiza por WBOIT con soft particles.
    pub(crate) fn smoke() -> Self {
        Self {
            material: MaterialKind::Smoke,
            origin: [0.0, 0.3, 0.0],
            radius: 0.3,
            velocity: [0.0, 1.15, 0.0],
            spread: 0.3,
            size: 0.9,
            size_variation: 0.4,
            life: 3.8,
            life_variation: 0.4,
            rate: 90.0,
            max_particles: 1024,
            color_start: [0.16, 0.16, 0.19, 0.55],
            color_end: [0.36, 0.36, 0.4, 0.0],
            gravity: 1.35,
            drag: 0.55,
            turbulence: 1.2,
            cohesion: 0.0,
            ground_y: 0.0,
            spin: 0.0,
            wind: [0.0, 0.0],
            volume: [0.0, 0.0, 0.0],
            follow_camera: false,
            billboard_mode: BILLBOARD_CAMERA,
            intensity: 1.0,
            soft: true,
            ground_collision: false,
            accumulate: false,
            splash: false,
            bounciness: 0.0,
            friction: 0.0,
            size_end: 1.9,
            stretch: 0.0,
        }
    }

    /// Preset de niebla (F3/F6): nubes enormes sin forma apoyadas en el suelo, WBOIT +
    /// soft particles. La intensidad NO sube la cantidad de nubes: las hace más espesas
    /// (el alpha escala con ella en `batches`); la tasa es fija para que la capa se
    /// forme en pocos segundos. `intensity` escala la opacidad.
    pub(crate) fn fog(intensity: f32) -> Self {
        let intensity = intensity.clamp(1.0, 10.0);
        Self {
            material: MaterialKind::Fog,
            origin: [0.0, 0.0, 0.0],
            radius: 0.0,
            velocity: [0.35, 0.0, 0.25],
            spread: 0.15,
            size: 9.0,
            size_variation: 0.4,
            life: 45.0,
            life_variation: 0.0,
            rate: 120.0,
            max_particles: 512,
            color_start: [0.72, 0.78, 0.86, 0.3],
            color_end: [0.72, 0.78, 0.86, 0.3],
            gravity: 0.0,
            drag: 0.0,
            turbulence: 0.0,
            cohesion: 0.0,
            ground_y: 0.0, // niebla rasante: la capa se apoya en el suelo (clamp en kernel)
            spin: 0.0,
            wind: [0.4, 0.3],
            volume: [26.0, 14.0, 26.0],
            follow_camera: true,
            billboard_mode: 0,
            intensity,
            soft: true,
            ground_collision: false,
            accumulate: false,
            splash: false,
            bounciness: 0.0,
            friction: 0.0,
            size_end: 1.0,
            stretch: 0.0,
        }
    }

    /// Niebla de emisor de escena: nace en un disco junto al origen y muere por vida.
    /// El clima (`fog(intensity)`) sigue usando volumen de cámara y `follow_camera`.
    pub(crate) fn scene_fog() -> Self {
        let mut config = Self::fog(1.0);
        config.follow_camera = false;
        config.volume = [0.0, 0.0, 0.0];
        config.radius = 1.6;
        config.size = 2.2;
        config.size_variation = 0.35;
        config.life = 10.0;
        config.life_variation = 0.25;
        config.rate = 36.0;
        config.max_particles = 256;
        config.wind = [0.2, 0.12];
        config
    }

    /// Preset de lluvia (F2/F6-bis): balística con velocidad terminal, viento y reciclado
    /// en el volumen de cámara. La intensidad (1–10) escala la DENSIDAD vía capacidad y
    /// tasa (reciclan para siempre: saturan el pool, así que solo la capacidad sube el
    /// número de gotas visibles).
    pub(crate) fn rain(intensity: f32, fall_multiplier: f32, wind_x: f32) -> Self {
        let intensity = intensity.clamp(1.0, 10.0);
        Self {
            material: MaterialKind::Rain,
            origin: [0.0, 0.0, 0.0],
            radius: 0.0,
            velocity: [0.0, -18.0 * fall_multiplier, 0.0], // velocidad terminal de la gota
            spread: 0.5,
            size: 0.035,
            size_variation: 0.35,
            life: 60.0, // reciclado por volumen; la vida es válvula de seguridad
            life_variation: 0.0,
            rate: 300.0 * intensity,
            max_particles: (512.0 * intensity) as u32,
            color_start: [0.72, 0.82, 1.0, 0.34],
            color_end: [0.72, 0.82, 1.0, 0.34],
            gravity: 0.0,
            drag: 0.0,
            turbulence: 0.0,
            cohesion: 0.0,
            ground_y: 0.0,
            spin: 0.0,
            wind: [wind_x, 1.0],
            volume: [32.0, 22.0, 32.0],
            follow_camera: true,
            billboard_mode: BILLBOARD_VELOCITY,
            intensity,
            soft: true,
            ground_collision: false,
            accumulate: false,
            splash: false,
            bounciness: 0.0,
            friction: 0.0,
            size_end: 1.0,
            stretch: 0.12,
        }
    }

    /// Preset de nieve (F2/F6-bis): caída lenta con flutter sinusoidal + viento,
    /// reciclado en el volumen de cámara. La intensidad (1–10) escala densidad
    /// (capacidad + tasa); `fall_multiplier` y `wind_x` el movimiento.
    pub(crate) fn snow(intensity: f32, fall_multiplier: f32, wind_x: f32) -> Self {
        let intensity = intensity.clamp(1.0, 10.0);
        Self {
            material: MaterialKind::Snow,
            origin: [0.0, 0.0, 0.0],
            radius: 0.0,
            velocity: [0.0, -1.6 * fall_multiplier, 0.0],
            spread: 0.25,
            size: 0.075,
            size_variation: 0.5,
            life: 90.0,
            life_variation: 0.0,
            rate: 150.0 * intensity,
            max_particles: (384.0 * intensity) as u32,
            color_start: [1.0, 1.0, 1.0, 0.9],
            color_end: [1.0, 1.0, 1.0, 0.9],
            gravity: 0.0,
            drag: 0.0,
            turbulence: 0.0,
            cohesion: 0.0,
            ground_y: 0.0,
            spin: 0.35,
            wind: [wind_x, 0.6],
            volume: [30.0, 20.0, 30.0],
            follow_camera: true,
            billboard_mode: 0,
            intensity,
            soft: true,
            ground_collision: true,
            accumulate: true,
            splash: false,
            bounciness: 0.0,
            friction: 0.85,
            size_end: 1.0,
            stretch: 0.0,
        }
    }

    /// Preset de arena (F4): granos finos, sin rebote, fricción alta. El polvo
    /// suspendido vive en el emisor separado `sand_dust()`, no en este pool.
    pub(crate) fn sand() -> Self {
        Self {
            material: MaterialKind::Sand,
            origin: [0.0, 2.0, 0.0],
            radius: 0.45,
            velocity: [0.8, 0.4, 0.0],
            spread: 1.6,
            size: 0.008,
            size_variation: 0.35,
            life: 2.8,
            life_variation: 0.25,
            rate: 420.0,
            max_particles: 2048,
            color_start: [0.82, 0.66, 0.38, 0.95],
            color_end: [0.72, 0.55, 0.3, 0.0],
            gravity: -9.8,
            drag: 0.35,
            turbulence: 0.15,
            cohesion: 0.0,
            ground_y: 0.0,
            spin: 8.0,
            wind: [0.15, 0.05],
            volume: [0.0, 0.0, 0.0],
            follow_camera: false,
            billboard_mode: BILLBOARD_CAMERA,
            intensity: 1.0,
            soft: false,
            ground_collision: true,
            accumulate: false,
            splash: false,
            bounciness: 0.0,
            friction: 0.92,
            size_end: 0.006,
            stretch: 0.0,
        }
    }

    /// Segunda capa opcional de arena: polvo suspendido (billboards grandes, alpha
    /// muy bajo). Añadir como emisor extra en el mismo origen; no sustituye a `sand()`.
    #[allow(dead_code)]
    pub(crate) fn sand_dust() -> Self {
        Self {
            material: MaterialKind::Sand,
            origin: [0.0, 1.4, 0.0],
            radius: 0.7,
            velocity: [0.2, 0.35, 0.1],
            spread: 0.4,
            size: 0.22,
            size_variation: 0.6,
            life: 3.5,
            life_variation: 0.4,
            rate: 40.0,
            max_particles: 256,
            color_start: [0.78, 0.64, 0.42, 0.06],
            color_end: [0.7, 0.58, 0.36, 0.0],
            gravity: 0.15,
            drag: 1.8,
            turbulence: 0.7,
            cohesion: 0.0,
            ground_y: 0.0,
            spin: 0.4,
            wind: [0.55, 0.2],
            volume: [0.0, 0.0, 0.0],
            follow_camera: false,
            billboard_mode: 0,
            intensity: 1.0,
            soft: true,
            ground_collision: false,
            accumulate: false,
            splash: false,
            bounciness: 0.0,
            friction: 0.0,
            size_end: 2.2,
            stretch: 0.0,
        }
    }

    /// Preset de agua (F4): gotas/chorros en stretched billboard; al suelo salpican
    /// con billboard de cámara y un rebote breve.
    pub(crate) fn water() -> Self {
        Self {
            material: MaterialKind::Water,
            origin: [0.0, 2.2, 0.0],
            radius: 0.12,
            velocity: [0.0, -7.5, 0.0],
            spread: 0.55,
            size: 0.012,
            size_variation: 0.4,
            life: 1.6,
            life_variation: 0.25,
            rate: 280.0,
            max_particles: 1536,
            color_start: [0.62, 0.82, 1.0, 0.25],
            color_end: [0.4, 0.62, 0.88, 0.0],
            gravity: -9.8,
            drag: 0.18,
            turbulence: 0.12,
            cohesion: 0.15,
            ground_y: 0.0,
            spin: 0.0,
            wind: [0.0, 0.0],
            volume: [0.0, 0.0, 0.0],
            follow_camera: false,
            billboard_mode: BILLBOARD_STRETCHED,
            intensity: 1.0,
            soft: true,
            ground_collision: true,
            accumulate: false,
            splash: true,
            bounciness: 0.04,
            friction: 0.88,
            size_end: 0.02,
            stretch: 0.42,
        }
    }

    // (fin de presets)
}

/// Un emisor vivo: configuración + estado de simulación (todo CPU).
pub(crate) struct Emitter {
    pub id: u64,
    pub config: EmitterConfig,
    /// Contador de ping-pong PROPIO (precisión 1 del plan): solo avanza cuando este emisor
    /// ejecuta su compute ese frame. Un emisor desactivado o en pausa no se desincroniza.
    pub sim_step: u32,
    /// Semilla RNG que avanza por frame de sim.
    pub seed: u32,
    /// Acarreo fraccional de la emisión: `rate * dt` puede ser < 1 por frame.
    pub spawn_carry: f32,
    /// F6: `true` cuando el emisor reutiliza un id cuyo pool puede tener partículas
    /// viejas (el clima se reemplaza con el MISMO id): el primer frame pide al pool
    /// reiniciar contadores e indirecto, en vez de esperar a que mueran las antiguas.
    pub reset: bool,
}

impl Emitter {
    fn new(id: u64, config: EmitterConfig) -> Self {
        Self {
            id,
            config,
            sim_step: 0,
            seed: 0x9E37_79B9 ^ id as u32,
            spawn_carry: 0.0,
            reset: false,
        }
    }
}

/// Lote que el engine le pasa al `FrameRenderer` cada frame por emisor activo.
///
/// `read_is_a`/`spawn`/`seed` ya quedan resueltos aquí: al construir el lote, el emisor
/// AVANZA su `sim_step` (por eso solo se construye cuando el emisor va a simular).
pub(crate) struct ParticleBatch {
    pub id: u64,
    pub max_particles: u32,
    /// Partículas nuevas este frame (acarreo incluido, ya acotado por capacidad).
    pub spawn: u32,
    /// 1 = el pool debe reiniciar contadores/indirecto antes de simular (clima
    /// reemplazado con el mismo id, F6).
    pub reset: u32,
    /// Emisión estimada del frame siguiente para `build_indirect.comp`.
    pub next_spawn: u32,
    pub dt: f32,
    pub seed: u32,
    pub read_is_a: u32,
    /// 0 = blending aditivo (fuego), 1 = alpha estándar (lluvia/nieve).
    pub blend_alpha: u32,
    /// 1 = renderizar por WBOIT (humo/niebla, F3) en vez del pase directo.
    pub wboit: u32,
    /// Soft particles (F3): desvanecer contra la depth del forward.
    pub soft: u32,
    pub params: EmitterParams,
}

/// Reconstruye una configuración completa desde el snapshot, partiendo del preset del
/// material (colores/volumen de cámara) y aplicando los campos guardados encima.
fn config_from_saved(
    material: MaterialKind,
    saved: &crate::engine3d::ipc::SaveParticleEmitterSnapshot,
) -> EmitterConfig {
    let mut config = match material {
        MaterialKind::Fire => EmitterConfig::fire_demo(),
        MaterialKind::Smoke => EmitterConfig::smoke(),
        MaterialKind::Fog => EmitterConfig::scene_fog(),
        MaterialKind::Rain => EmitterConfig::rain(1.0, 1.0, 0.0),
        MaterialKind::Snow => EmitterConfig::snow(1.0, 1.0, 0.0),
        MaterialKind::Sand => EmitterConfig::sand(),
        MaterialKind::Water => EmitterConfig::water(),
    };
    config.follow_camera = false; // lo guardado usa posición fija
    config.origin = saved.position;
    config.radius = saved.radius;
    config.rate = saved.rate;
    config.max_particles = saved.max_particles.max(MIN_PARTICLES);
    config.velocity = saved.velocity;
    config.spread = saved.spread;
    config.size = saved.size;
    config.size_variation = saved.size_variation;
    config.life = saved.life;
    config.life_variation = saved.life_variation;
    config.gravity = saved.gravity;
    config.drag = saved.drag;
    config.turbulence = saved.turbulence;
    config.cohesion = saved.cohesion;
    config.ground_y = saved.ground_y;
    config.spin = saved.spin;
    config.wind = saved.wind;
    config.billboard_mode = saved.billboard;
    config.soft = saved.soft;
    config.ground_collision = saved.ground_collision;
    config.accumulate = saved.accumulate;
    config.splash = saved.splash;
    if let Some(value) = saved.bounciness {
        config.bounciness = value;
    }
    if let Some(value) = saved.friction {
        config.friction = value;
    }
    if let Some(value) = saved.size_end {
        config.size_end = value;
    }
    if let Some(value) = saved.stretch {
        config.stretch = value;
    }
    config
}

/// Delta time máximo entregado al kernel: un hitch de 2 s no debe teleportar partículas.
const MAX_DT: f32 = 0.05;

/// Id reservado del emisor de clima (único por sistema: lluvia o nieve, F2).
const WEATHER_ID: u64 = u64::MAX;

#[derive(Default)]
pub(crate) struct ParticleSystem {
    emitters: Vec<Emitter>,
    next_id: u64,
}

impl ParticleSystem {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Crea un emisor y devuelve su id.
    pub(crate) fn add(&mut self, config: EmitterConfig) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.emitters.push(Emitter::new(id, config));
        id
    }

    /// Alta o reemplazo con id externo (la UI usa ids altos para no chocar con los del
    /// motor). Si el id ya existe, actualiza la config in-place en vez de crear otro
    /// emisor; si no viene id, asigna el siguiente automático.
    pub(crate) fn add_or_with_id(&mut self, id: Option<u64>, config: EmitterConfig) -> u64 {
        match id {
            Some(id) if id != WEATHER_ID => {
                if let Some(existing) = self.emitters.iter_mut().find(|emitter| emitter.id == id) {
                    existing.config = config;
                    existing.spawn_carry = 0.0;
                    existing.reset = true;
                    return id;
                }
                self.next_id = self.next_id.max(id + 1);
                self.emitters.push(Emitter::new(id, config));
                id
            }
            _ => self.add(config),
        }
    }

    pub(crate) fn contains(&self, id: u64) -> bool {
        self.emitters.iter().any(|emitter| emitter.id == id)
    }

    pub(crate) fn remove(&mut self, id: u64) -> bool {
        let before = self.emitters.len();
        self.emitters.retain(|emitter| emitter.id != id);
        before != self.emitters.len()
    }

    pub(crate) fn clear(&mut self) {
        self.emitters.clear();
    }

    /// Velocidad de caída base (m/s, negativa) del preset de cada clima; el multiplicador
    /// de la UI escala sobre esto.
    pub(crate) fn base_fall(kind: MaterialKind) -> f32 {
        match kind {
            MaterialKind::Rain => -18.0,
            MaterialKind::Snow => -1.6,
            _ => 0.0,
        }
    }

    /// Clima global (F2/F6-bis): un único emisor reservado para lluvia/nieve/niebla.
    /// `kind = None` (IPC "none") lo retira. `fall_multiplier` y `wind_x` solo afectan a
    /// lluvia/nieve (la niebla deriva, no cae). Cambiar de clima destruye el emisor
    /// anterior y crea uno nuevo: el pool GPU recicla por inactividad, sin fuga.
    pub(crate) fn set_weather(
        &mut self,
        kind: Option<MaterialKind>,
        intensity: f32,
        fall_multiplier: f32,
        wind_x: f32,
    ) {
        self.emitters.retain(|emitter| emitter.id != WEATHER_ID);
        let Some(kind) = kind else {
            return;
        };
        let fall = fall_multiplier.clamp(0.25, 3.0);
        let config = match kind {
            MaterialKind::Rain => EmitterConfig::rain(intensity, fall, wind_x),
            MaterialKind::Snow => EmitterConfig::snow(intensity, fall, wind_x),
            MaterialKind::Fog => EmitterConfig::fog(intensity),
            // Humo y fuego son emisores de escena (add_particle_emitter), no clima;
            // arena/agua llegan en F4.
            MaterialKind::Fire | MaterialKind::Smoke | MaterialKind::Sand | MaterialKind::Water => {
                return;
            }
        };
        let mut emitter = Emitter::new(WEATHER_ID, config);
        // El pool puede seguir teniendo las partículas del clima anterior (mismo id):
        // pedir reset para que el nuevo clima arranque vacío al instante.
        emitter.reset = true;
        self.emitters.push(emitter);
    }

    /// Restaura el estado desde el snapshot de persistencia (F5): reemplaza todo el
    /// sistema por lo guardado. Los colores vienen del preset del material.
    pub(crate) fn restore(&mut self, snapshot: &crate::engine3d::ipc::SaveParticlesSnapshot) {
        self.clear();
        if let Some(weather) = &snapshot.weather {
            let kind = MaterialKind::from_wire(&weather.kind).filter(|kind| {
                matches!(
                    kind,
                    MaterialKind::Rain | MaterialKind::Snow | MaterialKind::Fog
                )
            });
            if let Some(kind) = kind {
                self.set_weather(
                    Some(kind),
                    weather.intensity,
                    weather.fall_speed.unwrap_or(1.0),
                    weather.wind_x.unwrap_or(0.0),
                );
            }
        }
        for saved in &snapshot.emitters {
            let Some(material) = MaterialKind::from_wire(&saved.material) else {
                continue;
            };
            let mut emitter = Emitter::new(saved.id, config_from_saved(material, saved));
            // El pool GPU puede conservar emisores de la escena anterior con estos ids.
            emitter.reset = true;
            self.emitters.push(emitter);
            self.next_id = self.next_id.max(saved.id + 1);
        }
    }

    /// ¿Hay emisor de clima activo? (lo consulta la UI del editor, F5.)
    #[allow(dead_code)]
    pub(crate) fn weather_active(&self) -> Option<MaterialKind> {
        self.emitters
            .iter()
            .find(|emitter| emitter.id == WEATHER_ID)
            .map(|emitter| emitter.config.material)
    }

    /// Snapshot de persistencia (F5): clima + emisores de escena con configuración.
    pub(crate) fn export(&self) -> crate::engine3d::ipc::SaveParticlesSnapshot {
        use crate::engine3d::ipc::{
            SaveParticleEmitterSnapshot, SaveParticlesSnapshot, SaveWeatherSnapshot,
        };

        let mut weather = None;
        let mut emitters = Vec::new();
        for emitter in &self.emitters {
            let config = &emitter.config;
            if emitter.id == WEATHER_ID {
                // El multiplicador de caída se recalcula contra la base del material
                // (la configuración guarda la velocidad final, no el multiplicador).
                let fall_multiplier = if Self::base_fall(config.material) != 0.0 {
                    config.velocity[1] / Self::base_fall(config.material)
                } else {
                    1.0
                };
                weather = Some(SaveWeatherSnapshot {
                    kind: config.material.wire().to_string(),
                    intensity: config.intensity,
                    fall_speed: Some(fall_multiplier),
                    wind_x: Some(config.wind[0]),
                });
                continue;
            }
            emitters.push(SaveParticleEmitterSnapshot {
                id: emitter.id,
                material: config.material.wire().to_string(),
                position: config.origin,
                radius: config.radius,
                rate: config.rate,
                max_particles: config.max_particles,
                velocity: config.velocity,
                spread: config.spread,
                size: config.size,
                size_variation: config.size_variation,
                life: config.life,
                life_variation: config.life_variation,
                gravity: config.gravity,
                drag: config.drag,
                turbulence: config.turbulence,
                cohesion: config.cohesion,
                ground_y: config.ground_y,
                spin: config.spin,
                wind: config.wind,
                billboard: config.billboard_mode,
                soft: config.soft,
                ground_collision: config.ground_collision,
                accumulate: config.accumulate,
                splash: config.splash,
                bounciness: Some(config.bounciness),
                friction: Some(config.friction),
                size_end: Some(config.size_end),
                stretch: Some(config.stretch),
            });
        }
        SaveParticlesSnapshot { weather, emitters }
    }

    // Los consume la UI del editor (F5, dropdown del sidebar derecho).
    #[allow(dead_code)]
    pub(crate) fn len(&self) -> usize {
        self.emitters.len()
    }

    #[allow(dead_code)]
    pub(crate) fn is_empty(&self) -> bool {
        self.emitters.is_empty()
    }

    /// Lotes del frame: cada emisor activo avanza su `sim_step` y su RNG aquí, así que
    /// solo se llama cuando el frame realmente va a simular (precisión 1 del plan).
    /// `camera_pos` re-centra los emisores con `follow_camera` (clima, F2).
    pub(crate) fn batches(&mut self, delta_time: f32, camera_pos: [f32; 3]) -> Vec<ParticleBatch> {
        let dt = delta_time.clamp(0.0, MAX_DT);
        self.emitters
            .iter_mut()
            .map(|emitter| {
                let config = &emitter.config;
                // Emisión con acarreo fraccional: rate*dt + resto del frame anterior.
                emitter.spawn_carry += config.rate.max(0.0) * dt;
                let spawn = emitter.spawn_carry as u32;
                emitter.spawn_carry -= spawn as f32;
                let next_carry = emitter.spawn_carry + config.rate.max(0.0) * dt;
                let next_spawn = next_carry as u32;

                let read_is_a = emitter.sim_step & 1;
                emitter.sim_step = emitter.sim_step.wrapping_add(1);
                let seed = emitter.seed;
                emitter.seed = emitter
                    .seed
                    .wrapping_mul(1_664_525)
                    .wrapping_add(0x9E37_79B9);
                // El reset se consume en el primer lote: el pool limpia las partículas
                // viejas y el clima nuevo arranca vacío (sin esperar su muerte natural).
                let reset = emitter.reset as u32;
                emitter.reset = false;

                let max_particles = config.max_particles.max(MIN_PARTICLES);
                let spawn = spawn.min(max_particles.saturating_sub(1));
                let next_spawn = next_spawn.min(max_particles.saturating_sub(1));
                let origin = if config.follow_camera {
                    camera_pos
                } else {
                    config.origin
                };
                // Niebla (F6): la intensidad es ESPESOR, no cantidad — el alpha escala
                // con ella y la tasa queda fija en el preset.
                let fog_density = if config.material == MaterialKind::Fog {
                    config.intensity
                } else {
                    1.0
                };
                let alpha_start = (config.color_start[3] * fog_density).min(0.95);
                let alpha_end = (config.color_end[3] * fog_density).min(0.95);

                ParticleBatch {
                    id: emitter.id,
                    max_particles,
                    spawn,
                    reset,
                    next_spawn,
                    dt,
                    seed,
                    read_is_a,
                    blend_alpha: config.material.alpha_blend(),
                    wboit: config.material.uses_wboit() as u32,
                    soft: config.soft as u32,
                    params: EmitterParams {
                        origin_radius: [origin[0], origin[1], origin[2], config.radius.max(0.0)],
                        velocity_spread: [
                            config.velocity[0],
                            config.velocity[1],
                            config.velocity[2],
                            config.spread.max(0.0),
                        ],
                        size_life: [
                            config.size.max(0.001),
                            config.size_variation.max(0.0),
                            config.life.max(0.05),
                            config.life_variation.max(0.0),
                        ],
                        color_start: [
                            config.color_start[0],
                            config.color_start[1],
                            config.color_start[2],
                            alpha_start,
                        ],
                        color_end: [
                            config.color_end[0],
                            config.color_end[1],
                            config.color_end[2],
                            alpha_end,
                        ],
                        phys: [
                            config.gravity,
                            config.drag.max(0.0),
                            config.turbulence,
                            config.cohesion.clamp(0.0, 1.0),
                        ],
                        env: [config.ground_y, config.spin, config.wind[0], config.wind[1]],
                        misc: [
                            config.material.as_u32(),
                            max_particles,
                            config.billboard_mode,
                            (config.ground_collision as u32)
                                | ((config.accumulate as u32) << 1)
                                | ((config.splash as u32) << 2),
                        ],
                        volume: [
                            config.volume[0].max(0.0),
                            config.volume[1].max(0.0),
                            config.volume[2].max(0.0),
                            0.0,
                        ],
                        collide: [
                            config.bounciness.max(0.0),
                            config.friction.clamp(0.0, 1.0),
                            config.size_end.max(0.0),
                            config.stretch.max(0.0),
                        ],
                    },
                }
            })
            .collect()
    }
}

impl crate::engine3d::engine::State {
    /// Alta o reemplazo de un emisor por IPC: preset del material + overrides opcionales.
    /// La UI (F5) asigna ids altos vía `id` para editar/borrar sin eco del motor.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn add_particle_emitter(
        &mut self,
        material: MaterialKind,
        position: [f32; 3],
        radius: Option<f32>,
        rate: Option<f32>,
        max_particles: Option<u32>,
        velocity: Option<[f32; 3]>,
        size: Option<f32>,
        life: Option<f32>,
        gravity: Option<f32>,
        drag: Option<f32>,
        turbulence: Option<f32>,
        ground_y: Option<f32>,
        billboard: Option<u32>,
        cohesion: Option<f32>,
        wind: Option<[f32; 2]>,
        ground_collision: Option<bool>,
        accumulate: Option<bool>,
        splash: Option<bool>,
        id: Option<u64>,
    ) -> u64 {
        let mut config = match material {
            MaterialKind::Fire => EmitterConfig::fire_demo(),
            MaterialKind::Smoke => EmitterConfig::smoke(),
            MaterialKind::Fog => EmitterConfig::scene_fog(),
            MaterialKind::Rain => {
                let mut config = EmitterConfig::rain(1.0, 1.0, 0.0);
                config.follow_camera = false; // emisor manual: usa la posición pedida
                config
            }
            MaterialKind::Snow => {
                let mut config = EmitterConfig::snow(1.0, 1.0, 0.0);
                config.follow_camera = false;
                config
            }
            // F4: presets granulares reales (arena/agua con suelo y salpicadura).
            MaterialKind::Sand => EmitterConfig::sand(),
            MaterialKind::Water => EmitterConfig::water(),
        };
        config.material = material;
        config.origin = position;
        if let Some(value) = radius {
            config.radius = value;
        }
        if let Some(value) = rate {
            config.rate = value;
        }
        if let Some(value) = max_particles {
            config.max_particles = value;
        }
        if let Some(value) = velocity {
            config.velocity = value;
        }
        if let Some(value) = size {
            config.size = value;
        }
        if let Some(value) = life {
            config.life = value;
        }
        if let Some(value) = gravity {
            config.gravity = value;
        }
        if let Some(value) = drag {
            config.drag = value;
        }
        if let Some(value) = turbulence {
            config.turbulence = value;
        }
        if let Some(value) = ground_y {
            config.ground_y = value;
        }
        if let Some(value) = billboard {
            config.billboard_mode = value;
        }
        if let Some(value) = cohesion {
            config.cohesion = value.clamp(0.0, 1.0);
        }
        if let Some(value) = wind {
            config.wind = value;
        }
        if let Some(value) = ground_collision {
            config.ground_collision = value;
        }
        if let Some(value) = accumulate {
            config.accumulate = value;
        }
        if let Some(value) = splash {
            config.splash = value;
        }
        self.particles.add_or_with_id(id, config)
    }

    /// Restaura partículas desde el `world.particles` del snapshot (carga de escena).
    pub(crate) fn restore_particles_from_value(&mut self, value: &serde_json::Value) {
        match serde_json::from_value::<crate::engine3d::ipc::SaveParticlesSnapshot>(value.clone()) {
            Ok(snapshot) => self.particles.restore(&snapshot),
            Err(error) => log::warn!("[partículas] snapshot ilegible, se descarta: {error}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_returns_unique_ids() {
        let mut system = ParticleSystem::new();
        let first = system.add(EmitterConfig::fire_demo());
        let second = system.add(EmitterConfig::fire_demo());
        assert_ne!(first, second);
        assert_eq!(system.len(), 2);
    }

    #[test]
    fn add_or_with_id_replaces_existing_instead_of_duplicating() {
        let mut system = ParticleSystem::new();
        let mut first = EmitterConfig::fire_demo();
        first.rate = 40.0;
        let id = system.add_or_with_id(Some(0x6000_0000_0000_0001), first);
        let mut edited = EmitterConfig::fire_demo();
        edited.rate = 220.0;
        let same = system.add_or_with_id(Some(id), edited);
        assert_eq!(same, id);
        assert_eq!(system.len(), 1);
        assert_eq!(system.emitters[0].config.rate, 220.0);
        assert!(system.emitters[0].reset);
    }

    #[test]
    fn add_or_with_id_does_not_reuse_weather_id() {
        let mut system = ParticleSystem::new();
        system.set_weather(Some(MaterialKind::Rain), 1.0, 1.0, 0.0);
        let assigned = system.add_or_with_id(Some(WEATHER_ID), EmitterConfig::fire_demo());
        assert_ne!(assigned, WEATHER_ID);
        assert_eq!(system.len(), 2);
        assert_eq!(system.weather_active(), Some(MaterialKind::Rain));
    }

    #[test]
    fn remove_and_clear_drop_emitters() {
        let mut system = ParticleSystem::new();
        let id = system.add(EmitterConfig::fire_demo());
        assert!(system.remove(id));
        assert!(!system.remove(id));
        assert!(system.is_empty());
        system.add(EmitterConfig::fire_demo());
        system.clear();
        assert!(system.is_empty());
    }

    #[test]
    fn batches_advance_sim_step_only_when_emitting() {
        let mut system = ParticleSystem::new();
        let id = system.add(EmitterConfig::fire_demo());

        let first = system.batches(0.016, [0.0; 3]).remove(0);
        let second = system.batches(0.016, [0.0; 3]).remove(0);
        // El contador de ping-pong alternó: el segundo frame lee el buffer opuesto.
        assert_ne!(first.read_is_a, second.read_is_a);

        system.remove(id);
        assert!(system.batches(0.016, [0.0; 3]).is_empty());
    }

    #[test]
    fn carry_accumulates_fractional_spawn() {
        let mut system = ParticleSystem::new();
        let mut config = EmitterConfig::fire_demo();
        config.rate = 30.0; // 30/s a 60 fps = 0.5 por frame
        let id = system.add(config);

        let first = system.batches(1.0 / 60.0, [0.0; 3]).remove(0);
        assert_eq!(first.spawn, 0); // 0.5 → acarreo
        let second = system.batches(1.0 / 60.0, [0.0; 3]).remove(0);
        assert_eq!(second.spawn, 1); // 0.5 + 0.5 → 1
        let _ = id;
    }

    #[test]
    fn dt_is_clamped_for_kernel_safety() {
        let mut system = ParticleSystem::new();
        system.add(EmitterConfig::fire_demo());
        let batch = system.batches(2.0, [0.0; 3]).remove(0);
        assert_eq!(batch.dt, MAX_DT);
        assert_eq!(batch.dt, 0.05);
    }

    #[test]
    fn fire_params_carry_flammability_upward() {
        let mut system = ParticleSystem::new();
        system.add(EmitterConfig::fire_demo());
        let batch = system.batches(0.016, [0.0; 3]).remove(0);
        assert!(batch.params.phys[0] > 0.0); // flotabilidad +Y
        assert!(batch.params.phys[2] > 0.0); // turbulencia
        assert!(batch.params.env[1] > 0.0); // spin
        assert!(batch.params.collide[2] < 1.0); // encoge al morir
        assert_eq!(batch.params.misc[0], MaterialKind::Fire.as_u32());
        assert_eq!(batch.params.misc[2], BILLBOARD_CAMERA);
        assert_eq!(batch.blend_alpha, 0); // aditivo
        assert_eq!(batch.wboit, 0); // pase directo
        assert_eq!(batch.soft, 1); // soft particles activos
    }

    #[test]
    fn smoke_goes_through_wboit() {
        let mut system = ParticleSystem::new();
        system.add(EmitterConfig::smoke());
        let batch = system.batches(0.016, [0.0; 3]).remove(0);
        assert_eq!(batch.wboit, 1);
        assert_eq!(batch.soft, 1);
        assert!((batch.params.phys[0] - 1.35).abs() < 0.01); // flotabilidad
        assert!((batch.params.phys[1] - 0.55).abs() < 0.01); // drag
        assert!((batch.params.velocity_spread[1] - 1.15).abs() < 0.01);
        assert!((batch.params.size_life[2] - 3.8).abs() < 0.01); // columna más alta
        // El humo crece con la edad: el shader lo decide, la CPU solo trae la base.
        assert!(batch.params.size_life[0] > 0.5);
    }

    #[test]
    fn fog_recycles_around_the_camera_without_ground_death() {
        let mut system = ParticleSystem::new();
        system.set_weather(Some(MaterialKind::Fog), 1.0, 1.0, 0.0);
        let batch = system.batches(0.016, [4.0, 2.0, 8.0]).remove(0);
        assert_eq!(batch.wboit, 1);
        assert_eq!(batch.params.origin_radius[0], 4.0);
        // Niebla rasante (F6): el kernel la apoya en el plano (0), no flota a -1000.
        assert_eq!(batch.params.env[0], 0.0);
        assert!(batch.params.volume[0] > 0.0);
        // Nubes enormes sin forma (F6): el usuario pidió nubes, no circulitos.
        assert!(batch.params.size_life[0] >= 8.0);
    }

    #[test]
    fn scene_fog_stays_on_the_emitter_not_the_camera() {
        let mut system = ParticleSystem::new();
        let mut config = EmitterConfig::scene_fog();
        config.origin = [3.0, 1.2, -2.0];
        system.add(config);
        let batch = system.batches(0.016, [40.0, 8.0, -12.0]).remove(0);
        assert_eq!(batch.wboit, 1);
        assert_eq!(batch.params.origin_radius[0], 3.0);
        assert_eq!(batch.params.origin_radius[2], -2.0);
        assert!(batch.params.volume[0] < 0.5);
        assert!(batch.params.origin_radius[3] > 1.0);
        assert!(batch.params.size_life[0] < 3.0);
        assert!(batch.params.size_life[2] <= 10.0);
    }

    #[test]
    fn sand_carries_granular_flags() {
        let mut system = ParticleSystem::new();
        system.add(EmitterConfig::sand());
        let batch = system.batches(0.016, [0.0; 3]).remove(0);
        assert_eq!(batch.params.misc[0], MaterialKind::Sand.as_u32());
        // bit0 colisión con suelo + bit2 salpicadura; sin acumulación.
        assert_eq!(batch.params.misc[3], 0b001); // suelo, sin splash elástico
        assert!(batch.params.phys[0] < -9.0); // gravedad real
        assert_eq!(batch.params.collide[0], 0.0); // sin rebote
        assert!(batch.params.collide[1] > 0.8); // fricción alta
        assert_eq!(batch.params.misc[2], BILLBOARD_CAMERA);
        assert_eq!(batch.wboit, 0); // pase directo con alpha
        assert!(batch.params.size_life[0] <= 0.008); // granos finos, no discos
        assert!(batch.params.collide[2] < 0.1); // encogen al morir
    }

    #[test]
    fn sand_dust_layer_is_softer_than_grains() {
        let grains = EmitterConfig::sand();
        let dust = EmitterConfig::sand_dust();
        assert!(dust.size > grains.size);
        assert!(dust.color_start[3] <= 0.06);
        assert!(dust.color_start[3] < grains.color_start[3]);
        assert!(!dust.ground_collision);
        assert!(dust.size_end > 1.0);
        assert!(grains.size <= 0.008);
    }

    #[test]
    fn water_splashes_through_wboit() {
        let mut system = ParticleSystem::new();
        system.add(EmitterConfig::water());
        let batch = system.batches(0.016, [0.0; 3]).remove(0);
        assert_eq!(batch.wboit, 1);
        assert_eq!(batch.params.misc[3], 0b101); // suelo + splat
        assert_eq!(batch.params.misc[2], BILLBOARD_STRETCHED);
        assert!(batch.params.collide[0] < 0.1); // casi sin rebote
        assert!(batch.params.collide[3] > 0.2); // stretch por velocidad
        assert!(batch.params.size_life[0] <= 0.012);
        assert!(batch.params.collide[2] <= 0.02); // no crecen al morir
        assert!(batch.params.color_start[3] <= 0.25);
    }

    #[test]
    fn snow_accumulates_on_the_ground() {
        let mut system = ParticleSystem::new();
        system.set_weather(Some(MaterialKind::Snow), 1.0, 1.0, 0.0);
        let batch = system.batches(0.016, [0.0; 3]).remove(0);
        assert_eq!(batch.params.misc[3], 0b011); // suelo + acumulación, sin salpicadura
        assert_eq!(batch.wboit, 0); // nieve sigue por alpha directo
    }

    #[test]
    fn weather_follows_the_camera() {
        let mut system = ParticleSystem::new();
        system.set_weather(Some(MaterialKind::Rain), 5.0, 2.0, -4.0);
        let batch = system.batches(0.016, [12.0, 5.0, -3.0]).remove(0);
        // El origen del emisor de clima se re-centra en la cámara cada frame.
        assert_eq!(batch.params.origin_radius[0], 12.0);
        assert_eq!(batch.params.origin_radius[1], 5.0);
        assert_eq!(batch.params.origin_radius[2], -3.0);
        assert_eq!(batch.params.misc[0], MaterialKind::Rain.as_u32());
        assert_eq!(batch.params.misc[2], 1); // billboard orientado a velocidad
        assert_eq!(batch.blend_alpha, 1); // alpha estándar
        assert!(batch.params.volume[0] > 0.0);
        // Viento a la izquierda (select del editor) y caída acelerada ×2.
        assert_eq!(batch.params.env[2], -4.0);
        assert_eq!(batch.params.velocity_spread[1], -36.0);
        // La densidad escala con la intensidad (capacidad, no solo tasa).
        assert_eq!(batch.max_particles, 2560);
    }

    #[test]
    fn weather_switch_replaces_the_single_emitter() {
        let mut system = ParticleSystem::new();
        system.set_weather(Some(MaterialKind::Rain), 1.0, 1.0, 0.0);
        system.add(EmitterConfig::fire_demo());
        assert_eq!(system.len(), 2);

        system.set_weather(Some(MaterialKind::Snow), 0.5, 1.0, 0.0);
        // El clima anterior desapareció, el fuego sobrevive, la nieve está activa.
        assert_eq!(system.len(), 2);
        assert_eq!(system.weather_active(), Some(MaterialKind::Snow));

        system.set_weather(None, 1.0, 1.0, 0.0);
        assert_eq!(system.weather_active(), None);
        assert_eq!(system.len(), 1); // solo el fuego
    }

    #[test]
    fn weather_intensity_scales_density() {
        let mut system = ParticleSystem::new();
        system.set_weather(Some(MaterialKind::Rain), 10.0, 1.0, 0.0);
        let full = system.batches(1.0, [0.0; 3]).remove(0);
        system.set_weather(Some(MaterialKind::Rain), 1.0, 1.0, 0.0);
        let light = system.batches(1.0, [0.0; 3]).remove(0);
        // La intensidad escala densidad: capacidad Y tasa (lluvia a 10 = supernavalga).
        assert!(full.max_particles > light.max_particles * 5);
        assert!(full.spawn > light.spawn * 5);
    }
}
