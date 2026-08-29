// ---------------------------------------------------------------------------
// Gizmos — ejes X/Y/Z visuales
//
// Se renderizan como flechas sólidas 3D usando triángulos. El pipeline ignora
// el depth buffer para que siempre sean visibles encima de la geometría.
// ---------------------------------------------------------------------------

use wgpu::util::DeviceExt;

pub use rer_engine_shared::player_ui::ndc_draw::NdcVertex as GizmoVertex;

/// Filas del uniform `GizmoUniforms` en `gizmo.wgsl` (view_proj + model + flags + axis_start).
pub const GIZMO_UNIFORM_ROW_COUNT: usize = 10;

/// view_proj + model identidad; sin hover/eje activo ni offset de flechas.
pub const GIZMO_UNIFORM_IDENTITY: [[f32; 4]; GIZMO_UNIFORM_ROW_COUNT] = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
    [-1.0, -1.0, 0.0, 0.0],
    [0.0, 0.0, 0.0, 0.0],
];

pub fn gizmo_uniform(
    view_proj: [[f32; 4]; 4],
    model: [[f32; 4]; 4],
    hovered_axis: f32,
    active_axis: f32,
    axis_start: [f32; 3],
) -> [[f32; 4]; GIZMO_UNIFORM_ROW_COUNT] {
    [
        view_proj[0],
        view_proj[1],
        view_proj[2],
        view_proj[3],
        model[0],
        model[1],
        model[2],
        model[3],
        [hovered_axis, active_axis, 0.0, 0.0],
        [axis_start[0], axis_start[1], axis_start[2], 0.0],
    ]
}

/// `view_proj` + model identidad; para overlays de líneas (grid, debug, sky).
pub fn gizmo_uniform_world(view_proj: [[f32; 4]; 4]) -> [[f32; 4]; GIZMO_UNIFORM_ROW_COUNT] {
    gizmo_uniform(
        view_proj,
        [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
        -1.0,
        -1.0,
        [0.0, 0.0, 0.0],
    )
}

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

pub const GIZMO_ROTATION_RING_RADIUS: f32 = 1.0;
pub const GIZMO_ROTATION_RING_SEGMENTS: u32 = 64;
pub const GIZMO_ROTATION_RING_TUBE: f32 = 0.042;

/// Punto en un anillo de rotación local (0=X, 1=Y, 2=Z).
pub fn rotation_ring_point(axis: usize, radius: f32, angle: f32) -> [f32; 3] {
    let (c, s) = angle.sin_cos();
    match axis {
        0 => [0.0, radius * c, radius * s],
        1 => [radius * c, 0.0, radius * s],
        _ => [radius * c, radius * s, 0.0],
    }
}

fn ring_plane_normal(axis: usize) -> [f32; 3] {
    match axis {
        0 => [1.0, 0.0, 0.0],
        1 => [0.0, 1.0, 0.0],
        _ => [0.0, 0.0, 1.0],
    }
}

fn ring_binormal(axis: usize, tangent: [f32; 3], tube: f32) -> [f32; 3] {
    let n = ring_plane_normal(axis);
    let bx = n[1] * tangent[2] - n[2] * tangent[1];
    let by = n[2] * tangent[0] - n[0] * tangent[2];
    let bz = n[0] * tangent[1] - n[1] * tangent[0];
    let len = (bx * bx + by * by + bz * bz).sqrt().max(1e-6);
    [bx / len * tube, by / len * tube, bz / len * tube]
}

fn v3_add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn v3_sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn v3_scale(v: [f32; 3], s: f32) -> [f32; 3] {
    [v[0] * s, v[1] * s, v[2] * s]
}

/// Segmento de anillo con sección cuadrada (binormal + normal del plano) para grosor 3D.
fn push_ring_segment(
    verts: &mut Vec<GizmoVertex>,
    axis: usize,
    radius: f32,
    a0: f32,
    a1: f32,
    color: [f32; 4],
) {
    let tube = GIZMO_ROTATION_RING_TUBE;
    let p0 = rotation_ring_point(axis, radius, a0);
    let p1 = rotation_ring_point(axis, radius, a1);
    let tangent = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
    let binormal = ring_binormal(axis, tangent, tube);
    let normal = v3_scale(ring_plane_normal(axis), tube);

    let ring_cross = |center: [f32; 3]| {
        let c00 = v3_sub(v3_sub(center, binormal), normal);
        let c01 = v3_sub(v3_add(center, binormal), normal);
        let c11 = v3_add(v3_add(center, binormal), normal);
        let c10 = v3_add(v3_sub(center, binormal), normal);
        [c00, c01, c11, c10]
    };

    let [c00, c01, c11, c10] = ring_cross(p0);
    let [d00, d01, d11, d10] = ring_cross(p1);

    push_quad(verts, c00, c01, d01, d00, color);
    push_quad(verts, c10, c11, d11, d10, color);
    push_quad(verts, c00, c10, d10, d00, color);
    push_quad(verts, c01, c11, d11, d01, color);
}

pub fn build_rotation_rings(device: &wgpu::Device, radius: f32) -> GizmoBuffer {
    let mut verts = Vec::new();
    let segments = GIZMO_ROTATION_RING_SEGMENTS;
    let colors = [
        [1.0, 0.18, 0.18, 1.0],
        [0.18, 1.0, 0.18, 1.0],
        [0.18, 0.55, 1.0, 1.0],
    ];
    for (axis, &color) in colors.iter().enumerate() {
        for seg in 0..segments {
            let a0 = std::f32::consts::TAU * seg as f32 / segments as f32;
            let a1 = std::f32::consts::TAU * (seg + 1) as f32 / segments as f32;
            push_ring_segment(&mut verts, axis, radius, a0, a1, color);
        }
    }

    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("gizmo-rotation-rings-vbo"),
        contents: bytemuck::cast_slice(&verts),
        usage: wgpu::BufferUsages::VERTEX,
    });

    GizmoBuffer {
        vertex_buffer,
        vertex_count: verts.len() as u32,
    }
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
