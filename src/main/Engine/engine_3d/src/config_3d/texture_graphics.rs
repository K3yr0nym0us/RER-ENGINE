//! Nivel gráfico global de texturas GLB y LOD por distancia a cámara.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum TextureGraphicsTier {
    #[default]
    Low,
    Medium,
    High,
    Ultra,
}

impl TextureGraphicsTier {
    pub fn from_wire(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "low" | "bajo" => Some(Self::Low),
            "medium" | "medio" => Some(Self::Medium),
            "high" | "alto" => Some(Self::High),
            "ultra" => Some(Self::Ultra),
            _ => None,
        }
    }

    pub fn wire(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Ultra => "ultra",
        }
    }

    pub fn max_dimension(self) -> u32 {
        match self {
            Self::Low => 512,
            Self::Medium => 1024,
            Self::High => 2048,
            Self::Ultra => 4096,
        }
    }
}

pub const DEFAULT_TEXTURE_DETAIL_NEAR_M: f32 = 10.0;
pub const TEXTURE_DISTANCE_STEP_M: f32 = 12.0;
pub const MIN_TEXTURE_CAP_PX: u32 = 256;

/// Tope efectivo según distancia: dentro de `near_m` usa `base_cap`; cada `STEP_M` adicional divide entre 2.
pub fn distance_adjusted_cap(base_cap: u32, distance_m: f32, near_m: f32) -> u32 {
    let near = near_m.clamp(1.0, 500.0);
    if distance_m <= near {
        return base_cap;
    }
    let extra = distance_m - near;
    let halvings = (extra / TEXTURE_DISTANCE_STEP_M).floor() as u32;
    let mut cap = base_cap;
    for _ in 0..halvings.min(4) {
        cap = (cap / 2).max(MIN_TEXTURE_CAP_PX);
        if cap <= MIN_TEXTURE_CAP_PX {
            break;
        }
    }
    cap
}

/// Elige la variante embebida más cercana al tope: mayor resolución ≤ cap, o la menor disponible si todas superan cap.
pub fn pick_image_index_for_cap(variants: &[(u32, u32, u32)], cap_px: u32) -> Option<u32> {
    if variants.is_empty() {
        return None;
    }
    let cap = u64::from(cap_px.max(1));
    let mut best_under: Option<(u32, u64)> = None;
    let mut best_over: Option<(u64, u32)> = None;
    for &(idx, w, h) in variants {
        let max_dim = u64::from(w.max(h));
        let area = u64::from(w) * u64::from(h);
        if max_dim <= cap {
            if best_under.is_none_or(|(_, a)| area > a) {
                best_under = Some((idx, area));
            }
        } else if best_over.is_none_or(|(md, _)| max_dim < md) {
            best_over = Some((max_dim, idx));
        } else if best_over.is_some_and(|(md, _)| max_dim == md)
            && let Some((_, prev_idx)) = best_over
        {
            let prev_area = variants
                .iter()
                .find(|(i, _, _)| *i == prev_idx)
                .map(|(_, w, h)| u64::from(*w) * u64::from(*h))
                .unwrap_or(u64::MAX);
            if area < prev_area {
                best_over = Some((max_dim, idx));
            }
        }
    }
    best_under
        .map(|(i, _)| i)
        .or_else(|| best_over.map(|(_, i)| i))
}
