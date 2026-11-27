//! Sistema de texto CPU v0: shaping, BiDi, fallback y rasterizado RGBA.
//!
//! No depende de Vulkan, del atlas del HUD ni del renderizador de texto actual.

mod fonts;
mod style;
mod system;

pub use fonts::{FontSource, LoadedFont};
pub use style::{FontFamily, FontWeight, TextAlign, TextStyle, TextWrap};
pub(crate) use style::{hud_style, prompt_style};
pub use system::{RasterizedText, ShapedGlyph, TextSystem};

/// Validación compartida por los comandos de fuentes de ambos motores.
pub(crate) fn validate_font_file(path: &str) -> Result<(), String> {
    TextSystem::validate_font_file(std::path::Path::new(path))
}

#[cfg(test)]
mod tests;
