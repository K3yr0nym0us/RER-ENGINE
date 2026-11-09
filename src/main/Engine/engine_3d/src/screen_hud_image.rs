//! Wrapper del motor 3D: atlas HUD en Vulkan + instancia GPU local (`mesh::InstanceData`).

use ash::vk;
use glam::Mat4;

pub use rer_engine_core::player_ui::screen_hud_atlas::ScreenHudPackedImage;

use crate::mesh;
use crate::vulkan::{GpuImage, SharedGpu, VulkanError};

/// Atlas 2D para overlays de pantalla (shelf packing) sobre Ash.
///
/// Sustituye al `ScreenHudAtlas` de wgpu del motor 2D: misma API de empaquetado,
/// pero sube cada subrect con un command buffer de un solo uso.
pub struct HudAtlas {
    image: GpuImage,
    width: u32,
    height: u32,
    cursor_x: u32,
    cursor_y: u32,
    row_h: u32,
}

impl HudAtlas {
    pub const SIZE: u32 = 4096;

    pub fn new(gpu: &SharedGpu) -> Result<Self, VulkanError> {
        let image = gpu.create_image(
            vk::Extent2D {
                width: Self::SIZE,
                height: Self::SIZE,
            },
            vk::Format::R8G8B8A8_SRGB,
            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
            vk::ImageAspectFlags::COLOR,
            "hud-atlas",
        )?;
        let mut atlas = Self {
            image,
            width: Self::SIZE,
            height: Self::SIZE,
            cursor_x: 0,
            cursor_y: 0,
            row_h: 0,
        };
        atlas.reset(gpu);
        Ok(atlas)
    }

    pub fn image(&self) -> &GpuImage {
        &self.image
    }

    pub fn reset(&mut self, gpu: &SharedGpu) {
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.row_h = 0;
        let _ = self.pack_rgba(gpu, &[255, 255, 255, 255], 1, 1);
    }

    /// Empaca RGBA en el atlas (tamaño nativo, sin escalar a capa cuadrada).
    pub fn pack_rgba(
        &mut self,
        gpu: &SharedGpu,
        rgba: &[u8],
        w: u32,
        h: u32,
    ) -> ScreenHudPackedImage {
        let uv = self.pack_raw(gpu, rgba, w, h);
        ScreenHudPackedImage {
            uv_rect: uv,
            pixel_width: w as f32,
            pixel_height: h as f32,
        }
    }

    fn pack_raw(&mut self, gpu: &SharedGpu, rgba: &[u8], w: u32, h: u32) -> [f32; 4] {
        if self.cursor_x + w > self.width {
            self.cursor_y += self.row_h;
            self.cursor_x = 0;
            self.row_h = 0;
        }
        if self.cursor_y + h > self.height {
            log::error!("[hud_atlas] atlas lleno: no se puede empacar {w}x{h} (solo HUD pantalla)");
            return [0.0, 0.0, 1.0 / Self::SIZE as f32, 1.0 / Self::SIZE as f32];
        }

        if let Err(error) =
            gpu.upload_rgba_region(&self.image, self.cursor_x, self.cursor_y, w, h, rgba)
        {
            log::error!("[hud_atlas] fallo al subir {w}x{h}: {error}");
        }

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

/// Instancia para `screen_hud_pipeline`. `tex_layer_pad` = UV rect.
pub fn build_screen_hud_instance(
    packed: ScreenHudPackedImage,
    model: Mat4,
    alpha: f32,
) -> mesh::InstanceData {
    let mut inst = mesh::InstanceData::new(model, 0.0, 0);
    inst.tex_layer_pad = packed.uv_rect;
    inst.flag_pad[1] = alpha;
    inst
}
