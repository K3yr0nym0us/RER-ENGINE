//! Cascades de sombras (CSM): cortes por distancia y ajuste ortográfico de cada cascada.
//!
//! Matemática pura, sin GPU ni `State`, para poder probarla en host. `State` arma la base de
//! cámara en `config_3d/directional_light.rs`, llama a este módulo y vuelca el resultado en
//! `SceneUniforms`, que es lo que lee `shaders/raster/shadow.vert` (escribe) y
//! `shaders/raster/forward.frag` (muestrea).
//!
//! Convención de profundidad: se usa `Mat4::orthographic_rh` de glam, igual que hacía el shadow map
//! único, así que escribir y muestrear pasan por LA MISMA matriz. No se toca ese criterio: si se
//! cambiara aquí, el `depth_ref` del shader dejaría de comparar contra lo almacenado en el mapa.

use glam::{Mat4, Vec3};

/// Bytes de una `mat4` dentro del array `light_view_proj` del UBO (std140: 4 vec4, sin relleno).
pub const MATRIX_BYTES: usize = 64;

/// Capas del array de sombras. Fijas: cambiar de tier sólo reasigna resolución y cuántas capas se
/// dibujan, nunca el número de capas, así los descriptores del `sampler2DArrayShadow` no se tocan.
pub const MAX_SHADOW_CASCADES: usize = 4;

/// Mezcla log/uniforme de los cortes. `0.0` = uniformes (la primera cascada desperdicia resolución
/// cerca de la cámara); `1.0` = logarítmicos (poca densidad en la lejana y saltos bruscos de
/// texel). El punto intermedio es el habitual en motores en tiempo real.
pub const SPLIT_LAMBDA: f32 = 0.55;

/// El extent de cada cascada se redondea a múltiplos de este valor (unidades de mundo). Sin esto,
/// el tamaño de texel cambiaría cada frame con la cámara y la sombra herviría aunque el centro esté
/// alineado a la rejilla.
const EXTENT_QUANTUM: f32 = 0.5;

/// Margen de la proyección, en múltiplos del extent de la caja, por delante y por detrás del slice.
/// Sin margen, un caster más alto que el slice (un poste, la copa de un árbol) quedaría fuera de la
/// caja de profundidad y no proyectaría sombra.
const Z_MARGIN_IN_EXTENTS: f32 = 1.5;

/// Proyección ortográfica de una cascada ya ajustada a sus esquinas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CascadeFit {
    /// Vista × proyección de la luz, en la convención de glam (igual que el mapa único).
    pub view_proj: Mat4,
    /// Mitad del lado de la caja ortográfica, en unidades de mundo. Sirve para escalar el radio
    /// PCF: una cascada lejana cubre más mundo por texel y necesita taps más separados.
    pub extent: f32,
}

/// Borde lejano de cada cascada, medido desde el ojo de la cámara.
///
/// Devuelve siempre `MAX_SHADOW_CASCADES` entradas: las casillas de cascadas inexistentes valen
/// `f32::MAX`, así el shader puede buscar la primera cascada cuyo borde supera la profundidad sin
/// ramificar por tier y nunca se queda fuera del rango.
///
/// Creciente por construction: ambos términos (uniforme y logarítmico) crecen con la fracción, y la
/// mezcla de dos crecientes es creciente.
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
    // La última cascada debe terminar exactamente en el borde de sombra: el clamp de arriba ya lo
    // fuerza (fraction = 1), pero se fija explícito para que el shader no descarte el último píxel
    // por precisión de coma flotante.
    out[count - 1] = shadow_far;
    out
}

