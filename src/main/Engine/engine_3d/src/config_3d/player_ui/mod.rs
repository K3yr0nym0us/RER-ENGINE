//! UI HUD del jugador (editor 3D): configuración, texto, botones y render NDC.

pub(crate) mod config;
pub(crate) mod ndc_draw;
pub(crate) mod font;
pub(crate) mod text_render;
pub(crate) mod button_render;
pub(crate) mod text_input;
pub(crate) mod button;
pub(crate) mod image;
pub(crate) mod image_render;
pub(crate) mod hud;
pub(crate) mod hud_layers;
pub(crate) mod hud_props;
pub(crate) mod screens;
pub(crate) mod edit;

pub(crate) use config::{PlayerUiButton, PlayerUiImage, PlayerUiTextBox};
pub(crate) use text_input::PlayerUiTextDrag;
