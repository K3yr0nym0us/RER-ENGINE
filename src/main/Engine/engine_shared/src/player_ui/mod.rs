//! Lógica compartida de Player UI entre `engine_2d` y `engine_3d`.

pub mod config;
pub mod geometry;
pub mod text_input;
pub mod grid;
pub mod hud_image_asset;
pub mod hud_layers;
pub mod ndc_draw;
pub mod screen_hud_atlas;
pub mod viewport;

pub use config::{
    apply_resize_free, apply_resize_locked_aspect, box_corners, default_button_size_ndc,
    default_image_width_ndc, ndc_height_for_width, parse_hex_color_rgba,
    source_aspect_from_ndc_rect, HasUiHudRect, PlayerUiButton, PlayerUiImage, PlayerUiObject,
    PlayerUiResizeHandle, PlayerUiTextBox, UiHudRect, BOX_STACK_GAP,
};
pub use grid::{
    snap_ndc_point_to_grid, snap_ui_hud_rect_to_grid, player_ui_grid_steps,
    NDC_SCREEN_UNIFORM, PLAYER_UI_GRID_DIVISIONS,
};
pub use hud_image_asset::{probe_image_dimensions, validate_hud_image_file, HudImageAssetMeta};
pub use hud_layers::{
    hud_draw_order, hud_hit_test_order, next_z_index_for_screen, HudLayerKind, HudLayerRef,
};
pub use ndc_draw::{
    append_rect_fill, append_rect_outline, push_handle_disc, push_line_segment, push_quad,
    NdcVertex,
};
pub use geometry::{point_in_polygon, polygon_centroid};
pub use screen_hud_atlas::{
    ndc_transform_bottom_left, pick_localized_screen_hud, ScreenHudAtlas, ScreenHudBottomLeftLayout,
    ScreenHudPackedImage,
};
pub use viewport::{ndc_transform_top_left, pixel_to_ndc};
