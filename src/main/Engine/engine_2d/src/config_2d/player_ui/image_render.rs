//! Renderizado de imágenes HUD (marco de selección + quad texturizado).

use crate::gizmo::GizmoVertex;
use crate::screen_hud_image::ScreenHudAtlas;

use super::config::{PlayerUiImage, box_corners};
use super::ndc_draw::{push_handle_disc, push_line_segment, push_quad};
use super::text_render::HANDLE_RADIUS;

pub(crate) fn append_image_gizmo_verts(
    verts: &mut Vec<GizmoVertex>,
    images: &[PlayerUiImage],
    selected: Option<u32>,
) {
    for img in images {
        let rect = img.ui_hud_rect();
        let is_selected = selected == Some(img.id);
        if is_selected {
            let fill = [0.2_f32, 0.46, 0.9, 0.28];
            let border = [1.0_f32, 0.85, 0.2, 0.95];
            let (x0, y0, x1, y1) = box_corners(rect);
            push_quad(
                verts,
                [x0, y0, 0.0],
                [x1, y0, 0.0],
                [x1, y1, 0.0],
                [x0, y1, 0.0],
                fill,
            );
            push_line_segment(verts, [x0, y0, 0.0], [x1, y0, 0.0], border);
            push_line_segment(verts, [x1, y0, 0.0], [x1, y1, 0.0], border);
            push_line_segment(verts, [x1, y1, 0.0], [x0, y1, 0.0], border);
            push_line_segment(verts, [x0, y1, 0.0], [x0, y0, 0.0], border);
            let handle_color = [1.0_f32, 0.85, 0.2, 0.95];
            for (cx, cy) in [(x0, y1), (x1, y1), (x0, y0), (x1, y0)] {
                push_handle_disc(verts, cx, cy, HANDLE_RADIUS, handle_color);
            }
        }
    }
}

pub(crate) fn append_image_hud_glyphs(
    images: &[PlayerUiImage],
    atlas: &mut ScreenHudAtlas,
    queue: &wgpu::Queue,
    instances: &mut Vec<crate::mesh::InstanceData>,
    viewport_w: f32,
    viewport_h: f32,
    texture_cache: &mut std::collections::HashMap<
        String,
        crate::screen_hud_image::ScreenHudPackedImage,
    >,
) {
    for img in images {
        super::font::build_hud_texture_quad_cached(
            &img.image_path,
            img.ui_hud_rect(),
            viewport_w,
            viewport_h,
            texture_cache,
            atlas,
            queue,
            instances,
            1.0,
        );
    }
}
