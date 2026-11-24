//! Tests adversariales del orquestador para el reintento de CSM.
//!
//! No son tests de los agentes: intentan demostrar que el código generado está mal.
//! Los constratos se comprueban leyendo el código fuente (GLSL y Rust) como texto,
//! porque el bug original del CSM era precisamente semántico y compilaba sin error.

use std::path::{Path, PathBuf};

fn read(rel: &str) -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = root.join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("no se pudo leer {}: {err}", path.display()))
}

/// Comparación tolerante a saltos de línea, espacios y comentarios.
fn flat(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if in_line_comment {
            if c == '\n' {
                in_line_comment = false;
                out.push(' ');
            }
            i += 1;
            continue;
        }
        if in_block_comment {
            if c == '*' && next == Some('/') {
                in_block_comment = false;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if c == '/' && next == Some('/') {
            in_line_comment = true;
            i += 2;
            continue;
        }
        if c == '/' && next == Some('*') {
            in_block_comment = true;
            i += 2;
            continue;
        }
        if c.is_whitespace() {
            if !out.ends_with(' ') {
                out.push(' ');
            }
        } else {
            out.push(c.to_ascii_lowercase());
        }
        i += 1;
    }
    out
}

fn require(path: &str) -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let full = root.join(path);
    assert!(full.exists(), "falta el archivo {}", full.display());
    flat(&std::fs::read_to_string(&full).unwrap())
}

const SHADERS: [&str; 6] = [
    "shaders/raster/shadow.vert",
    "shaders/raster/shadow_skinned.vert",
    "shaders/raster/forward.frag",
    "shaders/raster/forward.vert",
    "shaders/raster/skinned.vert",
    "shaders/raster/depth_only.frag",
];

#[test]
fn forward_frag_samples_array_shadow_with_layer_in_z_and_depth_in_w() {
    let f = require("shaders/raster/forward.frag");
    assert!(
        f.contains("sampler2darrayshadow shadow_map"),
        "binding 1 debe ser sampler2DArrayShadow; el mapa único ya no sirve"
    );
    assert!(
        f.contains("texturelod(shadow_map"),
        "debe usarse textureLod (control flow no uniforme, commit 98c416c)"
    );
    assert!(
        f.contains("float(cascade), depth_ref"),
        "el vec4 debe traer LA CAPA en .z y Dsub en .w: vec4(uv+off, float(cascade), depth_ref)"
    );
    assert!(
        !f.contains("depth_ref, float(cascade)"),
        "REGRESIÓN del bug raíz: depth_ref y cascade están intercambiados en el vec4"
    );
    assert!(
        !f.contains("texture(shadow_map"),
        "texture() sin LOD fue retirado en 98c416c"
    );
}

#[test]
fn forward_frag_keeps_texel_centered_pcf_and_bias() {
    let f = require("shaders/raster/forward.frag");
    assert!(
        f.contains("shadow_bias"),
        "el bias restaurado debe conservarse"
    );
    assert!(
        f.contains("jitter.w"),
        "el bias escalado por texel debe conservarse"
    );
    assert!(
        f.contains("cascade_extent["),
        "el radio PCF debe derivar del extent de la cascada"
    );
    assert!(
        !f.contains("texel * radius"),
        "los taps deben caer en centros de texel (commit 4d85de2), no a radius*texel"
    );
}

#[test]
fn shadow_vert_takes_its_matrix_from_push_constants() {
    let v = require("shaders/raster/shadow.vert");
    assert!(
        v.contains("layout(push_constant) uniform cascadepush"),
        "falta el bloque CascadePush"
    );
    assert!(
        v.contains("mat4 view_proj"),
        "el push debe traer la matriz en 0..64"
    );
    assert!(
        v.contains("vec4 joint_base"),
        "el push debe traer el joint base en 64..80"
    );
    assert!(
        v.contains("pc.view_proj"),
        "shadow.vert debe proyectar con la matriz del push"
    );
    assert!(
        !v.contains("u.light_view_proj[0]"),
        "REGRESIÓN: volvió el parcheo del UBO en lugar de push constants"
    );
}

