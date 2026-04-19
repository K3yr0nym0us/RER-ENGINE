use wgpu::util::DeviceExt;

/// Textura en GPU lista para bindear: view + sampler.
pub struct GpuTexture {
    pub view:    wgpu::TextureView,
    pub sampler: wgpu::Sampler,
}

impl GpuTexture {
    // ── Constructor desde bytes RGBA crudos ───────────────────────────────────
    pub fn from_rgba(
        device: &wgpu::Device,
        queue:  &wgpu::Queue,
        rgba:   &[u8],
        width:  u32,
        height: u32,
        label:  &str,
    ) -> Self {
        let size = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };

        let texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label:           Some(label),
                size,
                mip_level_count: 1,
                sample_count:    1,
                dimension:       wgpu::TextureDimension::D2,
                format:          wgpu::TextureFormat::Rgba8UnormSrgb,
                usage:           wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats:    &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            rgba,
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label:          Some(&format!("{label}-sampler")),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter:     wgpu::FilterMode::Linear,
            min_filter:     wgpu::FilterMode::Linear,
            mipmap_filter:  wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        Self { view, sampler }
    }

    // ── Textura blanca 1×1 (fallback cuando no hay textura) ──────────────────
    pub fn white(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        Self::from_rgba(device, queue, &[255, 255, 255, 255], 1, 1, "white-fallback")
    }

    // ── Constructor desde bytes de imagen (PNG/JPEG vía crate `image`) ────────
    pub fn from_image_bytes(
        device: &wgpu::Device,
        queue:  &wgpu::Queue,
        bytes:  &[u8],
        label:  &str,
    ) -> Result<Self, String> {
        use image::ImageReader;
        use std::io::Cursor;

        let img = ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|e| e.to_string())?
            .decode()
            .map_err(|e| e.to_string())?
            .to_rgba8();

        let (w, h) = img.dimensions();
        Ok(Self::from_rgba(device, queue, &img, w, h, label))
    }

    // ── Constructor desde datos de imagen gltf ────────────────────────────────
    pub fn from_gltf_image(
        device: &wgpu::Device,
        queue:  &wgpu::Queue,
        img:    &gltf::image::Data,
        label:  &str,
    ) -> Self {
        use gltf::image::Format;

        // Normalizar al formato RGBA8
        let rgba: Vec<u8> = match img.format {
            Format::R8G8B8 => img
                .pixels
                .chunks_exact(3)
                .flat_map(|p| [p[0], p[1], p[2], 255])
                .collect(),
            Format::R8G8B8A8 => img.pixels.clone(),
            // Para otros formatos convertimos mediante la crate image
            _ => {
                use image::{DynamicImage, ImageBuffer, Rgba};
                // Intentar convertir como Rgba8 genérico
                let buf: ImageBuffer<Rgba<u8>, Vec<u8>> =
                    ImageBuffer::from_raw(img.width, img.height, img.pixels.clone())
                        .unwrap_or_else(|| ImageBuffer::new(img.width, img.height));
                DynamicImage::ImageRgba8(buf).to_rgba8().into_raw()
            }
        };

        Self::from_rgba(device, queue, &rgba, img.width, img.height, label)
    }

    // ── Bind group layout (group 1) ───────────────────────────────────────────
    pub fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label:   Some("texture-bgl"),
            entries: &[
                // binding 0 — texture
                wgpu::BindGroupLayoutEntry {
                    binding:    0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled:   false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type:    wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                // binding 1 — sampler
                wgpu::BindGroupLayoutEntry {
                    binding:    1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        })
    }

    /// Crea el bind group usando el layout de `bind_group_layout()`.
    pub fn create_bind_group(
        &self,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:   Some("texture-bg"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding:  0,
                    resource: wgpu::BindingResource::TextureView(&self.view),
                },
                wgpu::BindGroupEntry {
                    binding:  1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }
}
