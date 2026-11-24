//! Luz direccional (sol) para iluminación PBR compatible con pipelines RTX.
//! La posición del sol en el mundo define la dirección de la luz.

use glam::{Mat4, Vec3};

use crate::engine3d::config_3d::world_bounds::{SKY_SPHERE_RADIUS, WorldBounds3D};
use crate::engine3d::ecs::{EntityId, MeshComponent, Transform};
use crate::engine3d::engine::State;
use crate::engine3d::entity_save_meta::EntitySaveMeta;

/// Dirección por defecto (hacia la luz, mismo sentido que el shader histórico).
pub const DEFAULT_LIGHT_DIR: Vec3 = Vec3::new(0.6, 1.0, 0.4);
/// Color de la luz direccional (intensidad ~1; evita saturar el framebuffer sRGB).
pub const DEFAULT_LIGHT_COLOR: Vec3 = Vec3::new(1.0, 0.96, 0.88);
/// Factor de ambiente (0–1). Bajo = caras no iluminadas más oscuras.
pub const DEFAULT_LIGHT_AMBIENT: f32 = 0.06;
/// Multiplicador de color de luz direccional.
pub const DEFAULT_LIGHT_INTENSITY: f32 = 1.0;
/// Factor de oscuridad en zona de sombra (0–1). Alto = sombras más oscuras.
pub const DEFAULT_SHADOW_DARKNESS: f32 = 0.9;
/// Mínimo del rango de oscuridad de sombra en la UI.
pub const SHADOW_DARKNESS_MIN: f32 = 0.9;
/// Máximo del rango de oscuridad de sombra en la UI.
pub const SHADOW_DARKNESS_MAX: f32 = 1.0;
/// Desplazamiento mínimo hacia la luz al comparar sombras (metros).
/// Restaurado tras a125635: con 0.0 la comparación oscilaba con cada
/// micro-movimiento del sol (saltos rápidos con desplazamiento casi nulo).
pub const SHADOW_NORMAL_BIAS_MIN: f32 = 0.004;
/// Desplazamiento máximo en superficies casi paralelas a los rayos del sol.
pub const SHADOW_NORMAL_BIAS_MAX: f32 = 0.028;
/// Offset constante en profundidad del comparador de sombras (NDC).
pub const SHADOW_DEPTH_BIAS_CONST: f32 = 0.0008;
/// Escala de bias por pendiente (1 - N·L).
pub const SHADOW_DEPTH_BIAS_SLOPE: f32 = 0.002;
/// Margen mínimo (m) para buscar casters fuera del frustum de la cámara.
pub const SHADOW_CASTER_MARGIN_MIN_M: f32 = 8.0;
fn shadow_ortho_extent_m(world_radius: f32) -> f32 {
    let diagonal = (world_radius * 2.0) * 3.0_f32.sqrt();
    diagonal.max(48.0) * 0.55
}

/// Metros por texel de una ortho de lado `extent_m` con un mapa de `map_size`.
fn shadow_world_texel_from(extent_m: f32, map_size: u32) -> f32 {
    2.0 * extent_m / map_size.max(1) as f32
}

/// Con el sol bajo, las sombras se alargan: ampliar la ortho evita que se
/// recorten en el borde del mapa (que el shader interpreta como "sin sombra").
fn shadow_grazing_scale(dir_y: f32) -> f32 {
    let grazing = 1.0 - dir_y.clamp(0.0, 1.0);
    1.0 + 0.35 * grazing * grazing
}

/// Distancia «aguas arriba del sol» que se añade al AABB de cada caster durante
/// el culling. Crece con el ángulo rasante: con el sol bajo las sombras se
/// alargan y objetos lejos de la vista pueden proyectar dentro de ella.
fn shadow_caster_margin_from(
    world_radius: f32,
    dir_y: f32,
    override_m: Option<f32>,
    enabled: bool,
) -> f32 {
    if !enabled {
        return 0.0;
    }
    if let Some(margin) = override_m {
        return margin.max(0.0);
    }
    let grazing = shadow_grazing_scale(dir_y);
    let extent = shadow_ortho_extent_m(world_radius) * grazing;
    (world_radius * grazing).clamp(SHADOW_CASTER_MARGIN_MIN_M, extent * 2.0)
}

/// Margen sobre el suelo (m) para considerar que el sol proyecta sombras.
/// Debajo, el shadow map se capturaría desde el subsuelo y sería inválido.
pub const SUN_SHADOW_HORIZON_MARGIN_M: f32 = 0.5;
/// Radio orbital fijo como fracción de la esfera de cielo: 1600 × 0.85 = 1360 m.
/// El sol queda lejos de cualquier mundo permitido; la diagonal de la caja ya
/// no acorta la órbita cuando cambia la altura o el tamaño del terreno.
const SUN_ORBIT_INSIDE_SKY_FACTOR: f32 = 0.85;
/// Distancia orbital con la que se calibró el tamaño del halo (semidiagonal del
/// mundo por defecto × el factor histórico ≈ 88 m). Solo sirve de referencia
/// para conservar el tamaño ANGULAR del sol con la órbita fija actual.
const SUN_HALO_REFERENCE_DISTANCE_M: f32 = 88.0;

/// Un valor de la UI (1000 por tramo) equivale a 1 hora → 1 unidad = 3.6 s.
pub const SUN_CYCLE_SECONDS_PER_UNIT: f32 = 3.6;
/// Valor por defecto de CADA input: 1000 ms de día + 1000 ms de noche = 2000.
pub const SUN_CYCLE_DEFAULT_UI_VALUE: f32 = 1000.0;
/// Duración por defecto de cada tramo en segundos (1 h cada uno → 2 h la vuelta).
pub const DEFAULT_SUN_CYCLE_SEGMENT_SECS: f32 = 3600.0;
/// Punto cardinal por el que SALE el sol (la puesta es el opuesto).
/// Coincide con la rosa de los vientos de `world_bounds.rs`: norte = −Z.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SunRiseCardinal {
    #[default]
    East,
    North,
    West,
    South,
}

impl SunRiseCardinal {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::East => "east",
            Self::North => "north",
            Self::West => "west",
            Self::South => "south",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "north" => Self::North,
            "west" => Self::West,
            "south" => Self::South,
            _ => Self::East,
        }
    }

    /// Rotación alrededor de Y que lleva el amanecer de +X (Este) al cardinal elegido.
    const fn yaw_radians(self) -> f32 {
        match self {
            Self::East => 0.0,
            Self::North => std::f32::consts::FRAC_PI_2,
            Self::West => std::f32::consts::PI,
            Self::South => -std::f32::consts::FRAC_PI_2,
        }
    }
}

fn rotate_y(v: Vec3, angle: f32) -> Vec3 {
    let (s, c) = angle.sin_cos();
    Vec3::new(v.x * c + v.z * s, v.y, -v.x * s + v.z * c)
}

/// Dirección orbital local (antes de aplicar el cardinal): círculo en el plano
/// vertical X-Y, así que el sol llega al punto más bajo justo bajo el centro.
fn sun_local_direction(phase: f32) -> Vec3 {
    let a = phase.rem_euclid(1.0) * std::f32::consts::TAU;
    Vec3::new(a.cos(), a.sin(), 0.0)
}

/// Dirección mundial del sol: órbita local girada al cardinal de salida.
fn sun_cycle_direction(phase: f32, cardinal: SunRiseCardinal) -> Vec3 {
    rotate_y(sun_local_direction(phase), cardinal.yaw_radians())
}

