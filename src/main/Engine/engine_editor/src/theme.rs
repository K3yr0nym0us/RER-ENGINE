//! Paleta y estilo visual del editor, espejo de `src/renderer/src/styles/index.css`.

use std::sync::Arc;

use egui::Color32;

pub const SIDEBAR_WIDTH: f32 = 268.0;
pub const TOP_BAR_HEIGHT: f32 = 46.0;
pub const MIN_WINDOW_WIDTH: f32 = 900.0;
pub const MIN_WINDOW_HEIGHT: f32 = 600.0;

pub const SIDEBAR_BG: Color32 = Color32::from_rgb(0x09, 0x0b, 0x18);
pub const PANEL_BG: Color32 = Color32::from_rgb(0x1f, 0x22, 0x37);
pub const LOG_BG: Color32 = Color32::from_rgb(0x03, 0x05, 0x0d);
pub const METRICS_BG: Color32 = Color32::from_rgb(0x05, 0x08, 0x17);
pub const METRICS_BORDER: Color32 = Color32::from_rgb(0x20, 0x25, 0x45);
// Colores del FPS en el panel de métricas (React: #4ade80 / #facc15 / #f87171).
pub const METRIC_FPS_GOOD: Color32 = Color32::from_rgb(0x4a, 0xde, 0x80);
pub const METRIC_FPS_WARN: Color32 = Color32::from_rgb(0xfa, 0xcc, 0x15);
pub const METRIC_FPS_BAD: Color32 = Color32::from_rgb(0xf8, 0x71, 0x71);
pub const RANGE_TRACK: Color32 = Color32::from_rgb(0x49, 0x50, 0x57);
pub const SWITCH_OFF: Color32 = Color32::from_rgb(0x49, 0x50, 0x57);
/// Azul por defecto de los controles nativos (range/switch) en Bootstrap.
pub const CONTROL_BLUE: Color32 = Color32::from_rgb(0x0d, 0x6e, 0xfd);
#[allow(dead_code)] // La Fase 3/4 (acordeones) usa estos fondos.
pub const ACCORDION_BG: Color32 = Color32::from_rgb(0x0f, 0x12, 0x25);
pub const ACCORDION_HEADER_BG: Color32 = Color32::from_rgb(0x12, 0x16, 0x2b);
pub const ACCORDION_HEADER_HOVER: Color32 = Color32::from_rgb(0x1a, 0x1f, 0x3a);
pub const ACCORDION_OPEN_BG: Color32 = Color32::from_rgb(0x16, 0x1b, 0x34);
pub const BORDER: Color32 = Color32::from_rgb(0x2c, 0x31, 0x52);
pub const ACCENT: Color32 = Color32::from_rgb(0x03, 0x55, 0xc4);
pub const TEXT: Color32 = Color32::from_rgb(0xee, 0xf2, 0xff);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0xc5, 0xc9, 0xe8);

// Bootstrap: success / info / danger / warning usados por los botones del sidebar.
#[allow(dead_code)] // La Fase 5 muestra avisos de normalización.
pub const WARNING: Color32 = Color32::from_rgb(0xff, 0xc1, 0x07);

// Bootstrap: success / info / danger / warning usados por los botones del sidebar.
pub const SUCCESS: Color32 = Color32::from_rgb(0x19, 0x87, 0x54);
pub const INFO: Color32 = Color32::from_rgb(0x0d, 0xca, 0xf0);
pub const DANGER: Color32 = Color32::from_rgb(0xdc, 0x35, 0x45);
pub const WARNING_BADGE: Color32 = Color32::from_rgb(0xff, 0xc1, 0x07);
pub const WARNING_TEXT: Color32 = Color32::from_rgb(0x0d, 0x0d, 0x1a);
pub const OUTLINE_SECONDARY: Color32 = Color32::from_rgb(0x6c, 0x75, 0x7d);
/// `.btn-outline-light` del CSS: texto blanco y borde `--app-primary-outline`.
pub const PRIMARY_OUTLINE: Color32 = Color32::from_rgb(0x3d, 0x6e, 0xb8);

// Fila activa de escenas (React: border #38bdf8, fondo rgba(56,189,248,.08)).
pub const SCENE_ACTIVE_TEXT: Color32 = Color32::from_rgb(0x38, 0xbd, 0xf8);
pub const SCENE_ACTIVE_BG: Color32 = Color32::from_rgba_unmultiplied_const(56, 189, 248, 20);

