// ---------------------------------------------------------------------------
// Gizmos — ejes X/Y/Z visuales
//
// Se renderizan como flechas sólidas 3D usando triángulos. El pipeline ignora
// el depth buffer para que siempre sean visibles encima de la geometría.
// ---------------------------------------------------------------------------

use wgpu::util::DeviceExt;

pub use rer_engine_shared::player_ui::ndc_draw::NdcVertex as GizmoVertex;

pub struct GizmoBuffer {
    pub vertex_buffer: wgpu::Buffer,
    pub vertex_count: u32,
}

fn push_tri(verts: &mut Vec<GizmoVertex>, a: [f32; 3], b: [f32; 3], c: [f32; 3], color: [f32; 4]) {
    verts.push(GizmoVertex { position: a, color });
    verts.push(GizmoVertex { position: b, color });
    verts.push(GizmoVertex { position: c, color });
}

fn push_quad(
    verts: &mut Vec<GizmoVertex>,
    a: [f32; 3],
    b: [f32; 3],
    c: [f32; 3],
    d: [f32; 3],
    color: [f32; 4],
) {
    push_tri(verts, a, b, c, color);
    push_tri(verts, a, c, d, color);
}

fn add_arrow_x(verts: &mut Vec<GizmoVertex>, length: f32, color: [f32; 4]) {
    let tip = length;
    let base = length * 0.77;
    let shaft = length * 0.022;
    let head = length * 0.072;

    let p000 = [0.0, -shaft, -shaft];
    let p001 = [0.0, -shaft, shaft];
    let p010 = [0.0, shaft, -shaft];
    let p011 = [0.0, shaft, shaft];
    let p100 = [base, -shaft, -shaft];
    let p101 = [base, -shaft, shaft];
    let p110 = [base, shaft, -shaft];
    let p111 = [base, shaft, shaft];

    push_quad(verts, p000, p100, p110, p010, color);
    push_quad(verts, p001, p011, p111, p101, color);
    push_quad(verts, p000, p001, p101, p100, color);
    push_quad(verts, p010, p110, p111, p011, color);
    push_quad(verts, p000, p010, p011, p001, color);
    push_quad(verts, p100, p101, p111, p110, color);

    let b0 = [base, -head, -head];
    let b1 = [base, -head, head];
    let b2 = [base, head, head];
    let b3 = [base, head, -head];
    let apex = [tip, 0.0, 0.0];
    push_tri(verts, b0, b1, apex, color);
    push_tri(verts, b1, b2, apex, color);
    push_tri(verts, b2, b3, apex, color);
    push_tri(verts, b3, b0, apex, color);
}

fn add_arrow_y(verts: &mut Vec<GizmoVertex>, length: f32, color: [f32; 4]) {
    let tip = length;
    let base = length * 0.77;
    let shaft = length * 0.022;
    let head = length * 0.072;

    let p000 = [-shaft, 0.0, -shaft];
    let p001 = [-shaft, 0.0, shaft];
    let p010 = [shaft, 0.0, -shaft];
    let p011 = [shaft, 0.0, shaft];
    let p100 = [-shaft, base, -shaft];
    let p101 = [-shaft, base, shaft];
    let p110 = [shaft, base, -shaft];
    let p111 = [shaft, base, shaft];

    push_quad(verts, p000, p100, p110, p010, color);
    push_quad(verts, p001, p011, p111, p101, color);
    push_quad(verts, p000, p001, p101, p100, color);
    push_quad(verts, p010, p110, p111, p011, color);
    push_quad(verts, p000, p010, p011, p001, color);
    push_quad(verts, p100, p101, p111, p110, color);

    let b0 = [-head, base, -head];
    let b1 = [-head, base, head];
    let b2 = [head, base, head];
    let b3 = [head, base, -head];
    let apex = [0.0, tip, 0.0];
    push_tri(verts, b0, b1, apex, color);
    push_tri(verts, b1, b2, apex, color);
    push_tri(verts, b2, b3, apex, color);
    push_tri(verts, b3, b0, apex, color);
}

fn add_arrow_z(verts: &mut Vec<GizmoVertex>, length: f32, color: [f32; 4]) {
    let tip = length;
    let base = length * 0.77;
    let shaft = length * 0.022;
    let head = length * 0.072;

    let p000 = [-shaft, -shaft, 0.0];
    let p001 = [-shaft, shaft, 0.0];
    let p010 = [shaft, -shaft, 0.0];
    let p011 = [shaft, shaft, 0.0];
    let p100 = [-shaft, -shaft, base];
    let p101 = [-shaft, shaft, base];
    let p110 = [shaft, -shaft, base];
    let p111 = [shaft, shaft, base];

    push_quad(verts, p000, p100, p110, p010, color);
    push_quad(verts, p001, p011, p111, p101, color);
    push_quad(verts, p000, p001, p101, p100, color);
    push_quad(verts, p010, p110, p111, p011, color);
    push_quad(verts, p000, p010, p011, p001, color);
    push_quad(verts, p100, p101, p111, p110, color);

    let b0 = [-head, -head, base];
    let b1 = [-head, head, base];
    let b2 = [head, head, base];
    let b3 = [head, -head, base];
    let apex = [0.0, 0.0, tip];
    push_tri(verts, b0, b1, apex, color);
    push_tri(verts, b1, b2, apex, color);
    push_tri(verts, b2, b3, apex, color);
    push_tri(verts, b3, b0, apex, color);
}

