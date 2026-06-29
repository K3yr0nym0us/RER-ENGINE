struct DebugUniforms {
    mode              : u32,
    max_roughness     : f32,
    near_plane        : f32,
    far_plane         : f32,
    cam_pos           : vec4<f32>,
    inv_view_proj     : mat4x4<f32>,
    view_proj         : mat4x4<f32>,
    view              : mat4x4<f32>,
    inv_view          : mat4x4<f32>,
    resolution        : vec2<f32>,
    gb_resolution     : vec2<f32>,
    max_distance_m    : f32,
    coarse_resolution : f32,
    thickness_m       : f32,
    binary_steps      : u32,
    coarse_max_iters  : u32,
    ssr_blur_enabled  : f32,
    _pad              : vec2<f32>,
}

@group(0) @binding(0) var<uniform> u : DebugUniforms;
@group(0) @binding(1) var t_scene : texture_2d<f32>;
@group(0) @binding(2) var t_depth : texture_2d<f32>;
@group(0) @binding(3) var t_normal_roughness : texture_2d<f32>;
@group(0) @binding(4) var t_reflection : texture_2d<f32>;
@group(0) @binding(5) var s_linear : sampler;
@group(0) @binding(6) var s_nearest : sampler;
@group(0) @binding(7) var t_surface : texture_2d<f32>;
@group(0) @binding(8) var t_direct : texture_2d<f32>;
@group(0) @binding(9) var t_base_color : texture_2d<f32>;

