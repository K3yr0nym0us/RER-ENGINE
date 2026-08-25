//! Atlas GPU y helpers NDC para imágenes HUD en espacio de pantalla.

use std::path::PathBuf;
use std::sync::Arc;

use glam::Mat4;

/// Atlas 2D dedicado a overlays de pantalla (shelf packing, sin deformar a cuadrado fijo).
pub struct ScreenHudAtlas {
    texture: wgpu::Texture,
    _view: wgpu::TextureView,
    _sampler: wgpu::Sampler,
    pub bind_group: Arc<wgpu::BindGroup>,
    width: u32,
    height: u32,
    cursor_x: u32,
    cursor_y: u32,
    row_h: u32,
}

/// Subrectángulo UV y tamaño en píxeles del PNG original (sin estirar al atlas).
#[derive(Debug, Clone, Copy)]
pub struct ScreenHudPackedImage {
    pub uv_rect: [f32; 4],
    pub pixel_width: f32,
    pub pixel_height: f32,
}

/// Posición del overlay en la esquina inferior izquierda (NDC).
#[derive(Debug, Clone, Copy)]
pub struct ScreenHudBottomLeftLayout {
    pub margin_px: f32,
    pub max_width_fraction: f32,
    pub max_width_px_min: f32,
    pub max_width_px_max: f32,
    pub display_scale: f32,
}

impl Default for ScreenHudBottomLeftLayout {
    fn default() -> Self {
        Self {
            margin_px: 18.0,
            max_width_fraction: 0.28,
            max_width_px_min: 120.0,
            max_width_px_max: 360.0,
            display_scale: 0.9,
        }
    }
}

impl ScreenHudAtlas {
    pub const SIZE: u32 = 4096;

    pub fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("screen-hud-atlas-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        })
    }

    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, bgl: &wgpu::BindGroupLayout) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("screen-hud-atlas"),
            size: wgpu::Extent3d {
                width: Self::SIZE,
                height: Self::SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("screen-hud-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let bind_group = Arc::new(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("screen-hud-atlas-bg"),
            layout: bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        }));

        let mut atlas = Self {
            texture,
            _view: view,
            _sampler: sampler,
            bind_group,
            width: Self::SIZE,
            height: Self::SIZE,
            cursor_x: 0,
            cursor_y: 0,
            row_h: 0,
        };
        atlas.pack_rgba(queue, &[255, 255, 255, 255], 1, 1);
        atlas
    }

    pub fn reset(&mut self, queue: &wgpu::Queue) {
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.row_h = 0;
        self.pack_rgba(queue, &[255, 255, 255, 255], 1, 1);
    }

    /// Empaca RGBA en el atlas (tamaño nativo, sin escalar a capa cuadrada).
    pub fn pack_rgba(
        &mut self,
        queue: &wgpu::Queue,
        rgba: &[u8],
        w: u32,
        h: u32,
    ) -> ScreenHudPackedImage {
        let uv = self.pack_raw(queue, rgba, w, h);
        ScreenHudPackedImage {
            uv_rect: uv,
            pixel_width: w as f32,
            pixel_height: h as f32,
        }
    }

    /// Lee `src/main/Engine/assets/<filename>` y lo empaqueta. Solo para HUD de pantalla.
    pub fn pack_png_from_engine_assets(
        &mut self,
        queue: &wgpu::Queue,
        filename: &str,
    ) -> Option<ScreenHudPackedImage> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../assets")
            .join(filename);
        let bytes = std::fs::read(&path).ok()?;
        use image::ImageReader;
        let img = ImageReader::new(std::io::Cursor::new(&bytes))
            .with_guessed_format()
            .ok()?
            .decode()
            .ok()?
            .to_rgba8();
        let (w, h) = img.dimensions();
        if w == 0 || h == 0 {
            return None;
        }
        Some(self.pack_rgba(queue, img.as_raw(), w, h))
    }

    fn pack_raw(&mut self, queue: &wgpu::Queue, rgba: &[u8], w: u32, h: u32) -> [f32; 4] {
        if self.cursor_x + w > self.width {
            self.cursor_y += self.row_h;
            self.cursor_x = 0;
            self.row_h = 0;
        }
        if self.cursor_y + h > self.height {
            log::error!(
                "[screen_hud_atlas] atlas lleno — no se puede empacar {w}×{h} (solo HUD pantalla)"
            );
            return [0.0, 0.0, 1.0 / Self::SIZE as f32, 1.0 / Self::SIZE as f32];
        }

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: self.cursor_x,
                    y: self.cursor_y,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * w),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );

        let uv = [
            self.cursor_x as f32 / self.width as f32,
            self.cursor_y as f32 / self.height as f32,
            (self.cursor_x + w) as f32 / self.width as f32,
            (self.cursor_y + h) as f32 / self.height as f32,
        ];
        self.cursor_x += w;
        self.row_h = self.row_h.max(h);
        uv
    }
}

/// Matriz NDC para un quad anclado abajo-izquierda con animación de entrada suave.
pub fn ndc_transform_bottom_left(
    viewport_w: f32,
    viewport_h: f32,
    packed: ScreenHudPackedImage,
    layout: ScreenHudBottomLeftLayout,
    eased_alpha: f32,
) -> Option<Mat4> {
    if viewport_w <= 0.0 || viewport_h <= 0.0 {
        return None;
    }
    let img_w = packed.pixel_width;
    let img_h = packed.pixel_height;
    if img_w <= 0.0 || img_h <= 0.0 {
        return None;
    }

    let max_width_px = (viewport_w * layout.max_width_fraction)
        .clamp(layout.max_width_px_min, layout.max_width_px_max);
    let scale_px = (max_width_px / img_w).min(1.0);
    let draw_w_px = img_w * scale_px * layout.display_scale;
    let draw_h_px = img_h * scale_px * layout.display_scale;

    let scale_in = 0.92 + 0.08 * eased_alpha;
    let slide_px = (1.0 - eased_alpha) * 14.0;

    let ndc_w = 2.0 * (draw_w_px / viewport_w) * scale_in;
    let ndc_h = 2.0 * (draw_h_px / viewport_h) * scale_in;
    let margin_x_ndc = 2.0 * layout.margin_px / viewport_w;
    let margin_y_ndc = 2.0 * layout.margin_px / viewport_h;
    let slide_ndc = 2.0 * slide_px / viewport_h;
    let cx = -1.0 + margin_x_ndc + ndc_w * 0.5;
    let cy = -1.0 + margin_y_ndc + ndc_h * 0.5 + slide_ndc;

    Some(
        Mat4::from_translation(glam::vec3(cx, cy, 0.0))
            * Mat4::from_scale(glam::vec3(ndc_w, ndc_h, 1.0)),
    )
}

/// Elige imagen localizada (es/en) para empaquetados en pares.
pub fn pick_localized_screen_hud(
    locale: &str,
    primary: Option<ScreenHudPackedImage>,
    fallback: Option<ScreenHudPackedImage>,
) -> Option<ScreenHudPackedImage> {
    if locale == "en" {
        primary.or(fallback)
    } else {
        fallback.or(primary)
    }
}
