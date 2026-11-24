use ash::vk;

use crate::engine3d::vulkan::{GpuImage, SharedGpu, VulkanError};

/// Índice de capa en el array de texturas compartido (group 1 del shader).
pub type TextureLayer = u32;

/// Lado del array GPU reservado para albedos de modelo que superan el tamaño estándar.
/// El shader sigue leyendo la capa directa (`v_tex.x`): se enlaza un segundo set de
/// descriptores con esta imagen, sin cambiar el shader ni el índice de capa.
pub const MODEL_TEXTURE_SIZE: u32 = 4096;

/// Bit alto usado como MARCA en los valores de capa que viajan por `tex_layers`,
/// `texture_path_layers` y `part_tex_layers`. Los números de capa Vulkan reales
/// caben en 8 bits (`MAX_LAYERS = 256`), así que marcar los del array 4K evita
/// que colisionen con los legacy en búsquedas por valor (`iter().position()`).
pub(crate) const HI_RES_LAYER_TAG: u32 = 0x8000_0000;

/// Etiqueta una capa del array 4K para que sea distinta de cualquier capa legacy.
pub(crate) fn tag_hi_res_layer(layer: u32) -> u32 {
    layer | HI_RES_LAYER_TAG
}

/// Recupera el número de capa Vulkan real (sin marca).
pub(crate) fn untag_layer(layer: u32) -> u32 {
    layer & !HI_RES_LAYER_TAG
}

/// Resultado de resize + mips en CPU (precarga de modelos).
pub(crate) struct LayerMipChain {
    pub mips: Vec<Vec<u8>>,
}

/// Array GPU `texture_2d_array`: una capa por material, UV [0,1] en el mesh.
/// El array vive en un único `GpuImage` (Vista 2D_ARRAY, todas las capas + mips).
pub struct TextureArray {
    image: GpuImage,
    width: u32,
    height: u32,
    max_layers: u32,
    next_layer: u32,
}

impl TextureArray {
    pub const TEXTURE_SIZE: u32 = 1024;
    /// Capacidad inicial: el array crece bajo demanda hasta `MAX_LAYERS`.
    pub const INITIAL_LAYERS: u32 = 32;
    /// Tope duro: el evento `TextureArrayExhausted` sigue disparándose aquí.
    pub const MAX_LAYERS: u32 = 256;

    pub const FORMAT: vk::Format = vk::Format::R8G8B8A8_UNORM;

    pub fn new(gpu: &SharedGpu) -> Result<Self, VulkanError> {
        Self::new_sized(gpu, Self::TEXTURE_SIZE, Self::INITIAL_LAYERS)
    }

    /// Crea un array con lado y capacidad inicial arbitrarios. `new()` corresponde
    /// a `(TEXTURE_SIZE, INITIAL_LAYERS)`; los modelos >1024 usan `MODEL_TEXTURE_SIZE`.
    pub fn new_sized(gpu: &SharedGpu, size: u32, initial_layers: u32) -> Result<Self, VulkanError> {
        let (extent, mip_levels) = array_geometry(size);
        let image = gpu.create_image_layers(
            extent,
            initial_layers,
            mip_levels,
            Self::FORMAT,
            vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::TRANSFER_DST
                | vk::ImageUsageFlags::TRANSFER_SRC,
            vk::ImageAspectFlags::COLOR,
            "texture-array",
        )?;

        let mut array = Self {
            image,
            width: size,
            height: size,
            max_layers: initial_layers,
            next_layer: 0,
        };
        array.reset(gpu)?;
        Ok(array)
    }

    pub fn fallback_layer() -> TextureLayer {
        0
    }

    pub fn reset(&mut self, gpu: &SharedGpu) -> Result<(), VulkanError> {
        self.next_layer = 0;
        // El fallback debe tener el lado de ESTE array: `upload_layer` sube el buffer
        // con `self.width`/`self.height`, y un blanco de 1024 en un array 4K dejaría
        // la capa 0 a medias.
        self.upload_layer(gpu, &solid_white_rgba(self.width), 0)?;
        Ok(())
    }

    pub fn pack(
        &mut self,
        gpu: &SharedGpu,
        rgba: &[u8],
        w: u32,
        h: u32,
    ) -> Result<TextureLayer, VulkanError> {
        // Solo el mip 0 se genera en CPU; el resto lo produce la GPU con blits.
        let base = resize_rgba_vec_to_layer(rgba.to_vec(), w, h, self.width, self.height);
        self.upload_layer_from_mips(gpu, &[base])
    }

