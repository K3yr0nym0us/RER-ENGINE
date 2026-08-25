use std::sync::Arc;

/// Índice de capa en el array de texturas compartido (group 1 del shader).
pub type TextureLayer = u32;

/// Resultado de resize + mips en CPU (precarga de modelos).
pub(crate) struct LayerMipChain {
    pub mips: Vec<Vec<u8>>,
}

/// Array GPU `texture_2d_array`: una capa por material, UV [0,1] en el mesh.
pub struct TextureArray {
    texture: wgpu::Texture,
    _view: wgpu::TextureView,
    _sampler: wgpu::Sampler,
    pub bind_group: Arc<wgpu::BindGroup>,
    width: u32,
    height: u32,
    max_layers: u32,
    next_layer: u32,
}

impl TextureArray {
    pub const TEXTURE_SIZE: u32 = 1024;
    pub const MAX_LAYERS: u32 = 256;

    pub fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("texture-array-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
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
                width: Self::TEXTURE_SIZE,
                height: Self::TEXTURE_SIZE,
                depth_or_array_layers: Self::MAX_LAYERS,
            },
            mip_level_count: mip_levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("texture-array-sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });

        let bind_group = Arc::new(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("texture-array-bg"),
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
        let chain = build_layer_mip_chain_timed(rgba.to_vec(), w, h);
        self.upload_layer_from_mips(queue, &chain.mips)
    }

    /// Sube mips ya preparados en CPU (p. ej. hilo de precarga de modelos).
    pub fn pack_prepared_mips(&mut self, queue: &wgpu::Queue, mips: &[Vec<u8>]) -> TextureLayer {
        if !layer_mip_chain_valid_for_array(mips) {
            log::error!(
                "[TextureArray] mip chain inválida ({} niveles) — reconstruyendo desde mip0",
                mips.len()
            );
            let base = mips
                .first()
                .cloned()
                .unwrap_or_else(|| vec![255, 255, 255, 255]);
            let (w, h) = infer_base_dimensions_from_mip0(&base);
            let chain = build_layer_mip_chain_timed(base, w, h);
            return self.upload_layer_from_mips(queue, &chain.mips);
        }
        self.upload_layer_from_mips(queue, mips)
    }

    fn upload_layer_from_mips(&mut self, queue: &wgpu::Queue, mips: &[Vec<u8>]) -> TextureLayer {
        if self.next_layer >= self.max_layers {
            log::error!(
                "[TextureArray] array lleno ({} capas) — usando fallback",
                self.max_layers
            );
            return Self::fallback_layer();
        }
        let layer = self.next_layer;
        self.write_layer_mips(queue, mips, layer);
        self.next_layer += 1;
        layer
    }

    fn upload_layer(&mut self, queue: &wgpu::Queue, rgba: &[u8]) -> TextureLayer {
        let mips = generate_mip_chain_owned(rgba.to_vec(), self.width, self.height);
        self.upload_layer_from_mips(queue, &mips)
    }

    fn write_layer_mips(&self, queue: &wgpu::Queue, mips: &[Vec<u8>], layer: TextureLayer) {
        for (level, mip_data) in mips.iter().enumerate() {
            let mip_w = (self.width >> level).max(1);
            let mip_h = (self.height >> level).max(1);
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.texture,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                mip_data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * mip_w),
                    rows_per_image: Some(mip_h),
                },
                wgpu::Extent3d {
                    width: mip_w,
                    height: mip_h,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
}

/// Promedia bloques 2×2 (box filter) para el siguiente nivel de mip.
fn box_filter_halve(src: &[u8], sw: u32, sh: u32) -> (Vec<u8>, u32, u32) {
    let dw = (sw / 2).max(1);
    let dh = (sh / 2).max(1);
    if sw <= 1 && sh <= 1 {
        return (src.to_vec(), sw, sh);
    }

    let mut dst = vec![0u8; (dw * dh * 4) as usize];
    for y in 0..dh {
        for x in 0..dw {
            let mut r = 0u32;
            let mut g = 0u32;
            let mut b = 0u32;
            let mut a = 0u32;
            let mut n = 0u32;
            let sy = y * 2;
            let sx = x * 2;
            for dy in 0..2u32 {
                for dx in 0..2u32 {
                    let py = sy + dy;
                    let px = sx + dx;
                    if py >= sh || px >= sw {
                        continue;
                    }
                    let i = ((py * sw + px) * 4) as usize;
                    r += src[i] as u32;
                    g += src[i + 1] as u32;
                    b += src[i + 2] as u32;
                    a += src[i + 3] as u32;
                    n += 1;
                }
            }
            let n = n.max(1);
            let di = ((y * dw + x) * 4) as usize;
            dst[di] = (r / n) as u8;
            dst[di + 1] = (g / n) as u8;
            dst[di + 2] = (b / n) as u8;
            dst[di + 3] = (a / n) as u8;
        }
    }
    (dst, dw, dh)
}

fn generate_mip_chain_owned(base: Vec<u8>, base_w: u32, base_h: u32) -> Vec<Vec<u8>> {
    let level_count = mip_level_count(base_w.max(base_h)) as usize;
    let mut chain = Vec::with_capacity(level_count);
    let mut cw = base_w;
    let mut ch = base_h;
    chain.push(base);
    loop {
        if cw <= 1 && ch <= 1 {
            break;
        }
        let src = chain.last().expect("mip chain vacía");
        let (next, nw, nh) = box_filter_halve(src, cw, ch);
        cw = nw;
        ch = nh;
        chain.push(next);
    }
    chain
}

pub(crate) fn layer_mip_chain_valid_for_array(mips: &[Vec<u8>]) -> bool {
    let tex_size = TextureArray::TEXTURE_SIZE;
    let expected_levels = mip_level_count(tex_size) as usize;
    if mips.len() != expected_levels {
        return false;
    }
    let mut w = tex_size;
    let mut h = tex_size;
    for mip in mips {
        let expected = (w as usize) * (h as usize) * 4;
        if mip.len() != expected {
            return false;
        }
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    true
}

fn infer_base_dimensions_from_mip0(mip0: &[u8]) -> (u32, u32) {
    let pixels = (mip0.len() / 4).max(1) as u32;
    let side = (pixels as f64).sqrt().round() as u32;
    if side * side * 4 == mip0.len() as u32 {
        (side, side)
    } else {
        (1, 1)
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

fn resize_rgba_vec_to_layer(rgba: Vec<u8>, w: u32, h: u32, tw: u32, th: u32) -> Vec<u8> {
    let expected = (tw * th * 4) as usize;
    if w == tw && h == th && rgba.len() == expected {
        return rgba;
    }
    if let Some(img) = image::RgbaImage::from_raw(w, h, rgba) {
        let resized = image::imageops::resize(&img, tw, th, image::imageops::FilterType::Triangle);
        return resized.into_raw();
    }
    vec![255; expected]
}

/// Resize a 1024² + mips (toma ownership del buffer RGBA).
pub(crate) fn build_layer_mip_chain_timed(rgba: Vec<u8>, w: u32, h: u32) -> LayerMipChain {
    let tw = TextureArray::TEXTURE_SIZE;
    let th = TextureArray::TEXTURE_SIZE;
    let base = resize_rgba_vec_to_layer(rgba, w, h, tw, th);
    let mips = generate_mip_chain_owned(base, tw, th);
    LayerMipChain { mips }
}

fn solid_white_rgba() -> Vec<u8> {
    resize_rgba_vec_to_layer(
        vec![255, 255, 255, 255],
        1,
        1,
        TextureArray::TEXTURE_SIZE,
        TextureArray::TEXTURE_SIZE,
    )
}

/// Promedio RGB mip0 (sRGB bytes → linear aproximado).
pub fn rgba_mip0_average_linear(mip0: &[u8]) -> [f32; 3] {
    if mip0.len() < 4 {
        return [1.0, 1.0, 1.0];
    }
    let px = mip0.len() / 4;
    if px == 0 {
        return [1.0, 1.0, 1.0];
    }
    let mut r = 0u64;
    let mut g = 0u64;
    let mut b = 0u64;
    for i in 0..px {
        let o = i * 4;
        r += mip0[o] as u64;
        g += mip0[o + 1] as u64;
        b += mip0[o + 2] as u64;
    }
    let n = px as f64;
    [
        (r as f64 / n / 255.0) as f32,
        (g as f64 / n / 255.0) as f32,
        (b as f64 / n / 255.0) as f32,
    ]
}
