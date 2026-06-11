// Política de texturas embebidas en glTF/GLB (carga en editor).
//
// Por defecto solo se decodifica la imagen de **menor resolución** del archivo.
// Un módulo de calidad futuro podrá usar `GltfTextureLoadMode::AllEmbedded`.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use gltf::image::Source;

/// Qué imágenes embebidas decodificar al importar un glTF/GLB.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GltfTextureLoadMode {
    /// Una sola imagen: la de menor área (p. ej. 720p si hay 4K/1080p/720p).
    SmallestEmbedded,
    /// Todas las imágenes (calidad máxima; reservado para configuración futura).
    #[allow(dead_code)]
    AllEmbedded,
}

impl Default for GltfTextureLoadMode {
    fn default() -> Self {
        Self::SmallestEmbedded
    }
}

/// Modo activo en cargas del editor (hasta existir módulo de calidad en UI).
pub fn editor_gltf_texture_load_mode() -> GltfTextureLoadMode {
    GltfTextureLoadMode::SmallestEmbedded
}

/// Dimensiones aproximadas leyendo solo cabecera JPEG/PNG (sin decodificar píxeles).
pub fn peek_encoded_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() >= 24 && bytes[0] == 0x89 && bytes[1..4] == *b"PNG" {
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        return (w > 0 && h > 0).then_some((w, h));
    }
    if bytes.len() >= 4 && bytes[0] == 0xFF && bytes[1] == 0xD8 {
        let mut i = 2usize;
        while i + 9 < bytes.len() {
            if bytes[i] != 0xFF {
                i += 1;
                continue;
            }
            let marker = bytes[i + 1];
            if (0xC0..=0xC3).contains(&marker)
                || (0xC5..=0xC7).contains(&marker)
                || marker == 0xC9
            {
                if i + 9 >= bytes.len() {
                    break;
                }
                let h = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u32;
                let w = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]) as u32;
                return (w > 0 && h > 0).then_some((w, h));
            }
            if marker == 0xD8 || marker == 0xD9 {
                i += 2;
                continue;
            }
            if i + 3 >= bytes.len() {
                break;
            }
            let seg_len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
            if seg_len < 2 {
                break;
            }
            i += 2 + seg_len;
        }
    }
    None
}

/// Texto para emparejar imágenes embebidas con materiales (nombre glTF + URI).
pub(crate) fn gltf_image_search_label(image: gltf::Image) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(name) = image.name() {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            parts.push(trimmed.to_lowercase());
        }
    }
    match image.source() {
        Source::Uri { uri, .. } => {
            let stem = Path::new(uri)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(uri);
            parts.push(stem.to_lowercase());
            parts.push(uri.to_lowercase());
        }
        _ => {}
    }
    parts.join(" ")
}

pub(crate) fn gltf_image_encoded_bytes(
    image: gltf::Image,
    buffers: &[gltf::buffer::Data],
    base: Option<&Path>,
) -> Result<Vec<u8>, String> {
    match image.source() {
        Source::View { view, .. } => {
            let parent = &buffers[view.buffer().index()].0;
            let begin = view.offset();
            let end = begin.saturating_add(view.length()).min(parent.len());
            Ok(parent[begin..end].to_vec())
        }
        Source::Uri { uri, .. } => {
            let base = base.ok_or_else(|| format!("imagen externa sin base: {uri}"))?;
            std::fs::read(base.join(uri.strip_prefix('/').unwrap_or(uri)))
                .map_err(|e| format!("no se pudo leer textura {uri}: {e}"))
        }
    }
}

fn base_color_image_indices(doc: &gltf::Document) -> HashSet<usize> {
    let mut out = HashSet::new();
    for mat in doc.materials() {
        if let Some(info) = mat.pbr_metallic_roughness().base_color_texture() {
            out.insert(info.texture().source().index());
        }
    }
    out
}

