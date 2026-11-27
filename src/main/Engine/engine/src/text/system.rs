use std::path::Path;

use cosmic_text::{
    Buffer, CacheKey, Cursor, FontSystem, Metrics, Shaping, SwashCache, SwashContent, SwashImage,
};
use fontdb::{Database, ID};
use rer_engine_core::i18n::LocaleId;

use super::fonts::{self, LoadedFont};
use super::style::{FontFamily, TextAlign, TextStyle, attrs};

const MAX_SUPERSAMPLE: u32 = 4;
const MAX_TEXT_DIMENSION: f32 = 4096.0;
const MAX_SUPERSAMPLED_PIXELS: u64 = 16_777_216;

/// Glifo ya conformado por HarfBuzz/BiDi y asociado a su clave de rasterizado.
///
/// `start` y `end` son offsets UTF-8 del texto completo. `x` e `y` están en
/// píxeles lógicos del bitmap final (`y` es la línea base); `level` es el nivel
/// de embedding Unicode BiDi (par = LTR, impar = RTL).
#[derive(Debug, Clone, PartialEq)]
pub struct ShapedGlyph {
    pub cache_key: CacheKey,
    pub font_id: ID,
    pub glyph_id: u16,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub start: usize,
    pub end: usize,
    pub level: u8,
}

/// Bitmap RGBA con origen arriba-izquierda, listo para copiar a un atlas.
///
/// `supersample` es el factor efectivamente usado. Las coordenadas de `glyphs`
/// son lógicas; sus `CacheKey` y el rasterizado interno usan píxeles físicos
/// multiplicados por este factor.
#[derive(Debug, Clone, PartialEq)]
pub struct RasterizedText {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub supersample: u32,
    pub glyphs: Vec<ShapedGlyph>,
}

/// Sistema de texto CPU con shaping avanzado, fallback y rasterizado Swash.
///
/// La base de fontdb contiene las fuentes empaquetadas y las fuentes de proyecto
/// registradas explícitamente. No se consultan directorios de fuentes del
/// sistema operativo.
#[derive(Debug)]
pub struct TextSystem {
    font_system: FontSystem,
    swash_cache: SwashCache,
    loaded_fonts: Vec<LoadedFont>,
}

impl TextSystem {
    /// Crea el sistema con locale determinista `en-US` y las seis fuentes.
    #[must_use]
    pub fn new() -> Self {
        let mut db = fontdb::Database::new();
        let loaded_fonts = fonts::load(&mut db);
        let font_system = fonts::font_system("en-US".to_owned(), db);
        Self {
            font_system,
            swash_cache: SwashCache::new(),
            loaded_fonts,
        }
    }

    /// Locale activo para el fallback por script.
    #[must_use]
    pub fn locale(&self) -> &str {
        self.font_system.locale()
    }

    /// Caras embebidas cargadas, en el mismo orden que [`FontSource::ALL`].
    #[must_use]
    pub fn loaded_fonts(&self) -> &[LoadedFont] {
        &self.loaded_fonts
    }

    /// Valida bytes TTF/OTF con el mismo parser fontdb usado por el runtime.
    pub(crate) fn validate_font_bytes(bytes: &[u8]) -> Result<(), String> {
        let mut db = Database::new();
        fonts::load_custom(&mut db, "__rer_font_validation__", bytes.to_vec())
            .map(|_| ())
            .map_err(|error| format!("Archivo de fuente inválido: {error}"))
    }

    /// Valida extensión, lectura y contenido de un archivo de fuente.
    pub(crate) fn validate_font_file(path: &Path) -> Result<(), String> {
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if extension != "ttf" && extension != "otf" {
            return Err("Se esperaba un archivo .ttf u .otf".to_owned());
        }
        let bytes =
            std::fs::read(path).map_err(|error| format!("No se pudo leer la fuente: {error}"))?;
        Self::validate_font_bytes(&bytes)
    }

    /// Lee y registra una fuente de proyecto bajo el alias de familia indicado.
    ///
    /// El archivo se copia a memoria: no se mantiene una dependencia de la ruta.
    /// Devuelve el `fontdb::ID` de la primera cara registrada.
    ///
    /// # Errors
    /// Devuelve un error si el alias está vacío o reservado, no se puede leer el
    /// archivo o no contiene ninguna cara de fuente válida.
    pub fn load_font(
        &mut self,
        family: impl Into<String>,
        path: impl AsRef<Path>,
    ) -> Result<ID, String> {
        let path = path.as_ref();
        let bytes = std::fs::read(path)
            .map_err(|error| format!("failed to read font {}: {error}", path.display()))?;
        self.font_from_bytes(family, bytes)
    }

