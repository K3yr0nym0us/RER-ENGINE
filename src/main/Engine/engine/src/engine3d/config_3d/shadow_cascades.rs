//! Cascadas de sombras (CSM): cortes por distancia y ajuste ortográfico de cada cascada.
//!
//! Matemática PURA: ni Vulkan ni `State`. `State` arma la base de la cámara de luz en
//! `config_3d/directional_light.rs` y usa estos resultados para volcarlos en el UBO. Separarlo
//! permite comprobar en host el ciclo completo (cortes → esquinas → matriz → muestreo), que es
//! donde aparecían los fallos de coherencia entre la capa que se ESCRIBE y la que se LEE.
//!
//! Convención de profundidad: se reutiliza `light_view_proj_from` (que usa `Mat4::orthographic_rh`
//! y `round()` para el texel-snapping), igual que el shadow map único. Escribir y muestrear pasan
//! por LA MISMA matriz; cambiar aquí la convención rompería el `depth_ref` del shader.

use glam::{Mat4, Vec3};

use crate::engine3d::config_3d::directional_light::light_view_proj_from;

/// Número máximo de capas del array de sombras. Fijo: cambiar de tier reasigna resolución y
/// cuántas capas se dibujan, nunca el tamaño del array, así los descriptores no se tocan.
pub const MAX_SHADOW_CASCADES: usize = 4;

/// Mezcla log/uniforme de los cortes. `0.0` = uniformes (la primera cascada desperdicia
/// resolución cerca de la cámara); `1.0` = logarítmicos (poca densidad en la lejana y saltos
/// bruscos de texel). El punto intermedio es el habitual en motores en tiempo real.
pub const SPLIT_LAMBDA: f32 = 0.55;

/// El extent de cada cascada se redondea a múltiplos de este valor (unidades de mundo). Sin esto,
/// el tamaño de texel cambiaría cada frame con la cámara y la sombra herviría aunque el centro
/// estuviera alineado a la rejilla. El `round()` del snapping es competencia de `light_view_proj_from`.
const EXTENT_QUANTUM: f32 = 0.5;

/// Borde lejano de cada cascada, medido desde el ojo de la cámara.
///
/// Devuelve siempre `MAX_SHADOW_CASCADES` entradas: las casillas de cascadas inexistentes valen
/// `f32::MAX`, así el shader puede buscar la primera cascada cuyo borde supera la profundidad sin
/// ramificar por tier y nunca se queda fuera de rango.
///
/// Creciente por construcción: uniforme y logarítmico crecen con la fracción, y la mezcla de dos
/// funciones crecientes es creciente. La última cascada termina exactamente en `shadow_far`.
#[must_use]
pub fn split_distances(near: f32, shadow_far: f32, count: usize) -> [f32; MAX_SHADOW_CASCADES] {
    let mut out = [f32::MAX; MAX_SHADOW_CASCADES];
    let count = count.clamp(1, MAX_SHADOW_CASCADES);
    let near = near.max(0.05);
    // Que el borde lejano nunca caiga por debajo del cercano: con cámaras muy pegadas al suelo
    // `far` puede llegar a ser menor que `near` redondeado.
    let shadow_far = shadow_far.max(near * 1.05);
    for (index, slot) in out.iter_mut().enumerate().take(count) {
        let fraction = (index + 1) as f32 / count as f32;
        let uniform = near + (shadow_far - near) * fraction;
        let logarithmic = near * (shadow_far / near).powf(fraction);
        *slot =
            (SPLIT_LAMBDA * logarithmic + (1.0 - SPLIT_LAMBDA) * uniform).clamp(near, shadow_far);
    }
    // Fijado explícitamente para que el shader no descarte el último píxel por precisión de f32.
    out[count - 1] = shadow_far;
    out
}

/// Esquinas de la loncha de frustum comprendida entre `near` y `far`.
///
/// `center` es el ojo de la cámara; `forward`, `right` y `up` son su base ortonormal (en ese
/// orden, como la deriva `State` de la matriz de vista).
/// `half_height` es la tangente de la mitad del FOV vertical, es decir, la mitad de la altura a
/// 1 m de distancia: a una distancia `r` la altura vale `r * half_height` y la anchura
/// `r * half_height * aspect`. Las 4 esquinas de cada plano se devuelven en orden
/// (-,-), (+,-), (-,+), (+,+); primero las del plano cercano y después las del lejano.
#[must_use]
pub fn slice_corners(
    center: Vec3,
    forward: Vec3,
    right: Vec3,
    up: Vec3,
    half_height: f32,
    aspect: f32,
    near: f32,
    far: f32,
) -> [Vec3; 8] {
    let mut corners = [Vec3::ZERO; 8];
    for (slot, radius) in [near.max(0.0), far.max(0.0)].iter().enumerate() {
        let height = radius * half_height.max(1e-4);
        let width = height * aspect.max(0.01);
        let middle = center + forward * *radius;
        for (index, (sx, sy)) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)]
            .iter()
            .enumerate()
        {
            corners[slot * 4 + index] = middle + right * (width * *sx) + up * (height * *sy);
        }
    }
    corners
}

