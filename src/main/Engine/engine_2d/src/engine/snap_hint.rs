use crate::mesh;

use super::State;

impl State {
    pub(super) fn load_snap_hint_uv(&mut self, filename: &str) -> (Option<[f32; 4]>, (f32, f32)) {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../assets")
            .join(filename);
        match std::fs::read(&path) {
            Ok(bytes) => {
                use image::ImageReader;
                match ImageReader::new(std::io::Cursor::new(&bytes))
                    .with_guessed_format()
                    .map_err(|e| e.to_string())
                    .and_then(|r| r.decode().map_err(|e| e.to_string()))
                {
                    Ok(img) => {
                        let img = img.to_rgba8();
                        let (w, h) = img.dimensions();
                        let uv = self.atlas.pack(&self.queue, img.as_raw(), w, h);
                        (Some(uv), (w as f32, h as f32))
                    }
                    Err(e) => {
                        log::warn!("[snap-hint] Error decodificando '{}': {}", path.display(), e);
                        (None, (0.0, 0.0))
                    }
                }
            }
            Err(e) => {
                log::warn!("[snap-hint] No se pudo leer '{}': {}", path.display(), e);
                (None, (0.0, 0.0))
            }
        }
    }

    pub(crate) fn reload_snap_hint_assets(&mut self) {
        let (uv_es, size_es) = self.load_snap_hint_uv("tooltip-btn-ctrl-to-auto-adjust.png");
        let (uv_en, size_en) = self.load_snap_hint_uv("tooltip-btn-ctrl-to-auto-adjust-english.png");
        self.snap_hint_uv = uv_es;
        self.snap_hint_size = size_es;
        self.snap_hint_uv_en = uv_en;
        self.snap_hint_size_en = size_en;
    }

    pub(super) fn update_snap_hint_alpha(&mut self) {
        let target = if self.show_snap_hint && !self.preview_playing && self.camera_2d.is_some() {
            1.0_f32
        } else {
            0.0_f32
        };
        // Suavizado exponencial frame-rate independiente.
        // Menor k => transición más visible y menos "instantánea".
        let k = if target > self.snap_hint_alpha { 4.2_f32 } else { 3.4_f32 };
        let blend = 1.0 - (-k * self.delta_time.max(0.0)).exp();
        self.snap_hint_alpha += (target - self.snap_hint_alpha) * blend;
        if (self.snap_hint_alpha - target).abs() < 0.001 {
            self.snap_hint_alpha = target;
        }
    }

    pub(super) fn build_snap_hint_instance_2d(&self) -> Option<mesh::InstanceData> {
        if self.snap_hint_alpha <= 0.003 || self.preview_playing {
            return None;
        }
        let (uv, img_w, img_h) = if self.snap_locale == "en" {
            let uv = self.snap_hint_uv_en.or(self.snap_hint_uv)?;
            let (w, h) = if self.snap_hint_uv_en.is_some() { self.snap_hint_size_en } else { self.snap_hint_size };
            (uv, w, h)
        } else {
            let uv = self.snap_hint_uv.or(self.snap_hint_uv_en)?;
            let (w, h) = if self.snap_hint_uv.is_some() { self.snap_hint_size } else { self.snap_hint_size_en };
            (uv, w, h)
        };
        let Some(cam) = &self.camera_2d else {
            return None;
        };
        if self.size.width == 0 || self.size.height == 0 {
            return None;
        }
        if img_w <= 0.0 || img_h <= 0.0 {
            return None;
        }

        let aspect = self.size.width as f32 / self.size.height as f32;
        let half_w = cam.half_h * aspect;
        let world_per_px_x = (half_w * 2.0) / self.size.width as f32;
        let world_per_px_y = (cam.half_h * 2.0) / self.size.height as f32;

        let margin_px = 18.0_f32;
        // Tamaño proporcional al viewport pero con tope para evitar que se vea enorme.
        let max_width_px = (self.size.width as f32 * 0.22).clamp(120.0, 320.0);
        let scale_px = (max_width_px / img_w).min(1.0);
        let draw_w_px = img_w * scale_px;
        let draw_h_px = img_h * scale_px;

        let draw_w_world = draw_w_px * world_per_px_x;
        let draw_h_world = draw_h_px * world_per_px_y;
        let margin_x_world = margin_px * world_per_px_x;
        let margin_y_world = margin_px * world_per_px_y;

        // Easing para que se perciba mejor la transición.
        let a = self.snap_hint_alpha.clamp(0.0, 1.0);
        let eased_alpha = a * a * (3.0 - 2.0 * a);
        let scale_in = 0.92 + 0.08 * eased_alpha;
        let slide_px = (1.0 - eased_alpha) * 14.0;

        let center_x = cam.x - half_w + margin_x_world + draw_w_world * 0.5;
        let center_y = cam.y + cam.half_h - margin_y_world - draw_h_world * 0.5 - slide_px * world_per_px_y;
        let model = glam::Mat4::from_translation(glam::vec3(center_x, center_y, 0.9))
            * glam::Mat4::from_scale(glam::vec3(draw_w_world * scale_in, draw_h_world * scale_in, 1.0));
        let mut inst = mesh::InstanceData::new(model, 0.0, uv);
        inst.flag_pad[1] = eased_alpha;
        Some(inst)
    }
}
