//! Readback 1×1 Rgba32Float del pass `fs_log` en `ssr.wgsl` (centro de pantalla).
#![allow(dead_code)]

/// ~3 s a 60 FPS (~0,3 logs/s). Antes: 45 frames (~1,3/s).
const LOG_COOLDOWN_FRAMES: u32 = 180;

pub struct SsrTraceReadback {
    staging: wgpu::Buffer,
    pending: bool,
    cooldown: u32,
}

impl SsrTraceReadback {
    pub fn new(device: &wgpu::Device) -> Self {
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ssr-trace-readback"),
            size: 256,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Self {
            staging,
            pending: false,
            cooldown: 0,
        }
    }

    /// `true` cuando toca muestrear (y avanza el cooldown en frames omitidos).
    pub fn should_sample(&mut self) -> bool {
        if self.cooldown > 0 {
            self.cooldown -= 1;
            return false;
        }
        true
    }

    pub fn queue_pixel(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        texture: &wgpu::Texture,
    ) {
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.staging,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(1),
                },
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        self.pending = true;
    }

    /// Tras `queue.submit`, registra traza SSR del centro de pantalla.
    pub fn finish_and_log(&mut self, device: &wgpu::Device) {
        if !self.pending {
            return;
        }
        self.pending = false;
        self.cooldown = LOG_COOLDOWN_FRAMES;

        let slice = self.staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        if rx.recv().ok().and_then(|r| r.ok()).is_none() {
            log::warn!("[SSR] readback: falló map_async");
            return;
        }

        let data = slice.get_mapped_range();
        if data.len() < 16 {
            drop(data);
            self.staging.unmap();
            return;
        }
        let r = f32::from_le_bytes(data[0..4].try_into().unwrap());
        let g = f32::from_le_bytes(data[4..8].try_into().unwrap());
        let b = f32::from_le_bytes(data[8..12].try_into().unwrap());
        let a = f32::from_le_bytes(data[12..16].try_into().unwrap());
        drop(data);
        self.staging.unmap();

        if r >= 0.0 {
            let hit_u = r;
            let hit_v = g;
            let ray_depth = b;
            let hit_depth = a;
            let depth_delta = ray_depth - hit_depth;
            log::info!(
                "[SSR] centro hit_uv=({hit_u:.4}, {hit_v:.4}) hit_depth={hit_depth:.4}m ray_depth={ray_depth:.4}m depth_delta={depth_delta:.4}m (surf_uv=0.5,0.5)"
            );
            return;
        }

        let code = r.round() as i32;
        match code {
            -1 => log::info!(
                "[SSR] centro miss=cielo (surf_uv=0.5,0.5) — sin geometría; orbita hacia RefTest R0"
            ),
            -2 => log::info!(
                "[SSR] centro miss=rugosidad rough={g:.3} metallic={b:.3} depth={a:.3}m — superficie mate o fuera del límite del tier"
            ),
            -3 => log::info!(
                "[SSR] centro miss=dir_refl rough={g:.3} metallic={b:.3} strength={a:.4} — normal/reflexión incoherente"
            ),
            -4 => log::info!(
                "[SSR] centro miss=marcha rough={g:.3} metallic={b:.3} strength={a:.4} — traza OK pero sin intersección en pantalla"
            ),
            -5 => log::info!(
                "[SSR] centro miss=fuerza rough={g:.3} metallic={b:.3} strength={a:.4} — Fresnel bajo"
            ),
            _ => log::info!(
                "[SSR] centro miss=código {code} rough={g:.3} metallic={b:.3} aux={a:.4}"
            ),
        }
    }
}
