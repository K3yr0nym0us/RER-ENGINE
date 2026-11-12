//! Seeds deterministas por dominio y RNG determinista del generador.
//!
//! Patrón de la referencia (`primitive-voxels-godot`): un `world_seed` base más un
//! salt por dominio (altura, biomas, cuevas, ríos, distribución…) producen seeds
//! descorrelacionadas. Todo lo aleatorio del plugin deriva de aquí, nunca de
//! estado global ni del reloj.

/// Mezcla determinista de 32 bits (estilo PCG/xorshift; equivalente a `_hash01` de la
/// referencia). Útil para hashing de celdas y seeds de dominio.
#[inline]
#[must_use]
pub fn mix_u32(mut v: u32) -> u32 {
    v ^= v >> 16;
    v = v.wrapping_mul(0x7feb352d);
    v ^= v >> 15;
    v = v.wrapping_mul(0x846ca68b);
    v ^= v >> 16;
    v
}

/// Seed de dominio: mezcla el seed del mundo con un salt de dominio.
///
/// El salt se elige por dominio (p. ej. `SALT_TERRAIN`, `SALT_CAVES`) de forma que
/// cada subsistema trabaje con una secuencia descorrelacionada del resto.
#[inline]
#[must_use]
pub fn domain_seed(world_seed: u32, salt: u32) -> i32 {
    (mix_u32(world_seed ^ mix_u32(salt.wrapping_add(0x9e37_79b9))) & 0x7fff_ffff) as i32
}

// Salts de dominio fijos por subsistema. Cambiarlos invalida mundos ya generados,
// por lo que son parte del contrato de persistencia (no renombrar valores).
/// Dominio de la altura base del terreno.
pub const SALT_TERRAIN: u32 = 0;
/// Dominio del detalle de alta frecuencia del terreno.
pub const SALT_DETAIL: u32 = 1_000;
/// Dominio del mapa de biomas.
pub const SALT_BIOME: u32 = 2_000;
/// Dominio de la máscara de montañas.
pub const SALT_MOUNTAIN: u32 = 3_000;
/// Dominio de máscara de acantilados/terrazas.
pub const SALT_CLIFF: u32 = 4_000;
/// Dominio de cuevas (túneles y rooms).
pub const SALT_CAVES: u32 = 5_000;
/// Dominio de ríos.
pub const SALT_RIVERS: u32 = 6_000;
/// Dominio de lagos.
pub const SALT_LAKES: u32 = 7_000;
/// Dominio de rugosidad de paredes de cuevas (ruido 3D).
pub const SALT_CAVE_NOISE: u32 = 8_000;
/// Dominio del planificador de features y del scatter de blueprints.
pub const SALT_PLANNER: u32 = 9_000;

/// RNG determinista xoshiro256\*\* sembrado con SplitMix64.
///
/// Pequeño, rápido y con período 2^256-1; suficiente para el planificador y el
/// scatter. No es criptográfico: nunca guardar seeds sensibles en él.
#[derive(Debug, Clone)]
pub struct Rng {
    s: [u64; 4],
}

impl Rng {
    /// Crea un RNG determinista a partir de una seed de dominio.
    #[must_use]
    pub fn new(seed: i32) -> Self {
        Self::from_u64((seed as u32 as u64) << 32 | 0x6a09_e667_7974_0429 ^ (seed as i64 as u64))
    }

    /// Crea un RNG determinista desde un u64 arbitrario (p. ej. hash de celda).
    #[must_use]
    pub fn from_u64(mut z: u64) -> Self {
        let mut s = [0u64; 4];
        for slot in &mut s {
            z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut x = z;
            x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            *slot = x ^ (x >> 31);
        }
        Self { s }
    }

    #[inline]
    fn next_u64(&mut self) -> u64 {
        let result = self.s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    /// f32 uniforme en `[0, 1)`.
    #[inline]
    #[must_use]
    pub fn next_f32(&mut self) -> f32 {
        // 24 bits de mantisa: uniforme en [0,1) sin sesgo de redondeo.
        ((self.next_u64() >> 40) as f32) * (1.0 / (1u32 << 24) as f32)
    }

    /// f32 uniforme en `[min, max)`.
    #[inline]
    #[must_use]
    pub fn range_f32(&mut self, min: f32, max: f32) -> f32 {
        min + self.next_f32() * (max - min)
    }

    /// i32 uniforme en `[min, max]` (ambos inclusive).
    #[inline]
    #[must_use]
    pub fn range_i32(&mut self, min: i32, max: i32) -> i32 {
        debug_assert!(max >= min);
        let span = (max - min + 1) as u64;
        min + (self.next_u64() % span) as i32
    }

    /// f32 con distribución normal aproximada (media 0, desviación `sigma`),
    /// por suma de uniformes (teorema central del límite, 4 muestras).
    #[inline]
    #[must_use]
    pub fn gaussian(&mut self, sigma: f32) -> f32 {
        let sum: f32 = (0..4).map(|_| self.next_f32()).sum();
        (sum - 2.0) * sigma * (3.0f32).sqrt()
    }

    /// Probabilidad booleana `p` en `[0, 1]`.
    #[inline]
    #[must_use]
    pub fn chance(&mut self, p: f32) -> bool {
        self.next_f32() < p
    }
}
