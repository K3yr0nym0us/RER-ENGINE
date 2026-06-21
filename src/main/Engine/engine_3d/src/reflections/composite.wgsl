struct CompositeUniforms {
    strength : f32,
    ssil_strength : f32,
    shadow_influence : f32,
    _pad2 : f32,
}

@group(0) @binding(0) var<uniform> u : CompositeUniforms;
@group(0) @binding(1) var t_scene : texture_2d<f32>;
@group(0) @binding(2) var t_reflection : texture_2d<f32>;
@group(0) @binding(3) var s_linear : sampler;
@group(0) @binding(4) var t_ssil : texture_2d<f32>;
@group(0) @binding(5) var t_shadow_mask : texture_2d<f32>;

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
    let k = clamp(refl.a * u.strength, 0.0, 1.0);
    if k < 0.01 {
        return base;
    }
    // Atenuar reflejo por sombra del receptor: en zonas oscuras el reflejo
    // se reduce en lugar de reemplazar la iluminacion base y ocultar sombras.
    let shadow = textureLoad(t_shadow_mask, vec2<i32>(in.pos.xy), 0).r;
    let shadow_atten = mix(u.shadow_influence, 1.0, shadow);
    let k_atten = k * shadow_atten;
    // Additive blend: el reflejo se suma como indirect specular sobre la base,
    // preservando las sombras que ya estan en la escena iluminada.
    let out_rgb = base.rgb + refl.rgb * k_atten;
    return vec4<f32>(out_rgb, base.a);
}
