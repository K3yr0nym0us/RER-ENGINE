//! Contenido de la ventana nativa del editor de HUD.
//!
//! Nombre del HUD editable, acordeones de texto/objetos/imágenes con filas de
//! bloqueo, z-index y borrado, y submodales para elegir fuente/imagen o
//! confirmar borrados. La vista usa el catálogo compartido de componentes UI.

use egui::{Align, Layout, RichText};
use egui_phosphor::regular as icons;
use serde_json::json;

use crate::editor::app::EditorApp;
use crate::editor::cli::Locale;
use crate::editor::hud_editor::{self, HudElement, HudElementKind, HudNested};
use crate::editor::i18n::tr;
use crate::editor::theme;

use super::components as ui_kit;

fn element_label(element: &HudElement, locale: Locale) -> String {
    let label = element
        .text_value
        .as_ref()
        .map_or(element.label.as_str(), |text| text.source_text());
    if label.trim().is_empty() {
        match element.kind {
            HudElementKind::Text => tr(locale, "Empty text", "Empty text").to_string(),
            HudElementKind::Button => tr(locale, "Button", "Button").to_string(),
            HudElementKind::Image => tr(locale, "Empty image", "Empty image").to_string(),
            HudElementKind::Object => format!("{} #{}", tr(locale, "Object", "Object"), element.id),
        }
    } else {
        label.to_string()
    }
}

fn z_index_input(ui: &mut egui::Ui, app: &mut EditorApp, element: &HudElement) {
    let mut committed = None;
    {
        let draft = app.hud_editor.z_drafts.entry(element.id).or_default();
        draft.sync(element.z_index as f32);
        if ui_kit::number_input(ui, 58.0, &mut draft.text) {
            if let Some(value) = draft.parsed() {
                let z = (value.round() as i32).clamp(-999, 9999);
                draft.force(z as f32);
                if z != element.z_index {
                    committed = Some(z);
                }
            } else {
                draft.force(element.z_index as f32);
            }
        }
    }
    if let Some(z) = committed {
        hud_editor::set_props(app, element, json!({ "z_index": z }));
    }
}

fn open_texture_picker(app: &mut EditorApp, element: &HudElement) {
    app.hud_editor.image_draft = element
        .texture_path
        .clone()
        .filter(|path| !path.trim().is_empty())
        .unwrap_or_else(|| {
            app.assets
                .hud_images
                .first()
                .map(|image| image.path.clone())
                .unwrap_or_default()
        });
    app.hud_editor.nested = Some(HudNested::AssignTexture(element.id));
}

fn element_row(ui: &mut egui::Ui, app: &mut EditorApp, element: &HudElement) {
    let locale = app.cli.locale;
    egui::Frame::new()
        .fill(theme::ACCORDION_BG)
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .corner_radius(6)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let controls_width = 154.0;
                let label_width =
                    (ui.available_width() - controls_width - ui.spacing().item_spacing.x).max(40.0);
                ui.add_sized(
                    [label_width, 20.0],
                    egui::Label::new(
                        RichText::new(element_label(element, locale))
                            .size(12.0)
                            .color(theme::TEXT),
                    )
                    .truncate(),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui_kit::solid_icon_button_click(
                        ui,
                        icons::TRASH,
                        tr(locale, "Delete element", "Delete element"),
                        theme::DANGER,
                        true,
                    ) {
                        let id = element.id;
                        let kind = element.kind;
                        let label = element_label(element, locale);
                        app.hud_editor.nested = Some(HudNested::ConfirmDelete(kind, id, label));
                    }

                    ui_kit::tiny(ui, "Z");
                    z_index_input(ui, app, element);

                    let mut locked = element.locked;
                    if ui_kit::small_icon_button_click(
                        ui,
                        if locked {
                            icons::LOCK
                        } else {
                            icons::LOCK_OPEN
                        },
                        if locked {
                            tr(locale, "Unlock element", "Unlock element")
                        } else {
                            tr(
                                locale,
                                "Lock element (cannot move in viewport)",
                                "Lock element (cannot move in viewport)",
                            )
                        },
                    ) {
                        locked = !locked;
                        hud_editor::set_props(app, element, json!({ "locked": locked }));
                    }
                });
            });

            if element.kind == HudElementKind::Object {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui_kit::small_icon_button_click(
                        ui,
                        icons::IMAGE,
                        tr(locale, "Assign texture", "Assign texture"),
                    ) {
                        open_texture_picker(app, element);
                    }
                    if let Some(color) = element.fill_color {
                        let mut rgba = color;
                        if ui.color_edit_button_rgba_unmultiplied(&mut rgba).changed() {
                            hud_editor::set_style(
                                app,
                                element.id,
                                json!({
                                    "fill_color": [rgba[0], rgba[1], rgba[2], rgba[3]],
                                    "live": true,
                                }),
                            );
                        }
                    }
                });
            }
        });
    ui.add_space(4.0);
}

