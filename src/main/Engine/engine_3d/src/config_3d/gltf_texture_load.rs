// Política de texturas embebidas en glTF/GLB (carga en editor).
// Solo se decodifica la imagen de **menor resolución** por material.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use gltf::image::Source;

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

/// Dimensiones JPEG/PNG sin copiar el blob embebido completo (crítico en GLB grandes).
pub(crate) fn peek_gltf_image_dimensions(
    image: gltf::Image,
    buffers: &[gltf::buffer::Data],
    base: Option<&Path>,
) -> Option<(u32, u32)> {
    match image.source() {
        Source::View { view, .. } => {
            let parent = &buffers[view.buffer().index()].0;
            let begin = view.offset();
            let end = begin.saturating_add(view.length()).min(parent.len());
            peek_encoded_dimensions(&parent[begin..end])
        }
        Source::Uri { uri, .. } => {
            let base = base?;
            let path = base.join(uri.strip_prefix('/').unwrap_or(uri));
            let mut file = std::fs::File::open(path).ok()?;
            let mut header = [0u8; 32 * 1024];
            let n = std::io::Read::read(&mut file, &mut header).ok()?;
            peek_encoded_dimensions(&header[..n])
        }
    }
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
        let Some((w, h)) = peek_gltf_image_dimensions(image, buffers, base) else {
            continue;
        };
        let area = u64::from(w) * u64::from(h);
        if best.is_none_or(|(_, a)| area < a) {
            best = Some((idx, area));
        }
    }
    best.map(|(i, _)| i)
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
pub(crate) fn import_material_smallest_albedos_profiled(
    doc: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    base: Option<&Path>,
    _model_path: Option<&str>,
) -> HashMap<usize, gltf::image::Data> {
    let mut out = HashMap::new();
    for (mi, _mat) in doc.materials().enumerate() {
        let Some(img_idx) = pick_smallest_embedded_for_material(doc, buffers, base, mi) else {
            continue;
        };
        let Ok(data) = decode_gltf_image_at_index(doc, buffers, base, img_idx) else {
            continue;
        };
        out.insert(mi, data);
    }
    out
}
