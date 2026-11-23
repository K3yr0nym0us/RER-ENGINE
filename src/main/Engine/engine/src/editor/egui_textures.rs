//! Texturas de egui grabadas en el command buffer del frame.
//!
//! `egui-ash-renderer::set_texture` envía un one-shot y hace `queue_wait_idle`,
//! lo que drena la cola gráfica (escena 3D y uploads del loader) en cada delta
//! del atlas. Aquí se copia al `frame.command` y el fence del slot libera el
//! staging: el loader del motor y el spinner del picker de modelos siguen en
//! su hilo / estado `importing`, sin congelar el event loop.

use std::collections::HashMap;

use ash::vk;
use egui::TextureId;
use egui::epaint::{ImageDelta, Primitive};
use egui_ash_renderer::allocator::DefaultAllocator;
use egui_ash_renderer::vulkan::{
    create_vulkan_descriptor_pool, create_vulkan_descriptor_set,
    create_vulkan_descriptor_set_layout,
};
use egui_ash_renderer::{Renderer, RendererError};
use rer_engine_core::vulkan::gpu::{GpuBuffer, GpuImage, MemoryLocation, cmd_image_barrier};
use rer_engine_core::vulkan::{FRAMES_IN_FLIGHT, SharedGpu};

const FORMAT: vk::Format = vk::Format::R8G8B8A8_SRGB;
const MAX_SETS: u32 = 1024;

struct LiveTexture {
    image: GpuImage,
    set: vk::DescriptorSet,
    user: TextureId,
    cpu: Vec<u8>,
    width: u32,
    height: u32,
}

struct Retired {
    /// Se retiene hasta el fence del slot; el `Drop` libera la imagen.
    _image: Option<GpuImage>,
    /// Staging del copy; mismo lifetime que `_image`.
    _staging: Option<GpuBuffer>,
    set: Option<vk::DescriptorSet>,
    user: Option<TextureId>,
}

/// Deltas de un frame que no llegó a `begin_frame` (swapchain outdated).
pub(crate) enum PendingTextureOp {
    Set(TextureId, ImageDelta),
    Free(TextureId),
}

pub(crate) struct EguiTextureSync {
    gpu: SharedGpu,
    layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    sampler: vk::Sampler,
    live: HashMap<TextureId, LiveTexture>,
    /// `TextureId::Managed` → `TextureId::User` registrado en el renderer.
    user_of: HashMap<TextureId, TextureId>,
    pending: [Vec<Retired>; FRAMES_IN_FLIGHT],
}

impl EguiTextureSync {
    pub(crate) fn new(gpu: SharedGpu) -> Result<Self, String> {
        let device = gpu.device();
        let layout = create_vulkan_descriptor_set_layout(device).map_err(map_renderer)?;
        let pool = create_vulkan_descriptor_pool(device, MAX_SETS).map_err(map_renderer)?;
        let sampler = unsafe {
            device
                .create_sampler(
                    &vk::SamplerCreateInfo::default()
                        .mag_filter(vk::Filter::LINEAR)
                        .min_filter(vk::Filter::LINEAR)
                        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .anisotropy_enable(false)
                        .max_anisotropy(1.0)
                        .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
                        .unnormalized_coordinates(false)
                        .compare_enable(false)
                        .compare_op(vk::CompareOp::ALWAYS)
                        .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
                        .mip_lod_bias(0.0)
                        .min_lod(0.0)
                        .max_lod(1.0),
                    None,
                )
                .map_err(|error| format!("No se pudo crear el sampler de egui: {error:?}"))?
        };
        Ok(Self {
            gpu,
            layout,
            pool,
            sampler,
            live: HashMap::new(),
            user_of: HashMap::new(),
            pending: [const { Vec::new() }; FRAMES_IN_FLIGHT],
        })
    }

    pub(crate) fn stash_skipped<I, D, F>(pending: &mut Vec<PendingTextureOp>, sets: I, frees: F)
    where
        I: IntoIterator<Item = (TextureId, D)>,
        D: IntoIterator<Item = ImageDelta>,
        F: IntoIterator<Item = TextureId>,
    {
        for (id, deltas) in sets {
            for delta in deltas {
                pending.push(PendingTextureOp::Set(id, delta));
            }
        }
        for id in frees {
            pending.push(PendingTextureOp::Free(id));
        }
    }