    /// Registra bytes TTF/TTC como fuente de proyecto bajo un alias de familia.
    ///
    /// La fuente custom se convierte en primaria mediante
    /// [`FontFamily::Custom`](super::FontFamily::Custom), manteniendo el fallback
    /// integrado para los glifos que no cubra. Registrar una fuente reconstruye
    /// `FontSystem` y limpia sus caches y la caché de Swash.
    ///
    /// # Errors
    /// Devuelve un error si el alias está vacío, está reservado por una familia
    /// integrada o los bytes no contienen ninguna cara de fuente válida.
    pub fn font_from_bytes(
        &mut self,
        family: impl Into<String>,
        bytes: Vec<u8>,
    ) -> Result<ID, String> {
        let family = family.into();
        let family = family.trim();
        if family.is_empty() {
            return Err("custom font family cannot be empty".to_owned());
        }
        if FontFamily::is_reserved_name(family) {
            return Err(format!("custom font family {family:?} is reserved"));
        }

        let locale = self.font_system.locale().to_owned();
        let mut db = self.font_system.db().clone();
        let id = fonts::load_custom(&mut db, family, bytes)?;
        self.font_system = fonts::font_system(locale, db);
        self.swash_cache = SwashCache::new();
        Ok(id)
    }

    /// Cambia el locale, conserva la DB y descarta todas las caches de shaping
    /// y rasterizado para evitar claves obsoletas.
    ///
    /// El locale se valida y normaliza con [`LocaleId`]. Si es inválido emite un
    /// warning y deja el sistema sin cambios.
    pub fn set_locale(&mut self, locale: impl Into<String>) {
        let input = locale.into();
        let locale = match LocaleId::parse(&input) {
            Ok(locale) => locale.to_string(),
            Err(error) => {
                log::warn!("[text] invalid locale {input:?}: {error}; keeping current locale");
                return;
            }
        };
        if self.font_system.locale() == locale {
            return;
        }
        let db = self.font_system.db().clone();
        self.font_system = fonts::font_system(locale, db);
        self.swash_cache = SwashCache::new();
    }

    /// Conforma y rasteriza `text` en un RGBA recortado al rect del estilo.
    ///
    /// La caja de tinta se alinea horizontalmente según [`TextAlign`] y se centra
    /// verticalmente como en el HUD actual. Si no se indican dimensiones máximas,
    /// el bitmap se ajusta a la tinta (o a una línea para texto vacío). El
    /// supersampling se reduce automáticamente si el buffer excedería el límite
    /// de seguridad de CPU.
    pub fn rasterize(&mut self, text: &str, style: &TextStyle) -> RasterizedText {
        self.with_style_locale(style, |system| {
            let buffer = system.shape_buffer(text, style);
            let mut glyphs = system.collect_glyphs(&buffer);
            let geometry = system.geometry(&glyphs, style);
            let raster_width = geometry.width * geometry.supersample;
            let raster_height = geometry.height * geometry.supersample;
            let mut rgba = vec![0; rgba_len(raster_width, raster_height)];
            let tint = sanitized_tint(style.tint);

            for glyph in &mut glyphs {
                let physical = glyph.layout.physical(
                    (
                        geometry.shift_x * geometry.supersample as f32,
                        geometry.shift_y * geometry.supersample as f32
                            + glyph.line_y * geometry.supersample as f32,
                    ),
                    geometry.supersample as f32,
                );
                glyph.shaped.cache_key = physical.cache_key;
                glyph.shaped.x += geometry.shift_x / geometry.supersample as f32;
                glyph.shaped.y += geometry.shift_y / geometry.supersample as f32;

                if let Some(image) = system
                    .swash_cache
                    .get_image(&mut system.font_system, physical.cache_key)
                {
                    composite_image(
                        image,
                        i64::from(physical.x) + i64::from(image.placement.left),
                        i64::from(physical.y) - i64::from(image.placement.top),
                        raster_width,
                        raster_height,
                        tint,
                        &mut rgba,
                    );
                }
            }

            let rgba = downsample(&rgba, raster_width, raster_height, geometry.supersample);
            RasterizedText {
                rgba,
                width: geometry.width,
                height: geometry.height,
                supersample: geometry.supersample,
                glyphs: glyphs.into_iter().map(|glyph| glyph.shaped).collect(),
            }
        })
    }

