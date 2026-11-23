//! Catálogo de UI del editor egui.
//!
//! **Lee este módulo antes de pintar una vista nueva.** El look del editor no
//! sale de `egui::Button` / `Slider` / `Checkbox` / `CollapsingHeader` / `ui.heading`:
//! sale de estas funciones y de `crate::editor::theme`.
//!
//! Import recomendado:
//! ```ignore
//! use crate::editor::ui::components::{self as ui_kit, ModalButtonStyle};
//! use crate::editor::theme;
//! use egui_phosphor::regular as icons;
//! ```
//!
//! # Qué usar
//!
//! | Necesitas | Usa | Nunca |
//! |-----------|-----|--------|
//! | Botón de modal (crear, aplicar, cancelar, borrar) | [`modal_button`] / [`modal_button_bold`] + [`ModalButtonStyle`] | `egui::Button`, `ui.button` |
//! | Botón ancho de sidebar | [`wide_button_click_sized`] | `egui::Button` |
//! | Botón de icono 30×30 | [`small_icon_button_click`] | `egui::Button` |
//! | Botón de icono sólido | [`solid_icon_button_click`] | `egui::Button` relleno a mano |
//! | Icono de fila (escenas, assets) | [`icon_action`] | `egui::Button` |
//! | Pastilla / tier (Low/Med/High) | [`pill_button_click`] / [`tier_row_choice`] | `RadioButton`, `SelectableLabel` |
//! | Interruptor on/off | [`switch_value`] | `egui::Checkbox` |
//! | Toggle cuadrado con icono | [`icon_toggle_value`] | `egui::Button` |
//! | Slider | [`range_slider`] / [`range_slider_no_tooltip`] + [`range_end_labels`] | `egui::Slider` |
//! | Combo de texto / FPS / rutas | [`combo_text`] / [`combo_u32_labeled`] / [`combo_string`] | `egui::ComboBox` suelto |
//! | Campo de texto | [`text_field`] / [`text_field_sized`] | `TextEdit` crudo |
//! | Campo numérico (draft) | [`number_input`] + [`NumberDraft`] | `DragValue`, `Slider` |
//! | Título / cuerpo de modal | [`modal_title`] / [`body_text`] | `ui.heading`, `ui.colored_label` |
//! | Caption / nota / hint | [`caption`] / [`note`] / [`hint`] / [`tiny`] / [`label_value`] | `RichText` ad-hoc |
//! | Marco de modal nativa | [`modal_panel`] | `Frame` con colores inventados |
//! | Pestaña de modal | [`modal_tab`] | `egui::Button` subrayado a mano |
//! | Acordeón de sidebar | [`accordion_item`] + [`accordion_header`] | `CollapsingHeader` |
//! | Fila de tarjetas de distinta altura | [`top_row`] | `ui.horizontal` (escalera) |
//! | Tarjeta de herramienta | [`tool_card_click`] | `egui::Button` grande |
//! | Aviso sobre el viewport | [`viewport_notification`] | `egui::Window` flotante |
//! | Idioma | [`language_button`] | botón genérico |
//! | Color | `crate::editor::theme::*` | `Color32::from_rgb` nuevo |
//! | Icono | `egui_phosphor::regular` | emoji, FontAwesome, `egui::include_image` |
//!
//! # Dónde vive cada cosa
//!
//! - Controles: `editor/ui/widgets.rs` (reexportados aquí).
//! - Acordeón: `editor/ui/accordion.rs`.
//! - Draft numérico: `editor/ui/number_draft.rs`.
//! - Paleta: `editor/theme.rs`.
//! - Modal nativa (ventana winit): `editor/modal_window.rs` + `NativeModal`.
//! - Modal overlay sobre el viewport: `egui::Modal` como en `clear_scene_modal`.
//!
//! # Layout permitido de egui
//!
//! `horizontal`, `vertical`, `ScrollArea`, `Grid`, `CentralPanel`, `Ui::add_space`,
//! `separator` y painter para gizmos únicos. El widget visual no.

// Reexportes del catálogo: el uso vive en las vistas, no en este módulo.
#![allow(unused_imports)]

pub use super::number_draft::{NumberDraft, format_number};
pub use super::widgets::*;

pub use super::accordion::{header as accordion_header, item as accordion_item};
