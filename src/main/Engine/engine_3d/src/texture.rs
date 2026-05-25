use std::sync::Arc;

/// Índice de capa en el array de texturas compartido (group 1 del shader).
pub type TextureLayer = u32;

/// Array GPU `texture_2d_array`: una capa por material, UV [0,1] en el mesh.
pub struct TextureArray {
    texture:    wgpu::Texture,
    _view:      wgpu::TextureView,
    _sampler:   wgpu::Sampler,
    pub bind_group: Arc<wgpu::BindGroup>,
    width:      u32,
    height:     u32,
    max_layers: u32,
    next_layer: u32,
}

impl TextureArray {
    pub const TEXTURE_SIZE: u32 = 1024;
    pub const MAX_LAYERS: u32 = 256;

    pub fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label:   Some("texture-array-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding:    0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled:   false,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        sample_type:    wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding:    1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        })
    }

    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, bgl: &wgpu::BindGroupLayout) -> Self {
        let mip_levels = mip_level_count(Self::TEXTURE_SIZE);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("texture-array"),
            size: wgpu::Extent3d {
                width:                 Self::TEXTURE_SIZE,
                height:                Self::TEXTURE_SIZE,
                depth_or_array_layers: Self::MAX_LAYERS,
            },
            mip_level_count: mip_levels,
            sample_count:    1,
            dimension:       wgpu::TextureDimension::D2,
            format:          wgpu::TextureFormat::Rgba8UnormSrgb,
            usage:           wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats:    &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label:          Some("texture-array-sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter:     wgpu::FilterMode::Linear,
            min_filter:     wgpu::FilterMode::Linear,
            mipmap_filter:  wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let bind_group = Arc::new(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:   Some("texture-array-bg"),
            layout:  bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding:  0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding:  1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        }));

        let mut array = Self {
            texture,
            _view: view,
            _sampler: sampler,
            bind_group,
            width: Self::TEXTURE_SIZE,
            height: Self::TEXTURE_SIZE,
            max_layers: Self::MAX_LAYERS,
            next_layer: 0,
        };
        array.upload_layer(queue, &solid_white_rgba());
        array
    }

    pub fn fallback_layer() -> TextureLayer {
        0
    }

    pub fn reset(&mut self, queue: &wgpu::Queue) {
        self.next_layer = 0;
        self.upload_layer(queue, &solid_white_rgba());
    }

    pub fn pack(&mut self, queue: &wgpu::Queue, rgba: &[u8], w: u32, h: u32) -> TextureLayer {
        let rgba = resize_rgba_to_layer(rgba, w, h, self.width, self.height);
        self.upload_layer(queue, &rgba)
    }

    fn upload_layer(&mut self, queue: &wgpu::Queue, rgba: &[u8]) -> TextureLayer {
        if self.next_layer >= self.max_layers {
            log::error!(
                "[TextureArray] array lleno ({} capas) — usando fallback",
                self.max_layers
            );
            return Self::fallback_layer();
        }
        let layer = self.next_layer;
        self.write_layer(queue, rgba, layer);
        self.next_layer += 1;
        layer
    }

    fn write_layer(&self, queue: &wgpu::Queue, rgba: &[u8], layer: TextureLayer) {
        let mips = generate_mip_chain(rgba, self.width, self.height);
        for (level, mip_data) in mips.iter().enumerate() {
            let mip_w = (self.width >> level).max(1);
            let mip_h = (self.height >> level).max(1);
            queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture:   &self.texture,
                    mip_level: level as u32,
                    origin:    wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer,
                    },
                    aspect:    wgpu::TextureAspect::All,
                },
                mip_data,
                wgpu::ImageDataLayout {
                    offset:         0,
                    bytes_per_row:  Some(4 * mip_w),
                    rows_per_image: Some(mip_h),
                },
                wgpu::Extent3d {
                    width:                 mip_w,
                    height:                mip_h,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
}

fn mip_level_count(size: u32) -> u32 {
    let mut levels = 1u32;
    let mut s = size;
    while s > 1 {
        s /= 2;
        levels += 1;
    }
    levels
}

fn generate_mip_chain(base: &[u8], base_w: u32, base_h: u32) -> Vec<Vec<u8>> {
    let mut chain = vec![base.to_vec()];
    let mut cw = base_w;
    let mut ch = base_h;
    let mut current = chain[0].clone();
    while cw > 1 || ch > 1 {
        let nw = (cw / 2).max(1);
        let nh = (ch / 2).max(1);
        if let Some(img) = image::RgbaImage::from_raw(cw, ch, current) {
            current = image::imageops::resize(
                &img,
                nw,
                nh,
                image::imageops::FilterType::Triangle,
            )
            .into_raw();
            cw = nw;
            ch = nh;
            chain.push(current.clone());
        } else {
            break;
        }
    }
    chain
}

fn solid_white_rgba() -> Vec<u8> {
    resize_rgba_to_layer(
        &[255, 255, 255, 255],
        1,
        1,
        TextureArray::TEXTURE_SIZE,
        TextureArray::TEXTURE_SIZE,
    )
}

fn resize_rgba_to_layer(rgba: &[u8], w: u32, h: u32, tw: u32, th: u32) -> Vec<u8> {
    if w == tw && h == th && rgba.len() == (tw * th * 4) as usize {
        return rgba.to_vec();
    }
    if let Some(img) = image::RgbaImage::from_raw(w, h, rgba.to_vec()) {
        let resized = image::imageops::resize(
            &img,
            tw,
            th,
            image::imageops::FilterType::Triangle,
        );
        if w != tw || h != th {
            log::debug!("[TextureArray] textura {w}×{h} escalada a {tw}×{th}");
        }
        return resized.into_raw();
    }
    vec![255; (tw * th * 4) as usize]
}