    /// Posición X del cursor, derivada del mismo [`Buffer`] que `rasterize`.
    ///
    /// `caret_char` es un índice de caracteres Unicode, no de bytes. Se convierte
    /// a un offset UTF-8 antes de consultar `LayoutRun::cursor_position`, por lo
    /// que no se recalculan advances glifo a glifo.
    pub fn caret_x(&mut self, text: &str, caret_char: usize, style: &TextStyle) -> Option<f32> {
        let byte_offset = text
            .char_indices()
            .nth(caret_char)
            .map_or(text.len(), |(offset, _)| offset);
        self.with_style_locale(style, |system| {
            let buffer = system.shape_buffer(text, style);
            let glyphs = system.collect_glyphs(&buffer);
            let geometry = system.geometry(&glyphs, style);
            let (line, index) = caret_location(&buffer, byte_offset);
            let cursor = Cursor::new(line, index);
            for run in buffer.layout_runs().filter(|run| run.line_i == line) {
                if let Some(x) = run.cursor_position(&cursor) {
                    let x = x / geometry.supersample as f32
                        + geometry.shift_x / geometry.supersample as f32;
                    return Some(x.clamp(0.0, geometry.width as f32));
                }
            }
            None
        })
    }

    fn normalized_style_language(&self, style: &TextStyle) -> Option<String> {
        let language = style.language.as_deref()?;
        match LocaleId::parse(language) {
            Ok(locale) => Some(locale.to_string()),
            Err(error) => {
                log::warn!(
                    "[text] invalid style language {language:?}: {error}; using current locale {}",
                    self.font_system.locale()
                );
                None
            }
        }
    }

    fn with_style_locale<T>(&mut self, style: &TextStyle, f: impl FnOnce(&mut Self) -> T) -> T {
        let Some(language) = self.normalized_style_language(style) else {
            return f(self);
        };
        if self.font_system.locale() == language {
            return f(self);
        }

        // cosmic-text 0.19 toma el locale de FontSystem, no de Attrs. Una
        // sustitución temporal permite que `style.language` afecte al fallback
        // sin cambiar el locale persistente ni invalidar la caché de imágenes.
        let db = self.font_system.db().clone();
        let replacement = fonts::font_system(language, db);
        let previous = std::mem::replace(&mut self.font_system, replacement);
        let result = f(self);
        self.font_system = previous;
        result
    }

    fn shape_buffer(&mut self, text: &str, style: &TextStyle) -> Buffer {
        let size = finite_positive(style.size, 1.0);
        let line_height = finite_positive(style.line_height, size * 1.25);
        let width = style
            .max_width
            .filter(|width| width.is_finite())
            .map(|width| width.clamp(0.0, MAX_TEXT_DIMENSION));
        let mut buffer = Buffer::new(&mut self.font_system, Metrics::new(size, line_height));
        buffer.set_wrap(style.wrap.cosmic());
        buffer.set_size(width, None);
        buffer.set_text(
            text,
            &attrs(style),
            Shaping::Advanced,
            Some(style.align.cosmic()),
        );
        buffer.shape_until_scroll(&mut self.font_system, false);
        buffer
    }

    fn collect_glyphs(&self, buffer: &Buffer) -> Vec<RenderGlyph> {
        let line_starts = line_starts(buffer);
        let mut glyphs = Vec::new();
        for run in buffer.layout_runs() {
            let line_start = line_starts[run.line_i];
            for layout in run.glyphs {
                let physical = layout.physical((0.0, run.line_y), 1.0);
                glyphs.push(RenderGlyph {
                    layout: layout.clone(),
                    line_y: run.line_y,
                    shaped: ShapedGlyph {
                        cache_key: physical.cache_key,
                        font_id: layout.font_id,
                        glyph_id: layout.glyph_id,
                        x: layout.x,
                        y: layout.y + run.line_y,
                        w: layout.w,
                        start: line_start + layout.start,
                        end: line_start + layout.end,
                        level: layout.level.number(),
                    },
                });
            }
        }
        glyphs
    }