fn pick_smallest_among_image_indices(
    doc: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    base: Option<&Path>,
    allowed: &HashSet<usize>,
) -> Option<usize> {
    let mut best: Option<(usize, u64)> = None;
    for image in doc.images() {
        let idx = image.index();
        if !allowed.is_empty() && !allowed.contains(&idx) {
            continue;
        }
        let Ok(bytes) = gltf_image_encoded_bytes(image, buffers, base) else {
            continue;
        };
        let Some((w, h)) = peek_encoded_dimensions(&bytes) else {
            continue;
        };
        let area = u64::from(w) * u64::from(h);
        if best.is_none_or(|(_, a)| area < a) {
            best = Some((idx, area));
        }
    }
    best.map(|(i, _)| i)
}

/// Índice de la imagen embebida con menor área (prioriza texturas `baseColor` del material).
pub fn pick_smallest_embedded_image_index(
    doc: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    base: Option<&Path>,
) -> Option<usize> {
    let base_color = base_color_image_indices(doc);
    if let Some(idx) = pick_smallest_among_image_indices(doc, buffers, base, &base_color) {
        return Some(idx);
    }
    pick_smallest_among_image_indices(doc, buffers, base, &HashSet::new())
}

fn primary_material_token(mat_name: &str) -> Option<String> {
    let lower = mat_name.trim().to_lowercase();
    if lower.is_empty() {
        return None;
    }
    if lower.starts_with("material") {
        return Some(lower);
    }
    lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 3)
        .max_by_key(|t| t.len())
        .map(|t| t.to_string())
}

/// `body` coincide con `body_diffuse` pero no con `bottom`.
pub(crate) fn token_matches_label(token: &str, label: &str) -> bool {
    if token.is_empty() || label.is_empty() {
        return false;
    }
    if label == token {
        return true;
    }
    if label.starts_with(token) {
        let rest = &label[token.len()..];
        if rest.is_empty() || rest.starts_with(|c: char| !c.is_alphanumeric()) {
            return true;
        }
    }
    for pat in [
        format!("{token}_"),
        format!("_{token}_"),
        format!("_{token}"),
        format!("-{token}-"),
        format!("-{token}"),
    ] {
        if label.contains(pat.as_str()) {
            return true;
        }
    }
    false
}

pub(crate) fn material_names_related(a: &str, b: &str) -> bool {
    let a = a.trim().to_lowercase();
    let b = b.trim().to_lowercase();
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    match (primary_material_token(&a), primary_material_token(&b)) {
        (Some(ta), Some(tb)) if ta == tb => true,
        (Some(ta), _) => token_matches_label(&ta, &b),
        (_, Some(tb)) => token_matches_label(&tb, &a),
        _ => false,
    }
}

pub(crate) fn material_name_matches_image_label(mat_name: &str, label: &str) -> bool {
    let label = label.trim().to_lowercase();
    if label.is_empty() {
        return false;
    }
    let mat = mat_name.trim().to_lowercase();
    if !mat.is_empty() && token_matches_label(&mat, &label) {
        return true;
    }
    if let Some(token) = primary_material_token(mat_name) {
        return token_matches_label(&token, &label);
    }
    false
}

/// Índices de imagen embebida que pertenecen a un material (catálogo UI + apply GPU).
pub(crate) fn discover_material_image_indices(
    doc: &gltf::Document,
    material_index: usize,
    mat_name: &str,
    image_labels: &HashMap<u32, String>,
    all_variants: &[(u32, u32, u32)],
    all_by_image_index: &[(u32, u32, u32)],
) -> HashSet<u32> {
    let material_count = doc.materials().len();
    let mut indices: HashSet<u32> = HashSet::new();

    if let Some(mat) = doc.materials().nth(material_index) {
        if let Some(info) = mat.pbr_metallic_roughness().base_color_texture() {
            indices.insert(info.texture().source().index() as u32);
        }
    }

    for (other_idx, other) in doc.materials().enumerate() {
        let other_name = other.name().unwrap_or("");
        if other_idx != material_index && !material_names_related(mat_name, other_name) {
            continue;
        }
        if let Some(info) = other.pbr_metallic_roughness().base_color_texture() {
            indices.insert(info.texture().source().index() as u32);
        }
    }

    for &(idx, _, _) in all_variants {
        if let Some(label) = image_labels.get(&idx) {
            if material_name_matches_image_label(mat_name, label) {
                indices.insert(idx);
            }
        }
    }

    if indices.len() <= 1 && material_count >= 2 && all_by_image_index.len() >= material_count * 2 {
        let per_mat = all_by_image_index.len() / material_count;
        if per_mat >= 2 {
            let start = material_index * per_mat;
            let end = start + per_mat;
            if end <= all_by_image_index.len() {
                for &(idx, _, _) in &all_by_image_index[start..end] {
                    indices.insert(idx);
                }
            }
        }
    }

    indices
}

