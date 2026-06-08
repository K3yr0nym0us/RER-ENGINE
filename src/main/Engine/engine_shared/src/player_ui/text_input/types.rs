//! Tipos y constantes compartidos para edición de texto HUD.

use std::time::Duration;

use crate::player_ui::config::{PlayerUiImage, PlayerUiResizeHandle, PlayerUiTextBox, UiHudRect};

#[derive(Clone, Debug)]
pub enum PlayerUiTextDrag {
    Move {
        id: u32,
        start_mouse: [f32; 2],
        start_center: [f32; 2],
    },
    Resize {
        id: u32,
        handle: PlayerUiResizeHandle,
        start_mouse: [f32; 2],
        start_box: PlayerUiTextBox,
    },
    ButtonMove {
        id: u32,
        start_mouse: [f32; 2],
        start_center: [f32; 2],
    },
    ButtonResize {
        id: u32,
        handle: PlayerUiResizeHandle,
        start_mouse: [f32; 2],
        start_rect: UiHudRect,
        source_aspect: f32,
    },
    ImageMove {
        id: u32,
        start_mouse: [f32; 2],
        start_center: [f32; 2],
    },
    ImageResize {
        id: u32,
        handle: PlayerUiResizeHandle,
        start_mouse: [f32; 2],
        start_image: PlayerUiImage,
    },
    ObjectMove {
        id: u32,
        start_mouse: [f32; 2],
        start_vertices: Vec<[f32; 2]>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerUiHitTarget {
    Image(u32),
    Button(u32),
    Object(u32),
    Text(u32),
}

pub const DEFAULT_BOX_W: f32 = 0.36;
pub const DEFAULT_BOX_H: f32 = 0.09;
pub const MIN_BOX_W: f32 = 0.06;
pub const MIN_BOX_H: f32 = 0.03;
pub const HANDLE_RADIUS: f32 = 0.016;
pub const DOUBLE_CLICK: Duration = Duration::from_millis(420);
pub const DRAG_THRESHOLD_PX: f32 = 4.0;
pub const CARET_BLINK_PERIOD_MS: u64 = 1060;
pub const CARET_BLINK_ON_MS: u64 = 530;
pub const MAX_TEXT_CHARS: usize = 512;