    /// El fence del slot ya esperó en `begin_frame`: se pueden soltar stagings
    /// e imágenes sustituidas en este índice.
    pub(crate) fn recycle(
        &mut self,
        renderer: &mut Renderer<DefaultAllocator>,
        slot: usize,
    ) -> Result<(), String> {
        let slot = slot.min(FRAMES_IN_FLIGHT - 1);
        let device = self.gpu.device();
        for retired in self.pending[slot].drain(..) {
            if let Some(user) = retired.user {
                renderer.remove_user_texture(user);
            }
            if let Some(set) = retired.set {
                unsafe {
                    device
                        .free_descriptor_sets(self.pool, &[set])
                        .map_err(|error| {
                            format!("No se pudo liberar un descriptor de egui: {error:?}")
                        })?;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn apply(
        &mut self,
        renderer: &mut Renderer<DefaultAllocator>,
        command: vk::CommandBuffer,
        slot: usize,
        pending: &mut Vec<PendingTextureOp>,
        current_sets: impl IntoIterator<Item = (TextureId, Vec<ImageDelta>)>,
    ) -> Result<(), String> {
        self.recycle(renderer, slot)?;
        let slot = slot.min(FRAMES_IN_FLIGHT - 1);
        let skipped = std::mem::take(pending);
        let mut skipped_frees = Vec::new();
        for op in skipped {
            match op {
                PendingTextureOp::Set(id, delta) => {
                    self.apply_one(renderer, command, slot, id, &delta)?;
                }
                PendingTextureOp::Free(id) => skipped_frees.push(id),
            }
        }
        self.queue_frees(slot, &skipped_frees);
        for (id, parts) in current_sets {
            if matches!(id, TextureId::User(_)) {
                continue;
            }
            for delta in parts {
                self.apply_one(renderer, command, slot, id, &delta)?;
            }
        }
        Ok(())
    }

    pub(crate) fn remap(&self, primitives: &mut [egui::ClippedPrimitive]) {
        for primitive in primitives {
            if let Primitive::Mesh(mesh) = &mut primitive.primitive
                && let Some(user) = self.user_of.get(&mesh.texture_id)
            {
                mesh.texture_id = *user;
            }
        }
    }

    pub(crate) fn queue_frees(&mut self, slot: usize, ids: &[TextureId]) {
        let slot = slot.min(FRAMES_IN_FLIGHT - 1);
        for id in ids {
            if let Some(live) = self.live.remove(id) {
                self.user_of.remove(id);
                self.pending[slot].push(Retired {
                    _image: Some(live.image),
                    _staging: None,
                    set: Some(live.set),
                    user: Some(live.user),
                });
            }
        }
    }

    fn apply_one(
        &mut self,
        renderer: &mut Renderer<DefaultAllocator>,
        command: vk::CommandBuffer,
        slot: usize,
        id: TextureId,
        delta: &ImageDelta,
    ) -> Result<(), String> {
        if matches!(id, TextureId::User(_)) {
            return Ok(());
        }
        let (src_w, src_h, pixels) = decode_delta(delta)?;
        let (cpu, width, height) = match delta.pos {
            Some([x, y]) => {
                let mut live = self.live.remove(&id).ok_or_else(|| {
                    "egui pidió un parche de textura que aún no existe.".to_string()
                })?;
                let x = u32::try_from(x)
                    .map_err(|_| "El parche de textura egui está fuera de rango.".to_string())?;
                let y = u32::try_from(y)
                    .map_err(|_| "El parche de textura egui está fuera de rango.".to_string())?;
                patch_rgba(&mut live.cpu, live.width, x, y, src_w, src_h, &pixels);
                self.pending[slot].push(Retired {
                    _image: Some(live.image),
                    _staging: None,
                    set: Some(live.set),
                    user: Some(live.user),
                });
                self.user_of.remove(&id);
                (live.cpu, live.width, live.height)
            }
            None => (pixels, src_w, src_h),
        };
        self.upload_full(renderer, command, slot, id, width, height, cpu)
    }

    fn upload_full(
        &mut self,
        renderer: &mut Renderer<DefaultAllocator>,
        command: vk::CommandBuffer,
        slot: usize,
        id: TextureId,
        width: u32,
        height: u32,
        cpu: Vec<u8>,
    ) -> Result<(), String> {
        let extent = vk::Extent2D { width, height };
        let image = self
            .gpu
            .create_image(
                extent,
                FORMAT,
                vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
                vk::ImageAspectFlags::COLOR,
                "egui-texture",
            )
            .map_err(|error| error.to_string())?;
        let staging = self
            .gpu
            .create_buffer_with_data(
                &cpu,
                vk::BufferUsageFlags::TRANSFER_SRC,
                MemoryLocation::CpuToGpu,
                "egui-texture-staging",
            )
            .map_err(|error| error.to_string())?;
        record_copy(
            self.gpu.device(),
            command,
            staging.buffer,
            image.image,
            width,
            height,
        );
        let set = create_vulkan_descriptor_set(
            self.gpu.device(),
            self.layout,
            self.pool,
            image.view,
            self.sampler,
        )
        .map_err(map_renderer)?;
        let user = renderer.add_user_texture(set);
        if let Some(previous) = self.live.insert(
            id,
            LiveTexture {
                image,
                set,
                user,
                cpu,
                width,
                height,
            },
        ) {
            self.pending[slot].push(Retired {
                _image: Some(previous.image),
                _staging: None,
                set: Some(previous.set),
                user: Some(previous.user),
            });
        }
        self.user_of.insert(id, user);
        self.pending[slot].push(Retired {
            _image: None,
            _staging: Some(staging),
            set: None,
            user: None,
        });
        Ok(())
    }
}

impl Drop for EguiTextureSync {
    fn drop(&mut self) {
        let device = self.gpu.device();
        self.live.clear();
        self.pending = [const { Vec::new() }; FRAMES_IN_FLIGHT];
        unsafe {
            device.destroy_descriptor_pool(self.pool, None);
            device.destroy_descriptor_set_layout(self.layout, None);
            device.destroy_sampler(self.sampler, None);
        }
    }
}

fn map_renderer(error: RendererError) -> String {
    format!("No se pudo preparar una textura de egui: {error}")
}

fn decode_delta(delta: &ImageDelta) -> Result<(u32, u32, Vec<u8>), String> {
    match &delta.image {
        egui::ImageData::Color(image) => {
            let width = u32::try_from(image.width())
                .map_err(|_| "La textura de egui es demasiado ancha.".to_string())?;
            let height = u32::try_from(image.height())
                .map_err(|_| "La textura de egui es demasiado alta.".to_string())?;
            let pixels = image
                .pixels
                .iter()
                .flat_map(|color| color.to_array())
                .collect();
            Ok((width, height, pixels))
        }
    }
}

fn patch_rgba(target: &mut [u8], width: u32, x: u32, y: u32, src_w: u32, src_h: u32, src: &[u8]) {
    for row in 0..src_h {
        let dst_index = (((y + row) * width + x) * 4) as usize;
        let src_index = ((row * src_w) * 4) as usize;
        let bytes = (src_w * 4) as usize;
        if dst_index + bytes <= target.len() && src_index + bytes <= src.len() {
            target[dst_index..dst_index + bytes]
                .copy_from_slice(&src[src_index..src_index + bytes]);
        }
    }
}

fn record_copy(
    device: &ash::Device,
    command: vk::CommandBuffer,
    staging: vk::Buffer,
    image: vk::Image,
    width: u32,
    height: u32,
) {
    cmd_image_barrier(
        device,
        command,
        image,
        vk::ImageAspectFlags::COLOR,
        1,
        1,
        vk::ImageLayout::UNDEFINED,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        vk::PipelineStageFlags::TOP_OF_PIPE,
        vk::PipelineStageFlags::TRANSFER,
        vk::AccessFlags::empty(),
        vk::AccessFlags::TRANSFER_WRITE,
    );
    let region = vk::BufferImageCopy::default()
        .image_subresource(vk::ImageSubresourceLayers {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            mip_level: 0,
            base_array_layer: 0,
            layer_count: 1,
        })
        .image_extent(vk::Extent3D {
            width,
            height,
            depth: 1,
        });
    unsafe {
        device.cmd_copy_buffer_to_image(
            command,
            staging,
            image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[region],
        );
    }
    cmd_image_barrier(
        device,
        command,
        image,
        vk::ImageAspectFlags::COLOR,
        1,
        1,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        vk::PipelineStageFlags::TRANSFER,
        vk::PipelineStageFlags::FRAGMENT_SHADER,
        vk::AccessFlags::TRANSFER_WRITE,
        vk::AccessFlags::SHADER_READ,
    );
}

#[cfg(test)]
mod tests {
    use super::patch_rgba;

    #[test]
    fn patch_writes_a_rect_inside_the_atlas() {
        let mut atlas = vec![0u8; 4 * 4 * 4];
        let src = vec![9u8; 8];
        patch_rgba(&mut atlas, 4, 1, 2, 2, 1, &src);
        assert_eq!(&atlas[36..44], &[9, 9, 9, 9, 9, 9, 9, 9]);
        assert_eq!(atlas[32], 0);
        assert_eq!(atlas[44], 0);
    }
}
