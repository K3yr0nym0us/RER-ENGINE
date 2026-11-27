//! Detalle ligero de una fuente del proyecto.
//!
//! Se abre desde el nombre en Recursos → Fuentes y reutiliza el selector nativo
//! de «Añadir texto» del editor HUD. Los bytes de la fuente se registran solo en
//! egui para la muestra; no se importa ni se registra nada en el motor.

use std::sync::Arc;

use egui::RichText;
use egui_phosphor::regular as icons;

use crate::editor::app::EditorApp;
use crate::editor::hud_editor;
use crate::editor::i18n::tr;
use crate::editor::theme;

use super::sidebar_left::{FontPreviewCache, SidebarLeftState};
use super::widgets;

const BUILTIN_FALLBACKS: [&str; 5] = [
    "Inter",
    "Noto Arabic",
    "Noto Hebrew",
    "Noto Sans SC",
    "Fira Mono",
];

/// Claves reales de las familias instaladas por `editor::theme`.
const PREVIEW_FALLBACK_FONTS: [&str; 5] = [
    "inter-regular",
    "Noto Sans SC",
    "Noto Sans Hebrew",
    "Noto Sans Arabic",
    "Fira Mono",
];

const MULTILINGUAL_PREVIEW: [(&str, &str); 5] = [
    (
        "Español",
        "El veloz murciélago hindú comía feliz cardillo y kiwi.",
    ),
    ("English", "Sphinx of black quartz, judge my vow."),
    ("العربية", "مرحباً بالعالم، هذا نص عربي."),
    ("中文", "你好，世界！汉字与标点符号。"),
    ("עברית", "שלום עולם, זהו טקסט עברי."),
];

/// Nombre estable y específico de la fuente cuyos bytes se registran en egui.
fn font_preview_family_name(path: &str) -> String {
    format!("rer-font-preview:{path}")
}

/// Evita que egui reciba bytes inválidos, porque su registro es diferido.
fn validate_preview_font(bytes: &[u8]) -> Result<(), String> {
    let mut db = fontdb::Database::new();
    let ids = db.load_font_source(fontdb::Source::Binary(Arc::new(bytes.to_vec())));
    if ids.is_empty() {
        Err("font bytes do not contain a valid font face".to_owned())
    } else {
        Ok(())
    }
}

/// Lee y registra una copia de la fuente solo para la muestra de egui.
fn load_preview_font(ctx: &egui::Context, path: &str) -> Result<String, String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("failed to read preview font {path}: {error}"))?;
    validate_preview_font(&bytes)?;

    let family_name = font_preview_family_name(path);
    let mut definitions = ctx.fonts(|fonts| fonts.definitions().clone());
    definitions.font_data.insert(
        family_name.clone(),
        Arc::new(egui::FontData::from_owned(bytes)),
    );
    let mut family_fonts = vec![family_name.clone()];
    family_fonts.extend(PREVIEW_FALLBACK_FONTS.iter().map(|name| (*name).to_owned()));
    definitions.families.insert(
        egui::FontFamily::Name(family_name.clone().into()),
        family_fonts,
    );
    ctx.set_fonts(definitions);
    Ok(family_name)
}

/// `set_fonts` activa la definición al inicio de la siguiente pasada de egui.
fn preview_family_is_bound(ctx: &egui::Context, family: &str) -> bool {
    ctx.fonts(|fonts| {
        fonts
            .definitions()
            .families
            .contains_key(&egui::FontFamily::Name(family.to_owned().into()))
    })
}