    /// Sube mips ya preparados en CPU (p. ej. hilo de precarga de modelos).
    /// Se conserva el mip 0 y se regenera la cadena en GPU.
    pub fn pack_prepared_mips(
        &mut self,
        gpu: &SharedGpu,
        mips: &[Vec<u8>],
    ) -> Result<TextureLayer, VulkanError> {
        if !layer_mip_chain_valid_for_array(mips) {
            log::error!(
                "[TextureArray] mip chain inválida ({} niveles) — reconstruyendo desde mip0",
                mips.len()
            );
        }
        let mip0 = mips
            .first()
            .cloned()
            .unwrap_or_else(|| vec![255, 255, 255, 255]);
        let (w, h) = infer_base_dimensions_from_mip0(&mip0);
        let base = resize_rgba_vec_to_layer(mip0, w, h, self.width, self.height);
        self.upload_layer_from_mips(gpu, &[base])
    }

    /// Copia un tile RGBA en una capa ya empaquetada (mip 0). El shader de
    /// terreno lee el splat en LOD 0, así que no hace falta regenerar mips.
    pub fn blit_layer_tile(
        &self,
        gpu: &SharedGpu,
        layer: TextureLayer,
        x: u32,
        y: u32,
        rgba: &[u8],
        w: u32,
        h: u32,
    ) -> Result<(), VulkanError> {
        if layer == 0 || layer >= self.next_layer {
            return Err(VulkanError::new(format!(
                "capa {layer} fuera de las empaquetadas (next={})",
                self.next_layer
            )));
        }
        if w == 0 || h == 0 || x.saturating_add(w) > self.width || y.saturating_add(h) > self.height
        {
            return Err(VulkanError::new(format!(
                "tile {w}×{h} en ({x},{y}) fuera de {}×{}",
                self.width, self.height
            )));
        }
        gpu.upload_rgba_layer_region(&self.image, 0, layer, x, y, w, h, rgba)
    }

    fn upload_layer_from_mips(
        &mut self,
        gpu: &SharedGpu,
        mips: &[Vec<u8>],
    ) -> Result<TextureLayer, VulkanError> {
        while self.next_layer >= self.max_layers {
            if self.max_layers >= Self::MAX_LAYERS {
                log::error!(
                    "[TextureArray] array lleno ({} capas) — usando fallback",
                    self.max_layers
                );
                return Ok(Self::fallback_layer());
            }
            self.ensure_capacity(gpu)?;
        }
        let layer = self.next_layer;
        let regions: Vec<_> = mips
            .iter()
            .enumerate()
            .map(|(level, data)| {
                (
                    (self.width >> level).max(1),
                    (self.height >> level).max(1),
                    data.as_slice(),
                )
            })
            .collect();
        gpu.upload_rgba_mips(&self.image, layer, &regions)?;
        record_mip_chain(gpu, &self.image, layer)?;
        self.next_layer += 1;
        Ok(layer)
    }

    /// Duplica la capacidad conservando todas las capas ya empaquetadas.
    /// La copia es 1:1 (capa `i` → capa `i`), así que los `tex_idx` del ECS siguen vigentes.
    fn ensure_capacity(&mut self, gpu: &SharedGpu) -> Result<(), VulkanError> {
        let new_capacity = next_capacity(self.max_layers);
        let mip_levels = self.image.mips;
        let extent = self.image.extent;

        unsafe { gpu.device().device_wait_idle() }.map_err(|error| {
            VulkanError::new(format!(
                "esperar a la GPU para agrandar el array de texturas (Vulkan {error:?})"
            ))
        })?;

        let new_image = gpu.create_image_layers(
            vk::Extent2D {
                width: extent.width,
                height: extent.height,
            },
            new_capacity,
            mip_levels,
            Self::FORMAT,
            vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::TRANSFER_DST
                | vk::ImageUsageFlags::TRANSFER_SRC,
            vk::ImageAspectFlags::COLOR,
            "texture-array",
        )?;
        gpu.copy_image_layers(&self.image, &new_image, self.next_layer, mip_levels)?;

        let old = std::mem::replace(&mut self.image, new_image);
        self.max_layers = new_capacity;
        drop(old);
        log::info!(
            "[TextureArray] array ampliado a {new_capacity} capas ({} copiadas)",
            self.next_layer
        );
        Ok(())
    }