pub fn build_axes(device: &wgpu::Device, length: f32) -> GizmoBuffer {
    let mut verts = Vec::new();
    add_arrow_x(&mut verts, length, [1.0, 0.18, 0.18, 1.0]);
    add_arrow_y(&mut verts, length, [0.18, 1.0, 0.18, 1.0]);
    add_arrow_z(&mut verts, length, [0.18, 0.55, 1.0, 1.0]);

    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("gizmo-vbo"),
        contents: bytemuck::cast_slice(&verts),
        usage: wgpu::BufferUsages::VERTEX,
    });

    GizmoBuffer {
        vertex_buffer,
        vertex_count: verts.len() as u32,
    }
}

/// Frustum visual de la cámara del jugador (modo editor 3D).
///
/// Dibuja un pequeño cubo en la posición del ojo + una pirámide de líneas hasta
/// un rectángulo lejano (vista de cámara de juego seleccionada).
/// Topología esperada: `LineList`. Vértices en espacio de mundo.
pub fn build_fps_camera_frustum(
    device: &wgpu::Device,
    eye: glam::Vec3,
    yaw: f32,
    pitch: f32,
    fov_y: f32,
    aspect: f32,
    far_dist: f32,
) -> GizmoBuffer {
    use glam::Vec3;

    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    // Mismo convenio que `Camera::view_forward` para que el gizmo apunte exactamente
    // a donde apuntará la cámara en Play.
    let forward = Vec3::new(-cy * cp, -sp, -sy * cp).normalize_or_zero();
    let world_up = Vec3::Y;
    let right = forward.cross(world_up).normalize_or_zero();
    let up = right.cross(forward).normalize_or_zero();

    let color: [f32; 4] = [1.0, 0.85, 0.18, 0.92];

    let mut verts: Vec<GizmoVertex> = Vec::with_capacity(40);
    let mut push_line = |a: Vec3, b: Vec3| {
        verts.push(GizmoVertex {
            position: a.to_array(),
            color,
        });
        verts.push(GizmoVertex {
            position: b.to_array(),
            color,
        });
    };

    // Cuerpo de la cámara: cubito wireframe pequeño centrado en el ojo.
    let s = 0.07_f32;
    let c000 = eye + (-right - up - forward) * s;
    let c001 = eye + (-right - up + forward) * s;
    let c010 = eye + (-right + up - forward) * s;
    let c011 = eye + (-right + up + forward) * s;
    let c100 = eye + (right - up - forward) * s;
    let c101 = eye + (right - up + forward) * s;
    let c110 = eye + (right + up - forward) * s;
    let c111 = eye + (right + up + forward) * s;

    // 12 aristas del cubo
    push_line(c000, c100);
    push_line(c001, c101);
    push_line(c010, c110);
    push_line(c011, c111);
    push_line(c000, c010);
    push_line(c001, c011);
    push_line(c100, c110);
    push_line(c101, c111);
    push_line(c000, c001);
    push_line(c010, c011);
    push_line(c100, c101);
    push_line(c110, c111);

    // Rectángulo lejano: tamaño según fov y aspect.
    let half_h = (fov_y * 0.5).tan() * far_dist;
    let half_w = half_h * aspect;
    let center = eye + forward * far_dist;
    let f_tl = center - right * half_w + up * half_h;
    let f_tr = center + right * half_w + up * half_h;
    let f_br = center + right * half_w - up * half_h;
    let f_bl = center - right * half_w - up * half_h;

    // Pirámide ojo → 4 esquinas lejanas.
    push_line(eye, f_tl);
    push_line(eye, f_tr);
    push_line(eye, f_br);
    push_line(eye, f_bl);

    // Cuadrado lejano.
    push_line(f_tl, f_tr);
    push_line(f_tr, f_br);
    push_line(f_br, f_bl);
    push_line(f_bl, f_tl);

    build_from_vertices(device, &verts)
}

/// Creates a GizmoBuffer from arbitrary pre-built line vertices (tool overlays, etc.).
pub fn build_from_vertices(device: &wgpu::Device, verts: &[GizmoVertex]) -> GizmoBuffer {
    // Always allocate at least one vertex so the buffer is valid.
    let data: &[u8] = if verts.is_empty() {
        &[0u8; std::mem::size_of::<GizmoVertex>()]
    } else {
        bytemuck::cast_slice(verts)
    };
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("tool-overlay-vbo"),
        contents: data,
        usage: wgpu::BufferUsages::VERTEX,
    });
    GizmoBuffer {
        vertex_buffer,
        vertex_count: verts.len() as u32,
    }
}
