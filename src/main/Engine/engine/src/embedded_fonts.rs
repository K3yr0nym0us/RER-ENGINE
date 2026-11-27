//! Fuentes fijas del motor, empaquetadas dentro del binario.
//!
//! El editor y el sistema de texto usan estas fuentes embebidas como base. El HUD
//! del jugador puede registrar fuentes custom del proyecto sobre este fallback.
//! No se leen rutas de fuentes del sistema operativo.
//! Los TTF viven en `Engine/assets/fonts` y se enlazan con `include_bytes!`.
//!
//! IMPORTANTE: estos TTF tienen eliminados de su `cmap` los mapeos del Área de
//! Uso Privado (U+E000..U+F8FF). Inter trae ~315 glifos que solapan con los
//! iconos de Phosphor y, al ser la fuente principal, los sustituían. Si se
//! reemplazan los archivos por los originales del zip hay que volver a quitar
//! esos mapeos, o los iconos del editor cambiarán.

/// Inter regular (optical size 18 pt): fuente de texto por defecto del editor.
#[cfg_attr(not(any(feature = "editor", feature = "engine3d")), allow(dead_code))]
pub(crate) const INTER_REGULAR: &[u8] = include_bytes!("../../assets/fonts/Inter_18pt-Regular.ttf");

/// Inter bold: familia `rer-bold` del editor (`<strong>` y etiquetas).
#[cfg_attr(not(feature = "editor"), allow(dead_code))]
pub(crate) const INTER_BOLD: &[u8] = include_bytes!("../../assets/fonts/Inter_18pt-Bold.ttf");

/// Noto Sans Arabic para shaping avanzado y escritura de derecha a izquierda.
pub(crate) const NOTO_SANS_ARABIC: &[u8] = include_bytes!("../../assets/fonts/NotoSansArabic.ttf");

/// Noto Sans Hebrew para shaping avanzado y texto mixto LTR/RTL.
pub(crate) const NOTO_SANS_HEBREW: &[u8] = include_bytes!("../../assets/fonts/NotoSansHebrew.ttf");

/// Noto Sans SC variable como fallback CJK/Han.
pub(crate) const NOTO_SANS_SC: &[u8] = include_bytes!("../../assets/fonts/NotoSansSC-Variable.ttf");

/// Fira Mono para código y ligaduras tipográficas.
#[cfg_attr(not(feature = "editor"), allow(dead_code))]
pub(crate) const FIRA_MONO: &[u8] = include_bytes!("../../assets/fonts/FiraMono-Medium.ttf");
