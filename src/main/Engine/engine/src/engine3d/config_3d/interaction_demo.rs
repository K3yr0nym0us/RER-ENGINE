//! Fila demo de interacción de la plantilla 3D (espejo de la fila de partículas).
//!
//! Tres objetos en x=-10 (frente a la fila de partículas en x=+10), con prompt
//! localizado, aura al mirarlos y un `on_interact` Rhai distinto en cada uno:
//! mover, desaparecer y soltar. Se siembran con el resto de la plantilla FP.

use glam::{Quat, Vec3};

use crate::engine3d::ecs::{Interactable, MeshComponent, NonSelectable, SurfacePbr, Transform};
use crate::engine3d::engine::State;
use crate::engine3d::entity_save_meta::{EntitySaveMeta, entity_path_marker};
use crate::engine3d::ipc::{EngineCommand, EngineCommandCommon, EngineEvent, send_event};

/// Misma x que la fila de partículas, pero en el lado opuesto.
pub(crate) const INTERACTION_DEMO_X: f32 = -10.0;
/// Protegido contra la fila de partículas (x=+10) y el suelo de materiales (z=-5..-1).
pub(crate) const INTERACTION_DEMO_Z0: f32 = -1.0;
pub(crate) const INTERACTION_DEMO_Z_STEP: f32 = 2.0;

pub const INTERACTION_DEMO_PATH: &str = "[InteractDemo]";
pub const INTERACTION_DEMO_LABEL_PATH: &str = "[InteractDemoLabel]";

const LABEL_FRONT: f32 = 1.7;
const LABEL_W: f32 = 1.55;
const LABEL_H: f32 = 0.45;

struct DemoSlot {
    name: &'static str,
    label: &'static str,
    accent: [f32; 3],
    y: f32,
    scale: [f32; 3],
    highlight_color: [u8; 3],
    tooltip_id: &'static str,
    hold: bool,
}

// El motor invoca `on_start` y `update` en todo script adjunto: sin estos no-ops
// el log se llena de errores "Function not found: update" cada frame.
fn demo_slots() -> [DemoSlot; 4] {
    [
        DemoSlot {
            name: "InteractDemo · Plataforma",
            label: "MOVE",
            accent: [0.18, 0.85, 0.95],
            y: 0.6,
            scale: [1.8, 1.2, 1.8],
            highlight_color: [31, 153, 224],
            tooltip_id: "demo_move",
            hold: false,
        },
        DemoSlot {
            name: "InteractDemo · Caja",
            label: "VANISH",
            accent: [1.0, 0.55, 0.22],
            y: 0.5,
            scale: [1.1, 1.0, 1.1],
            highlight_color: [242, 64, 89],
            tooltip_id: "demo_vanish",
            hold: false,
        },
        DemoSlot {
            name: "InteractDemo · Orbe",
            label: "DROP",
            accent: [0.78, 0.45, 1.0],
            y: 2.6,
            scale: [0.9, 0.9, 0.9],
            highlight_color: [199, 115, 255],
            tooltip_id: "demo_drop",
            hold: false,
        },
        DemoSlot {
            name: "InteractDemo · Mantener",
            label: "HOLD",
            accent: [1.0, 0.42, 0.12],
            y: 0.6,
            scale: [1.2, 1.2, 1.2],
            highlight_color: [255, 115, 51],
            tooltip_id: "demo_hold",
            hold: true,
        },
    ]
}

fn demo_entity_ids(state: &State, path: &str) -> Vec<crate::engine3d::ecs::EntityId> {
    state
        .save_registry
        .meta
        .iter()
        .filter(|(_, m)| entity_path_marker(&m.path) == Some(path))
        .map(|(&id, _)| id)
        .collect()
}

fn remove_demo_entities(state: &mut State) {
    let mut ids = demo_entity_ids(state, INTERACTION_DEMO_PATH);
    ids.extend(demo_entity_ids(state, INTERACTION_DEMO_LABEL_PATH));
    for id in ids {
        state.handle_command(EngineCommand::Common(EngineCommandCommon::RemoveEntity {
            id,
        }));
    }
}

fn demo_current(state: &State) -> bool {
    let ids = demo_entity_ids(state, INTERACTION_DEMO_PATH);
    ids.len() == 4 && demo_entity_ids(state, INTERACTION_DEMO_LABEL_PATH).len() == 4
}

