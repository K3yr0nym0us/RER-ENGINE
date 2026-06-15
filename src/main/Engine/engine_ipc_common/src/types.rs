use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Clone)]
pub struct AnimationFrameData {
    pub path: String,
    #[serde(default)]
    pub pivot_x: Option<f32>,
    #[serde(default)]
    pub pivot_y: Option<f32>,
    #[serde(default)]
    pub src_x: Option<u32>,
    #[serde(default)]
    pub src_y: Option<u32>,
    #[serde(default)]
    pub src_w: Option<u32>,
    #[serde(default)]
    pub src_h: Option<u32>,
}

impl AnimationFrameData {
    pub fn resolved_pivot(&self, fallback_w: u32, fallback_h: u32) -> (f32, f32) {
        if let (Some(x), Some(y)) = (self.pivot_x, self.pivot_y) {
            return (x, y);
        }
        let w = self.src_w.unwrap_or(fallback_w).max(1) as f32;
        let h = self.src_h.unwrap_or(fallback_h).max(1) as f32;
        (w * 0.5, h)
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct AnimScriptData {
    pub name:   String,
    pub source: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ControlScriptData {
    pub name:   String,
    pub source: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ControlBindingsData {
    #[serde(default)]
    pub keyboard_mouse: HashMap<String, ControlScriptData>,
    #[serde(default)]
    pub gamepad: HashMap<String, ControlScriptData>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct EntityRestoreTransform {
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub scale:    [f32; 3],
}

#[derive(Debug, Deserialize, Clone)]
pub struct EntityRestorePhysics {
    pub enabled:   bool,
    pub body_type: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct EntityRestoreAnimation {
    pub name:   String,
    pub frames: Vec<AnimationFrameData>,
    pub fps:    u32,
    #[serde(alias = "loop")]
    pub loop_:  bool,
    #[serde(default)]
    pub flip_horizontal: bool,
    #[serde(default)]
    pub audio_path: Option<String>,
    #[serde(default)]
    pub scripts: Vec<AnimScriptData>,
    #[serde(default)]
    pub is_cancelable: bool,
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct EntityRestoreScript {
    pub path:   String,
    pub source: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PlayerUiScreenInfo {
    pub id:      String,
    pub name:    String,
    pub active:  bool,
}

/// Payload JSON de `add_player_ui_button` (idéntico en 2D y 3D).
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct AddPlayerUiButtonPayload {
    #[serde(rename = "type")]
    pub shape_type: String,
    pub round: f32,
    #[serde(rename = "backgroundColor", alias = "background_color")]
    pub background_color: String,
    #[serde(rename = "texturePath", alias = "texture_path", default)]
    pub texture_path: Option<String>,
    #[serde(rename = "transparencyBackground", alias = "transparency_background")]
    pub transparency_background: f32,
    pub text: String,
    #[serde(rename = "textColor", alias = "text_color")]
    pub text_color: String,
    #[serde(rename = "transparencyText", alias = "transparency_text")]
    pub transparency_text: f32,
    #[serde(rename = "fontPath", alias = "font_path")]
    pub font_path: String,
    #[serde(rename = "fontName", alias = "font_name", default)]
    pub font_name: String,
    #[serde(rename = "borderColor", alias = "border_color")]
    pub border_color: String,
    #[serde(rename = "borderWeight", alias = "border_weight")]
    pub border_weight: f32,
}
