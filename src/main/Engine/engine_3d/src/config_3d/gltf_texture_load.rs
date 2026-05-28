// Política de texturas embebidas en glTF/GLB (carga en editor).
//
// Por defecto solo se decodifica la imagen de **menor resolución** del archivo.
// Un módulo de calidad futuro podrá usar `GltfTextureLoadMode::AllEmbedded`.

use std::collections::HashSet;
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

fn gltf_image_encoded_bytes(
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

/// Índice de la imagen embebida con menor área (prioriza texturas `baseColor` del material).
pub fn pick_smallest_embedded_image_index(
    doc: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    base: Option<&Path>,
) -> Option<usize> {
    let base_color = base_color_image_indices(doc);
    let mut best: Option<(usize, u64)> = None;
    for image in doc.images() {
        let idx = image.index();
        if !base_color.is_empty() && !base_color.contains(&idx) {
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
    if best.is_some() {
        return best.map(|(i, _)| i);
    }
    // Sin materiales PBR baseColor: cualquier imagen embebida.
    let mut fallback: Option<(usize, u64)> = None;
    for image in doc.images() {
        let idx = image.index();
        let Ok(bytes) = gltf_image_encoded_bytes(image, buffers, base) else {
            continue;
        };
        let Some((w, h)) = peek_encoded_dimensions(&bytes) else {
            continue;
        };
        let area = u64::from(w) * u64::from(h);
        if fallback.is_none_or(|(_, a)| area < a) {
            fallback = Some((idx, area));
        }
    }
    fallback.map(|(i, _)| i)
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
            let pick = pick_smallest_embedded_image_index(doc, buffers, base).unwrap_or(0);
            let mesh_albedo = decode_gltf_image_at_index(doc, buffers, base, pick)?;
            log::info!(
                "[gltf] texturas: {count} embebida/s, usando la menor (#{pick}, {}x{})",
                mesh_albedo.width,
                mesh_albedo.height
            );
            Ok((Vec::new(), Some(mesh_albedo)))
        }
    }
}