/// Esquinas de la loncha de frustum `[r0, r1]` en espacio mundo.
///
/// `right`/`up` son los ejes de pantalla ya normalizados y `tan_half_fov_y` es la tangente de la
/// mitad del FOV vertical, de modo que la altura a distancia `r` sea `r * tan_half_fov_y`.
#[must_use]
pub fn slice_corners(
    eye: Vec3,
    forward: Vec3,
    right: Vec3,
    up: Vec3,
    tan_half_fov_y: f32,
    aspect: f32,
    r0: f32,
    r1: f32,
) -> [Vec3; 8] {
    let mut corners = [Vec3::ZERO; 8];
    for (slot, radius) in [r0, r1].iter().enumerate() {
        let half_height = radius.max(0.0) * tan_half_fov_y;
        let half_width = half_height * aspect.max(0.01);
        let center = eye + forward * radius;
        for (index, (sx, sy)) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)]
            .iter()
            .enumerate()
        {
            corners[slot * 4 + index] =
                center + right * (half_width * sx) + up * (half_height * sy);
        }
    }
    corners
}

/// Ajusta la proyección ortográfica de la luz que encuadra `corners`.
///
/// Alinea el centro de la caja a la rejilla de texeles y cuantiza su extent: así la matriz sólo
/// cambia a saltos, no de forma continua, y la sombra deja de hervir cuando la cámara se desliza.
/// Devuelve `None` si la dirección de luz es nula o el resultado no es finito (escena degenerada),
/// en cuyo caso el llamador debe mantener la cascada anterior en vez de escribir basura.
#[must_use]
pub fn fit_cascade(corners: &[Vec3; 8], light_dir: Vec3, map_size: u32) -> Option<CascadeFit> {
    let dir = light_dir.normalize_or_zero();
    if dir == Vec3::ZERO {
        return None;
    }
    let map_size = map_size.max(1);
    let center = corners.iter().fold(Vec3::ZERO, |sum, c| sum + *c) / corners.len() as f32;
    // Radio de la nube de esquinas: coloca el ojo de la luz fuera de ella para que ninguna
    // esquina quede por detrás del plano near de la proyección.
    let radius = corners
        .iter()
        .map(|c| c.distance(center))
        .fold(0.0f32, f32::max)
        .max(0.5);
    let up_hint = if dir.y.abs() > 0.95 { Vec3::X } else { Vec3::Y };
    let view = Mat4::look_at_rh(center + dir * (radius * 3.0 + 1.0), center, up_hint);

    let local: Vec<Vec3> = corners.iter().map(|c| view.transform_point3(*c)).collect();
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    for point in &local {
        min = min.min(*point);
        max = max.max(*point);
    }

    let raw_extent = ((max.x - min.x).max(max.y - min.y) * 0.5).max(0.5);
    let extent = (raw_extent / EXTENT_QUANTUM).ceil() * EXTENT_QUANTUM;
    let texel = 2.0 * extent / map_size as f32;

    // Snapping en espacio de luz. `look_at_rh` mira hacia -Z, así que la profundidad de un punto es
    // -z; los márgenes se calculan con esa convención.
    let box_center = Vec3::new((min.x + max.x) * 0.5, (min.y + max.y) * 0.5, 0.0);
    let cx = (box_center.x / texel).floor() * texel;
    let cy = (box_center.y / texel).floor() * texel;
    let margin = (extent * Z_MARGIN_IN_EXTENTS).max(texel * 4.0);
    let z_near = (-max.z - margin).max(0.05);
    let z_far = (-min.z + margin).max(z_near + 0.1);

    let proj = Mat4::orthographic_rh(
        cx - extent,
        cx + extent,
        cy - extent,
        cy + extent,
        z_near,
        z_far,
    );
    let view_proj = proj * view;
    if !view_proj.is_finite() {
        return None;
    }
    Some(CascadeFit { view_proj, extent })
}

/// Ajuste completo de las cascadas de un frame, en forma de datos puros (sin `State` ni GPU).
#[derive(Clone, Copy, Debug)]
pub struct CascadeCamera {
    /// Posición del ojo.
    pub eye: Vec3,
    /// Ejes de vista ya normalizados y ortonormales.
    pub forward: Vec3,
    pub right: Vec3,
    pub up: Vec3,
    /// Tangente de la mitad del FOV vertical.
    pub tan_half_fov_y: f32,
    pub aspect: f32,
    /// Plano cercano de la cámara (inicio de la primera cascada).
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
    /// Cuántas cascadas se dibujarán y muestrearán.
    pub count: usize,
}

