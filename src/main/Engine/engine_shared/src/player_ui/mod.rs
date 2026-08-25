//! Lógica compartida de Player UI entre `engine_2d` y `engine_3d`.

pub mod config;
pub mod geometry;
pub mod grid;
pub mod hud_image_asset;
pub mod hud_layers;
pub mod ndc_draw;
pub mod screen_hud_atlas;
pub mod text_input;
pub mod viewport;

pub use config::{
    BOX_STACK_GAP, HasUiHudRect, PlayerUiButton, PlayerUiImage, PlayerUiObject,
    PlayerUiResizeHandle, PlayerUiTextBox, UiHudRect, apply_resize_free,
    apply_resize_locked_aspect, box_corners, default_button_size_ndc, default_image_width_ndc,
    ndc_height_for_width, parse_hex_color_rgba, source_aspect_from_ndc_rect,
};
pub use geometry::{point_in_polygon, polygon_centroid};
pub use grid::{
    NDC_SCREEN_UNIFORM, PLAYER_UI_GRID_DIVISIONS, player_ui_grid_steps, snap_ndc_point_to_grid,
    snap_ui_hud_rect_to_grid,
};
pub use hud_image_asset::{HudImageAssetMeta, probe_image_dimensions, validate_hud_image_file};
pub use hud_layers::{
    HudLayerKind, HudLayerRef, hud_draw_order, hud_hit_test_order, next_z_index_for_screen,
};
pub use ndc_draw::{
    NdcVertex, append_rect_fill, append_rect_outline, push_handle_disc, push_line_segment,
    push_quad,
};
pub use screen_hud_atlas::{
    ScreenHudAtlas, ScreenHudBottomLeftLayout, ScreenHudPackedImage, ndc_transform_bottom_left,
    pick_localized_screen_hud,
};
pub use viewport::{ndc_transform_top_left, pixel_to_ndc};