/// Mayor componente absoluta de un vector (glam 0.33 no tiene `max_component`).
fn max_axis(v: Vec3) -> f32 {
    v.x.max(v.y).max(v.z)
}

/// Proyección ortográfica de una cascada ya ajustada a sus esquinas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CascadeFit {
    /// Vista × proyección de la luz, en el mismo formato de columnas que el UBO.
    pub view_proj: [[f32; 4]; 4],
    /// Mitad del lado de la caja ortográfica, en unidades de mundo. Sirve para escalar el radio
    /// PCF: una cascada lejana cubre más mundo por texel y necesita taps más separados.
    pub extent: f32,
}

impl CascadeFit {
    /// La misma matriz en formato `glam`, útil para proyectar puntos en tests o depuración.
    #[cfg(test)]
    #[must_use]
    pub fn as_mat4(&self) -> Mat4 {
        Mat4::from_cols_array_2d(&self.view_proj)
    }
}

/// Ajusta la proyección ortográfica de la luz que encuadra `corners`.
///
/// El centro es la media de las esquinas; el extent es la mayor semiextensión de la nube en la
/// base de la luz (X, Y y Z, para que ninguna esquina quede detrás del plano near), cuantizado
/// hacia arriba a múltiplos de 0.5 m. El alineado a la rejilla lo hace `light_view_proj_from`,
/// que usa `round()` sobre el ancla del suelo; el margen añadido aquí absorba como máximo medio
/// texel de desplazamiento.
///
/// `up` debe ser la normal FIJA del plano de órbita del sol (la que devuelve
/// `light_up_for_cardinal`), nunca un `up_hint` que cambie con `dir.y`: eso giraba la base 180°
/// al cruzar el cenit. Devuelve `None` si la luz o la base son degeneradas o si la matriz no es
/// finita; en ese caso el llamador debe conservar la cascada anterior en vez de escribir basura.
#[must_use]
pub fn fit_cascade(
    corners: &[Vec3],
    light_dir: Vec3,
    map_size: u32,
    up: Vec3,
) -> Option<CascadeFit> {
    if corners.is_empty() || !corners.iter().all(|c| c.is_finite()) {
        return None;
    }
    let dir = light_dir.normalize_or_zero();
    if dir == Vec3::ZERO || !dir.is_finite() || !up.is_finite() {
        return None;
    }

    // Base de la luz. Mismo criterion que `Mat4::look_at_rh`: x = up × dir, y = dir × x.
    // Un `up` paralelo a la luz deja la cámara sin base: se rechaza en vez de escribir NaNs.
    let right = up.cross(dir);
    if right.length_squared() < 1e-10 {
        return None;
    }
    let right = right.normalize();
    let light_up = dir.cross(right).normalize();

    let center = corners.iter().fold(Vec3::ZERO, |sum, c| sum + *c) / corners.len() as f32;
    let center_local = Vec3::new(center.dot(right), center.dot(light_up), center.dot(dir));
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    let mut max_deviation = 0.0_f32;
    for corner in corners {
        let local = Vec3::new(corner.dot(right), corner.dot(light_up), corner.dot(dir));
        min = min.min(local);
        max = max.max(local);
        max_deviation = max_deviation.max(max_axis((local - center_local).abs()));
    }

    // Semiextensión de la caja en los tres ejes de la luz. La desviación respecto a la media
    // cubre también lonchas asimétricas (donde la media no coincide con el centro de la AABB).
    let raw = max_axis((max - min) * 0.5)
        .max(max_deviation)
        .max(EXTENT_QUANTUM);
    // `light_view_proj_from` desplaza el centro hasta medio texel para anclar el suelo. Con
    // `map_size` texeles ese desplazamiento vale como máximo extent / map_size; se reserva ese
    // margen antes de cuantizar para que las esquinas sigan dentro del ortho tras el snap.
    let map_size = map_size.max(2);
    let safety = raw / (map_size as f32 - 1.0);
    let extent = ((raw + safety) / EXTENT_QUANTUM).ceil() * EXTENT_QUANTUM;
    if !extent.is_finite() || extent <= 0.0 {
        return None;
    }

    let view_proj = light_view_proj_from(center, dir, extent, map_size, up);
    if !view_proj.iter().flatten().all(|value| value.is_finite()) {
        return None;
    }

    // La proyección de glam (`orthographic_rh`) deja la profundidad en NDC [-1, 1] y el ojo de
    // la luz a 2.5 * extent: con la semiextensión Z acotada por `extent`, todas las esquinas caen
    // en [1.5, 3.5] * extent, lejos de los planos 0.5 y 8 * extent del helper.
    let matrix = Mat4::from_cols_array_2d(&view_proj);
    for corner in corners {
        let clip = matrix * corner.extend(1.0);
        if clip.w <= 0.0 || !clip.is_finite() {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        if ndc.x.abs() > 1.0 + 1e-3 || ndc.y.abs() > 1.0 + 1e-3 || ndc.z.abs() > 1.0 {
            return None;
        }
    }

    Some(CascadeFit { view_proj, extent })
}

/// Primera cascada cuyo borde lejano cubre `view_dist`; `None` si la distancia queda más allá del
/// alcance de sombras o el número de cascadas no es válido. En el borde exacto se elige esa
/// cascada (el shader compara con `<=`), y una distancia negativa entra en la primera.
#[cfg(test)]
#[must_use]
pub fn select_cascade(
    view_dist: f32,
    splits: &[f32; MAX_SHADOW_CASCADES],
    count: usize,
) -> Option<usize> {
    if !(1..=MAX_SHADOW_CASCADES).contains(&count) || !view_dist.is_finite() {
        return None;
    }
    splits
        .iter()
        .take(count)
        .position(|split| view_dist <= *split)
}

/// Cámara que se reparte en cascadas. Se mantiene separada de `State` para poder simular en host.
#[derive(Clone, Copy, Debug)]
pub struct CascadeCamera {
    pub eye: Vec3,
    /// Ejes de vista ya normalizados y ortonormales.
    pub forward: Vec3,
    pub right: Vec3,
    pub up: Vec3,
    /// Tangente de la mitad del FOV vertical (`half_height` de `slice_corners`).
    pub tan_half_fov_y: f32,
    pub aspect: f32,
    pub near: f32,
    /// Borde lejano de la última cascada (alcance de las sombras).
    pub shadow_far: f32,
}

/// Conjunto de cascadas listo para volcar en `SceneUniforms`.
#[derive(Clone, Copy, Debug)]
pub struct CascadeSet {
    /// Una matriz `luz · proyección` por capa del array.
    pub matrices: [[[f32; 4]; 4]; MAX_SHADOW_CASCADES],
    /// Borde lejano de cada cascada desde el ojo; `f32::MAX` en las inactivas.
    pub splits: [f32; MAX_SHADOW_CASCADES],
    /// Mitad del lado de la caja ortográfica por cascada (unidades de mundo).
    pub extents: [f32; MAX_SHADOW_CASCADES],
    /// Cuántas cascadas se dibujarán y muestrearán (mínimo 1: con Off se calcula una de relleno).
    /// El conteo que ve el shader es el del tier, así que el motor lo toma de `ShadowTier` y este
    /// campo solo se consulta en los tests de la matemática pura.
    #[allow(dead_code)]
    pub count: usize,
}

/// Calcula cortes, cajas y matrices de todas las cascadas del frame en un solo paso.
///
/// `up` es la normal fija del plano de órbita del sol, compartida por todas las cascadas.
#[must_use]
pub fn cascade_set(
    camera: &CascadeCamera,
    count: usize,
    map_size: u32,
    light_dir: Vec3,
    up: Vec3,
) -> CascadeSet {
    let count = count.clamp(1, MAX_SHADOW_CASCADES);
    let identity = Mat4::IDENTITY.to_cols_array_2d();
    let mut set = CascadeSet {
        matrices: [identity; MAX_SHADOW_CASCADES],
        splits: [f32::MAX; MAX_SHADOW_CASCADES],
        extents: [0.0; MAX_SHADOW_CASCADES],
        count,
    };
    let splits = split_distances(camera.near, camera.shadow_far, count);
    let mut previous = camera.near.max(0.05);
    let mut last_valid: Option<([[f32; 4]; 4], f32)> = None;
    for (index, edge) in splits.iter().enumerate().take(count) {
        let corners = slice_corners(
            camera.eye,
            camera.forward,
            camera.right,
            camera.up,
            camera.tan_half_fov_y.max(1e-4),
            camera.aspect.max(0.01),
            previous,
            *edge,
        );
        previous = *edge;
        if let Some(fit) = fit_cascade(&corners, light_dir, map_size, up) {
            set.matrices[index] = fit.view_proj;
            set.extents[index] = fit.extent;
            last_valid = Some((fit.view_proj, fit.extent));
        }
        // El corte se guarda SIEMPRE que la cascada exista: si se omitiera al fallar un ajuste
        // vecino, el shader elegiría una cascada cuya matriz nunca se escribió.
        set.splits[index] = *edge;
    }
    if let Some((matrix, extent)) = last_valid {
        for slot in set.matrices.iter_mut().skip(count) {
            *slot = matrix;
        }
        for slot in set.extents.iter_mut().skip(count) {
            *slot = extent;
        }
    }
    set
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Caja cúbica centrada en `center`, con 8 esquinas en orden consistente.
    fn corners_around(center: Vec3, half: f32) -> [Vec3; 8] {
        let mut out = [Vec3::ZERO; 8];
        for (index, (sx, sy, sz)) in [
            (-1.0, -1.0, -1.0),
            (1.0, -1.0, -1.0),
            (-1.0, 1.0, -1.0),
            (1.0, 1.0, -1.0),
            (-1.0, -1.0, 1.0),
            (1.0, -1.0, 1.0),
            (-1.0, 1.0, 1.0),
            (1.0, 1.0, 1.0),
        ]
        .iter()
        .enumerate()
        {
            out[index] = center + Vec3::new(sx * half, sy * half, sz * half);
        }
        out
    }

    /// Proyecta con la matriz de la cascada (incluida la división entre `w`).
    fn project(fit: &CascadeFit, point: Vec3) -> Vec3 {
        let clip = fit.as_mat4() * point.extend(1.0);
        clip.truncate() / clip.w
    }

    /// Cámara de editor plausible (órbita mirando al origen) con ejes ortonormales reales.
    fn editor_camera(shadow_far: f32) -> CascadeCamera {
        let eye = Vec3::new(18.0, 16.0, 22.0);
        let forward = -eye.normalize();
        let right = forward.cross(Vec3::Y).normalize();
        let up = right.cross(forward).normalize();
        CascadeCamera {
            eye,
            forward,
            right,
            up,
            tan_half_fov_y: 22.5_f32.to_radians().tan(),
            aspect: 1.7,
            near: 0.1,
            shadow_far,
        }
    }

    #[test]
    fn split_distances_are_ordered_and_unused_slots_are_open() {
        let splits = split_distances(0.1, 80.0, 4);
        for window in splits.windows(2) {
            assert!(window[0] < window[1], "cortes no crecientes: {splits:?}");
        }
        assert!(splits[0] > 0.1 && splits[0] < 80.0);
        assert_eq!(
            splits[3], 80.0,
            "la última cascada llega al borde de sombra"
        );
    }

    #[test]
    fn split_distances_only_fill_the_requested_count() {
        let splits = split_distances(0.1, 40.0, 2);
        assert!(splits[0] < splits[1]);
        assert_eq!(splits[1], 40.0);
        assert_eq!(splits[2], f32::MAX);
        assert_eq!(splits[3], f32::MAX);
        // Una cascada única degenera en el shadow map clásico: cubre todo el rango.
        assert_eq!(split_distances(0.1, 40.0, 1)[0], 40.0);
    }

    #[test]
    fn first_cascade_is_denser_than_the_uniform_split() {
        // Con lambda > 0 el primer corte se acerca a la cámara: ahí se nota el CSM.
        let splits = split_distances(0.1, 100.0, 4);
        let uniform = 0.1 + (100.0 - 0.1) * 0.25;
        assert!(
            splits[0] < uniform,
            "esperado < {uniform}, got {}",
            splits[0]
        );
    }

    #[test]
    fn split_distances_clamp_count_and_degenerate_range() {
        assert_eq!(split_distances(0.1, 40.0, 0), split_distances(0.1, 40.0, 1));
        assert_eq!(
            split_distances(0.1, 40.0, 99),
            split_distances(0.1, 40.0, 4)
        );
        // far <= near no debe invertir los cortes ni salirse del rango.
        let degenerate = split_distances(2.0, 1.0, 3);
        assert!(degenerate[0] >= 2.0 && degenerate[2] > degenerate[0]);
        assert!(degenerate.iter().take(3).all(|v| v.is_finite()));
    }

    #[test]
    fn slice_corners_builds_eight_frustum_corners() {
        let eye = Vec3::new(0.0, 3.0, 10.0);
        let forward = Vec3::NEG_Z;
        let right = Vec3::X;
        let up = Vec3::Y;
        let corners = slice_corners(eye, forward, right, up, 0.5, 2.0, 2.0, 6.0);
        assert_eq!(corners.len(), 8);
        // Centro de cada plano a la distancia pedida, altura = r * tan y anchura = altura * aspect.
        for (slot, distance) in [2.0_f32, 6.0].iter().enumerate() {
            let slice = &corners[slot * 4..slot * 4 + 4];
            let middle = slice.iter().fold(Vec3::ZERO, |sum, c| sum + *c) / 4.0;
            assert!((middle - (eye + forward * *distance)).length() < 1e-5);
            let height = *distance * 0.5;
            assert!((slice[2].y - slice[0].y - 2.0 * height).abs() < 1e-5);
            assert!((slice[1].x - slice[0].x - 4.0 * height).abs() < 1e-5);
        }
        assert!(
            corners[4].z < corners[0].z,
            "el plano lejano debe estar más lejos"
        );
    }

    #[test]
    fn fit_contains_every_corner_inside_the_ortho_volume() {
        let corners = corners_around(Vec3::new(3.0, -2.0, 9.0), 4.0);
        for dir in [
            Vec3::new(-0.4, -1.0, 0.3),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(1.0, 0.1, 0.0).normalize(),
        ] {
            let fit = fit_cascade(&corners, dir, 1024, Vec3::Z).expect("ajuste válido");
            for corner in &corners {
                let p = project(&fit, *corner);
                assert!(
                    p.x >= -1.001 && p.x <= 1.001 && p.y >= -1.001 && p.y <= 1.001,
                    "esquina fuera del rectángulo: {p:?} (dir {dir:?})"
                );
                // `orthographic_rh` deja la profundidad en NDC [-1, 1]; [0, 1] sería `_zo`.
                assert!(
                    (-1.0..=1.0).contains(&p.z),
                    "esquina fuera del rango de profundidad: {p:?} (dir {dir:?})"
                );
            }
        }
    }

    #[test]
    fn fit_depth_is_monotonic_along_the_light_direction() {
        // `light_dir` apunta HACIA el sol: el ojo se coloca en center + dir * distancia, así que
        // lo más alto (más cerca del sol) debe proyectarse con menor profundidad.
        let fit = fit_cascade(&corners_around(Vec3::ZERO, 6.0), Vec3::Y, 2048, Vec3::Z)
            .expect("ajuste válido");
        let high = project(&fit, Vec3::new(0.0, 5.0, 0.0));
        let low = project(&fit, Vec3::new(0.0, -5.0, 0.0));
        assert!(high.z < low.z, "orden invertido: high {high:?} low {low:?}");
    }

    #[test]
    fn fit_extent_is_quantized_and_deterministic() {
        // Dos cajas casi idénticas deben dar el MISMO extent: si variara de forma continua el
        // tamaño de texel cambiaría cada frame y la sombra herviría.
        let light = Vec3::new(0.3, -1.0, -0.2);
        let a = fit_cascade(&corners_around(Vec3::ZERO, 5.0), light, 1024, Vec3::Z)
            .expect("ajuste válido");
        let b = fit_cascade(&corners_around(Vec3::ZERO, 5.0001), light, 1024, Vec3::Z)
            .expect("ajuste válido");
        let c = fit_cascade(&corners_around(Vec3::ZERO, 5.0), light, 1024, Vec3::Z)
            .expect("ajuste válido");
        assert_eq!(a.extent, b.extent, "el extent no está cuantizado");
        assert_eq!(a, c, "el ajuste no es determinista");
        let steps = a.extent / EXTENT_QUANTUM;
        assert!(
            (steps - steps.round()).abs() < 1e-3,
            "extent {} no es múltiplo de 0.5",
            a.extent
        );
        assert!(a.extent >= 5.0);
    }

    #[test]
    fn fit_rejects_degenerate_light_basis_and_empty_input() {
        assert!(fit_cascade(&corners_around(Vec3::ZERO, 1.0), Vec3::ZERO, 1024, Vec3::Z).is_none());
        // up paralelo a la luz no define base de cámara.
        assert!(fit_cascade(&corners_around(Vec3::ZERO, 1.0), Vec3::Y, 1024, Vec3::Y).is_none());
        assert!(fit_cascade(&[], Vec3::Y, 1024, Vec3::Z).is_none());
        assert!(fit_cascade(&[Vec3::new(f32::NAN, 0.0, 0.0); 8], Vec3::Y, 1024, Vec3::Z).is_none());
        let tiny = fit_cascade(&corners_around(Vec3::ZERO, 0.001), Vec3::Y, 1024, Vec3::Z)
            .expect("ajuste válido");
        assert!(tiny.extent > 0.0 && tiny.extent.is_finite());
    }

    #[test]
    fn fit_uses_the_passed_up_without_a_zenith_flip() {
        let corners = corners_around(Vec3::ZERO, 4.0);
        let a = fit_cascade(&corners, Vec3::Y, 1024, Vec3::Z).expect("ajuste válido");
        let b = fit_cascade(&corners, Vec3::Y, 1024, Vec3::X).expect("ajuste válido");
        assert_ne!(
            a.view_proj, b.view_proj,
            "el up recibido debe cambiar la base"
        );

        // Barrido que cruza el antiguo umbral |dir.y| > 0.95: con up FIJO la base no vuelca.
        let mut previous: Option<f32> = None;
        for i in 0..40 {
            let dy = 0.9 + (i as f32 / 39.0) * 0.099;
            let dir = Vec3::new(-(1.0 - dy * dy).sqrt(), dy, 0.0);
            let fit = fit_cascade(&corners, dir, 1024, Vec3::Z).expect("ajuste válido");
            let x = project(&fit, Vec3::new(1.0, 0.0, 0.0)).x;
            if let Some(prev) = previous {
                assert!(
                    (x - prev).abs() < 0.5,
                    "la base giró de golpe en dir.y={dy}: {prev} → {x}"
                );
            }
            previous = Some(x);
        }
    }

    #[test]
    fn select_cascade_picks_the_first_covering_split() {
        let splits = split_distances(0.1, 40.0, 2);
        assert_eq!(select_cascade(0.0, &splits, 2), Some(0));
        assert_eq!(select_cascade(splits[0], &splits, 2), Some(0));
        assert_eq!(select_cascade(splits[0] + 0.01, &splits, 2), Some(1));
        assert_eq!(select_cascade(splits[1], &splits, 2), Some(1));
        assert_eq!(select_cascade(splits[1] + 0.01, &splits, 2), None);
        assert_eq!(select_cascade(f32::MAX, &splits, 2), None);
    }

    #[test]
    fn select_cascade_rejects_invalid_counts_and_non_finite_distance() {
        let splits = split_distances(0.1, 40.0, 4);
        assert_eq!(select_cascade(1.0, &splits, 0), None);
        assert_eq!(select_cascade(1.0, &splits, MAX_SHADOW_CASCADES + 1), None);
        assert_eq!(select_cascade(f32::NAN, &splits, 4), None);
        assert_eq!(select_cascade(-1.0, &splits, 4), Some(0));
    }

    #[test]
    fn cascade_set_covers_the_reach_and_keeps_every_slot_finite() {
        let camera = editor_camera(24.0);
        let set = cascade_set(
            &camera,
            4,
            2048,
            Vec3::new(0.4, 1.0, 0.25).normalize(),
            Vec3::Z,
        );
        assert_eq!(set.count, 4);
        assert!(set.splits[0] < set.splits[1] && set.splits[1] < set.splits[2]);
        assert_eq!(set.splits[3], 24.0);
        for index in 1..4 {
            assert!(
                set.extents[index] >= set.extents[index - 1],
                "la cascada {index} encuadra menos mundo que la anterior: {:?}",
                set.extents
            );
        }
        for matrix in &set.matrices {
            for column in matrix {
                for value in column {
                    assert!(value.is_finite(), "matriz de cascada con NaN/inf");
                }
            }
        }
        // Con 3 cascadas queda una ranura libre a f32::MAX.
        let three = cascade_set(&camera, 3, 2048, Vec3::Y, Vec3::Z);
        assert_eq!(three.splits[2], 24.0);
        assert_eq!(three.splits[3], f32::MAX);
    }
}
