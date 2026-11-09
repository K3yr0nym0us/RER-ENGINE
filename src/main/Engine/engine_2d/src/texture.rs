//! Atlas de sprites del 2D: shelf packing puro + subida a la imagen Vulkan del renderer.

use std::sync::Arc;

use crate::ipc::{EngineEvent, send_event};
use crate::vulkan::Vulkan2d;

/// Empaquetador shelf puro (sin GPU). Calcula la región de cada imagen en un atlas cuadrado.
pub(crate) struct ShelfPacker {
    width: u32,
    height: u32,
    pub(crate) cursor_x: u32,
    pub(crate) cursor_y: u32,
    pub(crate) row_h: u32,
}

impl ShelfPacker {
    pub(crate) fn new(size: u32) -> Self {
        Self {
            width: size,
            height: size,
            cursor_x: 0,
            cursor_y: 0,
            row_h: 0,
        }
    }

    pub(crate) fn reset(&mut self) {
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.row_h = 0;
    }

    /// Coloca `w×h` y devuelve `(uv_rect, x, y)`. `None` si el atlas está lleno.
    pub(crate) fn pack(&mut self, w: u32, h: u32) -> Option<([f32; 4], u32, u32)> {
        // Avanzar a la siguiente fila si la imagen no cabe en la actual.
        if self.cursor_x + w > self.width {
            self.cursor_y += self.row_h;
            self.cursor_x = 0;
            self.row_h = 0;
        }
        if self.cursor_y + h > self.height {
            return None;
        }
        let (x, y) = (self.cursor_x, self.cursor_y);
        let uv = [
            x as f32 / self.width as f32,
            y as f32 / self.height as f32,
            (x + w) as f32 / self.width as f32,
            (y + h) as f32 / self.height as f32,
        ];
        self.cursor_x += w;
        self.row_h = self.row_h.max(h);
        Some((uv, x, y))
    }
}

// ── Texture Atlas ─────────────────────────────────────────────────────────────
//
// Empaca múltiples imágenes RGBA en una sola imagen Vulkan 4096×4096 usando shelf packing
// (filas horizontales). Cada textura empacada recibe un UV rect [u_min, v_min, u_max, v_max] que
// el shader usa para muestrear la sub-región correcta del atlas.
pub struct TextureAtlas {
    vulkan: Arc<Vulkan2d>,
    packer: ShelfPacker,
    /// Evita spam de IPC/log cuando el atlas ya no tiene hueco reutilizable.
    exhausted_notified: bool,
}

impl TextureAtlas {
    pub const SIZE: u32 = 4096;

    /// Crea el atlas vacío y empaca un pixel blanco 1×1 en la posición (0,0)
    /// que actúa como UV de fallback cuando un tex_idx es inválido.
    pub fn new(vulkan: Arc<Vulkan2d>) -> Self {
        let mut atlas = Self {
            vulkan,
            packer: ShelfPacker::new(Self::SIZE),
            exhausted_notified: false,
        };
        atlas.pack_raw(&[255, 255, 255, 255], 1, 1);
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
    pub fn reset(&mut self) {
        self.packer.reset();
        self.exhausted_notified = false;
        self.pack_raw(&[255, 255, 255, 255], 1, 1);
    }

    /// Empaca una imagen RGBA en el atlas usando shelf packing.
    /// Retorna el UV rect [u_min, v_min, u_max, v_max] listo para el shader.
    pub fn pack(&mut self, rgba: &[u8], w: u32, h: u32) -> [f32; 4] {
        self.pack_raw(rgba, w, h)
    }

    fn pack_raw(&mut self, rgba: &[u8], w: u32, h: u32) -> [f32; 4] {
        let Some((uv, x, y)) = self.packer.pack(w, h) else {
            log::error!(
                "[TextureAtlas] atlas lleno — no se puede empacar {}×{}",
                w,
                h
            );
            if !self.exhausted_notified {
                self.exhausted_notified = true;
                send_event(&EngineEvent::AtlasExhausted {
                    atlas_size: Self::SIZE,
                    width: w,
                    height: h,
                });
            }
            return Self::fallback_uv();
        };
        if let Err(error) = self.vulkan.upload_atlas_region(rgba, x, y, w, h) {
            log::warn!("[TextureAtlas] no se pudo subir la región a Vulkan: {error}");
        }
        uv
    }

    /// Sobreescribe los píxeles de una región ya empacada (mismas dimensiones).
    /// Usado por `enter_pivot_edit_mode` para mostrar el frame temporalmente.
    pub fn update(&self, rgba: &[u8], uv_rect: [f32; 4]) {
        let size = Self::SIZE as f32;
        let x = (uv_rect[0] * size).round() as u32;
        let y = (uv_rect[1] * size).round() as u32;
        let w = ((uv_rect[2] - uv_rect[0]) * size).round() as u32;
        let h = ((uv_rect[3] - uv_rect[1]) * size).round() as u32;
        if w == 0 || h == 0 {
            return;
        }
        if let Err(error) = self.vulkan.upload_atlas_region(rgba, x, y, w, h) {
            log::warn!("[TextureAtlas] no se pudo actualizar la región en Vulkan: {error}");
        }
    }
}
