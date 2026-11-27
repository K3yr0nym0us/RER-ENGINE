use cosmic_text::{Attrs, Family, FeatureTag, FontFeatures, Weight};
use fontdb::Weight as FontDbWeight;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Familia embebida o de proyecto seleccionada para un bloque de texto.
///
/// Las familias integradas se serializan como cadenas legibles (`sans_serif`,
/// `arabic`, etc.). Una familia custom se serializa directamente con su nombre.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum FontFamily {
    /// Inter, resuelta a través de `sans-serif` en la base de fontdb.
    #[default]
    SansSerif,
    /// Noto Sans Arabic.
    Arabic,
    /// Noto Sans Hebrew.
    Hebrew,
    /// Noto Sans SC, incluyendo su eje variable de peso.
    SimplifiedChinese,
    /// Fira Mono.
    Monospace,
    /// Familia registrada mediante [`TextSystem::load_font`](crate::text::TextSystem::load_font)
    /// o [`TextSystem::font_from_bytes`](crate::text::TextSystem::font_from_bytes).
    Custom(String),
}

impl FontFamily {
    pub(crate) fn is_reserved_name(name: &str) -> bool {
        matches!(
            name,
            "sans_serif" | "arabic" | "hebrew" | "simplified_chinese" | "monospace"
        )
    }

    pub(crate) fn cosmic(&self) -> Family<'_> {
        match self {
            Self::SansSerif => Family::SansSerif,
            Self::Arabic => Family::Name("Noto Sans Arabic"),
            Self::Hebrew => Family::Name("Noto Sans Hebrew"),
            Self::SimplifiedChinese => Family::Name("Noto Sans SC"),
            Self::Monospace => Family::Monospace,
            Self::Custom(name) => Family::Name(name),
        }
    }
}

impl Serialize for FontFamily {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match self {
            Self::SansSerif => "sans_serif",
            Self::Arabic => "arabic",
            Self::Hebrew => "hebrew",
            Self::SimplifiedChinese => "simplified_chinese",
            Self::Monospace => "monospace",
            Self::Custom(name) => name,
        })
    }
}

impl<'de> Deserialize<'de> for FontFamily {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(match value.as_str() {
            "sans_serif" => Self::SansSerif,
            "arabic" => Self::Arabic,
            "hebrew" => Self::Hebrew,
            "simplified_chinese" => Self::SimplifiedChinese,
            "monospace" => Self::Monospace,
            _ => Self::Custom(value),
        })
    }
}

/// Pesos soportados por las fuentes embebidas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FontWeight {
    #[default]
    Regular,
    Bold,
}

impl FontWeight {
    pub(crate) const fn fontdb(self) -> FontDbWeight {
        match self {
            Self::Regular => FontDbWeight::NORMAL,
            Self::Bold => FontDbWeight::BOLD,
        }
    }

    pub(crate) fn cosmic(self) -> Weight {
        Weight(self.fontdb().0)
    }
}

/// Modo de ajuste de línea del [`Buffer`](cosmic_text::Buffer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextWrap {
    /// No inserta saltos de línea automáticos.
    None,
    /// Ajusta preferentemente en palabras.
    Word,
    /// Ajusta en cualquier glifo, incluidos los ideogramas CJK.
    Glyph,
    /// Ajusta en palabras y recurre a glifos cuando una palabra no cabe.
    #[default]
    WordOrGlyph,
}

impl TextWrap {
    pub(crate) const fn cosmic(self) -> cosmic_text::Wrap {
        match self {
            Self::None => cosmic_text::Wrap::None,
            Self::Word => cosmic_text::Wrap::Word,
            Self::Glyph => cosmic_text::Wrap::Glyph,
            Self::WordOrGlyph => cosmic_text::Wrap::WordOrGlyph,
        }
    }
}

/// Alineación de cada línea dentro del ancho disponible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    #[default]
    Center,
    Left,
    Right,
}

impl TextAlign {
    pub(crate) const fn cosmic(self) -> cosmic_text::Align {
        match self {
            Self::Left => cosmic_text::Align::Left,
            Self::Center => cosmic_text::Align::Center,
            Self::Right => cosmic_text::Align::Right,
        }
    }
}

/// Estilo serializable de un bloque de texto.
///
/// `size`, `line_height`, `max_width` y `max_height` se expresan en píxeles
/// lógicos. `supersample` se limita internamente al rango 1–4. El lenguaje es
/// una etiqueta BCP 47 opcional que sustituye al locale del [`TextSystem`]
/// durante el fallback de esa llamada.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextStyle {
    pub family: FontFamily,
    pub size: f32,
    pub line_height: f32,
    pub weight: FontWeight,
    pub wrap: TextWrap,
    pub align: TextAlign,
    pub language: Option<String>,
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
    pub tint: [f32; 4],
    pub supersample: u32,
}

impl TextStyle {
    /// Crea un estilo con una altura de línea de 1.25 veces el tamaño.
    pub const fn new(family: FontFamily, size: f32) -> Self {
        Self {
            family,
            size,
            line_height: size * 1.25,
            weight: FontWeight::Regular,
            wrap: TextWrap::WordOrGlyph,
            align: TextAlign::Center,
            language: None,
            max_width: None,
            max_height: None,
            tint: [1.0; 4],
            supersample: 1,
        }
    }
}

impl Default for TextStyle {
    fn default() -> Self {
        Self::new(FontFamily::SansSerif, 18.0)
    }
}

/// Estilo compartido por los textos del HUD en 2D y 3D.
pub(crate) fn hud_style(
    family: FontFamily,
    width_px: u32,
    height_px: u32,
    tint: [f32; 4],
    language: Option<&str>,
) -> TextStyle {
    TextStyle {
        max_width: Some(width_px as f32),
        max_height: Some(height_px as f32),
        language: language.map(str::to_owned),
        tint,
        ..TextStyle::new(family, height_px as f32 * 0.62)
    }
}

/// Estilo del prompt de interacción: tamaño mayor, negrita real y 4× SSAA.
pub(crate) fn prompt_style(
    family: FontFamily,
    width_px: u32,
    height_px: u32,
    weight: FontWeight,
    language: Option<&str>,
) -> TextStyle {
    TextStyle {
        weight,
        max_width: Some(width_px as f32),
        max_height: Some(height_px as f32),
        language: language.map(str::to_owned),
        supersample: 4,
        ..TextStyle::new(family, height_px as f32 * 0.78)
    }
}

pub(crate) fn attrs(style: &TextStyle) -> Attrs<'_> {
    let attrs = Attrs::new()
        .family(style.family.cosmic())
        .weight(style.weight.cosmic());
    if style.family == FontFamily::Monospace {
        // Fira Mono publica fi/fl como ligaduras discrecionales (`dlig`).
        let mut features = FontFeatures::new();
        features.enable(FeatureTag::DISCRETIONARY_LIGATURES);
        attrs.font_features(features)
    } else {
        attrs
    }
}
