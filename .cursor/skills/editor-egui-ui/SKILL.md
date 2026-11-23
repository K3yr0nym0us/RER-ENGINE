---
name: editor-egui-ui
description: >-
  Builds or restyles RER-ENGINE editor views with the project's egui component
  catalog (buttons, switches, sliders, combos, accordions, modal chrome). Use
  when designing a new editor view, modal, sidebar, form, panel, dialog, or
  widget in Rust/egui, or when the user mentions vista, modal, sidebar, botón,
  formulario, egui, o el look del editor.
---

# UI del editor egui

The editor is `rer_engine_editor` (egui). Visuals come from **project widgets**, not stock egui.

## Before writing UI

1. Read `src/main/Engine/engine/src/editor/ui/components/mod.rs` (lookup table).
2. Copy a nearby view of the same kind: sidebar → `ui/sidebar_left/`, confirm modal → `ui/delete_scene_modal.rs`, form modal → `ui/create_scene_modal.rs`.
3. Import:

```rust
use crate::editor::ui::components::{self as ui_kit, ModalButtonStyle};
use crate::editor::theme;
use crate::editor::i18n::tr;
use egui_phosphor::regular as icons;
```

## Mapping

| Need | Call |
|------|------|
| Primary / danger / cancel button | `ui_kit::modal_button(..., ModalButtonStyle::Primary\|Danger\|Secondary, width, enabled)` |
| Wide sidebar button | `ui_kit::wide_button_click_sized` |
| On/off | `ui_kit::switch_value` |
| Slider | `ui_kit::range_slider` |
| Combo | `ui_kit::combo_text` / `combo_u32_labeled` / `combo_string` |
| Text field | `ui_kit::text_field` |
| Number draft | `NumberDraft` + `ui_kit::number_input` |
| Modal chrome | `ui_kit::modal_panel` + `modal_title` + `body_text` |
| Accordion | `ui_kit::accordion_item` + `accordion_header` |
| Tiers | `ui_kit::tier_row_choice` |
| Colors / icons | `theme::*` / `icons::*` |

Never: `ui.button`, `egui::Button`, `Checkbox`, `Slider`, `CollapsingHeader`, `ui.heading`, new `Color32::from_rgb`, nested `egui::Window`.

If the control does not exist, add it in `editor/ui/widgets.rs` and re-export from `components`. Do not inline a one-off lookalike.

Native extra window → `editor/modal_window.rs` (`NativeModal`). Overlay on the viewport → `egui::Modal` like `clear_scene_modal.rs`.

## After editing

Keep identifiers in English. UI strings via `tr(locale, "es", "en")`. Do not restyle existing views unless asked.
