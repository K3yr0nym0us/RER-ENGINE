//! Caché e importación de ficheros glTF, listado de clips y entrada de carga.

use super::*;

pub(crate) fn gltf_import_cache_key(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| PathBuf::from(path))
        .display()
        .to_string()
}

pub(crate) fn gltf_import_cache() -> &'static Mutex<HashMap<String, Arc<GltfFile>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<GltfFile>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Invalida el parseo glTF en memoria (p. ej. tras reemplazar el archivo en disco).
pub fn invalidate_gltf_import_cache(path: &Path) {
    let key = gltf_import_cache_key(path);
    if let Ok(mut cache) = gltf_import_cache().lock() {
        cache.remove(&key);
    }
}

pub(crate) fn import_gltf_uncached(path: &Path) -> Result<GltfFile, String> {
    let gltf = gltf::Gltf::open(path).map_err(|e| format!("gltf error: {e}"))?;
    let base = path.parent();
    let doc = gltf.document;
    let blob = gltf.blob;
    let buffers = gltf::import_buffers(&doc, base, blob)
        .map_err(|e| format!("error importando buffers glTF: {e}"))?;
    let model_path = path.display().to_string();
    let (material_smallest_albedos, material_textures) =
        import_gltf_texture_layers(&doc, &buffers, base, &model_path);
    Ok(GltfFile {
        path: model_path,
        doc,
        buffers,
        material_smallest_albedos,
        material_textures,
    })
}

/// Importa glTF/GLB una sola vez por ruta canónica (decode + resize + mips compartidos).
pub fn import_gltf(path: &Path) -> Result<Arc<GltfFile>, String> {
    let key = gltf_import_cache_key(path);
    if let Ok(cache) = gltf_import_cache().lock()
        && let Some(hit) = cache.get(&key)
    {
        return Ok(Arc::clone(hit));
    }
    let file = Arc::new(import_gltf_uncached(path)?);
    if let Ok(mut cache) = gltf_import_cache().lock() {
        cache.insert(key, Arc::clone(&file));
    }
    Ok(file)
}

/// Convierte `gltf::image::Data` a RGBA8 en CPU (malla / skinned).
pub fn gltf_image_data_to_rgba(img: &gltf::image::Data) -> (Vec<u8>, u32, u32) {
    use gltf::image::Format;
    let pixels = match img.format {
        Format::R8G8B8 => img
            .pixels
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255u8])
            .collect(),
        Format::R8G8B8A8 => img.pixels.clone(),
        _ => vec![255, 255, 255, 255],
    };
    (pixels, img.width, img.height)
}

/// Props estáticos (p. ej. rocas) no necesitan `ModelAsset` (skinning/animación).
pub fn gltf_needs_model_asset(file: &GltfFile) -> bool {
    file.doc.skins().next().is_some() || file.doc.animations().next().is_some()
}

/// Metadatos de clips glTF/GLB sin cargar malla skinned.
pub fn list_gltf_clip_infos(path: &Path) -> Vec<ModelClipInfo> {
    let Ok(file) = import_gltf(path) else {
        return Vec::new();
    };
    list_gltf_clip_infos_from_file(file.as_ref())
}

pub fn list_gltf_clip_infos_from_file(file: &GltfFile) -> Vec<ModelClipInfo> {
    let doc = &file.doc;
    let buffers = &file.buffers;

    let mut out = Vec::new();
    for anim in doc.animations() {
        let name = anim
            .name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("Animation_{}", anim.index()));
        let mut max_t = 0.0f32;
        for channel in anim.channels() {
            let reader = channel.reader(|b| buffers.get(b.index()).map(|d| d.0.as_slice()));
            if let Some(inputs) = reader.read_inputs() {
                for t in inputs {
                    max_t = max_t.max(t);
                }
            }
        }
        out.push(ModelClipInfo {
            name,
            duration_s: max_t.max(1.0 / 30.0),
            fps: 30.0,
        });
    }
    out
}

/// Lista clips embebidos según extensión (FBX o glTF/GLB).
pub fn list_model_clip_infos(path: &Path) -> Vec<ModelClipInfo> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "fbx" => crate::engine3d::config_3d::model_asset_fbx::list_fbx_clip_infos(path),
        "glb" | "gltf" => list_gltf_clip_infos(path),
        _ => Vec::new(),
    }
}

pub fn load_model_asset(path: &Path, normalize_to_extent: Option<f32>) -> Option<Arc<ModelAsset>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "glb" | "gltf" => import_gltf(path)
            .ok()
            .and_then(|file| load_gltf_asset_from_file(file.as_ref(), normalize_to_extent)),
        "fbx" => {
            crate::engine3d::config_3d::model_asset_fbx::load_fbx_asset(path, normalize_to_extent)
        }
        _ => None,
    }
}

pub fn load_model_asset_from_gltf(
    file: &GltfFile,
    normalize_to_extent: Option<f32>,
) -> Option<Arc<ModelAsset>> {
    load_gltf_asset_from_file(file, normalize_to_extent)
}
