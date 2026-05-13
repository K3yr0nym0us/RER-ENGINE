use glam::Vec3 as GlamVec3;
use winit::dpi::PhysicalSize;

use crate::config_2d::Camera2D;
use crate::config_compat::Camera;

use super::types::{SceneUniforms, DEPTH_FORMAT};

pub(super) fn create_depth_texture(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
) -> wgpu::TextureView {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth-texture"),
        size: wgpu::Extent3d {
            width:                 config.width.max(1),
            height:                config.height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count:    1,
        dimension:       wgpu::TextureDimension::D2,
        format:          DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

pub(super) fn build_scene_uniforms(camera: &Camera, size: PhysicalSize<u32>) -> SceneUniforms {
    let aspect    = size.width as f32 / size.height as f32;
    let view_proj = camera.to_uniform(aspect).view_proj;
    let p = camera.position();
    SceneUniforms {
        view_proj,
        cam_pos: [p.x, p.y, p.z, 0.0],
    }
}

pub(super) fn build_scene_uniforms_2d(cam: &Camera2D, size: PhysicalSize<u32>) -> SceneUniforms {
    let aspect    = size.width as f32 / size.height as f32;
    let view_proj = cam.view_proj(aspect).to_cols_array_2d();
    let p = cam.position();
    SceneUniforms {
        view_proj,
        cam_pos: [p.x, p.y, p.z, 0.0],
    }
}

// ── Culling 2D por viewport ──────────────────────────────────────────────────

/// Culling 2D: comprueba si el AABB de la entidad es visible en el rectángulo
/// ortográfico de la cámara. Se añade un margen proporcional al zoom para evitar
/// popping en entidades con geometría más grande que su Transform (ej. sprites con pivot).
///
/// Retorna `true` si la entidad DEBE dibujarse.
pub(crate) fn is_visible_2d(cam: &Camera2D, pos: GlamVec3, scale: GlamVec3, aspect: f32) -> bool {
    let half_w = cam.half_h * aspect;
    // Margen de seguridad: la mitad del lado mayor de la entidad
    let margin = scale.x.abs().max(scale.y.abs()) * 0.5;
    let min_x = cam.x - half_w  - margin;
    let max_x = cam.x + half_w  + margin;
    let min_y = cam.y - cam.half_h - margin;
    let max_y = cam.y + cam.half_h + margin;

    let e_min_x = pos.x - scale.x.abs() * 0.5;
    let e_max_x = pos.x + scale.x.abs() * 0.5;
    let e_min_y = pos.y - scale.y.abs() * 0.5;
    let e_max_y = pos.y + scale.y.abs() * 0.5;

    e_max_x >= min_x && e_min_x <= max_x && e_max_y >= min_y && e_min_y <= max_y
}
