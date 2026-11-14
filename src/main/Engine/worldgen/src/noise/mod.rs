//! Puerto propio de [FastNoiseLite](https://github.com/Auburn/FastNoiseLite) (MIT, Auburn).
//!
//! Port fiel del algoritmo de `CSharp/FastNoiseLite.cs` para que los valores de ruido
//! coincidan con la referencia usada por el motor. Las tablas de gradientes
//! (`tables.rs`) se generaron programáticamente desde el código fuente original para
//! evitar errores de transcripción.
//!
//! Los literales de `tables.rs` se conservan tal cual del original, por lo que
//! algunos exceden la mantisa de f32 (warning cosmético que se ignora ahí).
#![allow(clippy::excessive_precision)]
//!
//! Cobertura: `OpenSimplex2`, `OpenSimplex2S`, `Perlin`, `ValueCubic` y `Value` en 2D/3D,
//! con fractales `FBM`, `Ridged` y `PingPong`. El dominio warp se añadirá cuando un
//! feature lo requiera.

pub mod tables;

use tables::{GRADIENTS_2D, GRADIENTS_3D};

/// Tipos de ruido soportados (equivalen a `NoiseType` en FastNoiseLite).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NoiseType {
    /// Simplex/OpenSimplex2: el tipo usado por el terreno del plugin.
    #[default]
    OpenSimplex2,
    /// Variante suave (C1-continua) de OpenSimplex2.
    OpenSimplex2S,
    /// Perlin clásico con interpolación quíntica.
    Perlin,
    /// Value noise con interpolación cúbica.
    ValueCubic,
    /// Value noise con interpolación Hermite.
    Value,
}

/// Tipos de fractal soportados (equivalen a `FractalType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FractalType {
    /// Ruido simple, sin octavas.
    #[default]
    None,
    /// Fractal Brownian Motion (suma ponderada de octavas).
    Fbm,
    /// `1 - |noise|` por octava: crestas afiladas, ideal para montañas.
    Ridged,
    /// Reflexión triangular entre octavas.
    PingPong,
}

/// Configuración de un generador de ruido determinista.
#[derive(Debug, Clone, Copy)]
pub struct Noise {
    seed: i32,
    frequency: f32,
    noise_type: NoiseType,
    fractal_type: FractalType,
    octaves: i32,
    lacunarity: f32,
    gain: f32,
    weighted_strength: f32,
    ping_pong_strength: f32,
    fractal_bounding: f32,
}

impl Noise {
    /// Ruido OpenSimplex2 simple (sin octavas) con la seed indicada.
    pub fn new(seed: i32) -> Self {
        Self {
            seed,
            frequency: 0.01,
            noise_type: NoiseType::OpenSimplex2,
            fractal_type: FractalType::None,
            octaves: 3,
            lacunarity: 2.0,
            gain: 0.5,
            weighted_strength: 0.0,
            ping_pong_strength: 2.0,
            fractal_bounding: 1.0,
        }
    }

    /// Frecuencia del ruido (escala de coordenadas).
    pub fn frequency(mut self, frequency: f32) -> Self {
        self.frequency = frequency;
        self
    }

    /// Tipo de ruido base.
    pub fn noise_type(mut self, noise_type: NoiseType) -> Self {
        self.noise_type = noise_type;
        self
    }

    /// Configuración fractal; recalcula el factor de normalización.
    pub fn fractal(
        mut self,
        fractal_type: FractalType,
        octaves: i32,
        gain: f32,
        weighted_strength: f32,
    ) -> Self {
        self.fractal_type = fractal_type;
        self.octaves = octaves.max(1);
        self.gain = gain;
        self.weighted_strength = weighted_strength;
        self.calculate_fractal_bounding();
        self
    }

    /// Lacunarity: multiplicador de frecuencia por octava (2.0 por defecto).
    pub fn lacunarity(mut self, lacunarity: f32) -> Self {
        self.lacunarity = lacunarity;
        self
    }

    /// Fuerza del ping-pong (solo con [`FractalType::PingPong`]).
    pub fn ping_pong_strength(mut self, strength: f32) -> Self {
        self.ping_pong_strength = strength;
        self
    }

    fn calculate_fractal_bounding(&mut self) {
        let gain = self.gain.abs();
        let mut amp = gain;
        let mut amp_fractal = 1.0;
        for _ in 1..self.octaves {
            amp_fractal += amp;
            amp *= gain;
        }
        self.fractal_bounding = 1.0 / amp_fractal;
    }
}

// Constantes internas del algoritmo original.
const PRIME_X: i32 = 501_125_321;
const PRIME_Y: i32 = 1_136_930_381;
const PRIME_Z: i32 = 1_720_413_743;

#[inline]
fn fast_floor(f: f32) -> i32 {
    if f >= 0.0 { f as i32 } else { f as i32 - 1 }
}

#[inline]
fn fast_round(f: f32) -> i32 {
    if f >= 0.0 {
        (f + 0.5) as i32
    } else {
        (f - 0.5) as i32
    }
}

#[inline]
fn fast_abs(f: f32) -> f32 {
    if f < 0.0 { -f } else { f }
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + t * (b - a)
}