#[test]
fn all_scene_blocks_mirror_the_appended_cascade_fields_in_order() {
    let fields = [
        "mat4 cascade_view_proj[4];",
        "vec4 cascade_splits;",
        "vec4 cascade_extent;",
        "vec4 cascade_params;",
    ];
    // Solo `forward.frag` muestrea sombras: es el único `Scene` que declara los
    // campos de cascada. `aura_color` va ANTES de ellos para que los offsets
    // 448/704/720/736 coincidan con el `SceneUniforms` de Rust (aura_color 432).
    let forward = require("shaders/raster/forward.frag");
    let mut last = 0usize;
    for field in fields {
        let at = forward.find(field).unwrap_or_else(|| {
            panic!("forward.frag: falta `{field}` en el bloque Scene (espejo Rust↔GLSL)")
        });
        assert!(
            at > last,
            "forward.frag: `{field}` fuera de orden en el bloque Scene"
        );
        last = at + field.len();
    }
    let aura = forward
        .find("vec4 aura_color")
        .unwrap_or_else(|| panic!("forward.frag: falta aura_color antes de los campos de cascada"));
    assert!(
        aura < forward.find(fields[0]).unwrap(),
        "forward.frag: aura_color debe declararse antes de cascade_view_proj (432 vs 448)"
    );
    for shader in SHADERS {
        if shader == "shaders/raster/forward.frag" {
            continue;
        }
        let s = require(shader);
        for field in fields {
            assert!(
                !s.contains(field),
                "{shader}: no debe declarar `{field}`; sin aura_color el UBO quedaría desplazado 16 B"
            );
        }
    }
}

#[test]
fn skinned_forward_and_skinned_shadow_use_separate_push_layouts() {
    // `skinned.vert` alimenta los pipelines del G-buffer (push de 8 B). Si declare el bloque
    // de 80 B del pase de sombras, VVL rechaza el pipeline y el skinning forward lee basura.
    let forward = require("shaders/raster/skinned.vert");
    assert!(
        forward.contains("uniform skinnedpush"),
        "el skinned del forward debe conservar su bloque de 8 B"
    );
    assert!(
        !forward.contains("uniform cascadepush"),
        "REGRESIÓN: el skinned del forward declaró el push de 80 B de sombras"
    );

    let shadow = require("shaders/raster/shadow_skinned.vert");
    assert!(
        shadow.contains("uniform cascadepush"),
        "falta el push de sombras del skinned"
    );
    assert!(
        shadow.contains("pc.view_proj"),
        "el skinned de sombras debe proyectar con la cascada"
    );
    assert!(
        shadow.contains("uint(pc.joint_base.x)"),
        "la base de paleta va en 64..80"
    );

    let mod_rs = require("src/engine3d/vulkan/frame/mod.rs");
    let at = mod_rs
        .find("let shadow_skinned = graphics")
        .expect("no se encontró la creación del pipeline shadow_skinned");
    let tail: String = mod_rs[at..].chars().take(400).collect();
    assert!(
        tail.contains("shadow_skinned_vert"),
        "el pipeline shadow_skinned debe usar shadow_skinned.vert, no skinned.vert"
    );
}

#[test]
fn shadow_pass_uses_push_constants_and_never_patches_the_ubo() {
    let s = require("src/engine3d/vulkan/frame/shadow.rs");
    assert!(
        s.contains("cmd_push_constants"),
        "el pase debe empujar la matriz por cascade"
    );
    assert!(
        !s.contains("cmd_update_buffer"),
        "REGRESIÓN: el pase volvió a parchear el UBO; el contrato exige push constants"
    );
}

#[test]
fn grounded_light_camera_snapping_was_not_broken() {
    let l = require("src/engine3d/config_3d/directional_light.rs");
    assert!(
        l.contains(".round()"),
        "el snapping con round() de ef5a5a5 debe conservarse"
    );
    assert!(
        !l.contains(".floor()"),
        "floor() se retiró en 98c416c y causaba saltos"
    );

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cascade_math = root.join("src/engine3d/config_3d/shadow_cascades.rs");
    if Path::new(&cascade_math).exists() {
        let c = flat(&std::fs::read_to_string(&cascade_math).unwrap());
        assert!(
            !c.contains("up_hint"),
            "el `up_hint` del CSM antiguo hacía volcar la base en el cenit; usar light_up_for_cardinal"
        );
        assert!(
            !c.contains(".floor()"),
            "ningún snapping del CSM debe usar floor()"
        );
    }
}

#[test]
fn docs_no_longer_claim_that_csm_was_withdrawn() {
    let path = "../../../../docs/yaml/rendering/Shadows_3D.yaml";
    let raw = read(path);
    let flat_raw = flat(&raw);
    assert!(
        !flat_raw.contains("status: retirado-y-aplazado"),
        "la documentación debe reflejar que el CSM está implementado"
    );
    assert!(
        flat_raw.contains("csm_attempt"),
        "la bitácora histórica debe conservarse"
    );
}

// ---------------------------------------------------------------------------
// Tests que usan las APIs ya entregadas por los agentes A (matemática) y B (datos).
// ---------------------------------------------------------------------------

use crate::engine3d::config_3d::shadow_cascades::{
    MAX_SHADOW_CASCADES, fit_cascade, select_cascade, split_distances,
};
use crate::engine3d::config_3d::shadow_graphics::ShadowTier;
use crate::engine3d::engine::types::{SCENE_UBO_BYTES, SceneUniforms};
use glam::Vec3;

