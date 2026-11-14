//! F1: determinismo del ruido, del campo y del RNG, y rangos de salida.

use rer_worldgen::config::WorldGenConfig;
use rer_worldgen::field::DensityField;
use rer_worldgen::noise::{FractalType, Noise, NoiseType};
use rer_worldgen::seed::Rng;

fn sample_grid(noise: &Noise, n: i32, step: f32) -> Vec<f32> {
    (0..n)
        .map(|i| noise.get2(i as f32 * step, (i * 7 % 13) as f32 * step))
        .collect()
}

#[test]
fn noise_is_deterministic_per_seed() {
    let a = Noise::new(42).frequency(0.01);
    let b = Noise::new(42).frequency(0.01);
    let ga = sample_grid(&a, 256, 0.731);
    let gb = sample_grid(&b, 256, 0.731);
    assert_eq!(
        ga, gb,
        "misma seed debe producir exactamente los mismos valores"
    );
}

#[test]
fn noise_differs_across_seeds() {
    let a = sample_grid(&Noise::new(1).frequency(0.01), 128, 0.731);
    let b = sample_grid(&Noise::new(2).frequency(0.01), 128, 0.731);
    let equal = a.iter().zip(&b).filter(|(x, y)| x == y).count();
    assert!(
        equal < a.len() / 8,
        "seeds distintas deben descorrelacionarse (iguales: {equal}/128)"
    );
}

#[test]
fn noise_stays_in_unit_range_for_all_types_and_fractals() {
    let types = [
        NoiseType::OpenSimplex2,
        NoiseType::OpenSimplex2S,
        NoiseType::Perlin,
        NoiseType::ValueCubic,
        NoiseType::Value,
    ];
    let fractals = [
        FractalType::None,
        FractalType::Fbm,
        FractalType::Ridged,
        FractalType::PingPong,
    ];
    for (ti, noise_type) in types.into_iter().enumerate() {
        for (fi, fractal_type) in fractals.into_iter().enumerate() {
            let noise = Noise::new(7)
                .noise_type(noise_type)
                .frequency(0.013)
                .fractal(fractal_type, 4, 0.5, 0.2);
            for i in 0..2_000i32 {
                let x = i as f32 * 0.137;
                let z = (i * 31 % 97) as f32 * 0.211;
                let v2 = noise.get2(x, z);
                assert!(
                    (-1.001..=1.001).contains(&v2),
                    "2D fuera de rango: type#{ti} fractal#{fi} -> {v2} en ({x}, {z})"
                );
                let v3 = noise.get3(x, z, x * 0.5 - z);
                assert!(
                    (-1.001..=1.001).contains(&v3),
                    "3D fuera de rango: type#{ti} fractal#{fi} -> {v3} en ({x}, {z})"
                );
            }
        }
    }
}

#[test]
fn ridged_produces_peaks_not_basins() {
    // Ridged normalizado: el valor medio debe ser positivo (crestas afiladas).
    let noise = Noise::new(11)
        .frequency(0.004)
        .fractal(FractalType::Ridged, 4, 0.5, 0.0);
    let mean: f32 = (0..4_000)
        .map(|i| noise.get2(i as f32 * 0.77, (i * 13 % 71) as f32 * 0.61))
        .sum::<f32>()
        / 4_000.0;
    assert!(
        mean > 0.0,
        "ridged debería tener media positiva (crestas), obtuvo {mean}"
    );
}

#[test]
fn heightfield_is_deterministic_and_clamped_to_bounds() {
    let config = WorldGenConfig::default();
    config.validate().expect("config por defecto valida");
    let field = DensityField::new(&config);
    let mut first_pass = Vec::new();
    for i in 0..500i32 {
        let x = (i * 37 % 511) as f32;
        let z = (i * 89 % 511) as f32;
        first_pass.push(field.height_at(x, z));
    }
    let field_b = DensityField::new(&config);
    for (i, h) in first_pass.iter().enumerate() {
        let x = (i as i32 * 37 % 511) as f32;
        let z = (i as i32 * 89 % 511) as f32;
        assert_eq!(
            *h,
            field_b.height_at(x, z),
            "heightfield no determinista en ({x}, {z})"
        );
        assert!(
            (2.0..=config.bounds.height - 2.0).contains(h),
            "altura {h} fuera de los límites en ({x}, {z})"
        );
    }
}

#[test]
fn density_sign_matches_height_on_intact_surface() {
    // Con carving activo (cuevas/ríos/lagos/roca), columnas concretas pueden
    // estar perforadas u overhangs elevados; la propiedad de signo se verifica
    // en las columnas intactas, que deben ser la gran mayoría del mundo.
    let config = WorldGenConfig::default();
    let field = DensityField::new(&config);
    let mut intact = 0;
    let mut total = 0;
    for x in 0..32i32 {
        for z in 0..32i32 {
            let (px, pz) = (x as f32 * 16.0 + 0.5, z as f32 * 16.0 + 0.5);
            let h = field.height_at(px, pz);
            total += 1;
            let below = field.density_at(px, h - 1.0, pz);
            let above = field.density_at(px, h + 1.0, pz);
            if below > 0.0 && above < 0.0 {
                intact += 1;
            } else {
                // Perforada u overhang: el signo puede diferir, pero nunca a la
                // inversa completa (superficie enterrada en aire masivo).
                assert!(
                    !(below < -5.0 && above > 5.0),
                    "inversión de superficie sospechosa en ({px}, {pz})"
                );
            }
        }
    }
    assert!(
        intact > total * 4 / 5,
        "más del 80% de la superficie debe estar intacta (intactas {intact}/{total})"
    );
}

#[test]
fn rng_is_deterministic_and_in_range() {
    let mut a = Rng::new(1234);
    let mut b = Rng::new(1234);
    for _ in 0..1_000 {
        assert_eq!(a.next_f32(), b.next_f32());
    }
    let mut rng = Rng::new(99);
    for _ in 0..2_000 {
        let v = rng.next_f32();
        assert!((0.0..1.0).contains(&v));
        let r = rng.range_f32(-5.0, 5.0);
        assert!((-5.0..5.0).contains(&r));
        let i = rng.range_i32(3, 7);
        assert!((3..=7).contains(&i));
    }
}
