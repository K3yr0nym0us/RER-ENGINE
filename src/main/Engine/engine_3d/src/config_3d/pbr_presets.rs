//! Reusable PBR material presets for validation scenes and tooling.

use crate::ecs::SurfacePbr;

/// Procedural albedo variant baked into `texture_array` layers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatValAlbedoKind {
    Solid,
    BrushedSteel,
    GlossyPlastic,
    Glass,
    Water,
}

/// Standard PBR parameters for a material row (no special entity types).
#[derive(Clone, Copy, Debug)]
pub struct PbrMaterialPreset {
    pub id: &'static str,
    /// Nombre en Scene Tree / UI.
    pub label: &'static str,
    /// Texto corto en la placa 3D (sin espacios largos).
    pub label_tag: &'static str,
    pub metallic: f32,
    pub roughness_min: f32,
    pub roughness_max: f32,
    /// Linear RGB 0–1
    pub base_color: [f32; 3],
    /// 0 = metal; >1 = dielectric IOR (glass ~1.5, water ~1.33).
    pub ior: f32,
    /// Opacidad visual (1 = opaco). Vidrio/agua usan <1 sin perder trazado SSR.
    pub opacity: f32,
    pub albedo_kind: MatValAlbedoKind,
}

pub const MAT_VAL_SPHERES_PER_ROW: usize = 5;
pub const MAT_VAL_ROW_COUNT: usize = 5;

pub const MATERIAL_VALIDATION_ROWS: [PbrMaterialPreset; MAT_VAL_ROW_COUNT] = [
    PbrMaterialPreset {
        id: "chrome",
        label: "Chrome",
        label_tag: "CHROME",
        metallic: 1.0,
        roughness_min: 0.0,
        roughness_max: 0.05,
        base_color: [0.08, 0.08, 0.09],
        ior: 0.0,
        opacity: 1.0,
        albedo_kind: MatValAlbedoKind::Solid,
    },
    PbrMaterialPreset {
        id: "brushed_steel",
        label: "Brushed Steel",
        label_tag: "STEEL",
        metallic: 1.0,
        roughness_min: 0.2,
        roughness_max: 0.5,
        base_color: [0.52, 0.54, 0.56],
        ior: 0.0,
        opacity: 1.0,
        albedo_kind: MatValAlbedoKind::BrushedSteel,
    },
    PbrMaterialPreset {
        id: "glossy_plastic",
        label: "Glossy Plastic",
        label_tag: "PLASTIC",
        metallic: 0.0,
        roughness_min: 0.05,
        roughness_max: 0.2,
        base_color: [0.82, 0.18, 0.22],
        ior: 0.0,
        opacity: 1.0,
        albedo_kind: MatValAlbedoKind::GlossyPlastic,
    },
    PbrMaterialPreset {
        id: "glass",
        label: "Glass",
        label_tag: "GLASS",
        metallic: 0.0,
        roughness_min: 0.0,
        roughness_max: 0.1,
        base_color: [0.92, 0.94, 0.96],
        ior: 1.5,
        opacity: 0.38,
        albedo_kind: MatValAlbedoKind::Glass,
    },
    PbrMaterialPreset {
        id: "water",
        label: "Water",
        label_tag: "WATER",
        metallic: 0.0,
        roughness_min: 0.0,
        roughness_max: 0.05,
        base_color: [0.08, 0.28, 0.42],
        ior: 1.33,
        opacity: 0.48,
        albedo_kind: MatValAlbedoKind::Water,
    },
];

/// Roughness for column `col` in `0..cols` (left = min, right = max).
pub fn preset_roughness_at_column(preset: &PbrMaterialPreset, col: usize, cols: usize) -> f32 {
    if cols <= 1 {
        return preset.roughness_min;
    }
    let t = col as f32 / (cols - 1) as f32;
    (preset.roughness_min + t * (preset.roughness_max - preset.roughness_min)).clamp(0.0, 1.0)
}

pub fn surface_pbr_from_preset(preset: &PbrMaterialPreset, roughness: f32) -> SurfacePbr {
    SurfacePbr {
        roughness: roughness.clamp(0.0, 1.0),
        metallic: preset.metallic.clamp(0.0, 1.0),
        ior: preset.ior,
        opacity: preset.opacity.clamp(0.0, 1.0),
    }
}

/// Alpha en `InstanceData.flag_pad.y` → pase transparente + alpha blend final.
pub fn instance_visual_alpha(pbr: &SurfacePbr) -> f32 {
    let mut alpha = pbr.opacity.clamp(0.0, 1.0);
    // Vidrio/agua: si opacity se perdió en save/carga, forzar semitransparencia por IOR.
    if pbr.ior > 1.01 && alpha >= 0.99 {
        alpha = 0.35;
    }
    alpha
}

pub fn uses_transparent_pass(pbr: &SurfacePbr) -> bool {
    instance_visual_alpha(pbr) < 0.99
}

fn linear_to_u8(c: f32) -> u8 {
    (c.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn solid_rgba(base: [f32; 3]) -> Vec<u8> {
    let r = linear_to_u8(base[0]);
    let g = linear_to_u8(base[1]);
    let b = linear_to_u8(base[2]);
    vec![r, g, b, 255]
}

/// Horizontal brushed streaks (fake normal / anisotropy in albedo until normal maps ship).
fn brushed_steel_rgba(base: [f32; 3], w: u32, h: u32) -> Vec<u8> {
    let mut px = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let streak = ((x + y / 3) % 6) as f32 / 6.0;
            let micro = ((x * 3 + y) % 5) as f32 / 5.0 * 0.04;
            let v = 0.88 + streak * 0.10 + micro;
            let i = ((y * w + x) * 4) as usize;
            px[i] = linear_to_u8(base[0] * v);
            px[i + 1] = linear_to_u8(base[1] * v);
            px[i + 2] = linear_to_u8(base[2] * v);
            px[i + 3] = 255;
        }
    }
    px
}