/// Cachea tanto el éxito como el fallo para no leer/registrar en cada frame.
fn ensure_font_preview(
    ctx: &egui::Context,
    state: &mut SidebarLeftState,
    path: &str,
) -> Option<String> {
    if state
        .font_preview
        .as_ref()
        .is_some_and(|cache| cache.path != path)
    {
        clear_font_preview(ctx, state);
    }
    if let Some(cache) = state.font_preview.as_ref()
        && cache.path == path
    {
        return cache
            .family
            .clone()
            .filter(|family| preview_family_is_bound(ctx, family));
    }

    let (family, error) = match load_preview_font(ctx, path) {
        Ok(family) => (Some(family), None),
        Err(error) => {
            log::warn!("[editor] no se pudo cargar la fuente de la muestra: {error}");
            (None, Some(error))
        }
    };
    let active_family = family
        .clone()
        .filter(|family| preview_family_is_bound(ctx, family));
    state.font_preview = Some(FontPreviewCache {
        path: path.to_owned(),
        family,
        error,
    });
    active_family
}

/// Da de baja la fuente temporal para no acumular bytes en las definiciones de egui.
fn clear_font_preview(ctx: &egui::Context, state: &mut SidebarLeftState) {
    let Some(cache) = state.font_preview.take() else {
        return;
    };
    let Some(family) = cache.family else {
        return;
    };

    let mut definitions = ctx.fonts(|fonts| fonts.definitions().clone());
    definitions.font_data.remove(&family);
    definitions
        .families
        .remove(&egui::FontFamily::Name(family.clone().into()));
    ctx.set_fonts(definitions);
}