// Badge de tipo de proyecto (verde del editor React).
pub const BADGE_GREEN_TEXT: Color32 = Color32::from_rgb(0x34, 0xd3, 0x99);
pub const BADGE_GREEN_BG: Color32 = Color32::from_rgba_unmultiplied_const(0x34, 0xd3, 0x99, 0x18);
pub const BADGE_GREEN_BORDER: Color32 =
    Color32::from_rgba_unmultiplied_const(0x34, 0xd3, 0x99, 0x40);

/// Color de limpieza del swapchain (sRGB la convierte a lineal).
pub fn clear_color() -> wgpu::Color {
    wgpu::Color {
        r: 0.02,
        g: 0.025,
        b: 0.06,
        a: 1.0,
    }
}

pub fn apply(ctx: &egui::Context) {
    install_fonts(ctx);
    ctx.set_theme(egui::Theme::Dark);

    ctx.all_styles_mut(|style| {
        let mut visuals = egui::Visuals::dark();

        visuals.panel_fill = PANEL_BG;
        visuals.window_fill = SIDEBAR_BG;
        visuals.faint_bg_color = ACCORDION_BG;
        visuals.extreme_bg_color = PANEL_BG;
        visuals.override_text_color = Some(TEXT);
        visuals.selection.bg_fill = ACCENT;
        visuals.hyperlink_color = Color32::from_rgb(0x4a, 0x90, 0xe8);
        visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, BORDER);
        visuals.widgets.inactive.bg_fill = ACCORDION_HEADER_BG;
        visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, BORDER);

        style.visuals = visuals;
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        // Animación de despliegue de los acordeones (Bootstrap ~0.35 s; 0.20 s
        // se siente ágil sin ser instantánea).
        style.animation_time = 0.20;

        // Tamaños base alineados con el CSS del renderer (html: 14 px).
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(13.5));
        style
            .text_styles
            .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
        style
            .text_styles
            .insert(egui::TextStyle::Monospace, egui::FontId::monospace(13.0));
    });
}

/// Carga una fuente de sistema si existe y añade la fuente de iconos.
///
/// La fuente de sistema hace que Windows y Linux se vean parecido; los iconos
/// vienen incluidos en el binario vía `egui-phosphor`.
fn install_fonts(ctx: &egui::Context) {
    let candidates: &[&str] = if cfg!(target_os = "windows") {
        &["C:/Windows/Fonts/segoeui.ttf"]
    } else if cfg!(target_os = "linux") {
        &[
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        ]
    } else {
        &[]
    };

    let mut fonts = egui::FontDefinitions::default();
    let mut system_font_loaded = false;

    for path in candidates {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };

        fonts.font_data.insert(
            "rer-system".to_owned(),
            Arc::new(egui::FontData::from_owned(bytes)),
        );

        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            family.insert(0, "rer-system".to_owned());
        }
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
            family.insert(0, "rer-system".to_owned());
        }

        log::info!("[editor] fuente cargada desde {path}");
        system_font_loaded = true;
        break;
    }

    // Familia en negrita para `<strong>` y etiquetas de propiedades.
    let bold_candidates: &[&str] = if cfg!(target_os = "windows") {
        &["C:/Windows/Fonts/segoeuib.ttf"]
    } else if cfg!(target_os = "linux") {
        &[
            "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
            "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
        ]
    } else {
        &[]
    };
    for path in bold_candidates {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        fonts.font_data.insert(
            "rer-bold".to_owned(),
            Arc::new(egui::FontData::from_owned(bytes)),
        );
        let mut family = vec!["rer-bold".to_owned()];
        if system_font_loaded {
            family.push("rer-system".to_owned());
        }
        fonts
            .families
            .insert(egui::FontFamily::Name("rer-bold".into()), family);
        log::info!("[editor] fuente bold cargada desde {path}");
        break;
    }
    if !fonts
        .families
        .contains_key(&egui::FontFamily::Name("rer-bold".into()))
    {
        let fallback = fonts
            .families
            .get(&egui::FontFamily::Proportional)
            .cloned()
            .unwrap_or_default();
        fonts
            .families
            .insert(egui::FontFamily::Name("rer-bold".into()), fallback);
    }

    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
        family.insert(1, "phosphor".to_owned());
    }

    ctx.set_fonts(fonts);

    if !system_font_loaded {
        log::warn!("[editor] no se encontró una fuente de sistema; se usan las de egui");
    }
}
