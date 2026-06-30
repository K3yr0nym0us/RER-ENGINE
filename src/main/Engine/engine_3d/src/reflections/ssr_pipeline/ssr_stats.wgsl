// Estadísticas SSR para logs de depuración (muestreo en resolución del pass SSR, pre-temporal).

struct SsrStatsOut {
    skip_depth    : atomic<u32>,
    skip_rough    : atomic<u32>,
    skip_specular : atomic<u32>,
    eligible      : atomic<u32>,
    miss_trace    : atomic<u32>,
    miss_vis      : atomic<u32>,
    screen_hits   : atomic<u32>,
    visible_hits  : atomic<u32>,
    sum_alpha     : atomic<u32>,
    sum_refl_lum  : atomic<u32>,
}

struct SsrStatsUniforms {
    refl_resolution : vec2<f32>,
    gbuffer_scale   : f32,
    max_roughness   : f32,
    alpha_threshold : f32,
    stride          : u32,
}

@group(0) @binding(0) var<uniform> u_stats : SsrStatsUniforms;
@group(0) @binding(1) var<storage, read_write> out_buf : SsrStatsOut;
@group(0) @binding(2) var t_depth : texture_2d<f32>;
@group(0) @binding(3) var t_surface : texture_2d<f32>;
@group(0) @binding(4) var t_reflection : texture_2d<f32>;
@group(0) @binding(5) var t_hit_uv : texture_2d<f32>;
@group(0) @binding(6) var t_direct : texture_2d<f32>;
@group(0) @binding(7) var t_base_color : texture_2d<f32>;

fn pack_alpha(a : f32) -> u32 {
    return u32(clamp(a, 0.0, 1.0) * 10000.0);
}

fn pack_lum(rgb : vec3<f32>) -> u32 {
    let lum = dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    return u32(clamp(lum, 0.0, 1.0) * 10000.0);
}

/// COPIA MANUAL de `lettier_specular_amount` (reflection_math.wgsl:151)
/// SIN Schlick Fresnel (stats no tiene normal/view uniforms).
/// Si cambias `lettier_specular_amount`, actualiza esta función también.
fn stats_specular_amount(metallic : f32, roughness : f32, albedo_rgb : vec3<f32>) -> f32 {
    let r = clamp(roughness, 0.0, 1.0);
    let sharp = (1.0 - r) * (1.0 - r);
    let f0v = mix(vec3<f32>(0.04), albedo_rgb, clamp(metallic, 0.0, 1.0));
    let f0_lum = dot(f0v, vec3<f32>(0.2126, 0.7152, 0.0722));
    return f0_lum * sharp;
}

/// Vacío / far plane del prepass Bevy (GL NDC z ≈ 1).
fn depth_prepass_invalid(depth_prepass : f32) -> bool {
    return depth_prepass > 0.999;
}

@compute @workgroup_size(8, 8, 1)
fn cs_main(@builtin(global_invocation_id) gid : vec3<u32>) {
    let refl_dims = textureDimensions(t_reflection);
    if gid.x >= refl_dims.x || gid.y >= refl_dims.y {
        return;
    }
    let stride = max(u_stats.stride, 1u);
    if (gid.x % stride) != 0u || (gid.y % stride) != 0u {
        return;
    }

    let gb_px = vec2<i32>(
        i32(f32(gid.x) * u_stats.gbuffer_scale + 0.5),
        i32(f32(gid.y) * u_stats.gbuffer_scale + 0.5),
    );
    let depth_prepass = textureLoad(t_depth, gb_px, 0).r;
    if depth_prepass_invalid(depth_prepass) {
        atomicAdd(&out_buf.skip_depth, 1u);
        return;
    }

    let roughness = textureLoad(t_surface, gb_px, 0).g;
    if roughness > u_stats.max_roughness {
        atomicAdd(&out_buf.skip_rough, 1u);
        return;
    }

    let metallic = textureLoad(t_direct, gb_px, 0).a;
    let albedo = textureLoad(t_base_color, gb_px, 0).rgb;
    let specular_amount = stats_specular_amount(metallic, roughness, albedo);
    if specular_amount <= 0.0 {
        atomicAdd(&out_buf.skip_specular, 1u);
        return;
    }

    atomicAdd(&out_buf.eligible, 1u);

    let refl_px = vec2<i32>(i32(gid.x), i32(gid.y));
    let hit_uv = textureLoad(t_hit_uv, refl_px, 0).rg;
    let screen_hit = hit_uv.x >= 0.0 && hit_uv.y >= 0.0;
    let refl = textureLoad(t_reflection, refl_px, 0);
    let alpha = refl.a;

    if screen_hit {
        atomicAdd(&out_buf.screen_hits, 1u);
        if alpha > u_stats.alpha_threshold {
            atomicAdd(&out_buf.visible_hits, 1u);
            atomicAdd(&out_buf.sum_alpha, pack_alpha(alpha));
            atomicAdd(&out_buf.sum_refl_lum, pack_lum(refl.rgb));
        } else {
            atomicAdd(&out_buf.miss_vis, 1u);
        }
    } else {
        atomicAdd(&out_buf.miss_trace, 1u);
    }
}
