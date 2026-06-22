struct DenoiseUniforms {
    resolution     : vec2<f32>,
    depth_sigma    : f32,
    normal_sigma   : f32,
    luminance_sigma: f32,
    gbuffer_scale  : f32,
    _pad           : vec2<f32>,
}

const DENOISE_RADIUS : u32 = 3u;

@group(0) @binding(0) var<uniform> u : DenoiseUniforms;
@group(0) @binding(1) var t_input : texture_2d<f32>;
@group(0) @binding(2) var t_depth : texture_2d<f32>;
@group(0) @binding(3) var t_normal_roughness : texture_2d<f32>;
@group(0) @binding(4) var s_linear : sampler;
@group(0) @binding(5) var output_tex : texture_storage_2d<rgba8unorm, write>;

fn dz_weight(z1 : f32, z2 : f32, sigma : f32) -> f32 {
    let dz = abs(z1 - z2);
    let denom = max(sigma * (z1 + z2) * 0.5, 1e-8);
    return exp(-dz / denom);
}

fn dn_weight(n1 : vec3<f32>, n2 : vec3<f32>, sigma : f32) -> f32 {
    return pow(max(dot(n1, n2), 0.0), sigma);
}

fn dl_weight(l1 : f32, l2 : f32, sigma : f32) -> f32 {
    return exp(-abs(l1 - l2) * sigma);
}

fn denoise_pixel(uv : vec2<f32>) -> vec4<f32> {
    let texel = vec2<f32>(1.0) / u.resolution;
    let gb_res = u.resolution * u.gbuffer_scale;
    let gb_texel = vec2<f32>(1.0) / gb_res;

    let center = textureSampleLevel(t_input, s_linear, uv, 0.0);
    let center_lum = dot(center.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));

    let center_gb_px = vec2<i32>(uv * gb_res - vec2<f32>(0.5));
    let center_depth = textureLoad(t_depth, center_gb_px, 0).r;
    let center_n = refl_normal_from_packed(textureLoad(t_normal_roughness, center_gb_px, 0));

    if center_depth <= 0.0001 {
        return center;
    }

    var sum = center.rgb;
    var wsum = 1.0;

    for (var oy = -(i32(DENOISE_RADIUS)); oy <= i32(DENOISE_RADIUS); oy++) {
        for (var ox = -(i32(DENOISE_RADIUS)); ox <= i32(DENOISE_RADIUS); ox++) {
            if ox == 0 && oy == 0 {
                continue;
            }
            let tap_uv = uv + vec2<f32>(f32(ox), f32(oy)) * texel;
            if tap_uv.x < 0.0 || tap_uv.x > 1.0 || tap_uv.y < 0.0 || tap_uv.y > 1.0 {
                continue;
            }

            let tap = textureSampleLevel(t_input, s_linear, tap_uv, 0.0);
            let tap_gb_px = vec2<i32>(tap_uv * gb_res - vec2<f32>(0.5));
            let tap_depth = textureLoad(t_depth, tap_gb_px, 0).r;
            if tap_depth <= 0.0001 {
                continue;
            }

            let w_z = dz_weight(center_depth, tap_depth, u.depth_sigma);
            if w_z < 0.01 {
                continue;
            }

            let tap_n = refl_normal_from_packed(textureLoad(t_normal_roughness, tap_gb_px, 0));
            let w_n = dn_weight(center_n, tap_n, u.normal_sigma);

            let tap_lum = dot(tap.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
            let w_l = dl_weight(center_lum, tap_lum, u.luminance_sigma);

            let w = w_z * w_n * w_l;
            sum += tap.rgb * w;
            wsum += w;
        }
    }

    return vec4<f32>(sum / wsum, center.a);
}

@compute @workgroup_size(8, 8)
fn cs_main(@builtin(global_invocation_id) gid : vec3<u32>) {
    if gid.x >= u32(u.resolution.x) || gid.y >= u32(u.resolution.y) {
        return;
    }
    let uv = (vec2<f32>(gid.xy) + vec2<f32>(0.5)) / u.resolution;
    let result = denoise_pixel(uv);
    textureStore(output_tex, vec2<i32>(i32(gid.x), i32(gid.y)), result);
}
