//! Wrapper del motor 2D: atlas compartido + espejo Vulkan + instancia GPU local
//! (`mesh::InstanceData`).
//!
//! El atlas real (empaquetado shelf + textura wgpu) vive en `engine_shared`. Durante la fase 5b la
//! ruta Ash no puede leer esa textura de wgpu, así que este wrapper replica cada región empacada en
//! una imagen `VkImage` propia: las dos rutas ven exactamente los mismos píxeles y el mismo layout
//! de UVs. En la fase 6 desaparecerá la mitad wgpu y solo quedará el espejo.

use std::sync::Arc;

use glam::Mat4;

pub use rer_engine_shared::player_ui::screen_hud_atlas::ScreenHudPackedImage;

use crate::mesh;
use crate::vulkan::Vulkan2d;

pub struct ScreenHudAtlas {
    inner: rer_engine_shared::player_ui::screen_hud_atlas::ScreenHudAtlas,
    /// Espejo Vulkan (fase 5b). `None` cuando la ruta wgpu es la activa.
    vulkan: Option<Arc<Vulkan2d>>,
}

impl ScreenHudAtlas {
    pub const SIZE: u32 = 4096;

    pub fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        rer_engine_shared::player_ui::screen_hud_atlas::ScreenHudAtlas::bind_group_layout(device)
    }

    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bgl: &wgpu::BindGroupLayout,
        vulkan: Option<Arc<Vulkan2d>>,
    ) -> Self {
        let inner =
            rer_engine_shared::player_ui::screen_hud_atlas::ScreenHudAtlas::new(device, queue, bgl);
        let atlas = Self { inner, vulkan };
        // El constructor compartido reserva un píxel blanco 1×1 en (0,0) como UV de fallback.
        atlas.upload_white_fallback();
        atlas
    }

    pub fn reset(&mut self, queue: &wgpu::Queue) {
        self.inner.reset(queue);
        self.upload_white_fallback();
    }

    fn upload_white_fallback(&self) {
        if let Some(vulkan) = &self.vulkan
            && let Err(error) = vulkan.upload_hud_atlas_region(&[255, 255, 255, 255], 0, 0, 1, 1)
        {
            log::warn!("[screen-hud-atlas] no se pudo espejar el píxel de fallback: {error}");
        }
    }

    /// Empaca RGBA en el atlas compartido y copia los mismos píxeles a la imagen Vulkan.
    pub fn pack_rgba(
        &mut self,
        queue: &wgpu::Queue,
        rgba: &[u8],
        w: u32,
        h: u32,
    ) -> ScreenHudPackedImage {
        let packed = self.inner.pack_rgba(queue, rgba, w, h);
        self.upload_region(rgba, packed.uv_rect, w, h);
        packed
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        self.inner.bind_group.as_ref()
    }

    /// Traduce el `uv_rect` que devolvió el empaquetador a la región del espejo Vulkan.
    fn upload_region(&self, rgba: &[u8], uv_rect: [f32; 4], w: u32, h: u32) {
        let Some(vulkan) = &self.vulkan else {
            return;
        };
        let size = Self::SIZE as f32;
        let x = (uv_rect[0] * size).round() as u32;
        let y = (uv_rect[1] * size).round() as u32;
        // El empaquetador devuelve un UV de fallback (0,0,1/SIZE,1/SIZE) si el atlas se llenó: en
        // ese caso la región no coincide con `w×h` y no hay nada válido que espejar.
        let packed_w = ((uv_rect[2] - uv_rect[0]) * size).round() as u32;
        let packed_h = ((uv_rect[3] - uv_rect[1]) * size).round() as u32;
        if packed_w != w || packed_h != h || x + w > Self::SIZE || y + h > Self::SIZE {
            return;
        }
        if let Err(error) = vulkan.upload_hud_atlas_region(rgba, x, y, w, h) {
            log::warn!("[screen-hud-atlas] no se pudo espejar la región en Vulkan: {error}");
        }
    }
}

/// Instancia para `screen_hud_pipeline`. `tex_layer_pad` = UV rect.
pub fn build_screen_hud_instance(
    packed: ScreenHudPackedImage,
    model: Mat4,
    alpha: f32,
) -> mesh::InstanceData {
    let mut inst = mesh::InstanceData::new(model, 0.0, [0.0; 4]);
    inst.uv_rect = packed.uv_rect;
    inst.flag_pad[1] = alpha;
    inst
}
