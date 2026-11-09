//! Texturas de material y conversión de imágenes glTF a RGBA.

use super::*;

pub(crate) fn empty_rgba_placeholder() -> Arc<[u8]> {
    static EMPTY: OnceLock<Arc<[u8]>> = OnceLock::new();
    EMPTY.get_or_init(|| Arc::from(&[][..])).clone()
}

pub(crate) fn shared_white_material_texture() -> Arc<MaterialTextureCpu> {
    static WHITE: OnceLock<Arc<MaterialTextureCpu>> = OnceLock::new();
    WHITE
        .get_or_init(|| {
            Arc::new(MaterialTextureCpu {
                rgba: Arc::from([255u8, 255, 255, 255].as_slice()),
                width: 1,
                height: 1,
                layer_mips: None,
            })
        })
        .clone()
}

/// Decodifica albedos por material y genera mip chain 1024² una vez (modo editor).
pub(crate) fn build_material_textures_with_mips(
    _doc: &gltf::Document,
    albedos: &HashMap<usize, gltf::image::Data>,
    _model_path: Option<&str>,
) -> HashMap<u32, Arc<MaterialTextureCpu>> {
    let tex_size = crate::engine3d::texture::TextureArray::TEXTURE_SIZE;
    let mut material_indices: Vec<usize> = albedos.keys().copied().collect();
    material_indices.sort_unstable();

    let built: Vec<(u32, Arc<MaterialTextureCpu>)> = material_indices
        .par_iter()
        .map(|mi| {
            let mi = *mi;
            let img = albedos.get(&mi).expect("material index from albedos keys");
            let (rgba_vec, w, h) = gltf_image_data_to_rgba(img);
            let chain = crate::engine3d::texture::build_layer_mip_chain_timed(rgba_vec, w, h);
            let tex = Arc::new(MaterialTextureCpu {
                rgba: empty_rgba_placeholder(),
                width: tex_size,
                height: tex_size,
                layer_mips: Some(Arc::new(chain.mips)),
            });
            (mi as u32, tex)
        })
        .collect();

    let mut out = HashMap::with_capacity(built.len());
    for (mi, tex) in built {
        out.insert(mi, tex);
    }
    out
}

pub(crate) fn import_gltf_texture_layers(
    doc: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    base: Option<&Path>,
    model_path: &str,
) -> (
    HashMap<usize, gltf::image::Data>,
    HashMap<u32, Arc<MaterialTextureCpu>>,
) {
    let material_smallest_albedos =
        crate::engine3d::config_3d::gltf_texture_load::import_material_smallest_albedos_profiled(
            doc,
            buffers,
            base,
            Some(model_path),
        );
    let material_textures =
        build_material_textures_with_mips(doc, &material_smallest_albedos, Some(model_path));
    (material_smallest_albedos, material_textures)
}

pub(crate) fn gltf_texture_rgba(
    primitive: &gltf::Primitive,
    material_albedos: &HashMap<usize, gltf::image::Data>,
) -> (Vec<u8>, u32, u32) {
    let mat_idx = primitive.material().index().unwrap_or(0);
    if let Some(img) = material_albedos.get(&mat_idx) {
        gltf_image_data_to_rgba(img)
    } else {
        (vec![255, 255, 255, 255], 1, 1)
    }
}
