//! Contenido de la ventana nativa del editor de HUD.
//!
//! Réplica del `PlayerUiEditorPanel`: nombre del HUD editable, acordeones de
//! texto/objetos/imágenes con filas de bloqueo, z-index y borrado, y submodales
//! para elegir fuente/imagen y confirmar borrados. Sin Electron ni React.

use egui::RichText;
use egui_phosphor::regular as icons;
use serde_json::json;

use crate::editor::app::EditorApp;
use crate::editor::cli::Locale;
use crate::editor::hud_editor::{self, HudElement, HudElementKind, HudNested};
use crate::editor::i18n::tr;
use crate::editor::theme;

use super::widgets;

fn element_label(element: &HudElement, locale: Locale) -> String {
    if element.label.trim().is_empty() {
        match element.kind {
            HudElementKind::Text => tr(locale, "Empty text", "Empty text").to_string(),
            HudElementKind::Button => tr(locale, "Button", "Button").to_string(),
            HudElementKind::Image => tr(locale, "Empty image", "Empty image").to_string(),
            HudElementKind::Object => format!("{} #{}", tr(locale, "Object", "Object"), element.id),
        }
    } else {
        element.label.clone()
    }
}

fn element_row(ui: &mut egui::Ui, app: &mut EditorApp, element: &HudElement) {
    let locale = app.cli.locale;
    egui::Frame::new()
        .fill(theme::PANEL_BG)
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(element_label(element, locale))
                        .size(12.0)
                        .color(theme::TEXT),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::icon_action(
                        ui,
                        icons::TRASH,
                        14.0,
                        theme::DANGER,
                        &tr(locale, "Delete element", "Delete element"),
                    ) {
                        let id = element.id;
                        let kind = element.kind;
                        let label = element_label(element, locale);
                        app.hud_editor.nested = Some(HudNested::ConfirmDelete(kind, id, label));
                    }

                    let mut z = element.z_index;
                    ui.add(
                        egui::DragValue::new(&mut z)
                            .speed(1.0)
                            .range(-999..=9999)
                            .custom_formatter(|value, _| format!("{:.0}", value)),
                    );
                    if z != element.z_index {
                        hud_editor::set_props(app, element, json!({ "z_index": z }));
                    }

                    let mut locked = element.locked;
                    if widgets::icon_action(
                        ui,
                        if locked {
                            icons::LOCK
                        } else {
                            icons::LOCK_OPEN
                        },
                        14.0,
                        if locked {
                            theme::WARNING
                        } else {
                            theme::TEXT_MUTED
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
                ui.horizontal(|ui| {
                    if ui
                        .button(icons::IMAGE)
                        .on_hover_text(tr(locale, "Assign texture", "Assign texture"))
                        .clicked()
                    {
                        app.hud_editor.nested = Some(HudNested::AssignTexture(element.id));
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
    title: &str,
    add_label: &str,
    empty: &str,
    items: Vec<HudElement>,
    on_add: fn(&mut EditorApp),
) {
    let open = app.hud_editor.open_group == index;
    egui::Frame::new()
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .corner_radius(4)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .button(if open {
                        icons::CARET_DOWN
                    } else {
                        icons::CARET_RIGHT
                    })
                    .clicked()
                {
                    app.hud_editor.open_group = if open { usize::MAX } else { index };
                }
                ui.label(RichText::new(title).size(12.0).strong().color(theme::TEXT));
                ui.label(
                    RichText::new(format!("({})", items.len()))
                        .size(11.0)
                        .color(theme::TEXT_MUTED),
                );
            });
            if open {
                ui.add_space(4.0);
                if widgets::modal_button(
                    ui,
                    icons::PLUS,
                    add_label,
                    widgets::ModalButtonStyle::Primary,
                    130.0,
                    true,
                ) {
                    on_add(app);
                }
                ui.add_space(6.0);
                if items.is_empty() {
                    ui.label(
                        RichText::new(empty)
                            .size(11.0)
                            .italics()
                            .color(theme::TEXT_MUTED),
                    );
                } else {
                    for element in &items {
                        element_row(ui, app, element);
                    }
                }
            }
        });
    ui.add_space(4.0);
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

fn asset_selector(ui: &mut egui::Ui, app: &mut EditorApp, is_font: bool) {
    let locale = app.cli.locale;
    let entries: Vec<_> = if is_font {
        app.assets.fonts.clone()
    } else {
        app.assets.hud_images.clone()
    };
    let field = if is_font {
        &mut app.hud_editor.font_draft
    } else {
        &mut app.hud_editor.image_draft
    };
    egui::ComboBox::from_id_salt(if is_font {
        "hud_font_combo"
    } else {
        "hud_image_combo"
    })
    .selected_text(
        entries
            .iter()
            .find(|entry| entry.path == *field)
            .map(|entry| entry.name.clone())
            .unwrap_or_else(|| field.clone()),
    )
    .width(ui.available_width())
    .show_ui(ui, |ui| {
        for entry in &entries {
            ui.selectable_value(field, entry.path.clone(), &entry.name);
        }
    });
    ui.add_space(8.0);
    if widgets::modal_button(
        ui,
        icons::CHECK,
        &tr(locale, "Add", "Add"),
        widgets::ModalButtonStyle::Primary,
        100.0,
        !field.is_empty(),
    ) {
        let path = field.clone();
        hud_editor::add_from_asset(app, is_font, &path);
    }
    ui.add_space(6.0);
    if widgets::modal_button(
        ui,
        icons::X,
        &tr(locale, "Cancelar", "Cancel"),
        widgets::ModalButtonStyle::Secondary,
        100.0,
        true,
    ) {
        app.hud_editor.nested = None;
    }
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
    let mut cancel = false;
    let modal = egui::Modal::new(egui::Id::new(id))
        .frame(
            egui::Frame::popup(&ui.ctx().style_of(egui::Theme::Dark))
                .fill(theme::PANEL_BG)
                .stroke(egui::Stroke::new(1.0, theme::BORDER))
                .inner_margin(egui::Margin::same(12)),
        )
        .show(ui, |ui| {
            ui.set_width(280.0);
            ui.label(RichText::new(&title).size(13.0).strong().color(theme::TEXT));
            ui.add_space(8.0);
            match &nested {
                HudNested::SelectFont => asset_selector(ui, app, true),
                HudNested::AddImage => asset_selector(ui, app, false),
                HudNested::AssignTexture(element_id) => {
                    let element_id = *element_id;
                    app.hud_editor.image_draft = app
                        .assets
                        .hud_images
                        .first()
                        .map(|image| image.path.clone())
                        .unwrap_or_default();
                    let entries = app.assets.hud_images.clone();
                    let field = &mut app.hud_editor.image_draft;
                    egui::ComboBox::from_id_salt("hud_texture_combo")
                        .selected_text(
                            entries
                                .iter()
                                .find(|entry| entry.path == *field)
                                .map(|entry| entry.name.clone())
                                .unwrap_or_else(|| field.clone()),
                        )
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            for entry in &entries {
                                ui.selectable_value(field, entry.path.clone(), &entry.name);
                            }
                        });
                    ui.add_space(8.0);
                    if widgets::modal_button(
                        ui,
                        icons::CHECK,
                        &tr(locale, "Assign", "Assign"),
                        widgets::ModalButtonStyle::Primary,
                        100.0,
                        !field.is_empty(),
                    ) {
                        let path = field.clone();
                        hud_editor::set_style(app, element_id, json!({ "texture_path": path }));
                        app.hud_editor.nested = None;
                    }
                    ui.add_space(6.0);
                    if widgets::modal_button(
                        ui,
                        icons::X,
                        &tr(locale, "Cancelar", "Cancel"),
                        widgets::ModalButtonStyle::Secondary,
                        100.0,
                        true,
                    ) {
                        cancel = true;
                    }
                }
                HudNested::ConfirmDelete(kind, element_id, _) => {
                    let kind = *kind;
                    let element_id = *element_id;
                    ui.label(
                        RichText::new(tr(
                            locale,
                            "¿Eliminar este elemento?",
                            "Delete this element?",
                        ))
                        .size(12.0)
                        .color(theme::TEXT),
                    );
                    ui.add_space(10.0);
                    if widgets::modal_button(
                        ui,
                        icons::TRASH,
                        &tr(locale, "Eliminar", "Delete"),
                        widgets::ModalButtonStyle::Danger,
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
                    ui.add_space(6.0);
                    if widgets::modal_button(
                        ui,
                        icons::X,
                        &tr(locale, "Cancelar", "Cancel"),
                        widgets::ModalButtonStyle::Secondary,
                        100.0,
                        true,
                    ) {
                        cancel = true;
                    }
                }
            }
        });
    if modal.should_close() || cancel {
        app.hud_editor.nested = None;
    }
}

/// Dibuja el cuerpo en la ventana modal externa.
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

    egui::Frame::new()
        .fill(theme::PANEL_BG)
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("hud_editor_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(tr(locale, "HUD name", "HUD name"))
                            .size(11.0)
                            .color(theme::TEXT_MUTED),
                    );
                    ui.horizontal(|ui| {
                        ui.add_enabled(
                            app.hud_editor.editing_name,
                            egui::TextEdit::singleline(&mut app.hud_editor.name_draft)
                                .desired_width(ui.available_width() - 30.0),
                        );
                        if !app.hud_editor.editing_name {
                            if widgets::icon_action(ui, icons::PENCIL, 16.0, theme::INFO,
                                &tr(locale, "Edit name", "Edit name"))
                            {
                                app.hud_editor.editing_name = true;
                            }
                        } else if widgets::icon_action(ui, icons::CHECK, 16.0, theme::SUCCESS,
                            &tr(locale, "Save changes", "Save changes"))
                        {
                            let name = app.hud_editor.name_draft.clone();
                            hud_editor::rename(app, &name);
                            app.hud_editor.editing_name = false;
                        }
                    });
                    ui.add_space(10.0);

                    group(ui, app, 0, &tr(locale, "Text", "Text"),
                        &tr(locale, "Add text", "Add text"), &empty_text,
                        groups.remove(&HudElementKind::Text).unwrap_or_default(), add_text);
                    group(ui, app, 1, &tr(locale, "Object", "Object"),
                        &if app.hud_editor.object_draw_active {
                            tr(locale, "Cancel drawing", "Cancel drawing")
                        } else {
                            tr(locale, "Add object", "Add object")
                        },
                        &empty_object,
                        groups.remove(&HudElementKind::Object).unwrap_or_default(), add_object);
                    group(ui, app, 2, &tr(locale, "Image", "Image"),
                        &tr(locale, "Add image", "Add image"), &empty_image,
                        groups.remove(&HudElementKind::Image).unwrap_or_default(), add_image);

                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(tr(locale,
                            "Double-click a text box in the viewport to edit. Backspace removes characters. Hold Ctrl while dragging to snap to the grid.",
                            "Double-click a text box in the viewport to edit. Backspace removes characters. Hold Ctrl while dragging to snap to the grid."))
                            .size(10.0)
                            .color(theme::TEXT_MUTED),
                    );

                    ui.add_space(12.0);
                    if widgets::modal_button(ui, icons::FLOPPY_DISK,
                        &tr(locale, "Guardar", "Save"), widgets::ModalButtonStyle::Primary, 300.0, true)
                    {
                        hud_editor::end_edit(app);
                    }
                });
        });

    nested_modal(ui, app);
}
