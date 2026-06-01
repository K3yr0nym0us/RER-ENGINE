//! Tipos y utilidades compartidas de la UI HUD del jugador (editor 3D).

/// Rectángulo en NDC (centro + tamaño).
#[derive(Clone, Copy, Debug)]
pub(crate) struct UiHudRect {
    pub center_x: f32,
    pub center_y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug)]
pub(crate) struct PlayerUiTextBox {
    pub id: u32,
    pub font_path: String,
    pub font_name: String,
    pub text: String,
    pub center_x: f32,
    pub center_y: f32,
    pub width: f32,
    pub height: f32,
}

pub(crate) trait HasUiHudRect {
    fn ui_hud_rect(&self) -> UiHudRect;
}

impl HasUiHudRect for PlayerUiTextBox {
    fn ui_hud_rect(&self) -> UiHudRect {
        UiHudRect {
            center_x: self.center_x,
            center_y: self.center_y,
            width: self.width,
            height: self.height,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PlayerUiButton {
    pub id: u32,
    pub shape_type: String,
    pub round: f32,
    pub background_color: [f32; 4],
    pub texture_path: Option<String>,
    pub transparency_background: f32,
    pub text: String,
    pub text_color: [f32; 4],
    pub transparency_text: f32,
    pub font_path: String,
    pub font_name: String,
    pub border_color: [f32; 4],
    pub border_weight: f32,
    pub center_x: f32,
    pub center_y: f32,
    pub width: f32,
    pub height: f32,
}

impl HasUiHudRect for PlayerUiButton {
    fn ui_hud_rect(&self) -> UiHudRect {
        UiHudRect {
            center_x: self.center_x,
            center_y: self.center_y,
            width: self.width,
            height: self.height,
        }
    }
}

pub(crate) fn default_button_size_ndc(shape_type: &str) -> (f32, f32) {
    match shape_type {
        "square" => (0.12, 0.12),
        "diamond" | "triangle" | "circle" => (0.14, 0.14),
        _ => (0.36, 0.09),
    }
}

pub(crate) fn parse_hex_color_rgba(hex: &str, alpha_percent: f32) -> [f32; 4] {
    let v = hex.trim().trim_start_matches('#');
    let (r, g, b) = if v.len() == 3 {
        (
            u8::from_str_radix(&v[0..1], 16).unwrap_or(0),
            u8::from_str_radix(&v[1..2], 16).unwrap_or(0),
            u8::from_str_radix(&v[2..3], 16).unwrap_or(0),
        )
    } else if v.len() >= 6 {
        (
            u8::from_str_radix(&v[0..2], 16).unwrap_or(0),
            u8::from_str_radix(&v[2..4], 16).unwrap_or(0),
            u8::from_str_radix(&v[4..6], 16).unwrap_or(0),
        )
    } else {
        (37, 99, 235)
    };
    let a = (alpha_percent.clamp(0.0, 100.0) / 100.0).clamp(0.0, 1.0);
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a]
}

pub(crate) fn box_corners(rect: UiHudRect) -> (f32, f32, f32, f32) {
    let hw = rect.width * 0.5;
    let hh = rect.height * 0.5;
    (
        rect.center_x - hw,
        rect.center_y - hh,
        rect.center_x + hw,
        rect.center_y + hh,
    )
}

pub(crate) const BOX_STACK_GAP: f32 = 0.11;
