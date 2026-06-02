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
    pub z_index: i32,
    pub locked: bool,
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
    /// Relación ancho/alto del marco HUD (proporción bloqueada al redimensionar).
    pub source_aspect: f32,
    pub z_index: i32,
    pub locked: bool,
}

impl PlayerUiButton {
    pub(crate) fn sync_height_for_viewport(&mut self, viewport_w: f32, viewport_h: f32) {
        self.height =
            ndc_height_for_width(self.width, self.source_aspect, viewport_w, viewport_h);
    }
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

#[derive(Clone, Debug)]
pub(crate) struct PlayerUiImage {
    pub id: u32,
    pub image_path: String,
    pub image_name: String,
    pub center_x: f32,
    pub center_y: f32,
    /// Ancho en NDC; el alto se deriva de `source_aspect` y el viewport.
    pub width: f32,
    pub height: f32,
    /// Relación ancho/alto en píxeles del archivo fuente.
    pub source_aspect: f32,
    pub z_index: i32,
    pub locked: bool,
}

impl PlayerUiImage {
    pub(crate) fn sync_height_for_viewport(&mut self, viewport_w: f32, viewport_h: f32) {
        self.height = ndc_height_for_width(self.width, self.source_aspect, viewport_w, viewport_h);
    }

    pub(crate) fn ui_hud_rect(&self) -> UiHudRect {
        UiHudRect {
            center_x: self.center_x,
            center_y: self.center_y,
            width: self.width,
            height: self.height,
        }
    }
}

/// Alto NDC para conservar proporción de píxeles en pantalla.
pub(crate) fn ndc_height_for_width(
    width_ndc: f32,
    source_aspect: f32,
    viewport_w: f32,
    viewport_h: f32,
) -> f32 {
    if source_aspect <= 0.0 || viewport_w <= 0.0 || viewport_h <= 0.0 {
        return width_ndc;
    }
    width_ndc * viewport_h / (viewport_w * source_aspect)
}

/// Relación ancho/alto en píxeles equivalente al rect NDC actual en pantalla.
pub(crate) fn source_aspect_from_ndc_rect(
    width_ndc: f32,
    height_ndc: f32,
    viewport_w: f32,
    viewport_h: f32,
) -> f32 {
    if height_ndc <= 0.0 || viewport_w <= 0.0 || viewport_h <= 0.0 {
        return 1.0;
    }
    (width_ndc * viewport_h / (height_ndc * viewport_w)).max(0.01)
}

pub(crate) fn default_image_width_ndc(source_aspect: f32) -> f32 {
    let _ = source_aspect;
    0.28_f32
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

/// Esquinas para redimensionar elementos HUD.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlayerUiResizeHandle {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// Redimensiona un rectángulo HUD manteniendo `source_aspect` (escala uniforme desde la esquina opuesta).
pub(crate) fn apply_resize_locked_aspect(
    rect: &mut UiHudRect,
    start: UiHudRect,
    handle: PlayerUiResizeHandle,
    start_mouse: [f32; 2],
    cur_mouse: [f32; 2],
    source_aspect: f32,
    viewport_w: f32,
    viewport_h: f32,
    min_width: f32,
) {
    let (sx0, sy0, sx1, sy1) = box_corners(start);
    let min_height = min_width * 0.25;

    let (anchor_x, anchor_y) = match handle {
        PlayerUiResizeHandle::TopLeft => (sx1, sy0),
        PlayerUiResizeHandle::TopRight => (sx0, sy0),
        PlayerUiResizeHandle::BottomLeft => (sx1, sy1),
        PlayerUiResizeHandle::BottomRight => (sx0, sy1),
    };

    let mut free = start;
    apply_resize_free(
        &mut free,
        start,
        handle,
        start_mouse,
        cur_mouse,
        min_width,
        min_height,
    );

    let (fx0, fy0, fx1, fy1) = box_corners(free);
    let mut cand_w = (fx1 - fx0).max(min_width);
    let mut cand_h = (fy1 - fy0).max(min_height);

    if source_aspect > 0.0 && viewport_w > 0.0 && viewport_h > 0.0 {
        let pixel_aspect = viewport_w / viewport_h;
        let w_from_h = cand_h * source_aspect / pixel_aspect;
        let h_from_w = ndc_height_for_width(cand_w, source_aspect, viewport_w, viewport_h);
        if w_from_h >= cand_w {
            cand_w = w_from_h.max(min_width);
            cand_h = ndc_height_for_width(cand_w, source_aspect, viewport_w, viewport_h)
                .max(min_height);
        } else {
            cand_h = h_from_w.max(min_height);
            cand_w = (cand_h * source_aspect / pixel_aspect).max(min_width);
            cand_h = ndc_height_for_width(cand_w, source_aspect, viewport_w, viewport_h)
                .max(min_height);
        }
    }

    let (x0, y0, x1, y1) = match handle {
        PlayerUiResizeHandle::TopLeft => (
            anchor_x - cand_w,
            anchor_y,
            anchor_x,
            anchor_y + cand_h,
        ),
        PlayerUiResizeHandle::TopRight => (
            anchor_x,
            anchor_y,
            anchor_x + cand_w,
            anchor_y + cand_h,
        ),
        PlayerUiResizeHandle::BottomLeft => (
            anchor_x - cand_w,
            anchor_y - cand_h,
            anchor_x,
            anchor_y,
        ),
        PlayerUiResizeHandle::BottomRight => (
            anchor_x,
            anchor_y - cand_h,
            anchor_x + cand_w,
            anchor_y,
        ),
    };

    rect.center_x = (x0 + x1) * 0.5;
    rect.center_y = (y0 + y1) * 0.5;
    rect.width = (x1 - x0).max(min_width);
    rect.height = (y1 - y0).max(min_height);
}

/// Redimensiona un rectángulo HUD sin bloquear proporción (ancho y alto independientes).
pub(crate) fn apply_resize_free(
    rect: &mut UiHudRect,
    start: UiHudRect,
    handle: PlayerUiResizeHandle,
    start_mouse: [f32; 2],
    cur_mouse: [f32; 2],
    min_width: f32,
    min_height: f32,
) {
    let (sx0, sy0, sx1, sy1) = box_corners(start);
    let dx = cur_mouse[0] - start_mouse[0];
    let dy = cur_mouse[1] - start_mouse[1];
    let (mut x0, mut y0, mut x1, mut y1) = (sx0, sy0, sx1, sy1);
    match handle {
        PlayerUiResizeHandle::TopLeft => {
            x0 = (sx0 + dx).min(sx1 - min_width);
            y1 = (sy1 + dy).max(sy0 + min_height);
        }
        PlayerUiResizeHandle::TopRight => {
            x1 = (sx1 + dx).max(sx0 + min_width);
            y1 = (sy1 + dy).max(sy0 + min_height);
        }
        PlayerUiResizeHandle::BottomLeft => {
            x0 = (sx0 + dx).min(sx1 - min_width);
            y0 = (sy0 + dy).min(sy1 - min_height);
        }
        PlayerUiResizeHandle::BottomRight => {
            x1 = (sx1 + dx).max(sx0 + min_width);
            y0 = (sy0 + dy).min(sy1 - min_height);
        }
    }
    rect.center_x = (x0 + x1) * 0.5;
    rect.center_y = (y0 + y1) * 0.5;
    rect.width = (x1 - x0).max(min_width);
    rect.height = (y1 - y0).max(min_height);
}
