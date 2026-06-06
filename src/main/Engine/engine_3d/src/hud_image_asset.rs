//! Metadatos de imágenes registradas para HUD (Resources → Images).

#[derive(Clone, Debug)]
pub struct HudImageAssetMeta {
    pub name: String,
    pub width_px: u32,
    pub height_px: u32,
}

pub fn probe_image_dimensions(path: &str) -> Result<(u32, u32), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("no se pudo leer {path}: {e}"))?;
    use image::{GenericImageView, ImageReader};
    let img = ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|e| format!("formato de imagen no reconocido: {e}"))?
        .decode()
        .map_err(|e| format!("error decodificando imagen: {e}"))?;
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return Err("imagen sin dimensiones válidas".into());
    }
    Ok((w, h))
}

pub fn validate_hud_image_file(path: &str) -> Result<(u32, u32), String> {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp") {
        return Err(format!(
            "extensión no soportada (.{ext}); use PNG, JPEG o WebP"
        ));
    }
    probe_image_dimensions(path)
}