/// Menor imagen embebida asociada al `baseColor` de un material concreto.
pub fn pick_smallest_embedded_for_material(
    doc: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    base: Option<&Path>,
    material_index: usize,
) -> Option<usize> {
    let mat = doc.materials().nth(material_index)?;
    let mat_indices: HashSet<usize> = mat
        .pbr_metallic_roughness()
        .base_color_texture()
        .map(|info| info.texture().source().index())
        .into_iter()
        .collect();
    if mat_indices.is_empty() {
        return None;
    }
    pick_smallest_among_image_indices(doc, buffers, base, &mat_indices)
        .or_else(|| mat_indices.iter().copied().next())
}

/// Albedo por defecto del editor: material 0 (variante más pequeña), no la menor global del GLB.
pub fn pick_editor_mesh_albedo_image_index(
    doc: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    base: Option<&Path>,
) -> Option<usize> {
    pick_smallest_embedded_for_material(doc, buffers, base, 0)
        .or_else(|| pick_smallest_embedded_image_index(doc, buffers, base))
}

pub fn decode_gltf_image_at_index(
    doc: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    base: Option<&Path>,
    index: usize,
) -> Result<gltf::image::Data, String> {
    let image = doc
        .images()
        .nth(index)
        .ok_or_else(|| format!("imagen glTF índice {index} inexistente"))?;
    gltf::image::Data::from_source(image.source(), base, buffers)
        .map_err(|e| format!("error decodificando imagen {index}: {e}"))
}

/// Decodifica la variante embebida más pequeña del `baseColor` de cada material.
pub fn import_material_smallest_albedos(
    doc: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    base: Option<&Path>,
) -> HashMap<usize, gltf::image::Data> {
    let mut out = HashMap::new();
    for (mi, _) in doc.materials().enumerate() {
        let Some(img_idx) = pick_smallest_embedded_for_material(doc, buffers, base, mi) else {
            continue;
        };
        if let Ok(data) = decode_gltf_image_at_index(doc, buffers, base, img_idx) {
            out.insert(mi, data);
        }
    }
    out
}

pub fn import_gltf_images(
    doc: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    base: Option<&Path>,
    mode: GltfTextureLoadMode,
) -> Result<(Vec<gltf::image::Data>, Option<gltf::image::Data>), String> {
    match mode {
        GltfTextureLoadMode::AllEmbedded => {
            let images = gltf::import_images(doc, base, buffers)
                .map_err(|e| format!("error importando imágenes glTF: {e}"))?;
            Ok((images, None))
        }
        GltfTextureLoadMode::SmallestEmbedded => {
            let count = doc.images().count();
            if count == 0 {
                return Ok((Vec::new(), None));
            }
            let material_albedos = import_material_smallest_albedos(doc, buffers, base);
            let mesh_albedo = material_albedos
                .get(&0)
                .cloned()
                .or_else(|| {
                    pick_editor_mesh_albedo_image_index(doc, buffers, base)
                        .and_then(|pick| decode_gltf_image_at_index(doc, buffers, base, pick).ok())
                });
            if let Some(ref img) = mesh_albedo {
                log::info!(
                    "[gltf] texturas: {count} embebida/s, {} material/es con variante menor",
                    material_albedos.len().max(1)
                );
                log::debug!(
                    "[gltf] albedo fallback mat0: {}x{}",
                    img.width,
                    img.height
                );
            }
            Ok((Vec::new(), mesh_albedo))
        }
    }
}