/// Centro del terreno sobre el suelo: punto al que mira la cámara de luz y
/// centro de la órbita del sol. La altura de la caja no desplaza el ancla:
/// las sombras y el horizonte se miden desde `y = 0`.
pub(crate) fn sun_orbit_center(bounds: &WorldBounds3D) -> Vec3 {
    Vec3::new(bounds.center().x, 0.0, bounds.center().z)
}

/// Altura real (m) del icono del sol sobre el suelo (`y = 0`).
/// Es la referencia del horizonte: no depende del radio de la esfera del cielo,
/// que es fijo y mucho mayor que la órbita del sol. Con `dir.y = 0` el sol
/// está exactamente en el horizonte del suelo.
pub(crate) fn sun_world_y(bounds: &WorldBounds3D, dir: Vec3, orbit_distance: f32) -> f32 {
    sun_orbit_center(bounds).y + dir.y * orbit_distance
}

/// Factor de luz diurna [0,1] según la altura real del sol: 1 de día, 0 de
/// noche, con crepúsculo suave alrededor del suelo (`y = 0`).
pub fn day_factor(sun_y: f32, orbit_distance: f32) -> f32 {
    let band = (orbit_distance * 0.08).max(1.0);
    let t = ((sun_y + band) / (2.0 * band)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Color de la luz solar según la altura del sol sobre el suelo.
///
/// Se tiñe solo cerca del horizonte: blanco al salir (`local.x > 0`, subiendo)
/// y rojizo al ponerse (`local.x < 0`, bajando).
pub fn sun_color_for_sun_height(sun_y: f32, orbit_distance: f32, local_dir: Vec3) -> Vec3 {
    // Banda estrecha: con el radio fijo de 1360 m, el 0.30 anterior teñía de
    // rojo desde ~400 m de altura (varios grados antes del horizonte).
    // 0.08 ≈ 109 m deja el color neutro casi hasta tocar el suelo.
    let band = (orbit_distance * 0.08).max(1.0);
    let proximity = 1.0 - (sun_y.max(0.0) / band).clamp(0.0, 1.0);
    let warmth = proximity * proximity;
    let sunrise = (local_dir.x > 0.0) as u8 as f32;
    let sunset = 1.0 - sunrise;
    let dawn_tint = Vec3::new(1.0, 1.0, 1.0);
    let dusk_tint = Vec3::new(1.0, 0.42, 0.24);
    let tint = dawn_tint * sunrise + dusk_tint * sunset;
    DEFAULT_LIGHT_COLOR.lerp(tint, warmth * 0.9)
}

/// Versión cómoda para la fase: la rotación del cardinal no cambia `dir.y`,
/// así que la altura se calcula igual con la dirección orbital local.
pub(crate) fn sun_color_for_phase(phase: f32, bounds: &WorldBounds3D) -> Vec3 {
    let local = sun_local_direction(phase);
    let distance = sun_orbit_distance_from(bounds);
    sun_color_for_sun_height(sun_world_y(bounds, local, distance), distance, local)
}

/// Fase [0,1) correspondiente a una dirección orbital (inversa de `sun_cycle_direction`).
fn sun_phase_from_direction(dir: Vec3, cardinal: SunRiseCardinal) -> f32 {
    let local = rotate_y(dir, -cardinal.yaw_radians());
    local.y.atan2(local.x).rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU
}

/// Posición del sol en su órbita para unos límites y una fase dados.
/// Única fuente de verdad: la usan el motor y el generador de escenas nuevas.
pub(crate) fn sun_position_from_bounds(
    bounds: &WorldBounds3D,
    phase: f32,
    cardinal: SunRiseCardinal,
) -> Vec3 {
    sun_orbit_center(bounds)
        + sun_cycle_direction(phase, cardinal) * sun_orbit_distance_from(bounds)
}

/// Radio de la órbita del sol: fijo, dentro de la esfera de cielo. Ya no depende
/// del tamaño ni de la diagonal de la caja; `_bounds` se conserva en la firma
/// para no tocar a los callers. Se calcula aparte de `State` para poder
/// comprobarlo sin Vulkan.
pub(crate) fn sun_orbit_distance_from(_bounds: &WorldBounds3D) -> f32 {
    SKY_SPHERE_RADIUS * SUN_ORBIT_INSIDE_SKY_FACTOR
}

/// Radio del emisor de halo (m) según la escala del icono y la distancia
/// orbital. Crece con la distancia para conservar el tamaño angular de siempre:
/// a 1360 m un halo de 0.56 m medía apenas unos píxeles y el sol desaparecía
/// aunque estuviera dentro del far plane. La escala del transform sigue
/// pudiendo agrandarlo a voluntad.
pub(crate) fn sun_halo_radius_m(scale: f32, orbit_distance: f32) -> f32 {
    let base = (scale.abs() * 0.5 + 0.06).max(0.1);
    let distance = orbit_distance.max(SUN_HALO_REFERENCE_DISTANCE_M);
    base * (distance / SUN_HALO_REFERENCE_DISTANCE_M)
}

/// Matriz vista-proyección de la luz. Separada de [`State`] para poder comprobar
/// que cambia con la dirección del sol sin necesidad de un `State` con Vulkan.
///
/// `up` debe ser la normal del plano de órbita del sol (constante para un cardinal),
/// nunca un valor que cambie con `dir.y`: el antiguo umbral `|dir.y| > 0.99`
/// conmutaba entre X e Y y giraba la base de la luz 180°, reflejando el shadow map
/// cada vez que el sol cruzaba el cenit.
fn light_view_proj_from(
    center: Vec3,
    dir: Vec3,
    extent: f32,
    map_size: u32,
    up: Vec3,
) -> [[f32; 4]; 4] {
    let dir = dir.normalize();
    let eye = center + dir * extent * 2.5;
    let view = Mat4::look_at_rh(eye, center, up);
    let proj = Mat4::orthographic_rh(-extent, extent, -extent, extent, 0.5, extent * 8.0);
    let mut m = (proj * view).to_cols_array_2d();

    // Texel snapping: anclar el centro del SUELO (donde se ven las sombras) a un
    // centro de texel del shadow map. Sin esto, la matriz de luz rota cada frame
    // con el sol y el punto se desliza subpíxel -> el comparador de profundidad
    // oscila (temblor). Con round(), la rejilla salta en pasos enteros de texel
    // y el depth de cada texel deja de recalcularse en cada micro-movimiento.
    let half = map_size.max(1) as f32 * 0.5;
    let anchor = glam::Vec4::new(center.x, 0.0, center.z, 1.0);
    let clip_x = m[0][0] * anchor.x + m[1][0] * anchor.y + m[2][0] * anchor.z + m[3][0] * anchor.w;
    let clip_y = m[0][1] * anchor.x + m[1][1] * anchor.y + m[2][1] * anchor.z + m[3][1] * anchor.w;
    let clip_w = m[0][3] * anchor.x + m[1][3] * anchor.y + m[2][3] * anchor.z + m[3][3] * anchor.w;
    let px = (clip_x / clip_w) * half;
    let py = (clip_y / clip_w) * half;
    let shift_x = (px.round() - px) / half;
    let shift_y = (py.round() - py) / half;
    // columna 3 = traslación NDC de la ortho; m[col][fila].
    m[3][0] += shift_x;
    m[3][1] += shift_y;
    m
}

/// Semi-lado mínimo de la ortho para que, con el ancla en el suelo, la cámara
/// de luz llegue desde el centro del terreno hasta las 8 esquinas de la caja.
fn shadow_ground_corner_extent_m(bounds: &WorldBounds3D) -> f32 {
    let ground = sun_orbit_center(bounds);
    let min = bounds.min_corner();
    let max = bounds.max_corner();
    let mut needed = 0.0_f32;
    for x in [min.x, max.x] {
        for y in [min.y, max.y] {
            for z in [min.z, max.z] {
                needed = needed.max(ground.distance(Vec3::new(x, y, z)));
            }
        }
    }
    needed
}

/// Semi-lado de la ortho de la luz para unos límites y una altura del sol.
fn light_shadow_extent_for(bounds: &WorldBounds3D, dir_y: f32) -> f32 {
    shadow_ortho_extent_m(bounds.radius()).max(shadow_ground_corner_extent_m(bounds))
        * shadow_grazing_scale(dir_y)
}

/// Matriz de luz para unos límites: usa siempre el suelo como centro, igual que
/// `State::build_light_view_proj`. Separada de `State` para probarla sin Vulkan.
fn light_view_proj_for_bounds(
    bounds: &WorldBounds3D,
    dir: Vec3,
    extent: f32,
    map_size: u32,
    cardinal: SunRiseCardinal,
) -> [[f32; 4]; 4] {
    let up = match cardinal {
        SunRiseCardinal::East | SunRiseCardinal::West => Vec3::Z,
        SunRiseCardinal::North | SunRiseCardinal::South => Vec3::X,
    };
    light_view_proj_from(sun_orbit_center(bounds), dir, extent, map_size, up)
}

impl State {
    /// Distancia del icono: fuera del cuadro de límites pero dentro de la esfera de cielo.
    pub(crate) fn sun_orbit_distance(&self) -> f32 {
        sun_orbit_distance_from(&self.world_bounds_3d)
    }

    pub(crate) fn default_sun_position(&self) -> Vec3 {
        sun_position_from_bounds(
            &self.world_bounds_3d,
            self.sun_cycle_phase,
            self.sun_rise_cardinal,
        )
    }

    /// Centro del terreno en el suelo: punto al que mira la cámara de luz y
    /// centro de la órbita del sol (misma ancla que el snapping de sombras).
    pub(crate) fn directional_light_scene_center(&self) -> Vec3 {
        sun_orbit_center(&self.world_bounds_3d)
    }

    /// Actualiza la dirección de luz: del centro de la escena hacia el icono del sol.
    pub(crate) fn sync_directional_light_from_sun(&mut self) {
        let Some(sun_id) = self.sun_entity else {
            return;
        };
        let Some(t) = self.world.get::<Transform>(sun_id) else {
            return;
        };
        let to_sun = t.position - self.directional_light_scene_center();
        if to_sun.length_squared() > 1e-8 {
            self.directional_light_dir = to_sun.normalize();
            // Si se movió con el gizmo, la fase del ciclo sigue al sol.
            self.sun_cycle_phase =
                sun_phase_from_direction(self.directional_light_dir, self.sun_rise_cardinal);
            self.directional_light_color =
                sun_color_for_phase(self.sun_cycle_phase, &self.world_bounds_3d);
        }
    }

    /// Coloca el sol en la fase indicada de su órbita (sin cambiar el radio).
    pub(crate) fn set_sun_cycle_phase(&mut self, phase: f32) {
        self.sun_cycle_phase = phase.rem_euclid(1.0);
        let Some(id) = self.sun_entity else {
            return;
        };
        let position = sun_position_from_bounds(
            &self.world_bounds_3d,
            self.sun_cycle_phase,
            self.sun_rise_cardinal,
        );
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.position = position;
        }
        let center = self.directional_light_scene_center();
        self.directional_light_dir = (position - center).normalize_or_zero();
        self.directional_light_color =
            sun_color_for_phase(self.sun_cycle_phase, &self.world_bounds_3d);
    }

    /// Avanza el ciclo día/noche un frame. Cada tramo (sol sobre el suelo /
    /// bajo el suelo) tiene su propia duración, así se puede acortar la noche.
    pub(crate) fn tick_sun_cycle(&mut self, dt: f32) -> f32 {
        if self.sun_cycle_enabled {
            let is_day = self.sun_above_horizon();
            let period = if is_day {
                self.sun_cycle_day_secs
            } else {
                self.sun_cycle_night_secs
            };
            if period > 0.0 {
                let phase = self.sun_cycle_phase + dt / period;
                self.set_sun_cycle_phase(phase);
            }
        }
        self.sun_cycle_phase
    }

    /// Recoloca el icono fuera de la caja, sin salir de la esfera de cielo.
    /// Conserva la dirección. No emite `ModelLoaded` para no mover la cámara.
    pub(crate) fn keep_sun_inside_sky(&mut self) {
        let Some(id) = self.sun_entity else {
            return;
        };
        let center = self.directional_light_scene_center();
        let target = self.sun_orbit_distance();
        let Some(transform) = self.world.get::<Transform>(id) else {
            return;
        };
        let offset = transform.position - center;
        let dir = if offset.length_squared() > 1e-8 {
            offset.normalize()
        } else {
            DEFAULT_LIGHT_DIR.normalize()
        };
        if (offset.length() - target).abs() < 0.5 {
            return;
        }
        let position = center + dir * target;
        if let Some(transform) = self.world.get_mut::<Transform>(id) {
            transform.position = position;
        }
        self.sync_directional_light_from_sun();
        log::info!(
            "[luz] sol a {:.1} m del centro (esfera {:.1} m, fuera de la caja)",
            target,
            self.world_bounds_3d.sky_uniform().center_radius[3]
        );
    }

    /// Tras mover el sol con el gizmo lo devuelve a su órbita (fuera de la caja
    /// y dentro de la esfera): el arrastre solo cambia la dirección de la luz.
    /// El halo sigue renderizándose aunque quede fuera de los límites del mundo.
    pub(crate) fn constrain_sun_to_sky(&mut self) {
        let Some(id) = self.sun_entity else {
            return;
        };
        let center = self.directional_light_scene_center();
        let target = self.sun_orbit_distance();
        let Some(transform) = self.world.get::<Transform>(id) else {
            return;
        };
        let offset = transform.position - center;
        let dir = if offset.length_squared() > 1e-8 {
            offset.normalize()
        } else {
            DEFAULT_LIGHT_DIR.normalize()
        };
        if let Some(transform) = self.world.get_mut::<Transform>(id) {
            transform.position = center + dir * target;
        }
        self.sync_directional_light_from_sun();
    }

    /// Misma posición que `ensure_default_sun` / plantilla FP al arrancar sin `.save`.
    pub(crate) fn align_editor_sun_to_default_position(&mut self) {
        let pos = self.default_sun_position();
        if let Some(id) = self.sun_entity {
            self.clear_sun_visual(id);
            if let Some(t) = self.world.get_mut::<Transform>(id) {
                t.position = pos;
                t.scale = Vec3::splat(1.0);
            }
            self.sync_directional_light_from_sun();
            let label = self
                .entity_display_name(id)
                .unwrap_or_else(|| rer_engine_core::editor_defaults::entity_label::SUN.to_string());
            self.send_model_loaded_event(id, &label);
        } else {
            self.spawn_sun("", pos.to_array(), [1.0, 1.0, 1.0]);
        }
    }

    pub(crate) fn scene_light_dir(&self) -> [f32; 4] {
        // De noche el ambiente baja mucho (pero no a cero: no es oscuridad absoluta).
        let day = self.day_factor();
        let ambient = self.directional_light_ambient * (0.3 + 0.7 * day);
        [
            self.directional_light_dir.x,
            self.directional_light_dir.y,
            self.directional_light_dir.z,
            ambient,
        ]
    }

    pub(crate) fn scene_light_color(&self) -> [f32; 4] {
        let shadows_enabled = if self.shadow_tier
            == crate::engine3d::config_3d::shadow_graphics::ShadowTier::Off
            || !self.sun_above_horizon()
        {
            0.0
        } else {
            1.0
        };
        [
            self.directional_light_color.x,
            self.directional_light_color.y,
            self.directional_light_color.z,
            shadows_enabled,
        ]
    }

    /// Lado real del shadow map según el tier, en px. Con `Off` no se renderiza ni se muestrea,
    /// pero la matriz de luz se sigue construyendo: se usa el tamaño base para que el snapping al
    /// texel no trabaje con un mapa de 1 px.
    pub(crate) fn active_shadow_map_size(&self) -> u32 {
        if self.shadow_tier == crate::engine3d::config_3d::shadow_graphics::ShadowTier::Off {
            crate::engine3d::engine::SHADOW_MAP_SIZE
        } else {
            self.shadow_map_size
                .max(crate::engine3d::engine::SHADOW_MAP_SIZE / 2)
        }
    }

    /// x = intensidad; y = oscuridad sombra (lit-composite / pase transparente); z = 1/texel sombra; w = radio PCF.
    pub(crate) fn scene_light_params(&self) -> [f32; 4] {
        let dist = self.camera.distance;
        // Radio PCF algo mayor con shadow map 1024 para compensar resolución.
        let pcf_radius = if dist < 12.0 {
            0.85
        } else if dist < 28.0 {
            0.95
        } else {
            1.15
        };
        // El radio PCF se expresa en TEXELES del mapa actual: sin escalarlo, pasar de 1024 a 4096
        // hace la penumbra 4 veces más fina en el mundo y la sombra se ve igual en todos los tiers
        // (solo cambia el coste). Con raíz cuadrada la penumbra se mantiene parecida mientras el
        // detalle sube de verdad.
        let size = self.active_shadow_map_size() as f32;
        let scale = (size / crate::engine3d::engine::SHADOW_MAP_SIZE as f32).sqrt();
        let day = self.day_factor();
        [
            self.light_intensity * (0.4 + 0.6 * day),
            self.shadow_darkness,
            1.0 / size,
            pcf_radius * scale,
        ]
    }

    pub(crate) fn scene_shadow_bias(&self) -> [f32; 4] {
        [
            SHADOW_NORMAL_BIAS_MIN,
            SHADOW_NORMAL_BIAS_MAX,
            SHADOW_DEPTH_BIAS_CONST,
            SHADOW_DEPTH_BIAS_SLOPE,
        ]
    }

    /// Factor de luz diurna del frame: altura real del sol sobre el suelo.
    /// No depende del radio (fijo) de la esfera del cielo.
    pub(crate) fn day_factor(&self) -> f32 {
        let distance = self.sun_orbit_distance();
        day_factor(
            sun_world_y(&self.world_bounds_3d, self.directional_light_dir, distance),
            distance,
        )
    }

    /// `true` mientras el sol está físicamente sobre el suelo (sendas sombras válidas).
    pub(crate) fn sun_above_horizon(&self) -> bool {
        sun_world_y(
            &self.world_bounds_3d,
            self.directional_light_dir,
            self.sun_orbit_distance(),
        ) > SUN_SHADOW_HORIZON_MARGIN_M
    }

    /// Semi-lado de la ortho de la luz (misma fórmula que `build_light_view_proj`).
    /// Crece con el ángulo rasante y, con el centro en el suelo, con la distancia
    /// hasta las esquinas altas de la caja para que el techo no quede cortado.
    pub(crate) fn light_shadow_extent_m(&self) -> f32 {
        light_shadow_extent_for(&self.world_bounds_3d, self.directional_light_dir.y)
    }

    /// Metros por texel del mapa de sombras. El shader del terreno usa esto de bias.
    pub(crate) fn shadow_world_texel_m(&self) -> f32 {
        shadow_world_texel_from(self.light_shadow_extent_m(), self.active_shadow_map_size())
    }

    /// Margen de culling de casters: objetos fuera del frustum de la cámara a
    /// esta distancia «aguas arriba» del sol pueden proyectar sombra en la vista.
    /// `0` desactiva la expansión (modo heredado).
    pub(crate) fn shadow_caster_margin_m(&self) -> f32 {
        shadow_caster_margin_from(
            self.world_bounds_3d.radius(),
            self.directional_light_dir.y,
            self.shadow_caster_margin_override_m,
            self.shadow_caster_culling_enabled,
        )
    }

    /// Proyección ortográfica de la luz que cubre los bounds del mundo.
    pub(crate) fn build_light_view_proj(&self) -> [[f32; 4]; 4] {
        light_view_proj_for_bounds(
            &self.world_bounds_3d,
            self.directional_light_dir,
            self.light_shadow_extent_m(),
            self.active_shadow_map_size(),
            self.sun_rise_cardinal,
        )
    }

    pub(crate) fn apply_directional_light_settings(
        &mut self,
        ambient: Option<f32>,
        intensity: Option<f32>,
        shadow_darkness: Option<f32>,
    ) {
        if let Some(v) = ambient {
            self.directional_light_ambient = v.clamp(0.0, 1.0);
        }
        if let Some(v) = intensity {
            self.light_intensity = v.clamp(0.05, 4.0);
        }
        if let Some(v) = shadow_darkness {
            self.shadow_darkness = v.clamp(SHADOW_DARKNESS_MIN, SHADOW_DARKNESS_MAX);
        }
    }

    /// Aplica los ajustes del ciclo día/noche. Los valores UI (1000 cada uno)
    /// se convierten a segundos aquí: 1000 + 1000 = 2000 → 2 horas por vuelta.
    pub(crate) fn apply_sun_cycle_settings(
        &mut self,
        enabled: Option<bool>,
        cardinal: Option<&str>,
        day_ui: Option<f32>,
        night_ui: Option<f32>,
        phase: Option<f32>,
    ) {
        if let Some(value) = enabled {
            self.sun_cycle_enabled = value;
        }
        if let Some(value) = cardinal {
            let next = SunRiseCardinal::parse(value);
            if next != self.sun_rise_cardinal {
                self.sun_rise_cardinal = next;
                self.set_sun_cycle_phase(self.sun_cycle_phase);
                log::info!("[luz] el sol sale por {}", next.as_str());
            }
        }
        if let Some(value) = day_ui {
            self.sun_cycle_day_secs = (value * SUN_CYCLE_SECONDS_PER_UNIT).max(1.0);
        }
        if let Some(value) = night_ui {
            self.sun_cycle_night_secs = (value * SUN_CYCLE_SECONDS_PER_UNIT).max(1.0);
        }
        if let Some(value) = phase {
            self.set_sun_cycle_phase(value);
        }
    }

    pub(crate) fn sun_icon_mesh_idx(&mut self) -> usize {
        if let Some(idx) = self.sun_icon_mesh_idx {
            return idx;
        }
        let idx = self.meshes.len();
        self.meshes
            .push(crate::engine3d::mesh::create_uv_sphere(&self.vulkan, 20));
        self.sun_icon_mesh_idx = Some(idx);
        idx
    }

    fn sun_icon_texture_idx(&mut self) -> usize {
        if let Some(idx) = self.sun_icon_tex_idx {
            return idx;
        }
        const N: u32 = 32;
        let mut px: Vec<u8> = Vec::with_capacity((N * N * 4) as usize);
        for y in 0..N {
            for x in 0..N {
                let u = (x as f32 + 0.5) / N as f32 * 2.0 - 1.0;
                let v = (y as f32 + 0.5) / N as f32 * 2.0 - 1.0;
                let r = (u * u + v * v).sqrt().min(1.0);
                let t = (1.0 - r).powf(0.55);
                let r8 = (255.0 * (0.97 + 0.03 * t)) as u8;
                let g8 = (255.0 * (0.96 + 0.04 * t)) as u8;
                let b8 = (255.0 * (0.93 + 0.07 * t)) as u8;
                px.extend_from_slice(&[r8, g8, b8, 255]);
            }
        }
        let tex_idx = self.tex_layers.len();
        let layer = self.texture_array.pack(&self.vulkan, &px, N, N).unwrap();
        self.tex_layers.push(layer);
        self.sun_icon_tex_idx = Some(tex_idx);
        tex_idx
    }

    /// Malla de la esfera/icono: la usan las pelotas físicas. El sol ya no la
    /// usa: su única imagen es la partícula de halo.
    fn apply_sun_icon_visual(&mut self, id: crate::engine3d::ecs::EntityId) {
        let mesh_idx = self.sun_icon_mesh_idx();
        let tex_idx = self.sun_icon_texture_idx();
        if let Some(mc) = self.world.get_mut::<MeshComponent>(id) {
            mc.mesh_idx = mesh_idx;
            mc.tex_idx = tex_idx;
        } else {
            self.world.insert(id, MeshComponent { mesh_idx, tex_idx });
        }
    }

    /// El sol es una entidad de LUZ: mantiene el `Transform` para la dirección
    /// de la luz y para el gizmo, pero no tiene geometría. Sin malla no puede
    /// proyectar sombra ni dibujar la esfera; el destello lo pone el emisor.
    pub(crate) fn clear_sun_visual(&mut self, id: crate::engine3d::ecs::EntityId) {
        self.world.remove_component::<MeshComponent>(id);
        self.world
            .remove_component::<crate::engine3d::ecs::MeshLocalAabb>(id);
    }

    /// Entidad de luz del sol: solo `Transform` (dirección + gizmo), sin malla.
    /// Idempotente al recargar `.save`.
    pub(crate) fn spawn_sun(&mut self, name: &str, position: [f32; 3], scale: [f32; 3]) {
        let label = self
            .resolve_entity_display_name(name, rer_engine_core::editor_defaults::entity_label::SUN);
        if let Some(id) = self.sun_entity {
            self.clear_sun_visual(id);
            if let Some(t) = self.world.get_mut::<Transform>(id) {
                t.position = Vec3::from_array(position);
                let s = scale.into_iter().fold(f32::INFINITY, f32::min).max(0.15);
                t.scale = Vec3::splat(s);
            }
            self.sync_directional_light_from_sun();
            self.send_model_loaded_event(id, &label);
            return;
        }

        let id = self.world.spawn(Some(&label));
        self.clear_sun_visual(id);
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.position = Vec3::from_array(position);
            let s = scale.into_iter().fold(f32::INFINITY, f32::min).max(0.15);
            t.scale = Vec3::splat(s);
        }

        self.sun_entity = Some(id);
        self.sync_directional_light_from_sun();
        self.save_registry.register_meta(
            id,
            EntitySaveMeta {
                kind: "directional_light".to_string(),
                path: "[Sun]".to_string(),
                visual_model_path: None,
                entity_category: None,
            },
        );
        self.send_model_loaded_event(id, &label);
    }

    pub(crate) fn ensure_default_sun(&mut self) {
        if self.sun_entity.is_some() {
            return;
        }
        let pos = self.default_sun_position();
        self.spawn_sun("", pos.to_array(), [1.0, 1.0, 1.0]);
    }

    /// La corona de partículas sigue al icono del sol: se recalcula cada frame
    /// porque el gizmo puede moverlo o cambiar su escala. El radio de emisión es
    /// la superficie de la esfera visual (radio 0.5 × escala) más un pequeño halo.
    pub(crate) fn sync_sun_particles(&mut self) {
        use crate::engine3d::particles::SUN_EMITTER_ID;

        let Some(id) = self.sun_entity else {
            self.particles.remove(SUN_EMITTER_ID);
            return;
        };
        let Some(transform) = self.world.get::<Transform>(id) else {
            self.particles.remove(SUN_EMITTER_ID);
            return;
        };
        let scale = transform
            .scale
            .x
            .abs()
            .min(transform.scale.y.abs())
            .min(transform.scale.z.abs());
        // El halo escala con la órbita fija: a 1360 m conserva el tamaño
        // angular que tenía con el radio derivado del mundo (~88 m).
        let radius = sun_halo_radius_m(scale, self.sun_orbit_distance());
        let light = self.directional_light_color;
        let halo_color = [light.x, light.y, light.z, 1.0];
        self.particles
            .sync_sun_emitter(transform.position.to_array(), radius, halo_color);
    }

    /// Esfera visual como el sol (malla UV + textura) sin luz direccional; física esférica.
    pub(crate) fn spawn_physics_ball(
        &mut self,
        name: &str,
        position: [f32; 3],
        scale: [f32; 3],
        physics_type: &str,
    ) -> EntityId {
        use crate::engine3d::ipc::EngineEvent;
        use crate::engine3d::ipc::send_event;

        let label = self.resolve_entity_display_name(
            name,
            rer_engine_core::editor_defaults::entity_label::BALL,
        );
        let id = self.world.spawn(Some(&label));
        self.apply_sun_icon_visual(id);
        // La pelota usa la misma esfera, pero con su propia textura de balón.
        if let Some(tex_idx) = self.ensure_football_texture()
            && let Some(mc) = self
                .world
                .get_mut::<crate::engine3d::ecs::MeshComponent>(id)
        {
            mc.tex_idx = tex_idx;
        }
        let radius = scale.into_iter().fold(f32::INFINITY, f32::min).max(0.15) * 0.5;
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.position = Vec3::from_array(position);
            t.scale = Vec3::splat(radius * 2.0);
        }
        self.physics
            .set_entity_sphere_physics(id, true, physics_type, position, radius);
        self.entity_colision.insert(id, true);
        self.scenario_entities.push(id);
        self.save_registry.register_meta(
            id,
            EntitySaveMeta {
                kind: "model".to_string(),
                path: "[Ball]".to_string(),
                visual_model_path: None,
                entity_category: Some("object".to_string()),
            },
        );
        send_event(&EngineEvent::ModelLoaded {
            id,
            name: Some(label.clone()),
            position: Some(position),
            scale: Some([radius * 2.0; 3]),
            rotation: Some([0.0, 0.0, 0.0, 1.0]),
            path: Some("[Ball]".to_string()),
            kind: Some("model".to_string()),
            blueprint_id: None,
            physics_enabled: Some(true),
            physics_type: Some(physics_type.to_string()),
            entity_category: Some("object".to_string()),
        });
        log::info!(
            "Pelota física «{label}» en [{:.1}, {:.1}, {:.1}]",
            position[0],
            position[1],
            position[2]
        );
        id
    }

    /// Entidad lógica de reflection probe: sin malla en escena (solo gizmo en editor/debug).
    pub(crate) fn spawn_reflection_probe_entity(
        &mut self,
        name: &str,
        position: [f32; 3],
        influence_radius_m: f32,
    ) -> EntityId {
        use crate::engine3d::ipc::EngineEvent;
        use crate::engine3d::ipc::send_event;

        let label = self.resolve_entity_display_name(
            name,
            rer_engine_core::editor_defaults::entity_label::REFLECTION_PROBE,
        );
        let id = self.world.spawn(Some(&label));
        let influence = influence_radius_m.max(0.15);
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.position = Vec3::from_array(position);
            // `reflection_probe_world_radius` usa scale.x * 0.5 como radio de influencia.
            t.scale = Vec3::splat(influence * 2.0);
        }
        self.entity_colision.insert(id, false);
        self.scenario_entities.push(id);
        self.save_registry.register_meta(
            id,
            EntitySaveMeta {
                kind: "model".to_string(),
                path: "[ReflectionProbe]".to_string(),
                visual_model_path: None,
                entity_category: Some("object".to_string()),
            },
        );
        self.allocate_probe_slot(id);
        send_event(&EngineEvent::ModelLoaded {
            id,
            name: Some(label.clone()),
            position: Some(position),
            scale: Some([influence * 2.0; 3]),
            rotation: Some([0.0, 0.0, 0.0, 1.0]),
            path: Some("[ReflectionProbe]".to_string()),
            kind: Some("model".to_string()),
            blueprint_id: None,
            physics_enabled: Some(false),
            physics_type: None,
            entity_category: Some("object".to_string()),
        });
        log::info!(
            "Sonda de reflejo «{label}» radio={influence:.1}m en [{:.1}, {:.1}, {:.1}]",
            position[0],
            position[1],
            position[2]
        );
        id
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_LIGHT_COLOR, SHADOW_CASTER_MARGIN_MIN_M, SUN_CYCLE_SECONDS_PER_UNIT,
        SUN_HALO_REFERENCE_DISTANCE_M, SunRiseCardinal, day_factor, light_shadow_extent_for,
        light_view_proj_for_bounds, light_view_proj_from, shadow_caster_margin_from,
        shadow_grazing_scale, shadow_ortho_extent_m, shadow_world_texel_from,
        sun_color_for_sun_height, sun_cycle_direction, sun_halo_radius_m, sun_local_direction,
        sun_orbit_center, sun_orbit_distance_from, sun_phase_from_direction,
        sun_position_from_bounds, sun_world_y,
    };
    use crate::engine3d::config_3d::world_bounds::{SKY_SPHERE_RADIUS, WorldBounds3D};
    use glam::{Mat4, Vec3};

    #[test]
    fn shadow_ortho_matches_world_aabb_diagonal() {
        let radius = 50.0;
        let expected = (radius * 2.0 * 3.0_f32.sqrt()).max(48.0) * 0.55;
        assert!((shadow_ortho_extent_m(radius) - expected).abs() < 1e-5);
    }

    #[test]
    fn caster_margin_grows_with_grazing_sun() {
        let high = shadow_caster_margin_from(32.0, 1.0, None, true);
        let low = shadow_caster_margin_from(32.0, 0.05, None, true);
        assert!(low > high, "rasante {low} debe ser mayor que cenit {high}");
        assert!(high >= SHADOW_CASTER_MARGIN_MIN_M);
    }

    #[test]
    fn caster_margin_can_be_disabled_or_overridden() {
        assert_eq!(shadow_caster_margin_from(32.0, 0.05, None, false), 0.0);
        assert_eq!(
            shadow_caster_margin_from(32.0, 0.05, Some(17.0), true),
            17.0
        );
        assert_eq!(shadow_caster_margin_from(32.0, 0.05, Some(-4.0), true), 0.0);
    }

    #[test]
    fn caster_margin_never_exceeds_twice_the_shadow_extent() {
        let extent = shadow_ortho_extent_m(900.0) * shadow_grazing_scale(0.0);
        let margin = shadow_caster_margin_from(900.0, 0.0, None, true);
        assert!(
            margin <= extent * 2.0 + 1e-4,
            "{margin} vs {}",
            extent * 2.0
        );
    }

    #[test]
    fn shadow_texel_grows_with_world_and_shrinks_with_map() {
        let small = shadow_world_texel_from(shadow_ortho_extent_m(50.0), 1024);
        let large = shadow_world_texel_from(shadow_ortho_extent_m(250.0), 1024);
        let fine = shadow_world_texel_from(shadow_ortho_extent_m(250.0), 4096);
        assert!(large > small);
        assert!(fine < large);
        assert!(small > 0.05 && small < 1.0);
    }

    #[test]
    fn sun_orbit_radius_is_fixed_to_the_sky_sphere_for_every_world() {
        // El radio ya NO se calcula con la diagonal de la caja: un mundo con
        // techo alto y laterales bajos no debe acercar el sol.
        let worlds = [
            ("por defecto", WorldBounds3D::default()),
            ("pequeño", WorldBounds3D::new_square(30.0)),
            ("grande", WorldBounds3D::new_square(2000.0)),
            (
                "techo alto",
                WorldBounds3D::with_faces(20.0, 20.0, 20.0, 20.0, 640.0),
            ),
        ];
        for (name, bounds) in worlds {
            let distance = sun_orbit_distance_from(&bounds);
            let sky_radius = bounds.sky_uniform().center_radius[3];
            assert!(
                (distance - SKY_SPHERE_RADIUS * 0.85).abs() < 1e-3,
                "{name}: radio {distance} != 1360"
            );
            assert!(
                distance < sky_radius,
                "{name}: el sol debe quedar dentro de la esfera ({distance} < {sky_radius})"
            );
        }
    }

    #[test]
    fn sun_orbit_radius_is_constant_for_every_phase() {
        // Si el sol pareciera acercarse al bajar sería perspectiva, no geometría.
        let bounds = WorldBounds3D::default();
        let center = sun_orbit_center(&bounds);
        let distance = sun_orbit_distance_from(&bounds);
        for phase in [0.0, 0.1, 0.25, 0.5, 0.75] {
            let position = sun_position_from_bounds(&bounds, phase, SunRiseCardinal::East);
            let radius = (position - center).length();
            assert!(
                (radius - distance).abs() < 1e-3,
                "fase {phase}: radio {radius} != {distance}"
            );
        }
    }

    #[test]
    fn sun_halo_keeps_its_angular_size_at_the_fixed_orbit() {
        let orbit = sun_orbit_distance_from(&WorldBounds3D::default());
        let reference = sun_halo_radius_m(1.0, SUN_HALO_REFERENCE_DISTANCE_M);
        let current = sun_halo_radius_m(1.0, orbit);

        // Misma relación radio/distancia: el tamaño aparente no cambia aunque
        // la órbita pase de ~88 m a 1360 m.
        let ratio_ref = reference / SUN_HALO_REFERENCE_DISTANCE_M;
        let ratio_now = current / orbit;
        assert!(
            (ratio_now - ratio_ref).abs() < 1e-6,
            "tamaño angular {ratio_now} != {ratio_ref}"
        );
        assert!(
            current > reference * 10.0,
            "halo {current} no escaló con la órbita {orbit}"
        );
        // La escala del transform sigue pudiendo agrandar el halo.
        assert!(sun_halo_radius_m(2.0, orbit) > current);
    }

    #[test]
    fn sun_orbit_center_is_on_the_ground_with_a_tall_world() {
        // 1000 m de altura pedida (el constructor la limita a 640) no debe
        // desplazar el ancla: el sol orbita alrededor del suelo.
        let bounds = WorldBounds3D::with_faces(80.0, 30.0, 60.0, 20.0, 1000.0);
        let center = sun_orbit_center(&bounds);
        assert!(
            center.y.abs() < 1e-6,
            "el centro debe estar en y=0: {center}"
        );
        assert!((center.x - bounds.center().x).abs() < 1e-6);
        assert!((center.z - bounds.center().z).abs() < 1e-6);
        // El centro de la CAJA sí está a media altura: no es el que buscamos.
        let box_center = (bounds.min_corner() + bounds.max_corner()) * 0.5;
        assert!(box_center.y > 0.0);
        assert!((box_center.y - center.y).abs() > 1.0);
    }

    #[test]
    fn light_camera_looks_at_ground_center() {
        let bounds = WorldBounds3D::with_faces(50.0, 50.0, 50.0, 50.0, 1000.0);
        let ground = sun_orbit_center(&bounds);
        let dir = sun_cycle_direction(0.0, SunRiseCardinal::East); // sol en el horizonte
        let extent = light_shadow_extent_for(&bounds, dir.y);
        let m = light_view_proj_for_bounds(&bounds, dir, extent, 1024, SunRiseCardinal::East);

        // Reconstruimos un punto del centro de la imagen: el eje de la cámara
        // de luz debe pasar por el suelo, no por media altura.
        let inv = Mat4::from_cols_array_2d(&m).inverse();
        let p = inv * glam::Vec4::new(0.0, 0.0, 0.0, 1.0);
        let p = Vec3::new(p.x / p.w, p.y / p.w, p.z / p.w);
        assert!(
            p.y.abs() < 1e-3,
            "la cámara de luz mira a y={}, no al suelo",
            p.y
        );
        assert!((p.z - ground.z).abs() < 1e-3);
        assert!((p.y - bounds.height * 0.5).abs() > 1.0);
    }

    #[test]
    fn shadow_extent_covers_the_world_from_the_ground_center() {
        // Con el ancla en el suelo la tapa alta queda más lejos: la ortho debe
        // seguir conteniendo las 8 esquinas para cualquier altura del sol.
        let bounds = WorldBounds3D::with_faces(50.0, 50.0, 50.0, 50.0, 640.0);
        let min = bounds.min_corner();
        let max = bounds.max_corner();
        for dy in [0.0_f32, 0.5, 1.0] {
            let dir = Vec3::new((1.0 - dy * dy).sqrt(), dy, 0.0);
            let extent = light_shadow_extent_for(&bounds, dir.y);
            let m = light_view_proj_for_bounds(&bounds, dir, extent, 1024, SunRiseCardinal::East);
            let proj = Mat4::from_cols_array_2d(&m);
            for x in [min.x, max.x] {
                for y in [min.y, max.y] {
                    for z in [min.z, max.z] {
                        let c = proj * glam::Vec4::new(x, y, z, 1.0);
                        let (nx, ny, nz) = (c.x / c.w, c.y / c.w, c.z / c.w);
                        assert!(nx.abs() <= 1.0 + 1e-4, "x={nx} fuera de la ortho");
                        assert!(ny.abs() <= 1.0 + 1e-4, "y={ny} fuera de la ortho");
                        assert!(
                            (-1.0..=1.0).contains(&nz),
                            "z={nz} fuera del rango de profundidad"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn sun_cycle_phase_maps_to_a_full_orbit_and_back() {
        for cardinal in [
            SunRiseCardinal::East,
            SunRiseCardinal::North,
            SunRiseCardinal::West,
            SunRiseCardinal::South,
        ] {
            for phase in [0.0, 0.125, 0.25, 0.5, 0.75, 0.999] {
                let dir = sun_cycle_direction(phase, cardinal);
                assert!((dir.length() - 1.0).abs() < 1e-5);
                assert!((sun_phase_from_direction(dir, cardinal) - phase).abs() < 1e-4);
            }
        }
        // El ciclo cubre la vuelta completa: hay fases sobre y bajo el horizonte.
        assert!(sun_cycle_direction(0.25, SunRiseCardinal::East).y > 0.0);
        assert!(sun_cycle_direction(0.75, SunRiseCardinal::East).y < 0.0);
    }

    #[test]
    fn sun_reaches_the_bottom_of_the_world_without_sideways_drift() {
        let bounds = WorldBounds3D::default();
        let center = sun_orbit_center(&bounds);
        let distance = sun_orbit_distance_from(&bounds);

        for cardinal in [
            SunRiseCardinal::East,
            SunRiseCardinal::North,
            SunRiseCardinal::West,
            SunRiseCardinal::South,
        ] {
            let lowest = sun_cycle_direction(0.75, cardinal);
            // El punto más bajo está exactamente bajo el centro (sin deriva XZ).
            assert!(
                (lowest.x).abs() < 1e-5 && (lowest.z).abs() < 1e-5,
                "el sol se desvía de la vertical: {lowest:?}"
            );
            assert!((lowest.y + 1.0).abs() < 1e-5);

            let position = center + lowest * distance;
            assert!(position.y < 0.0, "el sol debe bajar bajo el suelo");
            assert!((position.x - center.x).abs() < 1e-4);
            assert!((position.z - center.z).abs() < 1e-4);
        }
    }

    #[test]
    fn sun_rise_cardinal_matches_the_world_compass() {
        let east = sun_cycle_direction(0.0, SunRiseCardinal::East);
        let north = sun_cycle_direction(0.0, SunRiseCardinal::North);
        let west = sun_cycle_direction(0.0, SunRiseCardinal::West);
        let south = sun_cycle_direction(0.0, SunRiseCardinal::South);
        assert!((east.x - 1.0).abs() < 1e-5); // +X = Este
        assert!((north.z + 1.0).abs() < 1e-5); // -Z = Norte
        assert!((west.x + 1.0).abs() < 1e-5); // -X = Oeste
        assert!((south.z - 1.0).abs() < 1e-5); // +Z = Sur
    }

    #[test]
    fn sun_goes_below_the_world_at_night() {
        // Con la órbita centrada en el suelo, el punto más bajo queda por
        // debajo de y = 0 para cualquier tamaño de mundo.
        for bounds in [
            WorldBounds3D::default(),
            WorldBounds3D::with_faces(5.0, 5.0, 5.0, 5.0, 10.0),
            WorldBounds3D::with_faces(1000.0, 1000.0, 1000.0, 1000.0, 2000.0),
        ] {
            let center = sun_orbit_center(&bounds);
            let distance = sun_orbit_distance_from(&bounds);
            let lowest_y = center.y + sun_cycle_direction(0.75, SunRiseCardinal::East).y * distance;
            assert!(
                lowest_y < 0.0,
                "el sol debe ponerse bajo el suelo (y={lowest_y:.1})"
            );
            // Y seguir dentro de la esfera de cielo en todo momento.
            let radius = bounds.sky_uniform().center_radius[3];
            assert!(distance < radius);
        }
    }

    #[test]
    fn sun_cycle_ui_2000_means_two_hours() {
        assert!((2000.0 * SUN_CYCLE_SECONDS_PER_UNIT - 7200.0).abs() < 1e-3);
    }

    #[test]
    fn horizon_uses_the_sun_height_not_dir_y() {
        let bounds = WorldBounds3D::default();
        let distance = sun_orbit_distance_from(&bounds);

        // Fase 0.25: mediodía, sol en lo alto → día pleno.
        let noon = sun_cycle_direction(0.25, SunRiseCardinal::East);
        let noon_y = sun_world_y(&bounds, noon, distance);
        assert!(noon_y > 0.0);
        assert!((day_factor(noon_y, distance) - 1.0).abs() < 1e-5);

        // Fase 0.5: puesta exacta en el suelo → crepúsculo (0.5).
        let horizon = sun_cycle_direction(0.5, SunRiseCardinal::East);
        let horizon_y = sun_world_y(&bounds, horizon, distance);
        // f32 redondea π, así que sin(π) no es exactamente 0 (≈1e-4 m a 1360 m).
        assert!(horizon_y.abs() < 1e-2);
        assert!((day_factor(horizon_y, distance) - 0.5).abs() < 1e-5);

        // Solo se oscurece del todo cuando el sol baja bajo el suelo.
        let dusk = sun_cycle_direction(0.62, SunRiseCardinal::East);
        let dusk_y = sun_world_y(&bounds, dusk, distance);
        assert!(dusk_y < 0.0);
        assert!(day_factor(dusk_y, distance) < 1e-5);
    }

    #[test]
    fn sun_sets_at_the_horizon_when_dir_y_is_zero() {
        let bounds = WorldBounds3D::with_faces(1000.0, 1000.0, 1000.0, 1000.0, 1000.0);
        let distance = sun_orbit_distance_from(&bounds);

        let y = sun_world_y(&bounds, Vec3::X, distance);
        assert!(
            y.abs() < 1e-6,
            "con dir.y = 0 el sol debe estar en el suelo (y={y})"
        );
        // La posición orbital confirma la misma altura en el amanecer.
        let pos = sun_position_from_bounds(&bounds, 0.0, SunRiseCardinal::East);
        assert!((pos.y - y).abs() < 1e-5);
        // Y no depende de la altura del mundo.
        assert!(sun_world_y(&WorldBounds3D::default(), Vec3::X, distance).abs() < 1e-6);
    }

    #[test]
    fn twilight_bands_scale_with_the_fixed_orbit() {
        let bounds = WorldBounds3D::default();
        let distance = sun_orbit_distance_from(&bounds);
        let band = distance * 0.08;
        assert!((band - 108.8).abs() < 1e-3);
        assert!((day_factor(0.0, distance) - 0.5).abs() < 1e-6);
        assert!(day_factor(-band, distance) < 1e-5);
        assert!(day_factor(band, distance) > 1.0 - 1e-5);

        // El tinte solo aparece cerca del suelo (~109 m), no desde 400 m.
        let tint = |y: f32| sun_color_for_sun_height(y, distance, Vec3::NEG_X).z;
        assert!((tint(band) - DEFAULT_LIGHT_COLOR.z).abs() < 1e-3);
        assert!(tint(0.0) < DEFAULT_LIGHT_COLOR.z);
        assert!((tint(band * 5.0) - DEFAULT_LIGHT_COLOR.z).abs() < 1e-6);
    }

    #[test]
    fn sun_color_is_white_at_dawn_and_reddish_at_dusk() {
        let bounds = WorldBounds3D::default();
        let distance = sun_orbit_distance_from(&bounds);
        let color_at = |phase: f32| {
            let local = sun_local_direction(phase);
            sun_color_for_sun_height(sun_world_y(&bounds, local, distance), distance, local)
        };

        let dawn = color_at(0.93); // subiendo, cerca del suelo
        let dusk = color_at(0.58); // bajando, cerca del suelo
        let noon = color_at(0.25);

        // Amanecer: blanco (canal azul sube respecto al neutro).
        assert!(dawn.z > DEFAULT_LIGHT_COLOR.z);
        // Puesta: rojizo (cae el azul, sube el rojo).
        assert!(dusk.z < DEFAULT_LIGHT_COLOR.z);
        assert!(dusk.x >= dawn.x);
        // Mediodía (fase 0.25): color neutro histórico.
        assert!((noon - DEFAULT_LIGHT_COLOR).length() < 1e-5);
        // Fase 0.5 ya es la puesta (sol en el suelo): rojizo, no neutro.
        assert!(color_at(0.5).z < DEFAULT_LIGHT_COLOR.z);
    }

    #[test]
    fn shadow_extent_grows_at_grazing_angles() {
        assert!((shadow_grazing_scale(1.0) - 1.0).abs() < 1e-6);
        assert!(shadow_grazing_scale(0.1) > shadow_grazing_scale(0.5));
        assert!(shadow_grazing_scale(0.5) > 1.0);
    }

    #[test]
    fn light_view_proj_follows_the_sun_direction() {
        let center = Vec3::new(0.0, 10.0, 0.0);
        let east = light_view_proj_from(center, Vec3::X, 100.0, 1024, Vec3::Z);
        let west = light_view_proj_from(center, Vec3::NEG_X, 100.0, 1024, Vec3::Z);
        let zenith = light_view_proj_from(center, Vec3::Y, 100.0, 1024, Vec3::Z);

        assert_ne!(
            east, west,
            "la matriz debe cambiar al mover el sol al otro lado"
        );
        assert_ne!(east, zenith);
        assert_ne!(west, zenith);
    }

    #[test]
    fn light_basis_does_not_flip_across_zenith_threshold() {
        // El antiguo `up = if |dir.y| > 0.99 { X } else { Y }` giraba la base
        // 180° al cruzar 0.99, reflejando el shadow map. Con up fijo (normal de
        // la órbita) la proyección de un punto debe variar suave en todo el barrido.
        let center = Vec3::new(0.0, 50.0, 0.0);
        let extent = 60.97;
        let p = Vec3::new(5.0, 1.0, 5.0);
        let mut prev: Option<f32> = None;
        for i in 0..200 {
            let a = i as f32 / 199.0;
            let dy = 0.98 + a * 0.019; // 0.98 → 0.999, cruzando el viejo umbral 0.99
            let dx = -(1.0 - dy * dy).sqrt();
            let dir = Vec3::new(dx, dy, 0.0);
            let m = light_view_proj_from(center, dir, extent, 1024, Vec3::Z);
            let clip_x = m[0][0] * p.x + m[1][0] * p.y + m[2][0] * p.z + m[3][0];
            if let Some(prev_x) = prev {
                assert!(
                    (clip_x - prev_x).abs() < 0.01,
                    "la base de la luz giró de golpe en dir.y={dy}"
                );
            }
            prev = Some(clip_x);
        }
    }

    #[test]
    fn shadow_anchor_lands_on_texel_center() {
        // El centro del suelo debe caer siempre en un centro de texel del shadow
        // map, aunque la dirección del sol varíe: asi la rejilla no se desliza
        // subpíxel y el depth comparison no oscila (temblor de la sombra).
        let center = Vec3::new(0.0, 50.0, 0.0);
        let extent = 60.97;
        let map = 1024u32;
        let half = map as f32 * 0.5;
        let anchor = glam::Vec4::new(center.x, 0.0, center.z, 1.0);
        for i in 0..256 {
            let n = i as f32 * 1e-5;
            let dir = Vec3::new(-0.05 + n, 0.998 + n * 0.1, 0.0).normalize();
            let m = light_view_proj_from(center, dir, extent, map, Vec3::Z);
            let clip_x = m[0][0] * anchor.x + m[1][0] * anchor.y + m[2][0] * anchor.z + m[3][0];
            let clip_y = m[0][1] * anchor.x + m[1][1] * anchor.y + m[2][1] * anchor.z + m[3][1];
            let clip_w = m[0][3] * anchor.x + m[1][3] * anchor.y + m[2][3] * anchor.z + m[3][3];
            let px = (clip_x / clip_w) * half;
            let py = (clip_y / clip_w) * half;
            assert!(
                (px - px.round()).abs() < 1e-3,
                "el ancla no cae en centro de texel en X (px={px})"
            );
            assert!(
                (py - py.round()).abs() < 1e-3,
                "el ancla no cae en centro de texel en Y (py={py})"
            );
        }
    }
}
