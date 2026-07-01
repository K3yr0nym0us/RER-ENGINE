//! SSIL placeholder texture (composite bind slot; pass not wired yet).

use wgpu::{Device, TextureFormat, TextureView};

pub struct SsilPass {
    _texture: wgpu::Texture,
    output_view: TextureView,
}

impl SsilPass {
    pub fn new(device: &Device, _color_format: TextureFormat, width: u32, height: u32) -> Self {
        let (texture, output_view) = create_ssil_texture(device, width, height);
        Self {
            _texture: texture,
            output_view,
        }
    }

    pub fn output_view(&self) -> &TextureView {
        &self.output_view
    }
}

fn create_ssil_texture(device: &Device, width: u32, height: u32) -> (wgpu::Texture, TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("ssil-output"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}
