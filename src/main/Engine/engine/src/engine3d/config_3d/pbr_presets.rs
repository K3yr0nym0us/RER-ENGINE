//! Reusable PBR material presets for validation scenes and tooling.

use crate::engine3d::ecs::SurfacePbr;

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
    /// Linear RGB 0–1
    pub base_color: [f32; 3],
    /// 0 = metal; >1 = dielectric IOR (glass ~1.5, water ~1.33).
    pub ior: f32,
    /// Opacidad visual (1 = opaco). Vidrio/agua usan <1 sin perder trazado SSR.
    pub opacity: f32,
    pub albedo_kind: MatValAlbedoKind,
}

pub const MAT_VAL_ROW_COUNT: usize = 5;

pub const MATERIAL_VALIDATION_ROWS: [PbrMaterialPreset; MAT_VAL_ROW_COUNT] = [
    PbrMaterialPreset {
        id: "chrome",
        label: "Chrome",
        label_tag: "CHROME",
        metallic: 1.0,
        roughness_min: 0.0,
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
        base_color: [0.08, 0.28, 0.42],
        ior: 1.33,
        opacity: 0.48,
        albedo_kind: MatValAlbedoKind::Water,
    },
];

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
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'B' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        'C' => [
            0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110,
        ],
        'D' => [
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
        'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        'F' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'G' => [
            0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110,
        ],
        'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'I' => [
            0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10001, 0b10001, 0b10001, 0b10001,
        ],
        'N' => [
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ],
        'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'U' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001,
        ],
        _ => [0; 7],
    }
}

/// El array GPU es 1024²; el quad del letrero es ~4.44:1. Se dibuja en un
/// lienzo lógico ancho y se estira en Y (nearest) para que el resize a cuadrado
/// + el quad vuelvan a dejar las letras con la proporción correcta.
const LABEL_TEX: u32 = 1024;
const LABEL_LOG_H: u32 = 230;

fn plaque_bar_rgb(accent: [f32; 3]) -> [u8; 3] {
    let r = accent[0].clamp(0.0, 1.0);
    let g = accent[1].clamp(0.0, 1.0);
    let b = accent[2].clamp(0.0, 1.0);
    let maxc = r.max(g).max(b);
    let boost = if maxc < 0.38 {
        0.38 / maxc.max(0.04)
    } else {
        1.0
    };
    [
        linear_to_u8((r * boost).min(1.0)),
        linear_to_u8((g * boost).min(1.0)),
        linear_to_u8((b * boost).min(1.0)),
    ]
}

fn blit_logical(px: &mut [u8], x: i32, y: i32, rgba: [u8; 4]) {
    if x < 0 || y < 0 {
        return;
    }
    let x = x as u32;
    let y = y as u32;
    if x >= LABEL_TEX || y >= LABEL_LOG_H {
        return;
    }
    let y0 = y * LABEL_TEX / LABEL_LOG_H;
    let y1 = ((y + 1) * LABEL_TEX / LABEL_LOG_H)
        .max(y0 + 1)
        .min(LABEL_TEX);
    for ty in y0..y1 {
        let i = ((ty * LABEL_TEX + x) * 4) as usize;
        px[i..i + 4].copy_from_slice(&rgba);
    }
}

fn fill_rect_logical(px: &mut [u8], x0: i32, y0: i32, x1: i32, y1: i32, rgba: [u8; 4]) {
    let x0 = x0.max(0);
    let y0 = y0.max(0);
    let x1 = x1.min(LABEL_TEX as i32);
    let y1 = y1.min(LABEL_LOG_H as i32);
    for y in y0..y1 {
        for x in x0..x1 {
            blit_logical(px, x, y, rgba);
        }
    }
}

fn stamp_glyph_cell(
    px: &mut [u8],
    origin: [i32; 2],
    scale: u32,
    cell: [u32; 2],
    offset: [i32; 2],
    rgba: [u8; 4],
) {
    for sy in 0..scale {
        for sx in 0..scale {
            let x = origin[0] + (cell[0] * scale + sx) as i32 + offset[0];
            let y = origin[1] + (cell[1] * scale + sy) as i32 + offset[1];
            blit_logical(px, x, y, rgba);
        }
    }
}