    fn upload_layer(
        &mut self,
        gpu: &SharedGpu,
        rgba: &[u8],
        layer: TextureLayer,
    ) -> Result<(), VulkanError> {
        // El pixel blanco del fallback también genera sus mips en GPU.
        gpu.upload_rgba_mips(&self.image, layer, &[(self.width, self.height, rgba)])?;
        record_mip_chain(gpu, &self.image, layer)
    }

    pub fn image(&self) -> &GpuImage {
        &self.image
    }
}

/// Cadena de mips para el bake/precarga: reutiliza `imageops` en vez de un filtro propio.
fn generate_mip_chain_owned(base: Vec<u8>, base_w: u32, base_h: u32) -> Vec<Vec<u8>> {
    let level_count = mip_level_count(base_w.max(base_h)) as usize;
    let mut chain = Vec::with_capacity(level_count);
    let mut cw = base_w;
    let mut ch = base_h;
    let mut current = base;
    for _ in 0..level_count {
        chain.push(current.clone());
        if cw <= 1 && ch <= 1 {
            break;
        }
        let nw = (cw / 2).max(1);
        let nh = (ch / 2).max(1);
        current = resize_rgba_vec_to_layer(current, cw, ch, nw, nh);
        cw = nw;
        ch = nh;
    }
    chain
}

/// Genera los mips 1..N de una capa con `vkCmdBlitImage` y deja la imagen en GENERAL.
fn record_mip_chain(
    gpu: &SharedGpu,
    image: &GpuImage,
    layer: TextureLayer,
) -> Result<(), VulkanError> {
    let levels = image.mips;
    if levels <= 1 {
        return Ok(());
    }
    let handle = image.image;
    let aspect = image.aspect;
    let base_w = image.extent.width;
    let base_h = image.extent.height;

    gpu.one_shot(move |device, command| {
        let all = vk::ImageSubresourceRange {
            aspect_mask: aspect,
            base_mip_level: 0,
            level_count: levels,
            base_array_layer: layer,
            layer_count: 1,
        };
        let barrier = |old_layout, new_layout, src_access, dst_access, range| {
            vk::ImageMemoryBarrier::default()
                .old_layout(old_layout)
                .new_layout(new_layout)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(handle)
                .subresource_range(range)
                .src_access_mask(src_access)
                .dst_access_mask(dst_access)
        };

        // GENERAL (mip 0 ya subido) -> TRANSFER_SRC_OPTIMAL para toda la capa.
        let to_src = barrier(
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::AccessFlags::SHADER_READ | vk::AccessFlags::TRANSFER_WRITE,
            vk::AccessFlags::TRANSFER_READ,
            all,
        );
        unsafe {
            device.cmd_pipeline_barrier(
                command,
                vk::PipelineStageFlags::FRAGMENT_SHADER | vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[to_src],
            );
        }

        for mip in 0..levels - 1 {
            let dst_range = vk::ImageSubresourceRange {
                aspect_mask: aspect,
                base_mip_level: mip + 1,
                level_count: 1,
                base_array_layer: layer,
                layer_count: 1,
            };
            let to_dst = barrier(
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::AccessFlags::TRANSFER_READ,
                vk::AccessFlags::TRANSFER_WRITE,
                dst_range,
            );
            unsafe {
                device.cmd_pipeline_barrier(
                    command,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[to_dst],
                );
            }

            let blit = vk::ImageBlit {
                src_subresource: vk::ImageSubresourceLayers {
                    aspect_mask: aspect,
                    mip_level: mip,
                    base_array_layer: layer,
                    layer_count: 1,
                },
                src_offsets: [
                    vk::Offset3D::default(),
                    vk::Offset3D {
                        x: (base_w >> mip).max(1) as i32,
                        y: (base_h >> mip).max(1) as i32,
                        z: 1,
                    },
                ],
                dst_subresource: vk::ImageSubresourceLayers {
                    aspect_mask: aspect,
                    mip_level: mip + 1,
                    base_array_layer: layer,
                    layer_count: 1,
                },
                dst_offsets: [
                    vk::Offset3D::default(),
                    vk::Offset3D {
                        x: (base_w >> (mip + 1)).max(1) as i32,
                        y: (base_h >> (mip + 1)).max(1) as i32,
                        z: 1,
                    },
                ],
            };
            unsafe {
                device.cmd_blit_image(
                    command,
                    handle,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    handle,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[blit],
                    vk::Filter::LINEAR,
                );
            }

            // El destino pasa a ser la fuente del siguiente blit.
            let back_to_src = barrier(
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                vk::AccessFlags::TRANSFER_WRITE,
                vk::AccessFlags::TRANSFER_READ,
                dst_range,
            );
            unsafe {
                device.cmd_pipeline_barrier(
                    command,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[back_to_src],
                );
            }
        }

        let to_general = barrier(
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::ImageLayout::GENERAL,
            vk::AccessFlags::TRANSFER_READ,
            vk::AccessFlags::SHADER_READ,
            all,
        );
        unsafe {
            device.cmd_pipeline_barrier(
                command,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[to_general],
            );
        }
    })
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
    (u32::BITS - size.leading_zeros()).max(1)
}