    fn geometry(&mut self, glyphs: &[RenderGlyph], style: &TextStyle) -> Geometry {
        let mut supersample = style.supersample.clamp(1, MAX_SUPERSAMPLE);
        loop {
            let ink = measure_ink(
                &mut self.font_system,
                &mut self.swash_cache,
                glyphs,
                supersample,
            );
            let fallback_height = finite_positive(style.line_height, 1.0);
            let width = output_dimension(
                style.max_width,
                ink.map(|rect| (rect.max_x - rect.min_x) / supersample as f32),
                1.0,
            );
            let height = output_dimension(
                style.max_height,
                ink.map(|rect| (rect.max_y - rect.min_y) / supersample as f32),
                fallback_height,
            );
            let pixels = u64::from(width)
                * u64::from(height)
                * u64::from(supersample)
                * u64::from(supersample);
            if pixels <= MAX_SUPERSAMPLED_PIXELS || supersample == 1 {
                let raster_width = width as f32 * supersample as f32;
                let raster_height = height as f32 * supersample as f32;
                let shift_x = horizontal_shift(style.align, raster_width, ink);
                let shift_y = ink.map_or(raster_height * 0.5, |rect| {
                    (raster_height - (rect.max_y - rect.min_y)) * 0.5 - rect.min_y
                });
                return Geometry {
                    width,
                    height,
                    supersample,
                    shift_x,
                    shift_y,
                };
            }
            supersample /= 2;
        }
    }
}

impl Default for TextSystem {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
struct RenderGlyph {
    layout: cosmic_text::LayoutGlyph,
    line_y: f32,
    shaped: ShapedGlyph,
}

#[derive(Debug, Clone, Copy)]
struct InkRect {
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
}

#[derive(Debug, Clone, Copy)]
struct Geometry {
    width: u32,
    height: u32,
    supersample: u32,
    shift_x: f32,
    shift_y: f32,
}

fn measure_ink(
    font_system: &mut FontSystem,
    cache: &mut SwashCache,
    glyphs: &[RenderGlyph],
    supersample: u32,
) -> Option<InkRect> {
    let mut rect: Option<InkRect> = None;
    for glyph in glyphs {
        let physical = glyph
            .layout
            .physical((0.0, glyph.line_y), supersample as f32);
        let Some(image) = cache.get_image(font_system, physical.cache_key) else {
            continue;
        };
        if image.data.is_empty() || image.placement.width == 0 || image.placement.height == 0 {
            continue;
        }
        let min_x = physical.x as f32 + image.placement.left as f32;
        let min_y = physical.y as f32 - image.placement.top as f32;
        let max_x = min_x + image.placement.width as f32;
        let max_y = min_y + image.placement.height as f32;
        rect = Some(rect.map_or(
            InkRect {
                min_x,
                min_y,
                max_x,
                max_y,
            },
            |current| InkRect {
                min_x: current.min_x.min(min_x),
                min_y: current.min_y.min(min_y),
                max_x: current.max_x.max(max_x),
                max_y: current.max_y.max(max_y),
            },
        ));
    }
    rect
}

fn horizontal_shift(align: TextAlign, raster_width: f32, ink: Option<InkRect>) -> f32 {
    match (align, ink) {
        (TextAlign::Left, Some(rect)) => -rect.min_x,
        (TextAlign::Center, Some(rect)) => {
            (raster_width - (rect.max_x - rect.min_x)) * 0.5 - rect.min_x
        }
        (TextAlign::Right, Some(rect)) => raster_width - (rect.max_x - rect.min_x) - rect.min_x,
        (TextAlign::Left, None) => 0.0,
        (TextAlign::Center, None) => raster_width * 0.5,
        (TextAlign::Right, None) => raster_width,
    }
}

fn output_dimension(limit: Option<f32>, ink: Option<f32>, fallback: f32) -> u32 {
    let logical = limit
        .filter(|value| value.is_finite())
        .unwrap_or_else(|| ink.filter(|value| value.is_finite()).unwrap_or(fallback));
    let rounded = if limit.is_some() {
        logical.clamp(0.0, MAX_TEXT_DIMENSION).round()
    } else {
        logical.clamp(0.0, MAX_TEXT_DIMENSION).ceil()
    };
    rounded.max(0.0) as u32
}

fn finite_positive(value: f32, fallback: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        fallback
    }
}

fn rgba_len(width: u32, height: u32) -> usize {
    width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .and_then(|len| usize::try_from(len).ok())
        .unwrap_or(0)
}

fn line_starts(buffer: &Buffer) -> Vec<usize> {
    let mut start = 0;
    buffer
        .lines
        .iter()
        .map(|line| {
            let line_start = start;
            start += line.text().len() + line.ending().as_str().len();
            line_start
        })
        .collect()
}