@group(1) @binding(0) var t_probe_env : texture_cube_array<f32>;
@group(1) @binding(1) var s_probe_env : sampler;
@group(1) @binding(2) var<uniform> probe_meta : ReflProbeMeta;

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
    switch u.mode {
        case 1u: {
            let packed = textureSample(t_normal_roughness, s_linear, in.uv);
            let n = decode_octahedral(packed.zw);
            return vec4<f32>(n * 0.5 + vec3<f32>(0.5), 1.0);
        }
        case 2u: {
            let d_m = depth_at(in.uv, textureDimensions(t_depth));
            let vis = clamp(d_m / 50.0, 0.0, 1.0);
            return vec4<f32>(vec3<f32>(vis), 1.0);
        }
        case 3u: {
            let t = trace_ssr_debug(in.uv);
            if !t.eligible {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            if t.found {
                return vec4<f32>(1.0, 1.0, 1.0, 1.0);
            }
            return vec4<f32>(0.45, 0.12, 0.08, 1.0);
        }
        case 4u: {
            let strength = compute_reflection_strength(in.uv);
            return vec4<f32>(vec3<f32>(strength), 1.0);
        }
        case 5u: {
            return vec4<f32>(textureSample(t_scene, s_linear, in.uv).rgb, 1.0);
        }
        case 6u: {
            let r = textureSample(t_reflection, s_linear, in.uv);
            return vec4<f32>(r.rgb, 1.0);
        }
        case 7u: {
            let px = texel_px(in.uv, textureDimensions(t_surface));
            let roughness = textureLoad(t_surface, px, 0).g;
            return vec4<f32>(vec3<f32>(roughness), 1.0);
        }
        case 8u: {
            let px = texel_px(in.uv, textureDimensions(t_direct));
            let metallic = textureLoad(t_direct, px, 0).a;
            return vec4<f32>(vec3<f32>(metallic), 1.0);
        }
        case 9u: {
            let d_m = depth_at(in.uv, textureDimensions(t_depth));
            if d_m <= 0.0001 {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            let wp = world_pos_from_depth(in.uv, d_m);
            let rel = wp - u.cam_pos.xyz;
            return vec4<f32>(fract(rel * 0.08 + vec3<f32>(0.5)), 1.0);
        }
        case 10u: {
            let d_m = depth_at(in.uv, textureDimensions(t_depth));
            if d_m <= 0.0001 {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            let ndc_z_vk = refl_view_depth_to_ndc_z_vk(d_m, u.near_plane, u.far_plane);
            let ndc_z_gl = refl_vk_ndc_z_to_gl(ndc_z_vk);
            let ndc = vec3<f32>(uv_to_ndc_xy(in.uv), ndc_z_gl);
            return vec4<f32>(ndc * 0.5 + vec3<f32>(0.5), 1.0);
        }
        case 11u: {
            let d_m = depth_at(in.uv, textureDimensions(t_depth));
            if d_m <= 0.0001 {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            let vp = view_pos_from_depth(in.uv, d_m);
            return vec4<f32>(fract(vp * 0.05 + vec3<f32>(0.5)), 1.0);
        }
        case 12u: {
            let d_m = depth_at(in.uv, textureDimensions(t_depth));
            if d_m <= 0.0001 {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            let wp = world_pos_from_depth(in.uv, d_m);
            let clip = u.view_proj * vec4<f32>(wp, 1.0);
            let ndc = clip.xyz / clip.w;
            let reproj_uv = ndc_xy_to_uv(ndc.xy);
            let err = length(reproj_uv - in.uv);
            return vec4<f32>(reproj_uv, clamp(err * 20.0, 0.0, 1.0), 1.0);
        }
        case 13u: {
            let t = trace_ssr_debug(in.uv);
            if !t.eligible {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            return vec4<f32>(t.view_dir * 0.5 + vec3<f32>(0.5), 1.0);
        }
        case 14u: {
            let t = trace_ssr_debug(in.uv);
            if !t.eligible {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            return vec4<f32>(t.refl_dir * 0.5 + vec3<f32>(0.5), 1.0);
        }
        case 15u: {
            let t = trace_ssr_debug(in.uv);
            if !t.eligible {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            let path_vis = clamp(t.path_px / 200.0, 0.0, 1.0);
            let hit_vis = select(0.0, 1.0, t.found);
            return vec4<f32>(path_vis, t.steps_frac, hit_vis, 1.0);
        }
        case 16u: {
            let t = trace_ssr_debug(in.uv);
            if !t.eligible {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            let vis = clamp(abs(t.depth_delta) * 10.0, 0.0, 1.0);
            return vec4<f32>(vec3<f32>(vis), 1.0);
        }
        case 17u: {
            let t = trace_ssr_debug(in.uv);
            if !t.found {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            return vec4<f32>(t.hit_uv, 0.0, 1.0);
        }
        case 18u: {
            let t = trace_ssr_debug(in.uv);
            if !t.found {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            return vec4<f32>(lit_scene_at(t.hit_uv), 1.0);
        }
        case 19u: {
            let t = trace_ssr_debug(in.uv);
            if !t.found {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            let spacing_px = lettier_reflection_roughness(t.roughness) * 4.0 + 1.0;
            return vec4<f32>(lettier_debug_box_blur(t.hit_uv, spacing_px), 1.0);
        }
        case 20u: {
            let r = textureSample(t_reflection, s_linear, in.uv);
            return vec4<f32>(r.rgb, 1.0);
        }
        case 21u: {
            let px = texel_px(in.uv, textureDimensions(t_base_color));
            return vec4<f32>(textureLoad(t_base_color, px, 0).rgb, 1.0);
        }
        case 22u: {
            let strength = compute_reflection_strength(in.uv);
            return vec4<f32>(vec3<f32>(strength), 1.0);
        }
        case 23u: {
            let t = trace_ssr_debug(in.uv);
            if !t.found {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            return vec4<f32>(t.hit_uv, 0.0, 1.0);
        }
        case 24u: {
            let t = trace_ssr_debug(in.uv);
            if !t.found {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            return vec4<f32>(t.hit_uv, 0.0, 1.0);
        }
        case 25u: {
            let t = trace_ssr_debug(in.uv);
            if !t.found {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            return vec4<f32>(t.hit_uv, 0.0, 1.0);
        }
        case 26u: {
            let base = textureSample(t_scene, s_linear, in.uv);
            let refl = textureSample(t_reflection, s_linear, in.uv);
            let k = clamp(refl.a, 0.0, 1.0);
            let detail = max(refl.rgb - base.rgb, vec3<f32>(0.0));
            return vec4<f32>(base.rgb + detail * k, 1.0);
        }
        case 27u: {
            return vec4<f32>(textureSample(t_reflection, s_linear, in.uv).rgb, 1.0);
        }
        case 28u: {
            let d_m = depth_at(in.uv, textureDimensions(t_depth));
            if d_m <= 0.0001 {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            let wp = world_pos_from_depth(in.uv, d_m);
            let layer_i = refl_nearest_probe_layer_entries(wp, probe_meta.entries);
            let slot = u32(max(layer_i, 0));
            let hues = array<vec3<f32>, 8>(
                vec3<f32>(1.0, 0.15, 0.15),
                vec3<f32>(0.15, 1.0, 0.2),
                vec3<f32>(0.2, 0.35, 1.0),
                vec3<f32>(1.0, 0.9, 0.15),
                vec3<f32>(1.0, 0.2, 0.95),
                vec3<f32>(0.15, 0.95, 0.95),
                vec3<f32>(1.0, 0.55, 0.1),
                vec3<f32>(0.65, 0.2, 1.0),
            );
            if layer_i < 0 {
                return vec4<f32>(0.08, 0.08, 0.1, 1.0);
            }
            return vec4<f32>(hues[slot], 1.0);
        }
        case 29u: {
            let d_m = depth_at(in.uv, textureDimensions(t_depth));
            if d_m <= 0.0001 {
                return vec4<f32>(0.05, 0.05, 0.08, 1.0);
            }
            let wp = world_pos_from_depth(in.uv, d_m);
            let gb_px = texel_px(in.uv, textureDimensions(t_normal_roughness));
            let packed = textureLoad(t_normal_roughness, gb_px, 0);
            let n = decode_octahedral(packed.xy);
            let metallic = textureLoad(t_direct, gb_px, 0).a;
            let layer_i = refl_nearest_probe_layer_entries(wp, probe_meta.entries);
            let slot = u32(max(layer_i, 0));
            let hues = array<vec3<f32>, 8>(
                vec3<f32>(1.0, 0.15, 0.15),
                vec3<f32>(0.15, 1.0, 0.2),
                vec3<f32>(0.2, 0.35, 1.0),
                vec3<f32>(1.0, 0.9, 0.15),
                vec3<f32>(1.0, 0.2, 0.95),
                vec3<f32>(0.15, 0.95, 0.95),
                vec3<f32>(1.0, 0.55, 0.1),
                vec3<f32>(0.65, 0.2, 1.0),
            );
            var layer_vis = vec3<f32>(0.1, 0.1, 0.12);
            if layer_i >= 0 {
                layer_vis = hues[slot];
            }
            var cubemap_rgb = vec3<f32>(0.02, 0.02, 0.03);
            if layer_i >= 0 && metallic > 0.5 {
                let sample_dir = refl_cubemap_sample_dir(
                    wp,
                    u.cam_pos.xyz,
                    n,
                    layer_i,
                    probe_meta.entries,
                );
                cubemap_rgb = textureSampleLevel(t_probe_env, s_probe_env, sample_dir, layer_i, 0.0).rgb;
            }
            if in.uv.x < 0.5 {
                return vec4<f32>(layer_vis, 1.0);
            }
            return vec4<f32>(cubemap_rgb, 1.0);
        }
        case 30u: {
            let t = trace_ssr_debug(in.uv);
            if !t.eligible {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            if t.found {
                return vec4<f32>(lit_scene_at(t.hit_uv), 1.0);
            }
            return vec4<f32>(0.0, 1.0, 0.0, 1.0);
        }
        case 31u: {
            let t = trace_ssr_debug(in.uv);
            if !t.eligible {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            // exit_reason: 1=segmento degenerado, 2=sin delta px, 3=UV out, 4=sin_hit_coarse,
            // 5=self_hit, 6=sin_depth, 7=px_cerca, 8=vis_cero, 9=hit
            switch t.exit_reason {
                case 1u: { return vec4<f32>(1.0, 0.0, 0.0, 1.0); }      // rojo
                case 2u: { return vec4<f32>(1.0, 0.5, 0.0, 1.0); }      // naranja
                case 3u: { return vec4<f32>(1.0, 1.0, 0.0, 1.0); }      // amarillo
                case 4u: { return vec4<f32>(0.0, 1.0, 0.0, 1.0); }      // verde
                case 5u: { return vec4<f32>(0.0, 0.5, 1.0, 1.0); }      // celeste
                case 6u: { return vec4<f32>(0.0, 0.0, 1.0, 1.0); }      // azul
                case 7u: { return vec4<f32>(0.5, 0.0, 1.0, 1.0); }      // violeta
                case 8u: { return vec4<f32>(1.0, 0.0, 0.5, 1.0); }      // rosa
                default: { return vec4<f32>(lit_scene_at(t.hit_uv), 1.0); } // escena
            }
        }
        case 32u: {
            let t = trace_ssr_debug(in.uv);
            if !t.eligible {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            let refl = normalize(t.refl_dir);
            return vec4<f32>(refl * 0.5 + vec3<f32>(0.5), 1.0);
        }
        default: {
            return textureSample(t_scene, s_linear, in.uv);
        }
    }
}