/// Dibuja el detalle sobre el editor si hay una fuente seleccionada.
pub(crate) fn show(ctx: &egui::Context, app: &mut EditorApp) {
    let Some(asset) = app.left_sidebar.font_details.clone() else {
        return;
    };
    let locale = app.cli.locale;
    let has_hud = hud_editor::new_text_screen(app).is_some();
    let preview_family = ensure_font_preview(ctx, &mut app.left_sidebar, &asset.path);
    let preview_error = app
        .left_sidebar
        .font_preview
        .as_ref()
        .and_then(|cache| cache.error.clone());
    let mut close = false;
    let mut use_in_hud = false;

    let modal = egui::Modal::new(egui::Id::new("editor_font_details_modal"))
        .frame(
            egui::Frame::popup(&ctx.style_of(egui::Theme::Dark))
                .fill(theme::PANEL_BG)
                .stroke(egui::Stroke::new(1.0, theme::BORDER))
                .inner_margin(egui::Margin::same(16)),
        )
        .show(ctx, |ui| {
            ui.set_width(560.0);
            ui.label(
                RichText::new(format!(
                    "{}  {}",
                    icons::TEXT_AA,
                    tr(locale, "Detalle de la fuente", "Font details")
                ))
                .size(15.0)
                .strong()
                .color(theme::TEXT),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new(&asset.name)
                    .size(16.0)
                    .strong()
                    .color(theme::TEXT),
            );
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(tr(locale, "Ruta:", "Path:"))
                        .size(11.0)
                        .color(theme::TEXT_MUTED),
                );
                ui.label(
                    RichText::new(if asset.path.is_empty() { "—" } else { &asset.path })
                        .size(11.0)
                        .color(theme::TEXT_MUTED),
                );
            });
            ui.add_space(8.0);
            ui.separator();

            widgets::caption(
                ui,
                tr(locale, "Fuente primaria del HUD", "Primary HUD font"),
            );
            ui.label(
                RichText::new(tr(
                    locale,
                    "Los nuevos textos HUD creados con esta fuente la usan como fuente primaria.",
                    "New HUD text created with this font uses it as the primary font.",
                ))
                .size(12.0)
                .color(theme::TEXT),
            );
            ui.add_space(6.0);

            widgets::caption(
                ui,
                tr(locale, "Fallback automático integrado", "Built-in automatic fallback"),
            );
            ui.label(
                RichText::new(BUILTIN_FALLBACKS.join(" · "))
                    .size(12.0)
                    .color(theme::INFO),
            );
            widgets::hint(ui, tr(
                locale,
                "Si falta un glifo en la fuente primaria, el HUD usa el fallback integrado. Una fuente personalizada no tiene por qué cubrir todos estos scripts.",
                "If the primary font is missing a glyph, the HUD uses the built-in fallback. A custom font does not necessarily cover all of these scripts.",
            ));
            ui.add_space(8.0);

            widgets::caption(
                ui,
                tr(locale, "Muestra de la fuente primaria", "Primary font preview"),
            );
            widgets::hint(ui, tr(
                locale,
                "Esta muestra usa la fuente seleccionada como primaria. En el HUD, el fallback integrado se aplica automáticamente cuando falta un glifo.",
                "This preview uses the selected font as the primary font. In the HUD, the built-in fallback is applied automatically when a glyph is missing.",
            ));
            if let Some(error) = preview_error {
                let warning = ui.label(
                    RichText::new(format!(
                        "{}  {}",
                        icons::WARNING,
                        tr(
                            locale,
                            "No se pudo cargar la fuente para la muestra; se usa la fuente por defecto.",
                            "Could not load the font for the preview; using the default font.",
                        )
                    ))
                    .size(11.0)
                    .color(theme::WARNING),
                );
                warning.on_hover_text(error);
            }
            egui::Frame::new()
                .fill(theme::ACCORDION_BG)
                .stroke(egui::Stroke::new(1.0, theme::BORDER))
                .corner_radius(4)
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    for (language, sample) in MULTILINGUAL_PREVIEW {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(language)
                                    .size(11.0)
                                    .strong()
                                    .color(theme::TEXT_MUTED),
                            );
                            let rtl = matches!(language, "العربية" | "עברית");
                            ui.with_layout(
                                if rtl {
                                    egui::Layout::right_to_left(egui::Align::Center)
                                } else {
                                    egui::Layout::left_to_right(egui::Align::Center)
                                },
                                |ui| {
                                    let mut text = RichText::new(sample)
                                        .size(15.0)
                                        .color(theme::TEXT);
                                    if let Some(family) = &preview_family {
                                        text = text.family(egui::FontFamily::Name(
                                            family.clone().into(),
                                        ));
                                    }
                                    ui.label(text);
                                },
                            );
                        });
                    }
                });
            if !has_hud {
                ui.add_space(8.0);
                widgets::note(
                    ui,
                    tr(
                        locale,
                        "Crea una pantalla HUD en Interfaz de usuario para usar esta fuente en un texto nuevo.",
                        "Create a HUD screen under User interface to use this font in new text.",
                    ),
                );
            }
            ui.add_space(12.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::modal_button(
                    ui,
                    icons::X,
                    tr(locale, "Cerrar", "Close"),
                    widgets::ModalButtonStyle::Secondary,
                    100.0,
                    true,
                ) {
                    close = true;
                }
                if widgets::modal_button(
                    ui,
                    icons::TEXT_AA,
                    tr(locale, "Usar en HUD", "Use in HUD"),
                    widgets::ModalButtonStyle::Primary,
                    140.0,
                    has_hud,
                ) {
                    use_in_hud = true;
                }
            });
        });

    if modal.should_close() || close {
        app.left_sidebar.font_details = None;
        clear_font_preview(ctx, &mut app.left_sidebar);
    } else if use_in_hud {
        app.left_sidebar.font_details = None;
        clear_font_preview(ctx, &mut app.left_sidebar);
        if !hud_editor::open_add_text_with_font(app, &asset.path) {
            app.push_log(
                tr(
                    locale,
                    "No se pudo abrir el selector de fuente del HUD.",
                    "Could not open the HUD font selector.",
                ),
                true,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn run_test_frame(ctx: &egui::Context, ui: impl FnMut(&mut egui::Ui)) {
        let mut output = ctx.run_ui(egui::RawInput::default(), ui);
        output.textures_delta.clear();
    }

    #[test]
    fn preview_family_name_is_stable_and_path_specific() {
        assert_eq!(
            font_preview_family_name("fonts/My Font.ttf"),
            "rer-font-preview:fonts/My Font.ttf"
        );
        assert_ne!(
            font_preview_family_name("fonts/My Font.ttf"),
            font_preview_family_name("fonts/Other.ttf")
        );
    }

    #[test]
    fn preview_font_validation_accepts_fonts_and_rejects_invalid_bytes() {
        assert!(validate_preview_font(crate::embedded_fonts::INTER_REGULAR).is_ok());
        assert!(validate_preview_font(b"not a font").is_err());
    }

    #[test]
    fn preview_font_load_registers_a_named_family_and_caches_it() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../assets/fonts/Inter_18pt-Regular.ttf")
            .to_string_lossy()
            .into_owned();
        let mut state = SidebarLeftState::default();

        run_test_frame(&ctx, |ui| {
            let ctx = ui.ctx().clone();
            assert!(ensure_font_preview(&ctx, &mut state, &path).is_none());
            let cache = state.font_preview.as_ref().expect("cached preview font");
            let expected = font_preview_family_name(&path);
            assert_eq!(cache.path, path);
            assert_eq!(cache.family.as_deref(), Some(expected.as_str()));
            assert!(cache.error.is_none());
            // La definición queda activa en la siguiente pasada, no en esta.
            assert!(ensure_font_preview(&ctx, &mut state, &path).is_none());
            ui.label(RichText::new("Hola"));
        });

        run_test_frame(&ctx, |ui| {
            let ctx = ui.ctx().clone();
            let family = ensure_font_preview(&ctx, &mut state, &path).expect("active preview font");
            ui.label(RichText::new("Hola").family(egui::FontFamily::Name(family.clone().into())));
            ctx.fonts(|fonts| {
                let definitions = fonts.definitions();
                assert!(definitions.font_data.contains_key(&family));
                let key = egui::FontFamily::Name(family.clone().into());
                assert_eq!(
                    definitions
                        .families
                        .get(&key)
                        .and_then(|fonts| fonts.first()),
                    Some(&family)
                );
            });
        });

        let family = font_preview_family_name(&path);
        run_test_frame(&ctx, |ui| {
            let ctx = ui.ctx().clone();
            clear_font_preview(&ctx, &mut state);
        });
        assert!(state.font_preview.is_none());
        run_test_frame(&ctx, |ui| {
            ui.ctx().fonts(|fonts| {
                let definitions = fonts.definitions();
                assert!(!definitions.font_data.contains_key(&family));
                assert!(
                    !definitions
                        .families
                        .contains_key(&egui::FontFamily::Name(family.clone().into()))
                );
            });
        });
    }

    #[test]
    fn preview_font_load_failure_is_cached_and_uses_default_font() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut state = SidebarLeftState::default();
        let missing = format!(
            "{}/rer-missing-preview-font-{}.ttf",
            std::env::temp_dir().display(),
            std::process::id()
        );

        run_test_frame(&ctx, |ui| {
            let ctx = ui.ctx().clone();
            assert!(ensure_font_preview(&ctx, &mut state, &missing).is_none());
            let cache = state
                .font_preview
                .clone()
                .expect("failed load must also be cached");
            assert_eq!(cache.path, missing);
            assert!(cache.family.is_none());
            assert!(cache.error.is_some());
            assert!(ensure_font_preview(&ctx, &mut state, &missing).is_none());
            assert_eq!(state.font_preview, Some(cache));
        });
    }

    #[test]
    fn multilingual_preview_covers_the_requested_languages() {
        let labels: Vec<_> = MULTILINGUAL_PREVIEW
            .iter()
            .map(|(language, _)| *language)
            .collect();
        assert_eq!(labels, ["Español", "English", "العربية", "中文", "עברית"]);
        assert!(
            MULTILINGUAL_PREVIEW
                .iter()
                .all(|(_, sample)| !sample.is_empty())
        );
    }

    #[test]
    fn fallback_notice_lists_every_builtin_family() {
        assert_eq!(
            BUILTIN_FALLBACKS,
            [
                "Inter",
                "Noto Arabic",
                "Noto Hebrew",
                "Noto Sans SC",
                "Fira Mono"
            ]
        );
    }
}
