use wgpu::util::DeviceExt;

/// Textura en GPU lista para bindear: view + sampler.
#[allow(dead_code)]
pub struct GpuTexture {
    pub view:    wgpu::TextureView,
    pub sampler: wgpu::Sampler,
}

#[allow(dead_code)]
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

    // ── Textura de color sólido (1×1) ────────────────────────────────────────
    pub fn solid_color(device: &wgpu::Device, queue: &wgpu::Queue, r: u8, g: u8, b: u8) -> Self {
        Self::from_rgba(device, queue, &[r, g, b, 255], 1, 1, "solid-color")
    }

    // ── Textura checkerboard — usada para el plano de suelo ──────────────────
    // Genera un patrón de ajedrez de `size`×`size` píxeles con dos tonos de gris.
    pub fn checkerboard(device: &wgpu::Device, queue: &wgpu::Queue, size: u32) -> Self {
        let mut pixels: Vec<u8> = Vec::with_capacity((size * size * 4) as usize);
        for y in 0..size {
            for x in 0..size {
                let light = ((x + y) % 2) == 0;
                // claro: #3a3d50  |  oscuro: #1e2030
                let (r, g, b): (u8, u8, u8) = if light {
                    (58, 61, 80)
                } else {
                    (30, 32, 48)
                };
                pixels.push(r);
                pixels.push(g);
                pixels.push(b);
                pixels.push(255);
            }
        }
        Self::from_rgba(device, queue, &pixels, size, size, "checkerboard")
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

// ── Texture Atlas ─────────────────────────────────────────────────────────────
//
// Empaca múltiples imágenes RGBA en una sola textura GPU 4096×4096 usando
// shelf packing (filas horizontales). Todas las entidades comparten el mismo
// BindGroup del atlas, lo que permite fusionar TODOS los draw calls en uno.
//
// Cada textura empacada recibe un UV rect [u_min, v_min, u_max, v_max] que
// el shader usa para muestrear la sub-región correcta del atlas.
pub struct TextureAtlas {
    texture:         wgpu::Texture,
    _view:           wgpu::TextureView,   // vivo mientras exista el bind_group
    _sampler:        wgpu::Sampler,       // ídem
    /// Bind group compartido por TODOS los sprites (group 1 en el shader).
    pub bind_group:  std::sync::Arc<wgpu::BindGroup>,
    width:           u32,
    height:          u32,
    cursor_x:        u32,
    cursor_y:        u32,
    row_h:           u32,
}

impl TextureAtlas {
    pub const SIZE: u32 = 4096;

    /// Crea el atlas vacío y empaca un pixel blanco 1×1 en la posición (0,0)
    /// que actúa como UV de fallback cuando un tex_idx es inválido.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, bgl: &wgpu::BindGroupLayout) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label:           Some("texture-atlas"),
            size:            wgpu::Extent3d { width: Self::SIZE, height: Self::SIZE, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count:    1,
            dimension:       wgpu::TextureDimension::D2,
            format:          wgpu::TextureFormat::Rgba8UnormSrgb,
            usage:           wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats:    &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label:          Some("atlas-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter:     wgpu::FilterMode::Linear,
            min_filter:     wgpu::FilterMode::Linear,
            mipmap_filter:  wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let bg = std::sync::Arc::new(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:   Some("atlas-bg"),
            layout:  bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        }));

        let mut atlas = Self {
            texture,
            _view:    view,
            _sampler: sampler,
            bind_group: bg,
            width:    Self::SIZE,
            height:   Self::SIZE,
            cursor_x: 0,
            cursor_y: 0,
            row_h:    0,
        };
        // Pixel blanco de fallback en (0,0)
        atlas.pack_raw(queue, &[255, 255, 255, 255], 1, 1);
        atlas
    }

    /// UV rect [u_min, v_min, u_max, v_max] del pixel blanco 1×1 en (0,0).
    /// Devuelve textura blanca opaca cuando tex_idx es inválido.
    pub fn fallback_uv() -> [f32; 4] {
        [0.0, 0.0, 1.0 / Self::SIZE as f32, 1.0 / Self::SIZE as f32]
    }

    /// Reinicia el shelf packing del atlas y vuelve a escribir el pixel blanco
    /// de fallback en (0,0). Se usa al cambiar de escena cuando las cachés de UV
    /// se vacían, evitando que el cursor siga avanzando con contenido ya huérfano.
    pub fn reset(&mut self, queue: &wgpu::Queue) {
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.row_h = 0;
        self.pack_raw(queue, &[255, 255, 255, 255], 1, 1);
    }

    /// Empaca una imagen RGBA en el atlas usando shelf packing.
    /// Retorna el UV rect [u_min, v_min, u_max, v_max] listo para el shader.
    pub fn pack(&mut self, queue: &wgpu::Queue, rgba: &[u8], w: u32, h: u32) -> [f32; 4] {
        self.pack_raw(queue, rgba, w, h)
    }

    fn pack_raw(&mut self, queue: &wgpu::Queue, rgba: &[u8], w: u32, h: u32) -> [f32; 4] {
        // Avanzar a la siguiente fila si la imagen no cabe en la actual
        if self.cursor_x + w > self.width {
            self.cursor_y += self.row_h;
            self.cursor_x  = 0;
            self.row_h     = 0;
        }
        if self.cursor_y + h > self.height {
            log::error!("[TextureAtlas] atlas lleno — no se puede empacar {}×{}", w, h);
            return Self::fallback_uv();
        }

        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture:   &self.texture,
                mip_level: 0,
                origin:    wgpu::Origin3d { x: self.cursor_x, y: self.cursor_y, z: 0 },
                aspect:    wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::ImageDataLayout {
                offset:         0,
                bytes_per_row:  Some(4 * w),
                rows_per_image: None,
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );

        // Inset de medio texel: evita filtrar píxeles vacíos del atlas en colores sólidos 1×1.
        let u0 = (self.cursor_x as f32 + 0.5) / self.width as f32;
        let v0 = (self.cursor_y as f32 + 0.5) / self.height as f32;
        let u1 = (self.cursor_x + w) as f32 / self.width as f32 - 0.5 / self.width as f32;
        let v1 = (self.cursor_y + h) as f32 / self.height as f32 - 0.5 / self.height as f32;
        let uv = [u0, v0, u1.max(u0), v1.max(v0)];
        self.cursor_x += w;
        self.row_h     = self.row_h.max(h);
        uv
    }

    /// Sobreescribe los píxeles de una región ya empacada (mismas dimensiones).
    /// Usado por `enter_pivot_edit_mode` para mostrar el frame temporalmente.
    pub fn update(&self, queue: &wgpu::Queue, rgba: &[u8], uv_rect: [f32; 4]) {
        let x = (uv_rect[0] * self.width  as f32).round() as u32;
        let y = (uv_rect[1] * self.height as f32).round() as u32;
        let w = ((uv_rect[2] - uv_rect[0]) * self.width  as f32).round() as u32;
        let h = ((uv_rect[3] - uv_rect[1]) * self.height as f32).round() as u32;
        if w == 0 || h == 0 { return; }
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture:   &self.texture,
                mip_level: 0,
                origin:    wgpu::Origin3d { x, y, z: 0 },
                aspect:    wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::ImageDataLayout {
                offset:         0,
                bytes_per_row:  Some(4 * w),
                rows_per_image: None,
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
    }
}
