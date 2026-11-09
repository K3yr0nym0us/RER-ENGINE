//! Atlas de glyphs/UI de pantalla del 2D: shelf packing puro + subida a la imagen Vulkan.
//!
//! El empaquetado y la colocación viven aquí; la imagen y el sampler son del renderer Ash
//! (`upload_hud_atlas_region`). `ScreenHudPackedImage` y los helpers de layout son compartidos con
//! el 3D desde `rer_engine_shared::player_ui`.

use std::sync::Arc;

use glam::Mat4;

pub use rer_engine_shared::player_ui::screen_hud_atlas::ScreenHudPackedImage;

use crate::mesh;
use crate::texture::ShelfPacker;
use crate::vulkan::Vulkan2d;

pub struct ScreenHudAtlas {
    vulkan: Arc<Vulkan2d>,
    packer: ShelfPacker,
    exhausted_notified: bool,
}

impl ScreenHudAtlas {
    pub const SIZE: u32 = 4096;

    pub fn new(vulkan: Arc<Vulkan2d>) -> Self {
        let mut atlas = Self {
            vulkan,
            packer: ShelfPacker::new(Self::SIZE),
            exhausted_notified: false,
        };
        atlas.pack_white_fallback();
        atlas
    }

    pub fn reset(&mut self) {
        self.packer.reset();
        self.exhausted_notified = false;
        self.pack_white_fallback();
    }

    fn pack_white_fallback(&mut self) {
        let _ = self.pack_rgba(&[255, 255, 255, 255], 1, 1);
    }

    /// Empaca RGBA en el atlas y copia los mismos píxeles a la imagen Vulkan.
    pub fn pack_rgba(&mut self, rgba: &[u8], w: u32, h: u32) -> ScreenHudPackedImage {
        let Some((uv, x, y)) = self.packer.pack(w, h) else {
            log::error!(
                "[screen_hud_atlas] atlas lleno — no se puede empacar {w}×{h} (solo HUD pantalla)"
            );
            if !self.exhausted_notified {
                self.exhausted_notified = true;
            }
            let fallback = [0.0, 0.0, 1.0 / Self::SIZE as f32, 1.0 / Self::SIZE as f32];
            return ScreenHudPackedImage {
                uv_rect: fallback,
                pixel_width: w as f32,
                pixel_height: h as f32,
            };
        };
        if let Err(error) = self.vulkan.upload_hud_atlas_region(rgba, x, y, w, h) {
            log::warn!("[screen-hud-atlas] no se pudo subir la región a Vulkan: {error}");
        }
        ScreenHudPackedImage {
            uv_rect: uv,
            pixel_width: w as f32,
            pixel_height: h as f32,
        }
    }
}

/// Instancia para el pipeline HUD de pantalla. `uv_rect` es la sub-región del atlas.
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