fn spawn_demo_label(state: &mut State, label: &str, accent: [f32; 3], z: f32) {
    let position = Vec3::new(INTERACTION_DEMO_X + LABEL_FRONT, LABEL_H * 0.5, z);
    // Quad XY mira +Z; +90° en Y lo orienta hacia +X (el frente de esta fila).
    let rotation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
    let scale = Vec3::new(LABEL_W, LABEL_H, 1.0);
    let name = format!("InteractDemoLabel · {label}");
    let id = state.world.spawn(Some(&name));
    let mesh_idx = state.mat_val_label_mesh_idx();
    let tex_idx =
        state.texture_for_label_text(&format!("interact-demo-label:v1:{label}"), label, accent);
    state.world.insert(id, MeshComponent { mesh_idx, tex_idx });
    state.world.insert(id, NonSelectable);
    state.world.insert(
        id,
        SurfacePbr {
            roughness: 0.62,
            metallic: 0.0,
            ior: 0.0,
            opacity: 1.0,
        },
    );
    if let Some(t) = state.world.get_mut::<Transform>(id) {
        t.position = position;
        t.rotation = rotation;
        t.scale = scale;
    }
    state.save_registry.register_meta(
        id,
        EntitySaveMeta {
            kind: "model".to_string(),
            path: INTERACTION_DEMO_LABEL_PATH.to_string(),
            visual_model_path: None,
            entity_category: Some("object".to_string()),
        },
    );
    send_event(&EngineEvent::ModelLoaded {
        id,
        name: Some(name),
        position: Some(position.to_array()),
        scale: Some(scale.to_array()),
        rotation: Some([rotation.x, rotation.y, rotation.z, rotation.w]),
        path: Some(INTERACTION_DEMO_LABEL_PATH.to_string()),
        kind: Some("model".to_string()),
        blueprint_id: None,
        physics_enabled: Some(false),
        physics_type: None,
        entity_category: Some("object".to_string()),
    });
}

/// Siembra (o repara) la fila demo de interacción de la plantilla 3D.
pub fn ensure_interaction_demo(state: &mut State) {
    if demo_current(state) {
        return;
    }
    remove_demo_entities(state);

    for (index, slot) in demo_slots().into_iter().enumerate() {
        let z = INTERACTION_DEMO_Z0 + index as f32 * INTERACTION_DEMO_Z_STEP;
        let id = state.spawn_editor_box(slot.name, [INTERACTION_DEMO_X, slot.y, z], slot.scale);
        state.save_registry.register_meta(
            id,
            EntitySaveMeta {
                kind: "model".to_string(),
                path: INTERACTION_DEMO_PATH.to_string(),
                visual_model_path: None,
                entity_category: Some("object".to_string()),
            },
        );
        state.world.insert(
            id,
            Interactable {
                enabled: true,
                tooltip_id: Some(slot.tooltip_id.to_string()),
                key: "E".to_string(),
                max_distance: 5.0,
                tooltip_distance: 6.0,
                highlight: true,
                highlight_color: slot.highlight_color,
                trigger: if slot.hold {
                    crate::engine3d::ecs::InteractionTrigger::Hold
                } else {
                    crate::engine3d::ecs::InteractionTrigger::Press
                },
                ..Default::default()
            },
        );
        state.world.insert(
            id,
            SurfacePbr {
                roughness: 0.32,
                metallic: 0.12,
                ior: 0.0,
                opacity: 1.0,
            },
        );
        spawn_demo_label(state, slot.label, slot.accent, z);
    }
    log::info!("[interact-demo] fila de 4 interacciones sembrada en x={INTERACTION_DEMO_X}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_row_is_mirrored_from_particle_row() {
        assert_eq!(
            INTERACTION_DEMO_X,
            -crate::engine3d::particles::PARTICLE_DEMO_X
        );
        assert_eq!(
            INTERACTION_DEMO_Z0,
            crate::engine3d::particles::PARTICLE_DEMO_Z0
        );
        assert_eq!(
            INTERACTION_DEMO_Z_STEP,
            crate::engine3d::particles::PARTICLE_DEMO_Z_STEP
        );
    }

    #[test]
    fn demo_row_has_four_distinct_tooltips() {
        let slots = demo_slots();
        assert_eq!(slots.len(), 4);
        let names: Vec<_> = slots.iter().map(|s| s.tooltip_id).collect();
        assert_eq!(
            names,
            ["demo_move", "demo_vanish", "demo_drop", "demo_hold"]
        );
    }
}