fn plastic_rgba(base: [f32; 3], w: u32, h: u32) -> Vec<u8> {
    let mut px = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let speck = if (x + y) % 7 == 0 { 1.04 } else { 1.0 };
            let i = ((y * w + x) * 4) as usize;
            px[i] = linear_to_u8(base[0] * speck);
            px[i + 1] = linear_to_u8(base[1] * speck);
            px[i + 2] = linear_to_u8(base[2] * speck);
            px[i + 3] = 255;
        }
    }
    px
}

fn glass_rgba(base: [f32; 3], w: u32, h: u32) -> Vec<u8> {
    let mut px = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let edge = if x == 0 || y == 0 || x + 1 == w || y + 1 == h {
                0.92
            } else {
                1.0
            };
            let i = ((y * w + x) * 4) as usize;
            px[i] = linear_to_u8(base[0] * edge);
            px[i + 1] = linear_to_u8(base[1] * edge);
            px[i + 2] = linear_to_u8(base[2] * edge);
            px[i + 3] = 255;
        }
    }
    px
}

/// Static ripple pattern (animated normals not wired in forward pass yet).
fn water_rgba(base: [f32; 3], w: u32, h: u32) -> Vec<u8> {
    let mut px = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let u = x as f32 / w as f32;
            let v = y as f32 / h as f32;
            let wave = (u * 12.0 + v * 8.0).sin() * 0.04 + (u * 5.0 - v * 9.0).cos() * 0.03;
            let tint = 1.0 + wave;
            let i = ((y * w + x) * 4) as usize;
            px[i] = linear_to_u8(base[0] * tint);
            px[i + 1] = linear_to_u8((base[1] + 0.02) * tint);
            px[i + 2] = linear_to_u8((base[2] + 0.05) * tint);
            px[i + 3] = 255;
        }
    }
    px
}

pub fn build_preset_albedo_rgba(preset: &PbrMaterialPreset, size: u32) -> Vec<u8> {
    let w = size.max(4);
    let h = size.max(4);
    match preset.albedo_kind {
        MatValAlbedoKind::Solid => {
            let px = solid_rgba(preset.base_color);
            let mut out = Vec::with_capacity((w * h * 4) as usize);
            for _ in 0..(w * h) {
                out.extend_from_slice(&px);
            }
            out
        }
        MatValAlbedoKind::BrushedSteel => brushed_steel_rgba(preset.base_color, w, h),
        MatValAlbedoKind::GlossyPlastic => plastic_rgba(preset.base_color, w, h),
        MatValAlbedoKind::Glass => glass_rgba(preset.base_color, w, h),
        MatValAlbedoKind::Water => water_rgba(preset.base_color, w, h),
    }
}

/// 5×7 bitmap por fila (MSB = columna izquierda).
fn glyph_rows_5x7(c: char) -> [u8; 7] {
    match c {
        'A' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'C' => [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110],
        'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
        'G' => [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110],
        'H' => [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'I' => [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        'M' => [0b10001, 0b11011, 0b10101, 0b10001, 0b10001, 0b10001, 0b10001],
        'N' => [0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001],
        'O' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        'R' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
        'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
        'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        'W' => [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001],
        _ => [0; 7],
    }
}

fn draw_glyph_5x7(
    px: &mut [u8],
    w: u32,
    h: u32,
    origin_x: i32,
    origin_y: i32,
    scale: u32,
    ch: char,
) {
    let rows = glyph_rows_5x7(ch);
    for (row, &bits) in rows.iter().enumerate() {
        for col in 0..5u32 {
            if (bits >> (4 - col)) & 1 == 0 {
                continue;
            }
            for sy in 0..scale {
                for sx in 0..scale {
                    let x = origin_x + (col * scale + sx) as i32;
                    let y = origin_y + (row as u32 * scale + sy) as i32;
                    if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
                        continue;
                    }
                    let i = ((y as u32 * w + x as u32) * 4) as usize;
                    px[i] = 240;
                    px[i + 1] = 243;
                    px[i + 2] = 248;
                    px[i + 3] = 255;
                }
            }
        }
    }
}

pub fn build_label_rgba(text: &str, accent: [f32; 3]) -> (Vec<u8>, u32, u32) {
    const W: u32 = 112;
    const H: u32 = 24;
    const SCALE: u32 = 2;
    const CHAR_W: i32 = (5 * SCALE + 2) as i32;
    let mut px = vec![0u8; (W * H * 4) as usize];
    let bg = [24u8, 26, 30, 235];
    for chunk in px.chunks_exact_mut(4) {
        chunk.copy_from_slice(&bg);
    }
    let bar_h = 3u32;
    let ar = linear_to_u8(accent[0]);
    let ag = linear_to_u8(accent[1]);
    let ab = linear_to_u8(accent[2]);
    for y in 0..bar_h {
        for x in 0..W {
            let i = ((y * W + x) * 4) as usize;
            px[i] = ar;
            px[i + 1] = ag;
            px[i + 2] = ab;
            px[i + 3] = 255;
        }
    }
    let mut pen_x = 8i32;
    let pen_y = 6i32;
    for ch in text.chars() {
        draw_glyph_5x7(&mut px, W, H, pen_x, pen_y, SCALE, ch);
        pen_x += CHAR_W;
    }
    (px, W, H)
}