#[inline]
fn interp_hermite(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

#[inline]
fn interp_quintic(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

#[inline]
fn cubic_lerp(a: f32, b: f32, c: f32, d: f32, t: f32) -> f32 {
    let p = (d - c) - (a - b);
    t * t * t * p + t * t * ((a - b) - p) + t * (c - a) + b
}

#[inline]
fn ping_pong(t: f32) -> f32 {
    let t = t - (t * 0.5) as i32 as f32 * 2.0;
    if t < 1.0 { t } else { 2.0 - t }
}

#[inline]
fn hash2(seed: i32, x_primed: i32, y_primed: i32) -> i32 {
    (seed ^ x_primed ^ y_primed).wrapping_mul(0x27d4eb2d)
}

#[inline]
fn hash3(seed: i32, x_primed: i32, y_primed: i32, z_primed: i32) -> i32 {
    (seed ^ x_primed ^ y_primed ^ z_primed).wrapping_mul(0x27d4eb2d)
}

#[inline]
fn val_coord2(seed: i32, x_primed: i32, y_primed: i32) -> f32 {
    let mut hash = hash2(seed, x_primed, y_primed);
    hash = hash.wrapping_mul(hash);
    hash ^= hash << 19;
    hash as f32 * (1.0 / 2_147_483_648.0)
}

#[inline]
fn val_coord3(seed: i32, x_primed: i32, y_primed: i32, z_primed: i32) -> f32 {
    let mut hash = hash3(seed, x_primed, y_primed, z_primed);
    hash = hash.wrapping_mul(hash);
    hash ^= hash << 19;
    hash as f32 * (1.0 / 2_147_483_648.0)
}

#[inline]
fn grad_coord2(seed: i32, x_primed: i32, y_primed: i32, xd: f32, yd: f32) -> f32 {
    let mut hash = hash2(seed, x_primed, y_primed);
    hash ^= hash >> 15;
    hash &= 127 << 1;
    let xg = GRADIENTS_2D[hash as usize];
    let yg = GRADIENTS_2D[(hash | 1) as usize];
    xd * xg + yd * yg
}

#[inline]
fn grad_coord3(
    seed: i32,
    x_primed: i32,
    y_primed: i32,
    z_primed: i32,
    xd: f32,
    yd: f32,
    zd: f32,
) -> f32 {
    let mut hash = hash3(seed, x_primed, y_primed, z_primed);
    hash ^= hash >> 15;
    hash &= 63 << 2;
    let xg = GRADIENTS_3D[hash as usize];
    let yg = GRADIENTS_3D[(hash | 1) as usize];
    let zg = GRADIENTS_3D[(hash | 2) as usize];
    xd * xg + yd * yg + zd * zg
}

impl Noise {
    /// Muestreo 2D del ruido en la coordenada indicada (resultado en `[-1, 1]`).
    pub fn get2(&self, x: f32, y: f32) -> f32 {
        match self.fractal_type {
            FractalType::None => {
                let (x, y) = self.transform2(x, y);
                self.single2(self.seed, x, y)
            }
            FractalType::Fbm => self.fbm2(x, y),
            FractalType::Ridged => self.ridged2(x, y),
            FractalType::PingPong => self.ping_pong2(x, y),
        }
    }

    /// Muestreo 3D del ruido en la coordenada indicada (resultado en `[-1, 1]`).
    pub fn get3(&self, x: f32, y: f32, z: f32) -> f32 {
        match self.fractal_type {
            FractalType::None => {
                let (x, y, z) = self.transform3(x, y, z);
                self.single3(self.seed, x, y, z)
            }
            FractalType::Fbm => self.fbm3(x, y, z),
            FractalType::Ridged => self.ridged3(x, y, z),
            FractalType::PingPong => self.ping_pong3(x, y, z),
        }
    }

    /// Transformación de coordenada 2D: frecuencia + skew de OpenSimplex2/S.
    #[inline]
    fn transform2(&self, x: f32, y: f32) -> (f32, f32) {
        let mut x = x * self.frequency;
        let mut y = y * self.frequency;
        if matches!(
            self.noise_type,
            NoiseType::OpenSimplex2 | NoiseType::OpenSimplex2S
        ) {
            const SQRT3: f32 = 1.7320508075688772935274463415059;
            const F2: f32 = 0.5 * (SQRT3 - 1.0);
            let t = (x + y) * F2;
            x += t;
            y += t;
        }
        (x, y)
    }

    /// Transformación de coordenada 3D: frecuencia + rotación de OpenSimplex2/S.
    #[inline]
    fn transform3(&self, x: f32, y: f32, z: f32) -> (f32, f32, f32) {
        let mut x = x * self.frequency;
        let mut y = y * self.frequency;
        let mut z = z * self.frequency;
        if matches!(
            self.noise_type,
            NoiseType::OpenSimplex2 | NoiseType::OpenSimplex2S
        ) {
            const R3: f32 = 2.0 / 3.0;
            let r = (x + y + z) * R3; // Rotación, no skew
            x = r - x;
            y = r - y;
            z = r - z;
        }
        (x, y, z)
    }

    #[inline]
    fn single2(&self, seed: i32, x: f32, y: f32) -> f32 {
        match self.noise_type {
            NoiseType::OpenSimplex2 => self.simplex2(seed, x, y),
            NoiseType::OpenSimplex2S => self.open_simplex2s2(seed, x, y),
            NoiseType::Perlin => perlin2(seed, x, y),
            NoiseType::ValueCubic => value_cubic2(seed, x, y),
            NoiseType::Value => value2(seed, x, y),
        }
    }

    #[inline]
    fn single3(&self, seed: i32, x: f32, y: f32, z: f32) -> f32 {
        match self.noise_type {
            NoiseType::OpenSimplex2 => self.open_simplex2_3(seed, x, y, z),
            NoiseType::OpenSimplex2S => self.open_simplex2s3(seed, x, y, z),
            NoiseType::Perlin => perlin3(seed, x, y, z),
            NoiseType::ValueCubic => value_cubic3(seed, x, y, z),
            NoiseType::Value => value3(seed, x, y, z),
        }
    }

    fn fbm2(&self, x: f32, y: f32) -> f32 {
        let mut seed = self.seed;
        let mut sum = 0.0;
        let mut amp = self.fractal_bounding;
        let (mut x, mut y) = self.transform2(x, y);
        for _ in 0..self.octaves {
            let noise = self.single2(seed, x, y);
            sum += noise * amp;
            amp *= lerp(
                1.0,
                fast_min(noise + 1.0, 2.0) * 0.5,
                self.weighted_strength,
            );
            seed = seed.wrapping_add(1);
            x *= self.lacunarity;
            y *= self.lacunarity;
            amp *= self.gain;
        }
        sum
    }

    fn fbm3(&self, x: f32, y: f32, z: f32) -> f32 {
        let mut seed = self.seed;
        let mut sum = 0.0;
        let mut amp = self.fractal_bounding;
        let (mut x, mut y, mut z) = self.transform3(x, y, z);
        for _ in 0..self.octaves {
            let noise = self.single3(seed, x, y, z);
            sum += noise * amp;
            amp *= lerp(1.0, (noise + 1.0) * 0.5, self.weighted_strength);
            seed = seed.wrapping_add(1);
            x *= self.lacunarity;
            y *= self.lacunarity;
            z *= self.lacunarity;
            amp *= self.gain;
        }
        sum
    }

    fn ridged2(&self, x: f32, y: f32) -> f32 {
        let mut seed = self.seed;
        let mut sum = 0.0;
        let mut amp = self.fractal_bounding;
        let (mut x, mut y) = self.transform2(x, y);
        for _ in 0..self.octaves {
            let noise = fast_abs(self.single2(seed, x, y));
            sum += (noise * -2.0 + 1.0) * amp;
            amp *= lerp(1.0, 1.0 - noise, self.weighted_strength);
            seed = seed.wrapping_add(1);
            x *= self.lacunarity;
            y *= self.lacunarity;
            amp *= self.gain;
        }
        sum
    }

    fn ridged3(&self, x: f32, y: f32, z: f32) -> f32 {
        let mut seed = self.seed;
        let mut sum = 0.0;
        let mut amp = self.fractal_bounding;
        let (mut x, mut y, mut z) = self.transform3(x, y, z);
        for _ in 0..self.octaves {
            let noise = fast_abs(self.single3(seed, x, y, z));
            sum += (noise * -2.0 + 1.0) * amp;
            amp *= lerp(1.0, 1.0 - noise, self.weighted_strength);
            seed = seed.wrapping_add(1);
            x *= self.lacunarity;
            y *= self.lacunarity;
            z *= self.lacunarity;
            amp *= self.gain;
        }
        sum
    }

    fn ping_pong2(&self, x: f32, y: f32) -> f32 {
        let mut seed = self.seed;
        let mut sum = 0.0;
        let mut amp = self.fractal_bounding;
        let (mut x, mut y) = self.transform2(x, y);
        for _ in 0..self.octaves {
            let noise = ping_pong((self.single2(seed, x, y) + 1.0) * self.ping_pong_strength);
            sum += (noise - 0.5) * 2.0 * amp;
            amp *= lerp(1.0, noise, self.weighted_strength);
            seed = seed.wrapping_add(1);
            x *= self.lacunarity;
            y *= self.lacunarity;
            amp *= self.gain;
        }
        sum
    }

    fn ping_pong3(&self, x: f32, y: f32, z: f32) -> f32 {
        let mut seed = self.seed;
        let mut sum = 0.0;
        let mut amp = self.fractal_bounding;
        let (mut x, mut y, mut z) = self.transform3(x, y, z);
        for _ in 0..self.octaves {
            let noise = ping_pong((self.single3(seed, x, y, z) + 1.0) * self.ping_pong_strength);
            sum += (noise - 0.5) * 2.0 * amp;
            amp *= lerp(1.0, noise, self.weighted_strength);
            seed = seed.wrapping_add(1);
            x *= self.lacunarity;
            y *= self.lacunarity;
            z *= self.lacunarity;
            amp *= self.gain;
        }
        sum
    }

    /// Simplex 2D (el caso 2D de OpenSimplex2 usa el algoritmo de simplex ordinario).
    /// Las coordenadas ya vienen pre-skeweadas desde [`Self::transform2`].
    fn simplex2(&self, seed: i32, x: f32, y: f32) -> f32 {
        const SQRT3: f32 = 1.7320508075688772935274463415059;
        const G2: f32 = (3.0 - SQRT3) / 6.0;

        let i = fast_floor(x);
        let j = fast_floor(y);
        let xi = x - i as f32;
        let yi = y - j as f32;

        let t = (xi + yi) * G2;
        let x0 = xi - t;
        let y0 = yi - t;

        let i = i.wrapping_mul(PRIME_X);
        let j = j.wrapping_mul(PRIME_Y);

        let a = 0.5 - x0 * x0 - y0 * y0;
        let n0 = if a <= 0.0 {
            0.0
        } else {
            (a * a) * (a * a) * grad_coord2(seed, i, j, x0, y0)
        };

        let c = (2.0 * (1.0 - 2.0 * G2) * (1.0 / G2 - 2.0)) * t
            + (-2.0 * (1.0 - 2.0 * G2) * (1.0 - 2.0 * G2) + a);
        let n2 = if c <= 0.0 {
            0.0
        } else {
            let x2 = x0 + (2.0 * G2 - 1.0);
            let y2 = y0 + (2.0 * G2 - 1.0);
            (c * c)
                * (c * c)
                * grad_coord2(
                    seed,
                    i.wrapping_add(PRIME_X),
                    j.wrapping_add(PRIME_Y),
                    x2,
                    y2,
                )
        };

        let n1 = if y0 > x0 {
            let x1 = x0 + G2;
            let y1 = y0 + (G2 - 1.0);
            let b = 0.5 - x1 * x1 - y1 * y1;
            if b <= 0.0 {
                0.0
            } else {
                (b * b) * (b * b) * grad_coord2(seed, i, j.wrapping_add(PRIME_Y), x1, y1)
            }
        } else {
            let x1 = x0 + (G2 - 1.0);
            let y1 = y0 + G2;
            let b = 0.5 - x1 * x1 - y1 * y1;
            if b <= 0.0 {
                0.0
            } else {
                (b * b) * (b * b) * grad_coord2(seed, i.wrapping_add(PRIME_X), j, x1, y1)
            }
        };

        (n0 + n1 + n2) * 99.83685446303647
    }

    /// OpenSimplex2 3D: dos grids de cubo rotados con offset.
    /// Las coordenadas ya vienen pre-rotadas desde [`Self::transform3`].
    fn open_simplex2_3(&self, mut seed: i32, x: f32, y: f32, z: f32) -> f32 {
        let i = fast_round(x);
        let j = fast_round(y);
        let k = fast_round(z);
        let x0 = x - i as f32;
        let y0 = y - j as f32;
        let z0 = z - k as f32;

        let mut x_n_sign = (-1.0 - x0) as i32 | 1;
        let mut y_n_sign = (-1.0 - y0) as i32 | 1;
        let mut z_n_sign = (-1.0 - z0) as i32 | 1;

        let mut ax0 = x_n_sign as f32 * -x0;
        let mut ay0 = y_n_sign as f32 * -y0;
        let mut az0 = z_n_sign as f32 * -z0;

        let mut i = i.wrapping_mul(PRIME_X);
        let mut j = j.wrapping_mul(PRIME_Y);
        let mut k = k.wrapping_mul(PRIME_Z);

        let mut value = 0.0;
        let mut a = (0.6 - x0 * x0) - (y0 * y0 + z0 * z0);
        let (mut x0, mut y0, mut z0) = (x0, y0, z0);

        for l in 0..2 {
            if a > 0.0 {
                value += (a * a) * (a * a) * grad_coord3(seed, i, j, k, x0, y0, z0);
            }

            if ax0 >= ay0 && ax0 >= az0 {
                let b = a + ax0 + ax0;
                if b > 1.0 {
                    let b = b - 1.0;
                    value += (b * b)
                        * (b * b)
                        * grad_coord3(
                            seed,
                            i.wrapping_add((-x_n_sign).wrapping_mul(PRIME_X)),
                            j,
                            k,
                            x0 + x_n_sign as f32,
                            y0,
                            z0,
                        );
                }
            } else if ay0 > ax0 && ay0 >= az0 {
                let b = a + ay0 + ay0;
                if b > 1.0 {
                    let b = b - 1.0;
                    value += (b * b)
                        * (b * b)
                        * grad_coord3(
                            seed,
                            i,
                            j.wrapping_add((-y_n_sign).wrapping_mul(PRIME_Y)),
                            k,
                            x0,
                            y0 + y_n_sign as f32,
                            z0,
                        );
                }
            } else {
                let b = a + az0 + az0;
                if b > 1.0 {
                    let b = b - 1.0;
                    value += (b * b)
                        * (b * b)
                        * grad_coord3(
                            seed,
                            i,
                            j,
                            k.wrapping_add((-z_n_sign).wrapping_mul(PRIME_Z)),
                            x0,
                            y0,
                            z0 + z_n_sign as f32,
                        );
                }
            }

            if l == 1 {
                break;
            }

            ax0 = 0.5 - ax0;
            ay0 = 0.5 - ay0;
            az0 = 0.5 - az0;

            x0 = x_n_sign as f32 * ax0;
            y0 = y_n_sign as f32 * ay0;
            z0 = z_n_sign as f32 * az0;

            a += (0.75 - ax0) - (ay0 + az0);

            i = i.wrapping_add((x_n_sign >> 1) & PRIME_X);
            j = j.wrapping_add((y_n_sign >> 1) & PRIME_Y);
            k = k.wrapping_add((z_n_sign >> 1) & PRIME_Z);

            x_n_sign = -x_n_sign;
            y_n_sign = -y_n_sign;
            z_n_sign = -z_n_sign;

            seed = !seed;
        }

        value * 32.69428253173828125
    }

    /// OpenSimplex2S 2D: simplex 2D modificado.
    fn open_simplex2s2(&self, seed: i32, x: f32, y: f32) -> f32 {
        const SQRT3: f32 = 1.7320508075688772935274463415059;
        const G2: f32 = (3.0 - SQRT3) / 6.0;

        let i = fast_floor(x);
        let j = fast_floor(y);
        let xi = x - i as f32;
        let yi = y - j as f32;

        let i = i.wrapping_mul(PRIME_X);
        let j = j.wrapping_mul(PRIME_Y);
        let i1 = i.wrapping_add(PRIME_X);
        let j1 = j.wrapping_add(PRIME_Y);

        let t = (xi + yi) * G2;
        let x0 = xi - t;
        let y0 = yi - t;

        let a0 = (2.0 / 3.0) - x0 * x0 - y0 * y0;
        let mut value = (a0 * a0) * (a0 * a0) * grad_coord2(seed, i, j, x0, y0);

        let a1 = (2.0 * (1.0 - 2.0 * G2) * (1.0 / G2 - 2.0)) * t
            + (-2.0 * (1.0 - 2.0 * G2) * (1.0 - 2.0 * G2) + a0);
        let x1 = x0 - (1.0 - 2.0 * G2);
        let y1 = y0 - (1.0 - 2.0 * G2);
        value += (a1 * a1) * (a1 * a1) * grad_coord2(seed, i1, j1, x1, y1);

        // Los condicionales anidados resultan más rápidos que la lógica de bits compacta.
        let xmyi = xi - yi;
        if t > G2 {
            if xi + xmyi > 1.0 {
                let x2 = x0 + (3.0 * G2 - 2.0);
                let y2 = y0 + (3.0 * G2 - 1.0);
                let a2 = (2.0 / 3.0) - x2 * x2 - y2 * y2;
                if a2 > 0.0 {
                    value += (a2 * a2)
                        * (a2 * a2)
                        * grad_coord2(
                            seed,
                            i.wrapping_add(PRIME_X << 1),
                            j.wrapping_add(PRIME_Y),
                            x2,
                            y2,
                        );
                }
            } else {
                let x2 = x0 + G2;
                let y2 = y0 + (G2 - 1.0);
                let a2 = (2.0 / 3.0) - x2 * x2 - y2 * y2;
                if a2 > 0.0 {
                    value += (a2 * a2)
                        * (a2 * a2)
                        * grad_coord2(seed, i, j.wrapping_add(PRIME_Y), x2, y2);
                }
            }

            if yi - xmyi > 1.0 {
                let x3 = x0 + (3.0 * G2 - 1.0);
                let y3 = y0 + (3.0 * G2 - 2.0);
                let a3 = (2.0 / 3.0) - x3 * x3 - y3 * y3;
                if a3 > 0.0 {
                    value += (a3 * a3)
                        * (a3 * a3)
                        * grad_coord2(
                            seed,
                            i.wrapping_add(PRIME_X),
                            j.wrapping_add(PRIME_Y << 1),
                            x3,
                            y3,
                        );
                }
            } else {
                let x3 = x0 + (G2 - 1.0);
                let y3 = y0 + G2;
                let a3 = (2.0 / 3.0) - x3 * x3 - y3 * y3;
                if a3 > 0.0 {
                    value += (a3 * a3)
                        * (a3 * a3)
                        * grad_coord2(seed, i.wrapping_add(PRIME_X), j, x3, y3);
                }
            }
        } else {
            if xi + xmyi < 0.0 {
                let x2 = x0 + (1.0 - G2);
                let y2 = y0 - G2;
                let a2 = (2.0 / 3.0) - x2 * x2 - y2 * y2;
                if a2 > 0.0 {
                    value += (a2 * a2)
                        * (a2 * a2)
                        * grad_coord2(seed, i.wrapping_sub(PRIME_X), j, x2, y2);
                }
            } else {
                let x2 = x0 + (G2 - 1.0);
                let y2 = y0 + G2;
                let a2 = (2.0 / 3.0) - x2 * x2 - y2 * y2;
                if a2 > 0.0 {
                    value += (a2 * a2)
                        * (a2 * a2)
                        * grad_coord2(seed, i.wrapping_add(PRIME_X), j, x2, y2);
                }
            }

            if yi < xmyi {
                let x2 = x0 - G2;
                let y2 = y0 - (G2 - 1.0);
                let a2 = (2.0 / 3.0) - x2 * x2 - y2 * y2;
                if a2 > 0.0 {
                    value += (a2 * a2)
                        * (a2 * a2)
                        * grad_coord2(seed, i, j.wrapping_sub(PRIME_Y), x2, y2);
                }
            } else {
                let x2 = x0 + G2;
                let y2 = y0 + (G2 - 1.0);
                let a2 = (2.0 / 3.0) - x2 * x2 - y2 * y2;
                if a2 > 0.0 {
                    value += (a2 * a2)
                        * (a2 * a2)
                        * grad_coord2(seed, i, j.wrapping_add(PRIME_Y), x2, y2);
                }
            }
        }

        value * 18.24196194486065
    }

    /// OpenSimplex2S 3D: dos grids de cubo rotados con offset.
    fn open_simplex2s3(&self, seed: i32, x: f32, y: f32, z: f32) -> f32 {
        let i = fast_floor(x);
        let j = fast_floor(y);
        let k = fast_floor(z);
        let xi = x - i as f32;
        let yi = y - j as f32;
        let zi = z - k as f32;

        let i = i.wrapping_mul(PRIME_X);
        let j = j.wrapping_mul(PRIME_Y);
        let k = k.wrapping_mul(PRIME_Z);
        let seed2 = seed.wrapping_add(1_293_373);

        let x_n_mask = (-0.5 - xi) as i32;
        let y_n_mask = (-0.5 - yi) as i32;
        let z_n_mask = (-0.5 - zi) as i32;

        let x0 = xi + x_n_mask as f32;
        let y0 = yi + y_n_mask as f32;
        let z0 = zi + z_n_mask as f32;
        let a0 = 0.75 - x0 * x0 - y0 * y0 - z0 * z0;
        let mut value = (a0 * a0)
            * (a0 * a0)
            * grad_coord3(
                seed,
                i.wrapping_add(x_n_mask & PRIME_X),
                j.wrapping_add(y_n_mask & PRIME_Y),
                k.wrapping_add(z_n_mask & PRIME_Z),
                x0,
                y0,
                z0,
            );

        let x1 = xi - 0.5;
        let y1 = yi - 0.5;
        let z1 = zi - 0.5;
        let a1 = 0.75 - x1 * x1 - y1 * y1 - z1 * z1;
        value += (a1 * a1)
            * (a1 * a1)
            * grad_coord3(
                seed2,
                i.wrapping_add(PRIME_X),
                j.wrapping_add(PRIME_Y),
                k.wrapping_add(PRIME_Z),
                x1,
                y1,
                z1,
            );

        let x_a_flip_mask0 = (((x_n_mask | 1) << 1) as f32) * x1;
        let y_a_flip_mask0 = (((y_n_mask | 1) << 1) as f32) * y1;
        let z_a_flip_mask0 = (((z_n_mask | 1) << 1) as f32) * z1;
        let x_a_flip_mask1 = (-2 - (x_n_mask << 2)) as f32 * x1 - 1.0;
        let y_a_flip_mask1 = (-2 - (y_n_mask << 2)) as f32 * y1 - 1.0;
        let z_a_flip_mask1 = (-2 - (z_n_mask << 2)) as f32 * z1 - 1.0;

        let mut skip5 = false;
        let a2 = x_a_flip_mask0 + a0;
        if a2 > 0.0 {
            let x2 = x0 - (x_n_mask | 1) as f32;
            let y2 = y0;
            let z2 = z0;
            value += (a2 * a2)
                * (a2 * a2)
                * grad_coord3(
                    seed,
                    i.wrapping_add(!x_n_mask & PRIME_X),
                    j.wrapping_add(y_n_mask & PRIME_Y),
                    k.wrapping_add(z_n_mask & PRIME_Z),
                    x2,
                    y2,
                    z2,
                );
        } else {
            let a3 = y_a_flip_mask0 + z_a_flip_mask0 + a0;
            if a3 > 0.0 {
                let x3 = x0;
                let y3 = y0 - (y_n_mask | 1) as f32;
                let z3 = z0 - (z_n_mask | 1) as f32;
                value += (a3 * a3)
                    * (a3 * a3)
                    * grad_coord3(
                        seed,
                        i.wrapping_add(x_n_mask & PRIME_X),
                        j.wrapping_add(!y_n_mask & PRIME_Y),
                        k.wrapping_add(!z_n_mask & PRIME_Z),
                        x3,
                        y3,
                        z3,
                    );
            }

            let a4 = x_a_flip_mask1 + a1;
            if a4 > 0.0 {
                let x4 = (x_n_mask | 1) as f32 + x1;
                let y4 = y1;
                let z4 = z1;
                value += (a4 * a4)
                    * (a4 * a4)
                    * grad_coord3(
                        seed2,
                        i.wrapping_add(x_n_mask & (PRIME_X << 1)),
                        j.wrapping_add(PRIME_Y),
                        k.wrapping_add(PRIME_Z),
                        x4,
                        y4,
                        z4,
                    );
                skip5 = true;
            }
        }

        let mut skip9 = false;
        let a6 = y_a_flip_mask0 + a0;
        if a6 > 0.0 {
            let x6 = x0;
            let y6 = y0 - (y_n_mask | 1) as f32;
            let z6 = z0;
            value += (a6 * a6)
                * (a6 * a6)
                * grad_coord3(
                    seed,
                    i.wrapping_add(x_n_mask & PRIME_X),
                    j.wrapping_add(!y_n_mask & PRIME_Y),
                    k.wrapping_add(z_n_mask & PRIME_Z),
                    x6,
                    y6,
                    z6,
                );
        } else {
            let a7 = x_a_flip_mask0 + z_a_flip_mask0 + a0;
            if a7 > 0.0 {
                let x7 = x0 - (x_n_mask | 1) as f32;
                let y7 = y0;
                let z7 = z0 - (z_n_mask | 1) as f32;
                value += (a7 * a7)
                    * (a7 * a7)
                    * grad_coord3(
                        seed,
                        i.wrapping_add(!x_n_mask & PRIME_X),
                        j.wrapping_add(y_n_mask & PRIME_Y),
                        k.wrapping_add(!z_n_mask & PRIME_Z),
                        x7,
                        y7,
                        z7,
                    );
            }

            let a8 = y_a_flip_mask1 + a1;
            if a8 > 0.0 {
                let x8 = x1;
                let y8 = (y_n_mask | 1) as f32 + y1;
                let z8 = z1;
                value += (a8 * a8)
                    * (a8 * a8)
                    * grad_coord3(
                        seed2,
                        i.wrapping_add(PRIME_X),
                        j.wrapping_add(y_n_mask & (PRIME_Y << 1)),
                        k.wrapping_add(PRIME_Z),
                        x8,
                        y8,
                        z8,
                    );
                skip9 = true;
            }
        }

        let mut skip_d = false;
        let a_a = z_a_flip_mask0 + a0;
        if a_a > 0.0 {
            let x_a = x0;
            let y_a = y0;
            let z_a = z0 - (z_n_mask | 1) as f32;
            value += (a_a * a_a)
                * (a_a * a_a)
                * grad_coord3(
                    seed,
                    i.wrapping_add(x_n_mask & PRIME_X),
                    j.wrapping_add(y_n_mask & PRIME_Y),
                    k.wrapping_add(!z_n_mask & PRIME_Z),
                    x_a,
                    y_a,
                    z_a,
                );
        } else {
            let a_b = x_a_flip_mask0 + y_a_flip_mask0 + a0;
            if a_b > 0.0 {
                let x_b = x0 - (x_n_mask | 1) as f32;
                let y_b = y0 - (y_n_mask | 1) as f32;
                let z_b = z0;
                value += (a_b * a_b)
                    * (a_b * a_b)
                    * grad_coord3(
                        seed,
                        i.wrapping_add(!x_n_mask & PRIME_X),
                        j.wrapping_add(!y_n_mask & PRIME_Y),
                        k.wrapping_add(z_n_mask & PRIME_Z),
                        x_b,
                        y_b,
                        z_b,
                    );
            }

            let a_c = z_a_flip_mask1 + a1;
            if a_c > 0.0 {
                let x_c = x1;
                let y_c = y1;
                let z_c = (z_n_mask | 1) as f32 + z1;
                value += (a_c * a_c)
                    * (a_c * a_c)
                    * grad_coord3(
                        seed2,
                        i.wrapping_add(PRIME_X),
                        j.wrapping_add(PRIME_Y),
                        k.wrapping_add(z_n_mask & (PRIME_Z << 1)),
                        x_c,
                        y_c,
                        z_c,
                    );
                skip_d = true;
            }
        }

        if !skip5 {
            let a5 = y_a_flip_mask1 + z_a_flip_mask1 + a1;
            if a5 > 0.0 {
                let x5 = x1;
                let y5 = (y_n_mask | 1) as f32 + y1;
                let z5 = (z_n_mask | 1) as f32 + z1;
                value += (a5 * a5)
                    * (a5 * a5)
                    * grad_coord3(
                        seed2,
                        i.wrapping_add(PRIME_X),
                        j.wrapping_add(y_n_mask & (PRIME_Y << 1)),
                        k.wrapping_add(z_n_mask & (PRIME_Z << 1)),
                        x5,
                        y5,
                        z5,
                    );
            }
        }

        if !skip9 {
            let a9 = x_a_flip_mask1 + z_a_flip_mask1 + a1;
            if a9 > 0.0 {
                let x9 = (x_n_mask | 1) as f32 + x1;
                let y9 = y1;
                let z9 = (z_n_mask | 1) as f32 + z1;
                value += (a9 * a9)
                    * (a9 * a9)
                    * grad_coord3(
                        seed2,
                        i.wrapping_add(x_n_mask & (PRIME_X << 1)),
                        j.wrapping_add(PRIME_Y),
                        k.wrapping_add(z_n_mask & (PRIME_Z << 1)),
                        x9,
                        y9,
                        z9,
                    );
            }
        }

        if !skip_d {
            let a_d = x_a_flip_mask1 + y_a_flip_mask1 + a1;
            if a_d > 0.0 {
                let x_d = (x_n_mask | 1) as f32 + x1;
                let y_d = (y_n_mask | 1) as f32 + y1;
                let z_d = z1;
                value += (a_d * a_d)
                    * (a_d * a_d)
                    * grad_coord3(
                        seed2,
                        i.wrapping_add(x_n_mask & (PRIME_X << 1)),
                        j.wrapping_add(y_n_mask & (PRIME_Y << 1)),
                        k.wrapping_add(PRIME_Z),
                        x_d,
                        y_d,
                        z_d,
                    );
            }
        }

        value * 9.046026385208288
    }
}

/// Perlin 2D (coordenadas sin transformar; no usa skew/rotación).
fn perlin2(seed: i32, x: f32, y: f32) -> f32 {
    let x0 = fast_floor(x);
    let y0 = fast_floor(y);

    let xd0 = x - x0 as f32;
    let yd0 = y - y0 as f32;
    let xd1 = xd0 - 1.0;
    let yd1 = yd0 - 1.0;

    let xs = interp_quintic(xd0);
    let ys = interp_quintic(yd0);

    let x0 = x0.wrapping_mul(PRIME_X);
    let y0 = y0.wrapping_mul(PRIME_Y);
    let x1 = x0.wrapping_add(PRIME_X);
    let y1 = y0.wrapping_add(PRIME_Y);

    let xf0 = lerp(
        grad_coord2(seed, x0, y0, xd0, yd0),
        grad_coord2(seed, x1, y0, xd1, yd0),
        xs,
    );
    let xf1 = lerp(
        grad_coord2(seed, x0, y1, xd0, yd1),
        grad_coord2(seed, x1, y1, xd1, yd1),
        xs,
    );

    lerp(xf0, xf1, ys) * 1.4247691104677813
}

/// Perlin 3D.
fn perlin3(seed: i32, x: f32, y: f32, z: f32) -> f32 {
    let x0 = fast_floor(x);
    let y0 = fast_floor(y);
    let z0 = fast_floor(z);

    let xd0 = x - x0 as f32;
    let yd0 = y - y0 as f32;
    let zd0 = z - z0 as f32;
    let xd1 = xd0 - 1.0;
    let yd1 = yd0 - 1.0;
    let zd1 = zd0 - 1.0;

    let xs = interp_quintic(xd0);
    let ys = interp_quintic(yd0);
    let zs = interp_quintic(zd0);

    let x0 = x0.wrapping_mul(PRIME_X);
    let y0 = y0.wrapping_mul(PRIME_Y);
    let z0 = z0.wrapping_mul(PRIME_Z);
    let x1 = x0.wrapping_add(PRIME_X);
    let y1 = y0.wrapping_add(PRIME_Y);
    let z1 = z0.wrapping_add(PRIME_Z);

    let xf00 = lerp(
        grad_coord3(seed, x0, y0, z0, xd0, yd0, zd0),
        grad_coord3(seed, x1, y0, z0, xd1, yd0, zd0),
        xs,
    );
    let xf10 = lerp(
        grad_coord3(seed, x0, y1, z0, xd0, yd1, zd0),
        grad_coord3(seed, x1, y1, z0, xd1, yd1, zd0),
        xs,
    );
    let xf01 = lerp(
        grad_coord3(seed, x0, y0, z1, xd0, yd0, zd1),
        grad_coord3(seed, x1, y0, z1, xd1, yd0, zd1),
        xs,
    );
    let xf11 = lerp(
        grad_coord3(seed, x0, y1, z1, xd0, yd1, zd1),
        grad_coord3(seed, x1, y1, z1, xd1, yd1, zd1),
        xs,
    );

    let yf0 = lerp(xf00, xf10, ys);
    let yf1 = lerp(xf01, xf11, ys);

    lerp(yf0, yf1, zs) * 0.964921414852142333984375
}

/// ValueCubic 2D.
fn value_cubic2(seed: i32, x: f32, y: f32) -> f32 {
    let x1 = fast_floor(x);
    let y1 = fast_floor(y);

    let xs = x - x1 as f32;
    let ys = y - y1 as f32;

    let x1 = x1.wrapping_mul(PRIME_X);
    let y1 = y1.wrapping_mul(PRIME_Y);
    let x0 = x1.wrapping_sub(PRIME_X);
    let y0 = y1.wrapping_sub(PRIME_Y);
    let x2 = x1.wrapping_add(PRIME_X);
    let y2 = y1.wrapping_add(PRIME_Y);
    let x3 = x1.wrapping_add(PRIME_X << 1);
    let y3 = y1.wrapping_add(PRIME_Y << 1);

    cubic_lerp(
        cubic_lerp(
            val_coord2(seed, x0, y0),
            val_coord2(seed, x1, y0),
            val_coord2(seed, x2, y0),
            val_coord2(seed, x3, y0),
            xs,
        ),
        cubic_lerp(
            val_coord2(seed, x0, y1),
            val_coord2(seed, x1, y1),
            val_coord2(seed, x2, y1),
            val_coord2(seed, x3, y1),
            xs,
        ),
        cubic_lerp(
            val_coord2(seed, x0, y2),
            val_coord2(seed, x1, y2),
            val_coord2(seed, x2, y2),
            val_coord2(seed, x3, y2),
            xs,
        ),
        cubic_lerp(
            val_coord2(seed, x0, y3),
            val_coord2(seed, x1, y3),
            val_coord2(seed, x2, y3),
            val_coord2(seed, x3, y3),
            xs,
        ),
        ys,
    ) * (1.0 / (1.5 * 1.5))
}

/// ValueCubic 3D.
fn value_cubic3(seed: i32, x: f32, y: f32, z: f32) -> f32 {
    let x1 = fast_floor(x);
    let y1 = fast_floor(y);
    let z1 = fast_floor(z);

    let xs = x - x1 as f32;
    let ys = y - y1 as f32;
    let zs = z - z1 as f32;

    let x1 = x1.wrapping_mul(PRIME_X);
    let y1 = y1.wrapping_mul(PRIME_Y);
    let z1 = z1.wrapping_mul(PRIME_Z);

    let x0 = x1.wrapping_sub(PRIME_X);
    let y0 = y1.wrapping_sub(PRIME_Y);
    let z0 = z1.wrapping_sub(PRIME_Z);
    let x2 = x1.wrapping_add(PRIME_X);
    let y2 = y1.wrapping_add(PRIME_Y);
    let z2 = z1.wrapping_add(PRIME_Z);
    let x3 = x1.wrapping_add(PRIME_X << 1);
    let y3 = y1.wrapping_add(PRIME_Y << 1);
    let z3 = z1.wrapping_add(PRIME_Z << 1);

    // CubicLerp anidado: por capa Z, cuatro interpolaciones en X (y0..y3) y una
    // interpolación en Y entre ellas; el resultado final interpola las 4 capas en Z.
    let layer = |zp: i32| -> f32 {
        let row = |yp: i32| -> f32 {
            cubic_lerp(
                val_coord3(seed, x0, yp, zp),
                val_coord3(seed, x1, yp, zp),
                val_coord3(seed, x2, yp, zp),
                val_coord3(seed, x3, yp, zp),
                xs,
            )
        };
        cubic_lerp(row(y0), row(y1), row(y2), row(y3), ys)
    };

    cubic_lerp(layer(z0), layer(z1), layer(z2), layer(z3), zs) * (1.0 / (1.5 * 1.5 * 1.5))
}

/// Value 2D.
fn value2(seed: i32, x: f32, y: f32) -> f32 {
    let x0 = fast_floor(x);
    let y0 = fast_floor(y);

    let xs = interp_hermite(x - x0 as f32);
    let ys = interp_hermite(y - y0 as f32);

    let x0 = x0.wrapping_mul(PRIME_X);
    let y0 = y0.wrapping_mul(PRIME_Y);
    let x1 = x0.wrapping_add(PRIME_X);
    let y1 = y0.wrapping_add(PRIME_Y);

    let xf0 = lerp(val_coord2(seed, x0, y0), val_coord2(seed, x1, y0), xs);
    let xf1 = lerp(val_coord2(seed, x0, y1), val_coord2(seed, x1, y1), xs);

    lerp(xf0, xf1, ys)
}

/// Value 3D.
fn value3(seed: i32, x: f32, y: f32, z: f32) -> f32 {
    let x0 = fast_floor(x);
    let y0 = fast_floor(y);
    let z0 = fast_floor(z);

    let xs = interp_hermite(x - x0 as f32);
    let ys = interp_hermite(y - y0 as f32);
    let zs = interp_hermite(z - z0 as f32);

    let x0 = x0.wrapping_mul(PRIME_X);
    let y0 = y0.wrapping_mul(PRIME_Y);
    let z0 = z0.wrapping_mul(PRIME_Z);
    let x1 = x0.wrapping_add(PRIME_X);
    let y1 = y0.wrapping_add(PRIME_Y);
    let z1 = z0.wrapping_add(PRIME_Z);

    let xf00 = lerp(
        val_coord3(seed, x0, y0, z0),
        val_coord3(seed, x1, y0, z0),
        xs,
    );
    let xf10 = lerp(
        val_coord3(seed, x0, y1, z0),
        val_coord3(seed, x1, y1, z0),
        xs,
    );
    let xf01 = lerp(
        val_coord3(seed, x0, y0, z1),
        val_coord3(seed, x1, y0, z1),
        xs,
    );
    let xf11 = lerp(
        val_coord3(seed, x0, y1, z1),
        val_coord3(seed, x1, y1, z1),
        xs,
    );

    let yf0 = lerp(xf00, xf10, ys);
    let yf1 = lerp(xf01, xf11, ys);

    lerp(yf0, yf1, zs)
}

// fast_min/fast_max se usan en fbm2; se definen aquí para mantener el port 1:1.
#[inline]
fn fast_min(a: f32, b: f32) -> f32 {
    if a < b { a } else { b }
}
