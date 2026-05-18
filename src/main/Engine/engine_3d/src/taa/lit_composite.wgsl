struct LitCompositeUniforms {
    shadow_darkness : f32,
    shadows_enabled : f32,
    _pad0           : f32,
    _pad1           : f32,
}

@group(0) @binding(0) var<uniform> u : LitCompositeUniforms;
@group(0) @binding(1) var t_ambient : texture_2d<f32>;
@group(0) @binding(2) var t_direct  : texture_2d<f32>;
@group(0) @binding(3) var t_shadow  : texture_2d<f32>;
@group(0) @binding(4) var s_color   : sampler;
@group(0) @binding(5) var s_shadow  : sampler;

struct VsOut {
    @builtin(position) pos : vec4<f32>,
    @location(0) uv          : vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vi : u32) -> VsOut {
    var p : vec2<f32>;
    switch vi {
        case 0u: { p = vec2<f32>(-1.0, -1.0); }
        case 1u: { p = vec2<f32>( 3.0, -1.0); }
        default: { p = vec2<f32>(-1.0,  3.0); }
    }
    var out : VsOut;
    out.pos = vec4<f32>(p, 0.0, 1.0);
    out.uv  = p * 0.5 + vec2<f32>(0.5, 0.5);
    out.uv.y = 1.0 - out.uv.y;
    return out;
}

@fragment
fn fs_main(in : VsOut) -> @location(0) vec4<f32> {
    let amb = textureSample(t_ambient, s_color, in.uv);
    let dir = textureSample(t_direct, s_color, in.uv);
    if u.shadows_enabled < 0.5 {
        return vec4<f32>(amb.rgb + dir.rgb, amb.a);
    }
    let shadow = textureSample(t_shadow, s_shadow, in.uv).r;
    let shade = mix(u.shadow_darkness, 1.0, shadow);
    return vec4<f32>(amb.rgb + dir.rgb * shade, amb.a);
}