#[test]
fn scene_uniforms_prefix_offsets_are_untouched_by_csm() {
    use std::mem::{offset_of, size_of};
    assert_eq!(size_of::<SceneUniforms>(), 752);
    assert_eq!(SCENE_UBO_BYTES, 768);
    let expected = [
        (offset_of!(SceneUniforms, view_proj), 0usize),
        (offset_of!(SceneUniforms, view_proj_stable), 64),
        (offset_of!(SceneUniforms, prev_view_proj), 128),
        (offset_of!(SceneUniforms, inv_view_proj), 192),
        (offset_of!(SceneUniforms, cam_pos), 256),
        (offset_of!(SceneUniforms, light_dir), 272),
        (offset_of!(SceneUniforms, light_color), 288),
        (offset_of!(SceneUniforms, light_view_proj), 304),
        (offset_of!(SceneUniforms, light_params), 368),
        (offset_of!(SceneUniforms, jitter), 384),
        (offset_of!(SceneUniforms, depth_plane), 400),
        (offset_of!(SceneUniforms, shadow_bias), 416),
        (offset_of!(SceneUniforms, aura_color), 432),
        (offset_of!(SceneUniforms, cascade_view_proj), 448),
        (offset_of!(SceneUniforms, cascade_splits), 704),
        (offset_of!(SceneUniforms, cascade_extent), 720),
        (offset_of!(SceneUniforms, cascade_params), 736),
    ];
    for (got, want) in expected {
        assert_eq!(got, want, "offset del bloque Scene cambiado");
    }
}

#[test]
fn tier_cascade_table_matches_the_frozen_contract() {
    assert_eq!(MAX_SHADOW_CASCADES, 4);
    let table = [
        (ShadowTier::Off, 0, 1, 1.0),
        (ShadowTier::Low, 2, 1024, 1.0),
        (ShadowTier::Medium, 3, 1024, 1.0),
        (ShadowTier::High, 4, 2048, 1.0),
        (ShadowTier::Ultra, 4, 2048, 1.45),
    ];
    for (tier, count, side, scale) in table {
        assert_eq!(tier.cascade_count(), count, "cascadas de {}", tier.wire());
        assert_eq!(tier.shadow_map_size(), side, "lado de {}", tier.wire());
        assert!((tier.shadow_distance_scale() - scale).abs() < 1e-6);
    }
    // Presupuesto: el array se asigna completo siempre, 4 capas x 2048^2 x 4 B.
    let worst = 4u64 * 2048 * 2048 * 4;
    assert_eq!(worst, 64 * 1024 * 1024);
}

#[test]
fn split_and_selection_agree_with_the_shaders_less_or_equal_boundary() {
    for count in 1..=4 {
        let splits = split_distances(0.1, 80.0, count);
        assert_eq!(splits[count - 1], 80.0);
        for i in 1..count {
            assert!(
                splits[i - 1] < splits[i],
                "cortes no crecientes: {splits:?}"
            );
        }
        for i in 0..count {
            // El shader elige la PRIMERA cascada con view_dist <= split[i].
            assert_eq!(select_cascade(splits[i], &splits, count), Some(i));
            if i + 1 < count {
                let middle = (splits[i] + splits[i + 1]) * 0.5;
                assert_eq!(select_cascade(middle, &splits, count), Some(i + 1));
            }
        }
        assert_eq!(
            select_cascade(splits[count - 1] + 0.01, &splits, count),
            None
        );
    }
}

#[test]
fn cascade_matrices_use_the_vulkan_zero_to_one_depth_range() {
    // El test del agente A comprobaba z en [-1,1], un rango que también contiene [0,1] y por
    // tanto NO fijaba la convención. `orthographic_rh` de glam mapea a [0,1]; si alguien cambiara
    // a `_gl` o a una ortho de OpenGL, el depth_ref del forward dejaría de coincidir.
    let center = Vec3::new(3.0, -2.0, 9.0);
    let half = 4.0;
    let mut corners = [Vec3::ZERO; 8];
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
        corners[index] = center + Vec3::new(sx * half, sy * half, sz * half);
    }
    let fit = fit_cascade(&corners, Vec3::new(-0.4, -1.0, 0.3), 1024, Vec3::Z)
        .expect("el ajuste debe ser válido");
    let matrix = fit.as_mat4();
    for corner in corners {
        let clip = matrix * corner.extend(1.0);
        assert!(clip.w > 0.0, "esquina detrás de la cámara de luz");
        let ndc = clip.truncate() / clip.w;
        assert!(
            ndc.z >= -1e-4 && ndc.z <= 1.0 + 1e-4,
            "z NDC fuera de [0,1]: {}",
            ndc.z
        );
        assert!(ndc.x.abs() <= 1.0 + 1e-3 && ndc.y.abs() <= 1.0 + 1e-3);
    }
}