/// Geometría pura (sin GPU) de un array cuadrado: extent y niveles de mip.
/// La usa `new_sized` para crear la imagen y los tests para comprobar el tamaño.
fn array_geometry(size: u32) -> (vk::Extent2D, u32) {
    (
        vk::Extent2D {
            width: size,
            height: size,
        },
        mip_level_count(size),
    )
}

/// Siguiente capacidad del array (potencias de 2, tope duro `MAX_LAYERS`).
fn next_capacity(current: u32) -> u32 {
    match current {
        32 => 64,
        64 => 128,
        128 => 256,
        _ => TextureArray::MAX_LAYERS,
    }
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

fn solid_white_rgba(size: u32) -> Vec<u8> {
    resize_rgba_vec_to_layer(vec![255, 255, 255, 255], 1, 1, size, size)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_capacity_doubles_until_hard_cap() {
        assert_eq!(next_capacity(TextureArray::INITIAL_LAYERS), 64);
        assert_eq!(next_capacity(64), 128);
        assert_eq!(next_capacity(128), TextureArray::MAX_LAYERS);
    }

    #[test]
    fn next_capacity_never_exceeds_max_layers() {
        assert_eq!(
            next_capacity(TextureArray::MAX_LAYERS),
            TextureArray::MAX_LAYERS
        );
        assert_eq!(next_capacity(256), 256);
        assert_eq!(next_capacity(300), TextureArray::MAX_LAYERS);
    }

    #[test]
    fn new_sized_model_array_geometry_is_4k_with_13_mips() {
        let (extent, mips) = array_geometry(MODEL_TEXTURE_SIZE);
        assert_eq!(MODEL_TEXTURE_SIZE, 4096);
        assert_eq!(extent.width, 4096);
        assert_eq!(extent.height, 4096);
        assert_eq!(mips, 13);
    }

    #[test]
    fn reset_white_pixel_uses_the_array_size() {
        let white = solid_white_rgba(MODEL_TEXTURE_SIZE);
        assert_eq!(
            white.len(),
            (MODEL_TEXTURE_SIZE * MODEL_TEXTURE_SIZE * 4) as usize
        );
        assert!(white.iter().all(|&b| b == 255));

        // El array por defecto sigue generando un blanco de 1024².
        let white = solid_white_rgba(TextureArray::TEXTURE_SIZE);
        assert_eq!(
            white.len(),
            (TextureArray::TEXTURE_SIZE * TextureArray::TEXTURE_SIZE * 4) as usize
        );
        assert!(white.iter().all(|&b| b == 255));
    }

    #[test]
    fn hi_res_layer_tag_is_inverse_and_unique() {
        let legacy_layer = 1u32;
        let tagged = tag_hi_res_layer(legacy_layer);
        assert_eq!(untag_layer(tagged), legacy_layer);
        assert_eq!(untag_layer(legacy_layer), legacy_layer);
        assert!(tagged & HI_RES_LAYER_TAG != 0);
        assert_eq!(legacy_layer & HI_RES_LAYER_TAG, 0);
        // Una capa 4K etiquetada nunca compara igual que una legacy con el mismo
        // número de capa real: los dos arrays empiezan a contar en 0.
        assert_ne!(tagged, legacy_layer);
        assert_ne!(tagged, 0);
        assert!(
            untag_layer(tagged) < TextureArray::MAX_LAYERS,
            "la capa desenmascarada debe seguir siendo un índice Vulkan válido"
        );
    }

    #[test]
    fn default_new_keeps_1024_geometry() {
        // `new()` delega en `new_sized` con estos dos valores.
        assert_eq!(TextureArray::TEXTURE_SIZE, 1024);
        assert_eq!(TextureArray::INITIAL_LAYERS, 32);
        let (extent, mips) = array_geometry(TextureArray::TEXTURE_SIZE);
        assert_eq!(extent.width, 1024);
        assert_eq!(extent.height, 1024);
        assert_eq!(mips, 11);
    }
}