fn draw_glyph_5x7(
    px: &mut [u8],
    origin: [i32; 2],
    scale: u32,
    ch: char,
    fill: [u8; 4],
    outline: [u8; 4],
) {
    let rows = glyph_rows_5x7(ch);
    for (row, &bits) in rows.iter().enumerate() {
        let row = row as u32;
        for col in 0..5u32 {
            if (bits >> (4 - col)) & 1 == 0 {
                continue;
            }
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    stamp_glyph_cell(px, origin, scale, [col, row], [dx, dy], outline);
                }
            }
        }
    }
    for (row, &bits) in rows.iter().enumerate() {
        let row = row as u32;
        for col in 0..5u32 {
            if (bits >> (4 - col)) & 1 == 0 {
                continue;
            }
            stamp_glyph_cell(px, origin, scale, [col, row], [0, 0], fill);
        }
    }
}

pub fn build_label_rgba(text: &str, accent: [f32; 3]) -> (Vec<u8>, u32, u32) {
    const SCALE: u32 = 20;
    const GAP: i32 = 8;
    const CHAR_W: i32 = (5 * SCALE) as i32 + GAP;
    const BAR_H: i32 = 14;
    const BORDER: i32 = 4;
    let mut px = vec![0u8; (LABEL_TEX * LABEL_TEX * 4) as usize];
    let bg = [26u8, 28, 34, 255];
    let border = [62u8, 66, 76, 255];
    let fill = [236u8, 240, 246, 255];
    let outline = [10u8, 11, 14, 255];
    fill_rect_logical(&mut px, 0, 0, LABEL_TEX as i32, LABEL_LOG_H as i32, bg);
    fill_rect_logical(&mut px, 0, 0, LABEL_TEX as i32, BORDER, border);
    fill_rect_logical(
        &mut px,
        0,
        LABEL_LOG_H as i32 - BORDER,
        LABEL_TEX as i32,
        LABEL_LOG_H as i32,
        border,
    );
    fill_rect_logical(&mut px, 0, 0, BORDER, LABEL_LOG_H as i32, border);
    fill_rect_logical(
        &mut px,
        LABEL_TEX as i32 - BORDER,
        0,
        LABEL_TEX as i32,
        LABEL_LOG_H as i32,
        border,
    );
    let bar = plaque_bar_rgb(accent);
    let bar_rgba = [bar[0], bar[1], bar[2], 255];
    fill_rect_logical(
        &mut px,
        BORDER,
        BORDER,
        LABEL_TEX as i32 - BORDER,
        BAR_H,
        bar_rgba,
    );
    let n = text.chars().count() as i32;
    let text_w = (n * CHAR_W - GAP).max(0);
    let mut pen_x = ((LABEL_TEX as i32) - text_w).max(0) / 2;
    let glyph_h = (7 * SCALE) as i32;
    let pen_y = BAR_H + ((LABEL_LOG_H as i32 - BAR_H) - glyph_h).max(0) / 2;
    for ch in text.chars() {
        draw_glyph_5x7(&mut px, [pen_x, pen_y], SCALE, ch, fill, outline);
        pen_x += CHAR_W;
    }
    (px, LABEL_TEX, LABEL_TEX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha_min(px: &[u8]) -> u8 {
        px.chunks_exact(4).map(|c| c[3]).min().unwrap_or(0)
    }

    fn bright_count(px: &[u8]) -> usize {
        px.chunks_exact(4)
            .filter(|c| c[0] > 200 && c[1] > 200 && c[2] > 200)
            .count()
    }

    #[test]
    fn chrome_label_is_opaque_square_with_glyphs() {
        let (px, w, h) = build_label_rgba("CHROME", [0.08, 0.08, 0.09]);
        assert_eq!((w, h), (1024, 1024));
        assert_eq!(px.len(), 1024 * 1024 * 4);
        assert_eq!(alpha_min(&px), 255);
        assert!(bright_count(&px) > 8_000);
    }

    #[test]
    fn steel_label_keeps_opaque_plate() {
        let (px, _, _) = build_label_rgba("STEEL", [0.52, 0.54, 0.56]);
        assert_eq!(alpha_min(&px), 255);
        assert!(bright_count(&px) > 5_000);
    }

    #[test]
    fn particle_name_labels_draw_glyphs() {
        for tag in ["FIRE", "SMOKE", "SAND", "WATER", "FOG"] {
            let (px, _, _) = build_label_rgba(tag, [0.8, 0.4, 0.1]);
            assert_eq!(alpha_min(&px), 255, "{tag}");
            assert!(bright_count(&px) > 4_000, "{tag}");
        }
    }
}
