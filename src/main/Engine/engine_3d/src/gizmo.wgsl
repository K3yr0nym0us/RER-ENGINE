// Shader de gizmos — solo transforma por view_proj, sin modelo ni textura.
// El color ya viene en el vértice.

struct GizmoUniforms {
    view_proj : mat4x4<f32>,
    model     : mat4x4<f32>,
    // x = hovered_axis (-1=none, 0=X, 1=Y, 2=Z)
    // y = active_axis  (-1=none, 0=X, 1=Y, 2=Z)
    flags     : vec4<f32>,
}

@group(0) @binding(0)
var<uniform> u: GizmoUniforms;

struct VIn {
    @location(0) position : vec3<f32>,
    @location(1) color    : vec4<f32>,
}

struct VOut {
    @builtin(position) clip_pos : vec4<f32>,
    @location(0) color          : vec4<f32>,
}

@vertex
fn vs_main(in: VIn, @builtin(vertex_index) vtx_idx: u32) -> VOut {
    var out: VOut;
    out.clip_pos = u.view_proj * u.model * vec4<f32>(in.position, 1.0);

    let axis    = i32(vtx_idx / 48u);
    let h_axis  = i32(u.flags.x);
    let a_axis  = i32(u.flags.y);

    var col = in.color;
    if a_axis == axis {
        // Activo (arrastrando): blanco brillante
        col = vec4<f32>(min(col.rgb * 2.0 + 0.35, vec3<f32>(1.0)), 1.0);
    } else if h_axis == axis {
        // Hover: más brillante
        col = vec4<f32>(min(col.rgb * 1.55 + 0.12, vec3<f32>(1.0)), 1.0);
    }
    out.color = col;
    return out;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    return in.color;
}

struct SkyFragOut {
    @location(0) ambient          : vec4<f32>,
    @location(1) shadow             : vec4<f32>,
    @location(2) direct           : vec4<f32>,
    @location(3) depth            : vec4<f32>,
    @location(4) velocity_normal  : vec4<f32>,
}

/// Salida MRT del cielo interior (mismos targets que el pass principal).
@fragment
fn fs_sky_mrt(in: VOut) -> SkyFragOut {
    var out: SkyFragOut;
    out.ambient = in.color;
    // Sin sombra; rugosidad máxima → SSR/RT no trazan en píxeles de cielo.
    out.shadow = vec4<f32>(1.0, 1.0, 0.0, 0.0);
    out.direct = vec4<f32>(0.0);
    // 0 = sin geometría (coherente con el clear del G-buffer de profundidad).
    out.depth = vec4<f32>(0.0);
    out.velocity_normal = vec4<f32>(0.0);
    return out;
}
