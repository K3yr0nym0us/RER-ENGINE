//! Diagnóstico mínimo de skinning (solo avisos de fallos de carga).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemapDropReason {
    SkinJointIndexOob,
    NotInUnifiedSkeleton,
}

impl RemapDropReason {
    pub fn label(self) -> &'static str {
        match self {
            Self::SkinJointIndexOob => "skin_joint_oob",
            Self::NotInUnifiedSkeleton => "not_in_unified",
        }
    }
}

#[derive(Clone, Debug)]
struct RemapDropSample {
    vert_index:        usize,
    slot:              usize,
    local_joint_index: u32,
    skin_node_index:   Option<usize>,
    weight:            f32,
    reason:            RemapDropReason,
}

#[derive(Default)]
pub struct RemapDropCollector {
    pub total:   usize,
    samples:     Vec<RemapDropSample>,
    max_samples: usize,
}

impl RemapDropCollector {
    pub fn with_max_samples(max_samples: usize) -> Self {
        Self {
            total: 0,
            samples: Vec::new(),
            max_samples,
        }
    }

    pub fn record(
        &mut self,
        vert_index: usize,
        slot: usize,
        local_joint_index: u32,
        skin_node_index: Option<usize>,
        weight: f32,
        reason: RemapDropReason,
    ) {
        self.total += 1;
        if self.samples.len() < self.max_samples {
            self.samples.push(RemapDropSample {
                vert_index,
                slot,
                local_joint_index,
                skin_node_index,
                weight,
                reason,
            });
        }
    }

    pub fn log_if_any(&self, label: &str) {
        if self.total == 0 {
            return;
        }
        let samples: Vec<String> = self
            .samples
            .iter()
            .map(|s| {
                format!(
                    "v{}s{} lj={} node={:?} w={:.3} {}",
                    s.vert_index,
                    s.slot,
                    s.local_joint_index,
                    s.skin_node_index,
                    s.weight,
                    s.reason.label()
                )
            })
            .collect();
        log::warn!(
            "[model_asset] {label} remap_drop={} samples=[{}]",
            self.total,
            samples.join(" ")
        );
    }
}

pub fn log_skinned_unavailable(label: &str, reason: &str) {
    log::warn!("[model_asset] {label} skinning no disponible: {reason}");
}