fn group(
    ui: &mut egui::Ui,
    app: &mut EditorApp,
    index: usize,
    icon: &str,
    title: &str,
    add_label: &str,
    empty: &str,
    items: Vec<HudElement>,
    on_add: Option<fn(&mut EditorApp)>,
) {
    let open = app.hud_editor.open_group == index;
    let heading = format!("{title} ({})", items.len());
    let response = ui_kit::accordion_item(
        ui,
        egui::Id::new(("hud_editor_group", index)),
        0,
        open,
        |ui, openness| ui_kit::accordion_header(ui, icon, &heading, open, openness, 0),
        |ui| {
            if let Some(on_add) = on_add {
                let width = ui.available_width();
                if ui_kit::modal_button(
                    ui,
                    icons::PLUS,
                    add_label,
                    ui_kit::ModalButtonStyle::Primary,
                    width,
                    true,
                ) {
                    on_add(app);
                }
                ui.add_space(2.0);
            }
            if items.is_empty() {
                ui_kit::note(ui, empty);
            } else {
                for element in &items {
                    element_row(ui, app, element);
                }
            }
        },
    );
    if response.clicked() {
        app.hud_editor.open_group = if open { usize::MAX } else { index };
    }
    ui.add_space(6.0);
}

fn add_text(app: &mut EditorApp) {
    app.hud_editor.font_draft = app
        .assets
        .fonts
        .first()
        .map(|font| font.path.clone())
        .unwrap_or_default();
    app.hud_editor.nested = Some(HudNested::SelectFont);
}

fn add_image(app: &mut EditorApp) {
    app.hud_editor.image_draft = app
        .assets
        .hud_images
        .first()
        .map(|image| image.path.clone())
        .unwrap_or_default();
    app.hud_editor.nested = Some(HudNested::AddImage);
}

fn add_object(app: &mut EditorApp) {
    let active = !app.hud_editor.object_draw_active;
    hud_editor::toggle_draw(app, active);
}

fn asset_options(app: &EditorApp, is_font: bool) -> Vec<(String, String)> {
    if is_font {
        app.assets
            .fonts
            .iter()
            .map(|entry| (entry.path.clone(), entry.name.clone()))
            .collect()
    } else {
        app.assets
            .hud_images
            .iter()
            .map(|entry| (entry.path.clone(), entry.name.clone()))
            .collect()
    }
}

fn dialog_actions<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) {
    ui.with_layout(Layout::right_to_left(Align::Center), add_contents);
}

