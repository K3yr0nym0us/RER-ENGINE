struct CompositeUniforms {
    strength : f32,
    ssil_strength : f32,
    /// Cuánto se atenúa el color base donde hay hit SSR (1 = el reflejo domina).
    refl_mix : f32,
    _pad2 : f32,
}

@group(0) @binding(0) var<uniform> u : CompositeUniforms;
@group(0) @binding(1) var t_scene : texture_2d<f32>;
@group(0) @binding(2) var t_reflection : texture_2d<f32>;
@group(0) @binding(3) var s_linear : sampler;
@group(0) @binding(4) var t_ssil : texture_2d<f32>;
@group(0) @binding(5) var s_nearest : sampler;

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
    var base = textureSample(t_scene, s_linear, in.uv);
    if u.ssil_strength > 0.001 {
        let ssil = textureSample(t_ssil, s_linear, in.uv).rgb;
        base = vec4<f32>(base.rgb + ssil * u.ssil_strength, base.a);
    }
    let refl = textureSample(t_reflection, s_linear, in.uv);
    let refl_rgb = refl.rgb * u.strength;
    let refl_lum = dot(refl_rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    let refl_w = max(refl_lum, refl.a * 0.9);
    let base_lum = dot(base.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    // Base gris brillante (steel) → dejar que el SSR domine el especular.
    let blend_gain = mix(u.refl_mix * 2.5, u.refl_mix * 4.5, smoothstep(0.18, 0.52, base_lum));
    let blend = saturate(refl_w * blend_gain);
    let base_keep = 1.0 - blend;
    let out_rgb = base.rgb * base_keep + refl_rgb;
    return vec4<f32>(out_rgb, base.a);
}
