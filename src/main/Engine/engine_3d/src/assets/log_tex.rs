//! Logs PBR — foco en materiales del mesh skinned (personaje).
//!
//! Mats solo editor/play (ej. 8–12) no afectan al personaje → debug, sin WARN.

use std::collections::HashSet;
use std::sync::Once;

static PBR_NOTA: Once = Once::new();

/// Factores PBR de un material glTF o implícitos al cargar `.rerasset`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialPbrLog {
    pub base:         [f32; 4],
    pub metallic:     f32,
    pub roughness:    f32,
    pub emissive:     [f32; 3],
    pub alpha:        &'static str,
    pub double_sided: bool,
}

impl MaterialPbrLog {
    pub fn from_gltf(mat: &gltf::Material) -> Self {
        let pbr = mat.pbr_metallic_roughness();
        Self {
            base: pbr.base_color_factor(),
            metallic: pbr.metallic_factor(),
            roughness: pbr.roughness_factor(),
            emissive: mat.emissive_factor(),
            alpha: alpha_mode_label(mat.alpha_mode()),
            double_sided: mat.double_sided(),
        }
    }

    pub fn rerasset_load_implicit() -> Self {
        Self {
            base: [1.0, 1.0, 1.0, 1.0],
            metallic: 0.0,
            roughness: 1.0,
            emissive: [0.0, 0.0, 0.0],
            alpha: "Opaque",
            double_sided: false,
        }
    }

    pub fn is_default_implicit(&self) -> bool {
        self == &Self::rerasset_load_implicit()
    }

    pub fn pbr_differs_from_load_implicit(&self) -> bool {
        !self.is_default_implicit()
    }

    pub fn base_color_non_white(&self) -> bool {
        self.base != [1.0, 1.0, 1.0, 1.0]
    }
}

fn alpha_mode_label(mode: gltf::material::AlphaMode) -> &'static str {
    use gltf::material::AlphaMode;
    match mode {
        AlphaMode::Opaque => "Opaque",
        AlphaMode::Mask => "Mask",
        AlphaMode::Blend => "Blend",
    }
}

pub fn format_vec4(c: [f32; 4]) -> String {
    format!("[{:.3},{:.3},{:.3},{:.3}]", c[0], c[1], c[2], c[3])
}

pub fn format_vec3(c: [f32; 3]) -> String {
    format!("[{:.3},{:.3},{:.3}]", c[0], c[1], c[2])
}

pub fn format_tex_px(mip0: &[u8]) -> String {
    if mip0.len() >= 4 {
        format!("[{},{},{},{}]", mip0[0], mip0[1], mip0[2], mip0[3])
    } else if mip0.is_empty() {
        "none".to_string()
    } else {
        format!("short:{}", mip0.len())
    }
}

/// Factor con R dominante y G/B bajos → tinte rojo uniforme si el shader aplicara texture×factor.
pub fn red_tint_diagnosis(base: [f32; 4]) -> Option<&'static str> {
    if base[0] > 0.85 && base[1] < 0.35 && base[2] < 0.35 {
        Some("factor rojo dominante — explicaría texture×factor teñido de rojo")
    } else if base[0] > 0.7 && base[1] < 0.5 && base[2] < 0.5 && base[0] > base[1] + 0.2 && base[0] > base[2] + 0.2 {
        Some("factor rojizo — posible tinte al multiplicar con albedo")
    } else {
        None
    }
}

fn log_pbr_nota_once() {
    PBR_NOTA.call_once(|| {
        log::debug!(
            "[MAT_PBR] pbr_src=not_in_rerasset — solo albedo en .rerasset; shader sin texture×base_color_factor"
        );
    });
}

