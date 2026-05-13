// ── Grid 2D — cuadrícula y límites del mundo ─────────────────────────────────
//
// Genera un vertex buffer de líneas (LineList) que representa:
//  · Los límites del área de trabajo del mundo (rectángulo blanco)
//  · Las líneas de cuadrícula internas a intervalos de cell_size
//  · Los ejes de origen (X rojo, Y verde) ligeramente marcados
//
// El buffer se regenera cada vez que cambia la configuración.

use wgpu::util::DeviceExt;

use crate::gizmo::GizmoVertex;

// ── Configuración del mundo / cuadrícula ─────────────────────────────────────

pub struct GridConfig {
    /// Ancho total del área de trabajo en unidades de mundo.
    pub world_width:  f32,
    /// Alto total del área de trabajo en unidades de mundo.
    pub world_height: f32,
    /// Si la cuadrícula es visible.
    pub visible:      bool,
    /// Tamaño de cada celda en unidades de mundo.
    pub cell_size:    f32,
}

impl Default for GridConfig {
    fn default() -> Self {
        Self {
            world_width:  100.0,
            world_height: 50.0,
            visible:      true,
            cell_size:    1.0,
        }
    }
}

// ── Buffer de vértices ────────────────────────────────────────────────────────

pub struct GridBuffer {
    pub vertex_buffer: wgpu::Buffer,
    pub vertex_count:  u32,
}

/// Construye el vertex buffer de la cuadrícula a partir de la configuración actual.
/// El eje Z de todas las líneas es 0.5 para que queden en el plano de la
/// cuadrícula sin interferir con la geometría de escenario (Z=-1) o del
/// personaje (Z=0).
pub fn build_grid(device: &wgpu::Device, config: &GridConfig) -> GridBuffer {
    let mut verts: Vec<GizmoVertex> = Vec::new();

    let hw   = config.world_width  * 0.5;
    let hh   = config.world_height * 0.5;
    let z    = 0.5_f32;
    let cell = config.cell_size.max(0.05);

    // Límite del mundo — siempre visible (blanco brillante)
    let bc = [0.85_f32, 0.85, 0.85, 1.0];
    let corners = [[-hw, hh], [hw, hh], [hw, -hh], [-hw, -hh]];
    for i in 0..4_usize {
        let a = corners[i];
        let b = corners[(i + 1) % 4];
        verts.push(GizmoVertex { position: [a[0], a[1], z], color: bc });
        verts.push(GizmoVertex { position: [b[0], b[1], z], color: bc });
    }

    // Ejes de origen — siempre visibles; X rojo suave, Y verde suave
    let xc = [0.65_f32, 0.22, 0.22, 0.9];
    let yc = [0.22_f32, 0.65, 0.22, 0.9];
    verts.push(GizmoVertex { position: [-hw, 0.0, z], color: xc });
    verts.push(GizmoVertex { position: [ hw, 0.0, z], color: xc });
    verts.push(GizmoVertex { position: [0.0, -hh, z], color: yc });
    verts.push(GizmoVertex { position: [0.0,  hh, z], color: yc });

    // Líneas internas — solo cuando la cuadrícula está visible
    let cols = (config.world_width  / cell).round() as u32;
    let rows = (config.world_height / cell).round() as u32;

    if config.visible && cols <= 2000 && rows <= 2000 {
        let gc = [0.28_f32, 0.28, 0.42, 0.50];

        // Verticales
        let mut x = (-hw / cell).ceil() * cell;
        while x <= hw + 1e-4 {
            if x.abs() > cell * 0.01 {   // omitir eje Y (ya dibujado)
                verts.push(GizmoVertex { position: [x,  hh, z], color: gc });
                verts.push(GizmoVertex { position: [x, -hh, z], color: gc });
            }
            x += cell;
        }

        // Horizontales
        let mut y = (-hh / cell).ceil() * cell;
        while y <= hh + 1e-4 {
            if y.abs() > cell * 0.01 {   // omitir eje X (ya dibujado)
                verts.push(GizmoVertex { position: [-hw, y, z], color: gc });
                verts.push(GizmoVertex { position: [ hw, y, z], color: gc });
            }
            y += cell;
        }
    }

    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label:    Some("grid-vbuf"),
        contents: bytemuck::cast_slice(&verts),
        usage:    wgpu::BufferUsages::VERTEX,
    });

    GridBuffer { vertex_buffer, vertex_count: verts.len() as u32 }
}
