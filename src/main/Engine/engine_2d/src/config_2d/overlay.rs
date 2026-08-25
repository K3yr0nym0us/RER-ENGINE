use glam::Vec3 as GlamVec3;

use crate::ecs::Transform;
use crate::engine::State;
use crate::gizmo::{self, GizmoVertex};
use crate::mesh::{Mesh, Vertex, upload};

use super::character_collision_shape;

/// Quad en el plano XY (normal +Z).
/// `cx`, `cy` = centro en mundo  |  `w`, `h` = ancho y alto  |  UVs: 0..1
pub(super) fn create_quad_xy(
    device: &wgpu::Device,
    cx: f32,
    cy: f32,
    w: f32,
    h: f32,
    label: &str,
) -> Mesh {
    let hw = w / 2.0;
    let hh = h / 2.0;
    let vertices = vec![
        Vertex {
            position: [cx - hw, cy - hh, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [cx + hw, cy - hh, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [cx + hw, cy + hh, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [cx - hw, cy + hh, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 0.0],
        },
    ];
    let indices = vec![0u32, 1, 2, 2, 3, 0];
    upload(device, &vertices, &indices, label)
}

/// Construye el GizmoBuffer (LineList) de overlay para la herramienta de dibujo.
pub(super) fn build_tool_overlay(
    device: &wgpu::Device,
    pts: &[[f32; 2]],
    cursor: Option<[f32; 2]>,
) -> gizmo::GizmoBuffer {
    const ARM: f32 = 0.15;
    const Z: f32 = 0.1;
    let cross_color = [1.0_f32, 1.0, 1.0, 1.0]; // blanco
    let line_color = [1.0_f32, 0.75, 0.0, 1.0]; // naranja

    let mut verts: Vec<GizmoVertex> = Vec::new();

    // Cruz en cada punto acumulado
    for p in pts {
        let [x, y] = *p;
        verts.push(GizmoVertex {
            position: [x - ARM, y, Z],
            color: cross_color,
        });
        verts.push(GizmoVertex {
            position: [x + ARM, y, Z],
            color: cross_color,
        });
        verts.push(GizmoVertex {
            position: [x, y - ARM, Z],
            color: cross_color,
        });
        verts.push(GizmoVertex {
            position: [x, y + ARM, Z],
            color: cross_color,
        });
    }

    // Líneas entre puntos consecutivos
    for i in 0..pts.len().saturating_sub(1) {
        let [ax, ay] = pts[i];
        let [bx, by] = pts[i + 1];
        verts.push(GizmoVertex {
            position: [ax, ay, Z],
            color: line_color,
        });
        verts.push(GizmoVertex {
            position: [bx, by, Z],
            color: line_color,
        });
    }

    if let (Some(last), Some(cur)) = (pts.last().copied(), cursor) {
        verts.push(GizmoVertex {
            position: [last[0], last[1], Z],
            color: line_color,
        });
        verts.push(GizmoVertex {
            position: [cur[0], cur[1], Z],
            color: line_color,
        });

        // Preview de cierre del polígono: al definir el 4to punto, mostrar también
        // la línea desde el primer punto hasta el cursor para cuadrar mejor.
        if pts.len() >= 3 {
            let first = pts[0];
            verts.push(GizmoVertex {
                position: [first[0], first[1], Z],
                color: line_color,
            });
            verts.push(GizmoVertex {
                position: [cur[0], cur[1], Z],
                color: line_color,
            });
        }
    }

    gizmo::build_from_vertices(device, &verts)
}

/// Borde cyan + cruceta amarilla en el pivot actual del frame.
/// pivot_x, pivot_y: coordenadas en píxeles dentro del frame (0,0 = esquina superior-izquierda).
pub(super) fn build_pivot_edit_overlay_with_cross(
    device: &wgpu::Device,
    pos: GlamVec3,
    scale: GlamVec3,
    pivot_x: f32,
    pivot_y: f32,
    img_w: u32,
    img_h: u32,
) -> gizmo::GizmoBuffer {
    let left = pos.x - scale.x * 0.5;
    let right = pos.x + scale.x * 0.5;
    let bottom = pos.y - scale.y * 0.5;
    let top = pos.y + scale.y * 0.5;
    const Z: f32 = 0.2;
    let border_color = [0.2_f32, 0.9, 1.0, 1.0]; // cyan

    let mut verts = vec![
        GizmoVertex {
            position: [left, bottom, Z],
            color: border_color,
        },
        GizmoVertex {
            position: [right, bottom, Z],
            color: border_color,
        },
        GizmoVertex {
            position: [right, bottom, Z],
            color: border_color,
        },
        GizmoVertex {
            position: [right, top, Z],
            color: border_color,
        },
        GizmoVertex {
            position: [right, top, Z],
            color: border_color,
        },
        GizmoVertex {
            position: [left, top, Z],
            color: border_color,
        },
        GizmoVertex {
            position: [left, top, Z],
            color: border_color,
        },
        GizmoVertex {
            position: [left, bottom, Z],
            color: border_color,
        },
    ];

    // Cruceta en el pivot actual (solo si el pivot tiene coordenadas válidas)
    if img_w > 0 && img_h > 0 {
        let px = left + (pivot_x / img_w as f32) * scale.x;
        let py = top - (pivot_y / img_h as f32) * scale.y;
        let s = (scale.x.min(scale.y) * 0.07).max(0.005);
        let cross_color = [1.0_f32, 1.0, 0.0, 1.0]; // amarillo

        verts.extend_from_slice(&[
            GizmoVertex {
                position: [px - s, py, Z],
                color: cross_color,
            },
            GizmoVertex {
                position: [px + s, py, Z],
                color: cross_color,
            },
            GizmoVertex {
                position: [px, py - s, Z],
                color: cross_color,
            },
            GizmoVertex {
                position: [px, py + s, Z],
                color: cross_color,
            },
        ]);
    }

    gizmo::build_from_vertices(device, &verts)
}

/// Overlay naranja para el área lógica: rectángulo centrado en la entidad
/// con las dimensiones del bounding box lógico (w×h píxeles → mundo).
pub(super) fn build_logical_area_overlay(
    device: &wgpu::Device,
    pos: GlamVec3,
    orig_scale_y: f32,
    w: u32,
    h: u32,
) -> gizmo::GizmoBuffer {
    if h == 0 {
        return gizmo::build_from_vertices(device, &[]);
    }
    let aspect = w as f32 / h as f32;
    let world_h = orig_scale_y;
    let world_w = world_h * aspect;
    let left = pos.x - world_w * 0.5;
    let right = pos.x + world_w * 0.5;
    let bottom = pos.y - world_h * 0.5;
    let top = pos.y + world_h * 0.5;
    const Z: f32 = 0.15;
    let color = [1.0_f32, 0.42, 0.05, 1.0]; // naranja (área lógica del frame, no colisión)

    let verts = vec![
        GizmoVertex {
            position: [left, bottom, Z],
            color,
        },
        GizmoVertex {
            position: [right, bottom, Z],
            color,
        },
        GizmoVertex {
            position: [right, bottom, Z],
            color,
        },
        GizmoVertex {
            position: [right, top, Z],
            color,
        },
        GizmoVertex {
            position: [right, top, Z],
            color,
        },
        GizmoVertex {
            position: [left, top, Z],
            color,
        },
        GizmoVertex {
            position: [left, top, Z],
            color,
        },
        GizmoVertex {
            position: [left, bottom, Z],
            color,
        },
    ];

    gizmo::build_from_vertices(device, &verts)
}

/// Overlay de líneas para escenarios con colisión activa.
/// Se dibuja como un contorno AABB para que el editor muestre claramente
/// el área física del escenario y de los personajes, similar a las collision shapes visibles.
pub(crate) fn build_scenario_collision_overlay(
    device: &wgpu::Device,
    state: &State,
) -> gizmo::GizmoBuffer {
    if (!state.debug_mode && state.preview_playing) || state.camera_2d.is_none() {
        return gizmo::build_from_vertices(device, &[]);
    }

    let mut verts: Vec<GizmoVertex> = Vec::new();
    const Z: f32 = 0.18;
    let color = [0.2_f32, 0.9, 1.0, 1.0]; // cyan

    for &entity_id in &state.scenario_entities {
        if !state.physics_2d.has_physics(entity_id) {
            continue;
        }

        let Some(t) = state.world.get::<Transform>(entity_id) else {
            continue;
        };

        let left = t.position.x - t.scale.x * 0.5;
        let right = t.position.x + t.scale.x * 0.5;
        let bottom = t.position.y - t.scale.y * 0.5;
        let top = t.position.y + t.scale.y * 0.5;

        verts.extend_from_slice(&[
            GizmoVertex {
                position: [left, bottom, Z],
                color,
            },
            GizmoVertex {
                position: [right, bottom, Z],
                color,
            },
            GizmoVertex {
                position: [right, bottom, Z],
                color,
            },
            GizmoVertex {
                position: [right, top, Z],
                color,
            },
            GizmoVertex {
                position: [right, top, Z],
                color,
            },
            GizmoVertex {
                position: [left, top, Z],
                color,
            },
            GizmoVertex {
                position: [left, top, Z],
                color,
            },
            GizmoVertex {
                position: [left, bottom, Z],
                color,
            },
        ]);
    }

    let character_color = [0.25_f32, 0.95, 1.0, 1.0]; // cyan (colisión tight_bounds, distinto del área lógica)
    for &entity_id in &state.character_entities {
        if !state.physics_2d.has_physics(entity_id) {
            continue;
        }

        let Some(t) = state.world.get::<Transform>(entity_id) else {
            continue;
        };
        let Some((half_ext, local_offset)) = character_collision_shape(state, entity_id) else {
            continue;
        };

        let center_x = t.position.x + local_offset[0];
        let center_y = t.position.y + local_offset[1];
        let left = center_x - half_ext[0];
        let right = center_x + half_ext[0];
        let bottom = center_y - half_ext[1];
        let top = center_y + half_ext[1];

        verts.extend_from_slice(&[
            GizmoVertex {
                position: [left, bottom, Z],
                color: character_color,
            },
            GizmoVertex {
                position: [right, bottom, Z],
                color: character_color,
            },
            GizmoVertex {
                position: [right, bottom, Z],
                color: character_color,
            },
            GizmoVertex {
                position: [right, top, Z],
                color: character_color,
            },
            GizmoVertex {
                position: [right, top, Z],
                color: character_color,
            },
            GizmoVertex {
                position: [left, top, Z],
                color: character_color,
            },
            GizmoVertex {
                position: [left, top, Z],
                color: character_color,
            },
            GizmoVertex {
                position: [left, bottom, Z],
                color: character_color,
            },
        ]);
    }

    gizmo::build_from_vertices(device, &verts)
}