fn caret_location(buffer: &Buffer, mut byte_offset: usize) -> (usize, usize) {
    for (line, buffer_line) in buffer.lines.iter().enumerate() {
        let len = buffer_line.text().len();
        if byte_offset <= len {
            return (line, byte_offset.min(len));
        }
        byte_offset = byte_offset.saturating_sub(len + buffer_line.ending().as_str().len());
    }
    let last = buffer.lines.len().saturating_sub(1);
    (last, buffer.lines[last].text().len())
}

fn sanitized_tint(tint: [f32; 4]) -> [u8; 4] {
    let channel = |value: f32| {
        if value.is_finite() {
            (value.clamp(0.0, 1.0) * 255.0).round() as u8
        } else {
            0
        }
    };
    [
        channel(tint[0]),
        channel(tint[1]),
        channel(tint[2]),
        channel(tint[3]),
    ]
}

fn composite_image(
    image: &SwashImage,
    dest_x: i64,
    dest_y: i64,
    width: u32,
    height: u32,
    tint: [u8; 4],
    rgba: &mut [u8],
) {
    let channels = match image.content {
        SwashContent::Mask => 1,
        SwashContent::SubpixelMask | SwashContent::Color => 4,
    };
    let image_width = i64::from(image.placement.width);
    let image_height = i64::from(image.placement.height);
    let start_x = dest_x.max(0);
    let end_x = (dest_x + image_width).min(i64::from(width));
    let start_y = dest_y.max(0);
    let end_y = (dest_y + image_height).min(i64::from(height));
    if start_x >= end_x || start_y >= end_y {
        return;
    }

    for y in start_y..end_y {
        for x in start_x..end_x {
            let source = ((y - dest_y) * image_width + (x - dest_x)) as usize * channels;
            let Some(data) = image.data.get(source..) else {
                continue;
            };
            let src = match image.content {
                SwashContent::Mask => {
                    let alpha = (u32::from(data[0]) * u32::from(tint[3]) / 255) as u8;
                    [tint[0], tint[1], tint[2], alpha]
                }
                SwashContent::SubpixelMask | SwashContent::Color => {
                    let alpha = (u32::from(data[3]) * u32::from(tint[3]) / 255) as u8;
                    [data[0], data[1], data[2], alpha]
                }
            };
            let index = ((y as u32) * width + x as u32) as usize * 4;
            if let Some(dst) = rgba.get_mut(index..index + 4) {
                blend_pixel(dst, src);
            }
        }
    }
}

fn blend_pixel(dst: &mut [u8], src: [u8; 4]) {
    let src_alpha = f32::from(src[3]) / 255.0;
    if src_alpha <= 0.0 {
        return;
    }
    let dst_alpha = f32::from(dst[3]) / 255.0;
    let out_alpha = src_alpha + dst_alpha * (1.0 - src_alpha);
    if out_alpha <= 0.0 {
        return;
    }
    for channel in 0..3 {
        let src = f32::from(src[channel]) * src_alpha;
        let old = f32::from(dst[channel]) * dst_alpha * (1.0 - src_alpha);
        dst[channel] = ((src + old) / out_alpha).round().min(255.0) as u8;
    }
    dst[3] = (out_alpha * 255.0).round().min(255.0) as u8;
}

fn downsample(rgba: &[u8], width: u32, height: u32, supersample: u32) -> Vec<u8> {
    if supersample <= 1 {
        return rgba.to_vec();
    }
    let mut output = vec![0; rgba_len(width / supersample, height / supersample)];
    let scale = usize::try_from(supersample).unwrap_or(1);
    let raster_width = width as usize;
    for y in 0..height / supersample {
        for x in 0..width / supersample {
            let mut alpha = 0.0;
            let mut rgb = [0.0; 3];
            for sy in 0..scale {
                for sx in 0..scale {
                    let index =
                        ((y as usize * scale + sy) * raster_width + x as usize * scale + sx) * 4;
                    let Some(pixel) = rgba.get(index..index + 4) else {
                        continue;
                    };
                    let a = f32::from(pixel[3]) / 255.0;
                    alpha += a;
                    for channel in 0..3 {
                        rgb[channel] += f32::from(pixel[channel]) * a;
                    }
                }
            }
            let count = (scale * scale) as f32;
            let alpha = alpha / count;
            let index = (y * width / supersample + x) as usize * 4;
            if alpha > 0.0 {
                for channel in 0..3 {
                    output[index + channel] = (rgb[channel] / alpha).round().min(255.0) as u8;
                }
            }
            output[index + 3] = (alpha * 255.0).round() as u8;
        }
    }
    output
}