fn skinned_mat_tex_compact(pairs: &[(u32, u32)], skinned: &HashSet<u32>) -> String {
    let mut filtered: Vec<(u32, u32)> = pairs
        .iter()
        .copied()
        .filter(|(m, _)| skinned.contains(m))
        .collect();
    filtered.sort_by_key(|(m, _)| *m);
    filtered
        .iter()
        .map(|(m, t)| format!("mat{m}→tex{t}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn albedo_image_index(mat: &gltf::Material) -> String {
    mat.pbr_metallic_roughness()
        .base_color_texture()
        .map(|t| t.texture().source().index().to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn format_skinned_mat_px_entry(mat_index: u32, tex_idx: u32, mip0: &[u8]) -> String {
    format!("mat{mat_index}→tex{tex_idx}{}", format_tex_px(mip0))
}

fn log_skinned_mats_compact(
    tag: &str,
    skinned: &HashSet<u32>,
    mat_tex_pairs: &[(u32, u32)],
    read_tex: impl Fn(u32) -> Option<Vec<u8>>,
) {
    let mut skinned_sorted: Vec<u32> = skinned.iter().copied().collect();
    skinned_sorted.sort_unstable();
    if skinned_sorted.is_empty() {
        return;
    }
    let map = skinned_mat_tex_compact(mat_tex_pairs, skinned);
    let mut px_items: Vec<String> = Vec::new();
    for &(mat_idx, tex_idx) in mat_tex_pairs {
        if !skinned.contains(&mat_idx) {
            continue;
        }
        let Some(mip0) = read_tex(tex_idx) else {
            px_items.push(format!("mat{mat_idx}→tex{tex_idx}?"));
            continue;
        };
        px_items.push(format_skinned_mat_px_entry(mat_idx, tex_idx, &mip0));
    }
    log::info!(
        "[MAT_PBR_SKINNED] {tag} skinned={skinned_sorted:?} {map} | {}",
        px_items.join(" "),
    );
}

fn format_pbr_delta(gltf: &MaterialPbrLog) -> String {
    let imp = MaterialPbrLog::rerasset_load_implicit();
    let mut parts = Vec::new();
    if gltf.base != imp.base {
        parts.push(format!("base={}", format_vec4(gltf.base)));
    }
    if (gltf.metallic - imp.metallic).abs() > f32::EPSILON {
        parts.push(format!("metal={:.3}", gltf.metallic));
    }
    if (gltf.roughness - imp.roughness).abs() > f32::EPSILON {
        parts.push(format!("rough={:.3}", gltf.roughness));
    }
    if gltf.emissive != imp.emissive {
        parts.push(format!("emissive={}", format_vec3(gltf.emissive)));
    }
    if gltf.alpha != imp.alpha {
        parts.push(format!("alpha={}", gltf.alpha));
    }
    if gltf.double_sided != imp.double_sided {
        parts.push(format!("doubleSided={}", gltf.double_sided));
    }
    parts.join(" ")
}

fn log_skinned_pbr_loss_summary(tag: &str, skinned: &HashSet<u32>, pbr_map: &std::collections::HashMap<u32, MaterialPbrLog>) {
    let mut pbr_not_stored: Vec<String> = Vec::new();
    let mut color_suspects: Vec<String> = Vec::new();
    let mut skinned_sorted: Vec<u32> = skinned.iter().copied().collect();
    skinned_sorted.sort_unstable();

    for &mat in &skinned_sorted {
        let Some(gltf) = pbr_map.get(&mat) else {
            continue;
        };
        if !gltf.pbr_differs_from_load_implicit() {
            continue;
        }
        let delta = format_pbr_delta(gltf);
        pbr_not_stored.push(format!("mat{mat}:{delta}"));
        if gltf.base_color_non_white() {
            color_suspects.push(format!("mat{mat}:base={}", format_vec4(gltf.base)));
        } else if let Some(dx) = red_tint_diagnosis(gltf.base) {
            color_suspects.push(format!("mat{mat}:{dx}"));
        }
    }

    if !color_suspects.is_empty() {
        log::warn!(
            "[{tag}] skinned base_color_factor ≠ blanco: {}",
            color_suspects.join(" | "),
        );
    } else if !pbr_not_stored.is_empty() {
        log::debug!(
            "[{tag}] PBR no persistido (no explica tinte rojo si base=[1,1,1,1]): {}",
            pbr_not_stored.join(" | "),
        );
    }
}

/// Import GLB — solo conteo; detalle skinned en bake.
pub fn log_gltf_materials_from_doc(doc: &gltf::Document) {
    log_pbr_nota_once();
    let count = doc.materials().count();
    log::debug!("[GLTF_MAT] {count} material/es en GLB");
    for (mi, mat) in doc.materials().enumerate() {
        let pbr = MaterialPbrLog::from_gltf(&mat);
        log::trace!(
            "[GLTF_MAT] mat{mi} base={} metal={:.3} rough={:.3} tex={}",
            format_vec4(pbr.base),
            pbr.metallic,
            pbr.roughness,
            albedo_image_index(&mat),
        );
    }
}

pub fn log_bake_materials_with_gltf(
    gltf_path: &std::path::Path,
    materials: &[rer_engine_shared::assets::MaterialDesc],
    textures: &[rer_engine_shared::assets::RtexData],
    skinned: &HashSet<u32>,
) {
    log_pbr_nota_once();

    let pbr_map: std::collections::HashMap<u32, MaterialPbrLog> = gltf::Gltf::open(gltf_path)
        .map(|g| {
            g.document
                .materials()
                .enumerate()
                .map(|(mi, m)| (mi as u32, MaterialPbrLog::from_gltf(&m)))
                .collect()
        })
        .unwrap_or_default();

    let mut pairs: Vec<(u32, u32)> = materials
        .iter()
        .map(|m| (m.material_index, m.texture_chunk_index))
        .collect();
    pairs.sort_by_key(|(m, _)| *m);

    log_skinned_mats_compact("bake", skinned, &pairs, |tex_idx| {
        textures
            .get(tex_idx as usize)
            .and_then(|t| t.mips.first().cloned())
    });

    log_skinned_pbr_loss_summary("RERASSET_BAKE", skinned, &pbr_map);
}

pub fn log_rerasset_load_skinned_summary(
    label: &str,
    bytes: usize,
    tex: usize,
    mat: usize,
    skinned: &HashSet<u32>,
    mat_tex_pairs: &[(u32, u32)],
    read_tex: impl Fn(u32) -> Option<Vec<u8>>,
) {
    log_pbr_nota_once();
    let mut skinned_sorted: Vec<u32> = skinned.iter().copied().collect();
    skinned_sorted.sort_unstable();
    log::info!(
        "[RERASSET_LOAD] {label} | {bytes} bytes tex={tex} mat={mat} skinned={skinned_sorted:?}"
    );
    log_skinned_mats_compact("load", skinned, mat_tex_pairs, read_tex);
}

/// Resumen GPU — solo debug.
pub fn log_gpu_model_summary(model_id: &str, mat_layers: &[(u32, u32, u32)]) {
    if mat_layers.is_empty() {
        return;
    }
    let map: String = mat_layers
        .iter()
        .map(|(m, t, l)| format!("mat{m}:tex{t}→L{l}"))
        .collect::<Vec<_>>()
        .join(" ");
    log::debug!("[GPU_MATERIAL] {model_id} {map}");
}

pub fn skinned_material_set_from_parts(
    skinned_parts: Option<&[rer_engine_shared::assets::BakeSkinnedPart]>,
) -> HashSet<u32> {
    skinned_parts
        .map(|parts| parts.iter().map(|p| p.material_index).collect())
        .unwrap_or_default()
}

/// PBR del GLB fuente (si el archivo sigue en disco).
pub fn load_gltf_pbr_map(path: &std::path::Path) -> Option<std::collections::HashMap<u32, MaterialPbrLog>> {
    let gltf = gltf::Gltf::open(path).ok()?;
    Some(
        gltf.document
            .materials()
            .enumerate()
            .map(|(i, m)| (i as u32, MaterialPbrLog::from_gltf(&m)))
            .collect(),
    )
}

/// Resumen por material (no por parte) — una línea por bind skinned.
pub fn log_shader_bind_summary(
    entity_id: u32,
    pipeline: &str,
    total_parts: usize,
    fallback_layer: u32,
    entries: &[(u32, u32, u32, u32, bool)], // mat, tex_chunk, gpu_layer, part_count, assign_ok
) {
    if entries.is_empty() {
        log::warn!(
            "[SHADER_MAT] ent={entity_id} pipeline={pipeline} parts={total_parts} — sin capas GPU asignadas"
        );
        return;
    }
    let map: String = entries
        .iter()
        .map(|(mat, tex, layer, count, ok)| {
            let fail = if *ok {
                String::new()
            } else {
                " FAIL".into()
            };
            let fb = if *ok && *layer == fallback_layer {
                " fb".into()
            } else {
                String::new()
            };
            format!("mat{mat}:tex{tex}→L{layer}×{count}{fail}{fb}")
        })
        .collect::<Vec<_>>()
        .join(" ");
    let any_fail = entries.iter().any(|(_, _, _, _, ok)| !ok);
    let any_fb = entries
        .iter()
        .any(|(_, _, layer, _, ok)| *ok && *layer == fallback_layer);
    if any_fail {
        log::warn!(
            "[SHADER_MAT] ent={entity_id} pipeline={pipeline} parts={total_parts} | {map}"
        );
    } else {
        log::info!(
            "[SHADER_MAT] ent={entity_id} pipeline={pipeline} parts={total_parts} | {map}"
        );
    }
    if any_fb {
        log::warn!(
            "[SHADER_MAT] ent={entity_id} alguna parte usa fallback_layer={fallback_layer} (blanco)"
        );
    }
}

/// Si el bind skinned no corre, siempre visible (nivel warn).
pub fn log_shader_bind_skipped(entity_id: u32, reason: &str) {
    log::warn!("[SHADER_MAT] ent={entity_id} bind OMITIDO: {reason}");
}