fn asset_selector(ui: &mut egui::Ui, app: &mut EditorApp, is_font: bool) {
    let locale = app.cli.locale;
    let options = asset_options(app, is_font);
    let field = if is_font {
        &mut app.hud_editor.font_draft
    } else {
        &mut app.hud_editor.image_draft
    };
    let mut selected = (!field.is_empty()).then(|| field.clone());
    let width = ui.available_width();
    if ui_kit::combo_string(
        ui,
        if is_font {
            "hud_font_combo"
        } else {
            "hud_image_combo"
        },
        width,
        &options,
        &mut selected,
    ) {
        *field = selected.clone().unwrap_or_default();
    }
    if options.is_empty() {
        ui_kit::hint(
            ui,
            tr(
                locale,
                "No hay recursos disponibles.",
                "No assets available.",
            ),
        );
    }
    let has_selection = !field.is_empty();
    ui.add_space(10.0);
    dialog_actions(ui, |ui| {
        if ui_kit::modal_button(
            ui,
            icons::CHECK,
            tr(locale, "Add", "Add"),
            ui_kit::ModalButtonStyle::Primary,
            100.0,
            has_selection,
        ) {
            let path = if is_font {
                app.hud_editor.font_draft.clone()
            } else {
                app.hud_editor.image_draft.clone()
            };
            hud_editor::add_from_asset(app, is_font, &path);
        }
        if ui_kit::modal_button(
            ui,
            icons::X,
            tr(locale, "Cancelar", "Cancel"),
            ui_kit::ModalButtonStyle::Secondary,
            100.0,
            true,
        ) {
            app.hud_editor.nested = None;
        }
    });
}

fn texture_selector(ui: &mut egui::Ui, app: &mut EditorApp, element_id: u32) {
    let locale = app.cli.locale;
    let options = asset_options(app, false);
    let mut selected =
        (!app.hud_editor.image_draft.is_empty()).then(|| app.hud_editor.image_draft.clone());
    let width = ui.available_width();
    if ui_kit::combo_string(ui, "hud_texture_combo", width, &options, &mut selected) {
        app.hud_editor.image_draft = selected.clone().unwrap_or_default();
    }
    if options.is_empty() {
        ui_kit::hint(
            ui,
            tr(
                locale,
                "No hay imágenes disponibles.",
                "No images available.",
            ),
        );
    }
    ui.add_space(10.0);
    dialog_actions(ui, |ui| {
        if ui_kit::modal_button(
            ui,
            icons::CHECK,
            tr(locale, "Assign", "Assign"),
            ui_kit::ModalButtonStyle::Primary,
            100.0,
            !app.hud_editor.image_draft.is_empty(),
        ) {
            let path = app.hud_editor.image_draft.clone();
            hud_editor::set_style(app, element_id, json!({ "texture_path": path }));
            app.hud_editor.nested = None;
        }
        if ui_kit::modal_button(
            ui,
            icons::X,
            tr(locale, "Cancelar", "Cancel"),
            ui_kit::ModalButtonStyle::Secondary,
            100.0,
            true,
        ) {
            app.hud_editor.nested = None;
        }
    });
}

fn nested_modal(ui: &mut egui::Ui, app: &mut EditorApp) {
    let locale = app.cli.locale;
    let Some(nested) = app.hud_editor.nested.clone() else {
        return;
    };
    let (id, title) = match &nested {
        HudNested::SelectFont => (
            "hud_select_font",
            tr(locale, "Add text", "Add text").to_string(),
        ),
        HudNested::AddImage => (
            "hud_add_image",
            tr(locale, "Add image", "Add image").to_string(),
        ),
        HudNested::AssignTexture(_) => (
            "hud_assign_texture",
            tr(locale, "Assign texture", "Assign texture").to_string(),
        ),
        HudNested::ConfirmDelete(_, _, label) => (
            "hud_confirm_delete",
            format!(
                "{}: {label}",
                tr(locale, "Confirm deletion", "Confirm deletion")
            ),
        ),
    };
    let modal = egui::Modal::new(egui::Id::new(id))
        .frame(
            egui::Frame::popup(&ui.ctx().style_of(egui::Theme::Dark))
                .fill(theme::PANEL_BG)
                .stroke(egui::Stroke::new(1.0, theme::BORDER))
                .inner_margin(egui::Margin::same(16)),
        )
        .show(ui, |ui| {
            ui.set_width(320.0);
            ui_kit::modal_title(ui, &title);
            ui.add_space(10.0);
            match &nested {
                HudNested::SelectFont => asset_selector(ui, app, true),
                HudNested::AddImage => asset_selector(ui, app, false),
                HudNested::AssignTexture(element_id) => {
                    texture_selector(ui, app, *element_id);
                }
                HudNested::ConfirmDelete(kind, element_id, _) => {
                    let kind = *kind;
                    let element_id = *element_id;
                    ui_kit::body_text(
                        ui,
                        tr(locale, "¿Eliminar este elemento?", "Delete this element?"),
                    );
                    ui.add_space(12.0);
                    dialog_actions(ui, |ui| {
                        if ui_kit::modal_button(
                            ui,
                            icons::TRASH,
                            tr(locale, "Eliminar", "Delete"),
                            ui_kit::ModalButtonStyle::Danger,
                            110.0,
                            true,
                        ) {
                            let command = match kind {
                                HudElementKind::Text => "remove_player_ui_text_box",
                                HudElementKind::Button => "remove_player_ui_button",
                                HudElementKind::Image => "remove_player_ui_image",
                                HudElementKind::Object => "remove_player_ui_object",
                            };
                            app.send_engine(json!({ "cmd": command, "id": element_id }));
                            app.hud_editor.nested = None;
                        }
                        if ui_kit::modal_button(
                            ui,
                            icons::X,
                            tr(locale, "Cancelar", "Cancel"),
                            ui_kit::ModalButtonStyle::Secondary,
                            100.0,
                            true,
                        ) {
                            app.hud_editor.nested = None;
                        }
                    });
                }
            }
        });
    if modal.should_close() {
        app.hud_editor.nested = None;
    }
}

