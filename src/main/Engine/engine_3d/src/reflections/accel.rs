//! TLAS simplificado v1: AABB mundo por instancia estática (sin skinned).

use bytemuck::{Pod, Zeroable};

use crate::ecs::{MeshComponent, Transform};
use crate::engine::State;

pub const MAX_STATIC_REFLECTION_INSTANCES: usize = 256;

#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct StaticInstanceGpu {
    pub min: [f32; 4],
    pub max: [f32; 4],
}

/// Recolecta cajas mundo de mallas estáticas visibles en escena (excluye skinned / jugador).
pub fn collect_static_instances(state: &State) -> Vec<StaticInstanceGpu> {
    let mut out = Vec::new();
    for &entity in state.world.entities() {
        if state.model_animation_bindings.contains_key(&entity) {
            continue;
        }
        if state.play_character_entity == Some(entity) {
            continue;
        }
        if state.world.get::<MeshComponent>(entity).is_none() {
            continue;
        }
        let Some(t) = state.world.get::<Transform>(entity) else {
            continue;
        };
        let (center, half) = state.entity_world_pick_aabb(entity, t);
        let min = center - half;
        let max = center + half;
        out.push(StaticInstanceGpu {
            min: [min.x, min.y, min.z, 0.0],
            max: [max.x, max.y, max.z, 0.0],
        });
        if out.len() >= MAX_STATIC_REFLECTION_INSTANCES {
            break;
        }
    }
    out
}
