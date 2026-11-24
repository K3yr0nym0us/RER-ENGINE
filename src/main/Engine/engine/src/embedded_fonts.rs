//! Fuentes fijas del motor, empaquetadas dentro del binario.
//!
//! El editor (egui) y el HUD del jugador usan siempre estas fuentes: no se leen
//! rutas del sistema operativo y no hay ningún ajuste que permita cambiarlas.
//! Los TTF viven en `Engine/assets/fonts` y se enlazan con `include_bytes!`.
//!
//! IMPORTANTE: estos TTF tienen eliminados de su `cmap` los mapeos del Área de
//! Uso Privado (U+E000..U+F8FF). Inter trae ~315 glifos que solapan con los
//! iconos de Phosphor y, al ser la fuente principal, los sustituían. Si se
//! reemplazan los archivos por los originales del zip hay que volver a quitar
//! esos mapeos, o los iconos del editor cambiarán.

use std::sync::Arc;

use ab_glyph::FontArc;

/// Inter regular (optical size 18 pt): fuente de texto por defecto del editor.
pub(crate) const INTER_REGULAR: &[u8] = include_bytes!("../../assets/fonts/Inter_18pt-Regular.ttf");

/// Inter bold: familia `rer-bold` del editor (`<strong>` y etiquetas).
pub(crate) const INTER_BOLD: &[u8] = include_bytes!("../../assets/fonts/Inter_18pt-Bold.ttf");

/// La misma Inter regular para el rasterizador del HUD del jugador (`ab_glyph`).
pub(crate) fn inter_regular_font_arc() -> Option<Arc<FontArc>> {
    FontArc::try_from_vec(INTER_REGULAR.to_vec())
        .ok()
        .map(Arc::new)
}