/// Dibuja el cuerpo en la ventana modal externa y mide su alto real.
pub(crate) fn show_in_window(ui: &mut egui::Ui, app: &mut EditorApp) {
    let locale = app.cli.locale;
    // El motor envía el evento con scope/screen_id; si no los incluye, mostramos
    // la lista completa (comportamiento del sidebar React).
    let matches_screen = (app.hud_elements.scope.is_empty() || app.hud_elements.scope == "player")
        && (app.hud_elements.screen_id.is_empty()
            || app.hud_elements.screen_id == app.hud_editor.screen_id);
    let elements = if matches_screen {
        app.hud_elements.items.clone()
    } else {
        Vec::new()
    };
    let element_ids: std::collections::HashSet<u32> =
        elements.iter().map(|element| element.id).collect();
    let mut groups = std::collections::BTreeMap::new();
    for element in elements {
        groups
            .entry(element.kind)
            .or_insert_with(Vec::new)
            .push(element);
    }
    let empty_text = tr(
        locale,
        "No text elements on this screen.",
        "No text elements on this screen.",
    );
    let empty_image = tr(
        locale,
        "No image elements on this screen.",
        "No image elements on this screen.",
    );
    let empty_object = if app.hud_editor.object_draw_active {
        tr(
            locale,
            "Click to add points; click the first point again to finish (min. 3). Hold Ctrl to snap to grid. Esc cancels.",
            "Click to add points; click the first point again to finish (min. 3). Hold Ctrl to snap to grid. Esc cancels.",
        )
    } else {
        tr(
            locale,
            "No objects on this screen.",
            "No objects on this screen.",
        )
    };

    let mut content_height = 0.0;
    let panel = ui_kit::modal_panel(ui, |ui| {
        // El scroll nunca debe ocupar el espacio del pie fijo con «Guardar».
        const FOOTER_RESERVE: f32 = 56.0;
        let scroll_limit = (ui.available_height() - FOOTER_RESERVE).max(1.0);
        let scroll = egui::ScrollArea::vertical()
            .id_salt("hud_editor_scroll")
            .auto_shrink([true, true])
            .max_height(scroll_limit)
            .show_viewport(ui, |ui, _viewport| {
                ui_kit::caption(ui, tr(locale, "HUD name", "HUD name"));
                ui.horizontal(|ui| {
                    let buttons_width = 68.0;
                    let field_width =
                        (ui.available_width() - buttons_width - ui.spacing().item_spacing.x).max(80.0);
                    let editing_name = app.hud_editor.editing_name;
                    let mut name_draft = app.hud_editor.name_draft.clone();
                    let response = ui_kit::text_field_sized(
                        ui,
                        field_width,
                        &mut name_draft,
                        tr(locale, "HUD name", "HUD name"),
                    );
                    app.hud_editor.name_draft = name_draft;
                    if response.changed() {
                        app.hud_editor.editing_name = true;
                    }
                    if !editing_name {
                        if ui_kit::small_icon_button_click(
                            ui,
                            icons::PENCIL,
                            tr(locale, "Edit name", "Edit name"),
                        ) {
                            app.hud_editor.editing_name = true;
                        }
                    } else if ui_kit::small_icon_button_click(
                        ui,
                        icons::CHECK,
                        tr(locale, "Save changes", "Save changes"),
                    ) {
                        let name = app.hud_editor.name_draft.clone();
                        hud_editor::rename(app, &name);
                        app.hud_editor.editing_name = false;
                    }
                    if app.hud_editor.editing_name
                        && (response.lost_focus()
                            || ui.input(|input| input.key_pressed(egui::Key::Enter)))
                    {
                        let name = app.hud_editor.name_draft.clone();
                        hud_editor::rename(app, &name);
                        app.hud_editor.editing_name = false;
                    }
                });
                ui.add_space(12.0);

                group(
                    ui,
                    app,
                    0,
                    icons::TEXT_AA,
                    tr(locale, "Text", "Text"),
                    tr(locale, "Add text", "Add text"),
                    empty_text,
                    groups.remove(&HudElementKind::Text).unwrap_or_default(),
                    Some(add_text),
                );

                let buttons = groups.remove(&HudElementKind::Button).unwrap_or_default();
                if !buttons.is_empty() {
                    group(
                        ui,
                        app,
                        1,
                        icons::CURSOR_CLICK,
                        tr(locale, "Buttons", "Buttons"),
                        "",
                        "",
                        buttons,
                        None,
                    );
                }

                group(
                    ui,
                    app,
                    2,
                    icons::SQUARE,
                    tr(locale, "Object", "Object"),
                    if app.hud_editor.object_draw_active {
                        tr(locale, "Cancel drawing", "Cancel drawing")
                    } else {
                        tr(locale, "Add object", "Add object")
                    },
                    empty_object,
                    groups.remove(&HudElementKind::Object).unwrap_or_default(),
                    Some(add_object),
                );
                group(
                    ui,
                    app,
                    3,
                    icons::IMAGE,
                    tr(locale, "Image", "Image"),
                    tr(locale, "Add image", "Add image"),
                    empty_image,
                    groups.remove(&HudElementKind::Image).unwrap_or_default(),
                    Some(add_image),
                );

                ui_kit::hint(
                    ui,
                    tr(
                        locale,
                        "Double-click a text box in the viewport to edit. Backspace removes characters. Hold Ctrl while dragging to snap to the grid.",
                        "Double-click a text box in the viewport to edit. Backspace removes characters. Hold Ctrl while dragging to snap to the grid.",
                    ),
                );
            });
        let scroll_height = scroll.content_size.y;

        let footer_start = ui.cursor().min.y;
        ui.separator();
        ui.add_space(8.0);
        let save_width = ui.available_width();
        if ui_kit::modal_button(
            ui,
            icons::FLOPPY_DISK,
            tr(locale, "Guardar", "Save"),
            ui_kit::ModalButtonStyle::Primary,
            save_width,
            true,
        ) {
            hud_editor::end_edit(app);
        }
        let footer_height = ui.cursor().min.y - footer_start;
        content_height = scroll_height + footer_height + 32.0;
    });

    // Si el contenido cabe, el panel ya es la medida exacta. Si quedó limitado
    // por la ventana actual, conservamos la altura deseada para que el host
    // pueda crecer hasta su tope y dejar scroll solo cuando sea necesario.
    app.hud_editor.content_height = content_height.max(panel.response.rect.height());
    app.hud_editor
        .z_drafts
        .retain(|id, _| element_ids.contains(id));
    // Las listas se consumieron arriba; los drafts se limpian al cerrar la edición.
    if !app.hud_editor.open {
        app.hud_editor.z_drafts.clear();
    }

    nested_modal(ui, app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::cli::Cli;

    fn run_frame(ctx: &egui::Context, app: &mut EditorApp) {
        let _ = run_frame_with_height(ctx, app, 560.0);
    }

    fn run_frame_with_height(
        ctx: &egui::Context,
        app: &mut EditorApp,
        height: f32,
    ) -> egui::FullOutput {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(420.0, height),
            )),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| show_in_window(ui, app));
        output.textures_delta.clear();
        output
    }

    fn text_bottom(shapes: &[egui::epaint::ClippedShape], needle: &str) -> Option<f32> {
        shapes
            .iter()
            .find_map(|clipped| shape_text_bottom(&clipped.shape, needle))
    }

    fn shape_text_bottom(shape: &egui::Shape, needle: &str) -> Option<f32> {
        match shape {
            egui::Shape::Text(text) if text.galley.job.text.contains(needle) => {
                Some(text.pos.y + text.galley.size().y)
            }
            egui::Shape::Vec(children) => children
                .iter()
                .find_map(|child| shape_text_bottom(child, needle)),
            _ => None,
        }
    }

    #[test]
    fn texture_picker_preserves_the_elements_current_texture() {
        let mut app = EditorApp::new(Cli::default());
        let element = HudElement {
            kind: HudElementKind::Object,
            id: 7,
            label: "Object".to_owned(),
            text_value: None,
            z_index: 0,
            locked: false,
            fill_color: None,
            texture_path: Some("textures/current.png".to_owned()),
        };

        open_texture_picker(&mut app, &element);

        assert_eq!(app.hud_editor.image_draft, "textures/current.png");
        assert!(matches!(
            app.hud_editor.nested,
            Some(HudNested::AssignTexture(7))
        ));
    }

    #[test]
    fn open_texture_dialog_does_not_reset_the_selection_each_frame() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut app = EditorApp::new(Cli::default());
        app.hud_editor.open = true;
        app.hud_editor.screen_id = "main".to_owned();
        app.hud_editor.image_draft = "textures/chosen.png".to_owned();
        app.hud_editor.nested = Some(HudNested::AssignTexture(7));

        run_frame(&ctx, &mut app);
        run_frame(&ctx, &mut app);

        assert_eq!(app.hud_editor.image_draft, "textures/chosen.png");
    }

    #[test]
    fn overflowing_hud_keeps_save_visible_above_the_scroll_area() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut app = EditorApp::new(Cli::default());
        app.hud_editor.open = true;
        app.hud_editor.screen_id = "main".to_owned();
        app.hud_editor.name_draft = "Main HUD".to_owned();
        app.hud_elements.items = (0..40)
            .map(|id| HudElement {
                kind: HudElementKind::Text,
                id,
                label: format!("Text {id}"),
                text_value: Some(rer_engine_core::i18n::TextValue::literal(format!(
                    "Text {id}"
                ))),
                z_index: id as i32,
                locked: false,
                fill_color: None,
                texture_path: None,
            })
            .collect();

        let output = run_frame_with_height(&ctx, &mut app, 300.0);
        let save_bottom =
            text_bottom(&output.shapes, "Save").expect("el botón Guardar debe pintarse");

        assert!(app.hud_editor.content_height > 300.0);
        assert!(
            save_bottom <= 300.0,
            "Guardar quedó fuera del viewport: {save_bottom}"
        );
    }

    #[test]
    fn compact_hud_body_reports_content_height_instead_of_fixed_window_height() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut app = EditorApp::new(Cli::default());
        app.hud_editor.open = true;
        app.hud_editor.screen_id = "main".to_owned();
        app.hud_editor.name_draft = "Main HUD".to_owned();

        run_frame(&ctx, &mut app);

        assert!(app.hud_editor.content_height.is_finite());
        assert!(app.hud_editor.content_height > 120.0);
        assert!(app.hud_editor.content_height < 560.0);
    }
}
