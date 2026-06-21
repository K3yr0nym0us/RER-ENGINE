//! Readback de `depth_export` (R32Float) — infra de diagnóstico (sin logs activos).
#![allow(dead_code)]

pub struct DepthExportReadback {
    staging: wgpu::Buffer,
    bytes_per_row: u32,
    width: u32,
    height: u32,
    pending: bool,
    cooldown: u32,
}

pub struct DepthExportStats {
    pub min: f32,
    pub max: f32,
    pub avg: f32,
    pub center: f32,
    pub max_px: (u32, u32),
    pub nonzero_pixels: u32,
    pub total_pixels: u32,
}

impl DepthExportReadback {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let w = width.max(1);
        let h = height.max(1);
        let unpadded = w * 4;
        let bytes_per_row = unpadded.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let size = (bytes_per_row * h) as u64;
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("depth-export-readback"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Self {
            staging,
            bytes_per_row,
            width: w,
            height: h,
            pending: false,
            cooldown: 0,
        }
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        *self = Self::new(device, width, height);
    }

    /// Copia la textura justo antes del submit (contenido del G-buffer).
    pub fn queue_copy(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        texture: &wgpu::Texture,
    ) {
        if self.cooldown > 0 {
            self.cooldown -= 1;
            return;
        }
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
                    bytes_per_row: Some(self.bytes_per_row),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        self.pending = true;
    }

    /// Tras `queue.submit`, mapea el staging y devuelve
    pub fn finish_and_stats(&mut self, device: &wgpu::Device) -> Option<DepthExportStats> {
        if !self.pending {
            return None;
        }
        self.pending = false;
        self.cooldown = 30;

        let slice = self.staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        if rx.recv().ok()?.is_err() {
            log::warn!("[depth-export] readback: falló map_async");
            return None;
        }

        let stats = {
            let data = slice.get_mapped_range();
            scan_depth_f32(&data, self.width, self.height, self.bytes_per_row)
        };
        self.staging.unmap();
        Some(stats)
    }
}

fn scan_depth_f32(
    mapped: &[u8],
    width: u32,
    height: u32,
    bytes_per_row: u32,
) -> DepthExportStats {
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    let mut sum = 0.0f64;
    let mut nonzero = 0u32;
    let total = width * height;
    let cx = width / 2;
    let cy = height / 2;
    let mut center = 0.0f32;
    let mut max_px = (0u32, 0u32);

    for y in 0..height {
        let row_off = (y * bytes_per_row) as usize;
        for x in 0..width {
            let off = row_off + (x as usize) * 4;
            let v = f32::from_le_bytes([
                mapped[off],
                mapped[off + 1],
                mapped[off + 2],
                mapped[off + 3],
            ]);
            if !v.is_finite() {
                continue;
            }
            if v < min {
                min = v;
            }
            if v > max {
                max = v;
                max_px = (x, y);
            }
            sum += f64::from(v);
            if v > 0.000_1 {
                nonzero += 1;
            }
            if x == cx && y == cy {
                center = v;
            }
        }
    }

    if !min.is_finite() {
        min = 0.0;
    }
    if !max.is_finite() {
        max = 0.0;
    }

    DepthExportStats {
        min,
        max,
        avg: (sum / f64::from(total.max(1))) as f32,
        center,
        max_px,
        nonzero_pixels: nonzero,
        total_pixels: total,
    }
}

pub fn log_depth_export_stats(
    stats: &DepthExportStats,
    near_plane: f32,
    far_plane: f32,
    debug_label: &str,
) {
    let writes = stats.nonzero_pixels > 0 && stats.max > 0.000_1;
    log::info!(
        "[depth-export] {debug_label} min={:.6} max={:.6} avg={:.6} centro_pantalla={:.6} max_px=({}, {}) valor_max={:.6} píxeles>0={}/{} near={near_plane} far={far_plane} escritura={}",
        stats.min,
        stats.max,
        stats.avg,
        stats.center,
        stats.max_px.0,
        stats.max_px.1,
        stats.max,
        stats.nonzero_pixels,
        stats.total_pixels,
        if writes { "SÍ" } else { "NO" },
    );
}

/// Cruce CPU: clip desde `view_proj` para un punto de prueba (log una vez por readback).
pub fn log_depth_probe_cpu(
    view_proj: &[[f32; 4]; 4],
    inv_view_proj: &[[f32; 4]; 4],
    world: [f32; 3],
    near: f32,
    far: f32,
) {
    let vp = glam::Mat4::from_cols_array_2d(view_proj);
    let inv_vp = glam::Mat4::from_cols_array_2d(inv_view_proj);
    let clip = vp * glam::Vec4::new(world[0], world[1], world[2], 1.0);
    let clip_z = clip.z;
    let clip_w = clip.w;
    let ndc_gl = clip_z / clip_w;
    let ndc_vk = ndc_gl * 0.5 + 0.5;
    let linear_vk = if ndc_vk >= 0.0 && ndc_vk <= 1.0 {
        (near * far) / (far - ndc_vk * (far - near))
    } else {
        f32::NAN
    };
    let linear_gl = (2.0 * near * far) / (far + near - ndc_gl * (far - near));

    let ndc_x = clip.x / clip_w;
    let ndc_y = clip.y / clip_w;
    let ndc_z_gl = ndc_vk * 2.0 - 1.0;
    let unproj = inv_vp * glam::Vec4::new(ndc_x, ndc_y, ndc_z_gl, 1.0);
    let world_rt = unproj.truncate() / unproj.w;
    let clip2 = vp * glam::Vec4::new(world_rt.x, world_rt.y, world_rt.z, 1.0);
    let ndc2 = clip2.truncate() / clip2.w;
    let uv = glam::Vec2::new(ndc2.x * 0.5 + 0.5, 1.0 - (ndc2.y * 0.5 + 0.5));
    let uv_from_clip = glam::Vec2::new(
        (clip.x / clip.w) * 0.5 + 0.5,
        1.0 - ((clip.y / clip.w) * 0.5 + 0.5),
    );
    let reproj_err = (uv - uv_from_clip).length();

    log::info!(
        "[depth-probe CPU] world=({:.3},{:.3},{:.3}) clip_z={:.6} clip_w={:.6} ndc_gl={:.6} ndc_vk={:.6} linear_vk={:.6}m linear_gl={:.6}m reproj_err={:.6} world_rt=({:.3},{:.3},{:.3})",
        world[0],
        world[1],
        world[2],
        clip_z,
        clip_w,
        ndc_gl,
        ndc_vk,
        linear_vk,
        linear_gl,
        reproj_err,
        world_rt.x,
        world_rt.y,
        world_rt.z,
    );
}