/// Calcula cortes, cajas y matrices de todas las cascadas del frame.
///
/// Está separado de `State` para poder simular en host el ciclo completo (ajuste → bytes del UBO →
/// matriz que lee el shader): los bugs de sombras por cascada son casi siempre de coherencia entre
/// la matriz que ESCRIBE una capa y la que lee el forward, y eso solo se prueba sin GPU.
#[must_use]
pub fn cascade_set(
    camera: &CascadeCamera,
    count: usize,
    map_size: u32,
    light_dir: Vec3,
) -> CascadeSet {
    let count = count.clamp(1, MAX_SHADOW_CASCADES);
    let identity = Mat4::IDENTITY.to_cols_array_2d();
    let mut set = CascadeSet {
        matrices: [identity; MAX_SHADOW_CASCADES],
        splits: [f32::MAX; MAX_SHADOW_CASCADES],
        extents: [0.0; MAX_SHADOW_CASCADES],
        count,
    };
    let near = camera.near.max(0.05);
    let shadow_far = camera.shadow_far.max(near * 1.05);
    let splits = split_distances(near, shadow_far, count);
    let mut previous = near;
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
        let Some(fit) = fit_cascade(&corners, light_dir, map_size) else {
            continue;
        };
        let matrix = fit.view_proj.to_cols_array_2d();
        set.matrices[index] = matrix;
        set.extents[index] = fit.extent;
        // El corte se guarda SIEMPRE que la cascada exista: si se omitiera al fallar un ajuste
        // vecino, el shader elegiría una cascada cuya matriz nunca se escribió.
        set.splits[index] = *edge;
        last_valid = Some((matrix, fit.extent));
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

/// Lleva la matriz de la cascada `index` a la ranura 0 del array `light_view_proj` dentro de los
/// bytes del UBO, que es lo que lee `shadow.vert`.
///
/// `array_offset` es `offset_of!(SceneUniforms, light_view_proj)`. Devuelve `false` si el índice o
/// el búfer no caben: el llamador debe dejar el UBO como estaba antes que escribir fuera de rango.
pub fn patch_matrix_into_first_slot(scene: &mut [u8], array_offset: usize, index: usize) -> bool {
    // Sin este tope, un índice fuera de la array leería los campos siguientes del UBO y los pintaría
    // como matriz de luz: números basura proyectando sombras.
    if index >= MAX_SHADOW_CASCADES {
        return false;
    }
    let Some(end) = (array_offset + index * MATRIX_BYTES).checked_add(MATRIX_BYTES) else {
        return false;
    };
    if end > scene.len() || array_offset + MATRIX_BYTES > scene.len() {
        return false;
    }
    // `copy_within` resuelve solapamientos (es memmove), y con index = 0 es un no-op.
    scene.copy_within(array_offset + index * MATRIX_BYTES..end, array_offset);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// Proyecta un punto con la matriz de cascada (división entre `w` incluida) para comprobar bordes.
    fn project(view_proj: Mat4, point: Vec3) -> Vec3 {
        let clip = view_proj * point.extend(1.0);
        clip.truncate() / clip.w
    }

    #[test]
    fn split_distances_are_ordered_and_unused_slots_are_open() {
        let splits = split_distances(0.1, 80.0, 4);
        for window in splits.windows(2) {
            assert!(window[0] <= window[1], "cortes no crecientes: {splits:?}");
        }
        assert!(splits[0] > 0.1 && splits[0] < 80.0);
        assert_eq!(
            splits[3], 80.0,
            "la última cascada debe llegar al borde de sombra"
        );
    }

    #[test]
    fn split_distances_only_fill_the_cascade_count() {
        let splits = split_distances(0.1, 40.0, 2);
        assert!(splits[0] < splits[1]);
        assert_eq!(splits[1], 40.0);
        assert_eq!(splits[2], f32::MAX);
        assert_eq!(splits[3], f32::MAX);
        // Una cascada única degenera en el shadow map clásico: cubre todo el rango.
        let single = split_distances(0.1, 40.0, 1);
        assert_eq!(single[0], 40.0);
    }

    #[test]
    fn first_cascade_is_denser_than_the_uniform_split() {
        // Con lambda > 0 el primer corte se acerca a la cámara: ahí es donde se nota el CSM.
        let splits = split_distances(0.1, 100.0, 4);
        let uniform = 0.1 + (100.0 - 0.1) * 0.25;
        assert!(
            splits[0] < uniform,
            "esperado < {uniform}, got {}",
            splits[0]
        );
    }

    #[test]
    fn fit_contains_every_corner_inside_the_ortho_volume() {
        let fit = fit_cascade(
            &corners_around(Vec3::new(3.0, -2.0, 9.0), 4.0),
            Vec3::new(-0.4, -1.0, 0.3),
            1024,
        )
        .expect("ajuste válido");
        for corner in corners_around(Vec3::new(3.0, -2.0, 9.0), 4.0) {
            let p = project(fit.view_proj, corner);
            assert!(
                p.x >= -1.001 && p.x <= 1.001 && p.y >= -1.001 && p.y <= 1.001,
                "esquina fuera del rectángulo ortogonal: {p:?}"
            );
            assert!(
                p.z >= -1.001 && p.z <= 1.001,
                "esquina fuera del rango Z: {p:?}"
            );
        }
    }

    #[test]
    fn fit_is_monotonic_in_depth_along_the_light_direction() {
        // `light_dir` del motor apunta HACIA el sol (ver `SceneUniforms.light_dir`), así que el ojo de
        // la luz se coloca en center + dir * distancia: lo más cerca del sol debe quedar con z menor.
        let sun_up = Vec3::new(0.0, 1.0, 0.3);
        let fit =
            fit_cascade(&corners_around(Vec3::ZERO, 6.0), sun_up, 2048).expect("ajuste válido");
        let high = project(fit.view_proj, Vec3::new(0.0, 5.0, 0.0));
        let low = project(fit.view_proj, Vec3::new(0.0, -5.0, 0.0));
        assert!(high.z < low.z, "orden invertido: high {high:?} low {low:?}");
    }

    #[test]
    fn fit_quantizes_extent_so_texel_size_stops_jittering() {
        // Dos cajas casi idénticas (diferencia de micras) deben dar el MISMO extent. Si el extent
        // variara de forma continua, el tamaño de texel cambiaría cada frame con la cámara y la
        // sombra herviría aunque el centro esté alineado a la rejilla.
        let light = Vec3::new(0.3, -1.0, -0.2);
        let a = fit_cascade(&corners_around(Vec3::ZERO, 5.0), light, 1024).expect("ajuste válido");
        let b =
            fit_cascade(&corners_around(Vec3::ZERO, 5.0001), light, 1024).expect("ajuste válido");
        let c = fit_cascade(&corners_around(Vec3::ZERO, 5.0), light, 1024).expect("ajuste válido");
        assert_eq!(a.extent, b.extent, "el extent no está cuantizado");
        assert_eq!(a, c, "el ajuste no es determinista");
        let steps = a.extent / EXTENT_QUANTUM;
        assert!(
            (steps - steps.round()).abs() < 1e-3,
            "extent {} no es múltiplo de {EXTENT_QUANTUM}",
            a.extent
        );
    }

    #[test]
    fn fit_rejects_degenerate_light_and_keeps_positive_extent() {
        assert!(fit_cascade(&corners_around(Vec3::ZERO, 1.0), Vec3::ZERO, 1024).is_none());
        let fit =
            fit_cascade(&corners_around(Vec3::ZERO, 0.001), Vec3::Y, 1024).expect("ajuste válido");
        assert!(fit.extent > 0.0 && fit.extent.is_finite());
    }

    /// Cámara de editor plausible (órbita mirando al origen) con sus ejes ortonormales reales.
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

    fn matrix_at(bytes: &[u8], offset: usize) -> [[f32; 4]; 4] {
        let mut out = [[0.0f32; 4]; 4];
        for (column, slot) in out.iter_mut().enumerate() {
            for (row, cell) in slot.iter_mut().enumerate() {
                let base = offset + column * 16 + row * 4;
                *cell = f32::from_le_bytes(bytes[base..base + 4].try_into().expect("4 bytes"));
            }
        }
        out
    }

    /// Simula el ciclo completo del pase de sombras SIN GPU: el UBO se llena con `SceneUniforms`,
    /// por cascada se parchea la ranura 0 (lo que lee `shadow.vert`) y se comprueba que la matriz
    /// con la que se ESCRIBE la capa N es idéntica a la que el forward usa para LEER esa capa.
    ///
    /// Si esto falla, el síntoma en pantalla es justo el peor: la cascada muestreada tendría
    /// profundidad proyectada con otra matriz, todo compararía como "hay geometría delante" y la
    /// escena saldría en sombra salvo el radio de la primera cascada.
    #[test]
    fn ubo_patch_round_trip_writes_and_reads_the_same_matrix() {
        use crate::engine::SceneUniforms;
        const ARRAY: usize = std::mem::offset_of!(SceneUniforms, light_view_proj);
        let camera = editor_camera(24.0);
        let set = cascade_set(&camera, 4, 2048, Vec3::new(0.4, 1.0, 0.25).normalize());

        let mut scene = vec![0u8; std::mem::size_of::<SceneUniforms>()];
        for (index, matrix) in set.matrices.iter().enumerate() {
            for (column, col) in matrix.iter().enumerate() {
                for (row, value) in col.iter().enumerate() {
                    let base = ARRAY + index * MATRIX_BYTES + column * 16 + row * 4;
                    scene[base..base + 4].copy_from_slice(&value.to_le_bytes());
                }
            }
        }

        for index in 0..MAX_SHADOW_CASCADES {
            let mut patched = scene.clone();
            assert!(
                patch_matrix_into_first_slot(&mut patched, ARRAY, index),
                "el parche de la cascada {index} no cabió en el UBO"
            );
            assert_eq!(
                matrix_at(&patched, ARRAY),
                matrix_at(&scene, ARRAY + index * MATRIX_BYTES),
                "la capa {index} se escribiría con otra matriz que la que lee el forward"
            );
        }

        // Fuera de rango: se niega, no escribe basura.
        assert!(!patch_matrix_into_first_slot(
            &mut scene,
            ARRAY,
            MAX_SHADOW_CASCADES
        ));
        assert!(!patch_matrix_into_first_slot(&mut [], ARRAY, 0));
    }

    #[test]
    fn cascade_set_is_ordered_and_covers_the_reach() {
        let count = 4;
        let set = cascade_set(
            &editor_camera(24.0),
            count,
            2048,
            Vec3::new(0.4, 1.0, 0.25).normalize(),
        );
        assert_eq!(set.count, count);
        assert!(set.splits[0] < set.splits[1] && set.splits[1] < set.splits[2]);
        assert_eq!(set.splits[count - 1], 24.0);
        // Con las 4 cascadas activas no queda ninguna casilla inactiva que valga FLT_MAX: se mide
        // con un tier de 3, que es el caso donde sí quedan huecos (Medium).
        let three = cascade_set(
            &editor_camera(24.0),
            3,
            2048,
            Vec3::new(0.4, 1.0, 0.25).normalize(),
        );
        assert_eq!(three.splits[3], f32::MAX);
        assert_eq!(three.splits[2], 24.0);
        for index in 1..count {
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
    }

    #[test]
    fn cascade_set_follows_the_tier_reach() {
        let near_reach = cascade_set(&editor_camera(8.0), 2, 1024, Vec3::Y);
        let far_reach = cascade_set(&editor_camera(80.0), 2, 1024, Vec3::Y);
        assert!(near_reach.splits[0] < far_reach.splits[0]);
        assert_eq!(near_reach.splits[1], 8.0);
        assert_eq!(far_reach.splits[1], 80.0);
    }
}
