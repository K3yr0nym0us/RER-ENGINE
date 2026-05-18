struct SceneUniforms {
    view_proj       : mat4x4<f32>,
    cam_pos         : vec4<f32>,
    light_dir       : vec4<f32>,   // xyz hacia el sol, w = ambiente
    light_color     : vec4<f32>,   // w > 0.5 → sombras proyectadas
    light_view_proj : mat4x4<f32>,
    light_params    : vec4<f32>,   // x=intensidad, y=oscurecer sombra
}

@group(0) @binding(0)
var<uniform> u: SceneUniforms;

@group(0) @binding(1)
var t_shadow: texture_depth_2d;

@group(0) @binding(2)
var s_shadow: sampler_comparison;

@group(1) @binding(0) var t_albedo: texture_2d<f32>;
@group(1) @binding(1) var s_albedo: sampler;

struct VertexInput {
    @location(0) position : vec3<f32>,
    @location(1) normal   : vec3<f32>,
    @location(2) uv       : vec2<f32>,
}

struct InstanceInput {
    @location(3) model_0  : vec4<f32>,
    @location(4) model_1  : vec4<f32>,
    @location(5) model_2  : vec4<f32>,
    @location(6) model_3  : vec4<f32>,
    @location(7) flag_pad : vec4<f32>,
    @location(8) uv_rect  : vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_pos  : vec4<f32>,
    @location(0) world_pos       : vec3<f32>,
    @location(1) world_normal    : vec3<f32>,
    @location(2) uv              : vec2<f32>,
    @location(3) flag            : f32,
    @location(4) alpha_mul       : f32,
    @location(5) render_kind     : f32,
    @location(6) uv_center       : vec2<f32>,
}

fn scene_shadow(world_pos: vec3<f32>) -> f32 {
    let clip = u.light_view_proj * vec4<f32>(world_pos, 1.0);
    let ndc = clip.xyz / clip.w;
    var uv = ndc.xy * 0.5 + 0.5;
    uv.y = 1.0 - uv.y;
    if uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 {
        return 1.0;
    }
    let depth_ref = ndc.z - 0.0008;
    return textureSampleCompareLevel(t_shadow, s_shadow, uv, depth_ref);
}

@vertex
fn vs_shadow(in: VertexInput, inst: InstanceInput) -> @builtin(position) vec4<f32> {
    let model = mat4x4<f32>(inst.model_0, inst.model_1, inst.model_2, inst.model_3);
    let world = model * vec4<f32>(in.position, 1.0);
    return u.light_view_proj * world;
}

@vertex
fn vs_main(in: VertexInput, inst: InstanceInput) -> VertexOutput {
    let model      = mat4x4<f32>(inst.model_0, inst.model_1, inst.model_2, inst.model_3);
    let world_pos4 = model * vec4<f32>(in.position, 1.0);
    var out: VertexOutput;
    out.clip_pos     = u.view_proj * world_pos4;
    out.world_pos    = world_pos4.xyz;
    out.world_normal = normalize((model * vec4<f32>(in.normal, 0.0)).xyz);
    out.uv           = inst.uv_rect.xy + in.uv * (inst.uv_rect.zw - inst.uv_rect.xy);
    out.flag         = inst.flag_pad.x;
    out.alpha_mul    = inst.flag_pad.y;
    out.render_kind  = inst.flag_pad.z;
    out.uv_center    = (inst.uv_rect.xy + inst.uv_rect.zw) * 0.5;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Tooltips PNG (Esc, snap): textura completa + transparencia, sin iluminación.
    if in.render_kind >= 2.5 {
        let hud = textureSample(t_albedo, s_albedo, in.uv);
        return vec4(hud.rgb, hud.a * in.alpha_mul);
    }

    var albedo_samp = textureSample(t_albedo, s_albedo, in.uv);
    if in.render_kind >= 0.5 {
        albedo_samp = textureSample(t_albedo, s_albedo, in.uv_center);
    }
    let albedo = albedo_samp.rgb;

    var color = albedo;
    if in.render_kind < 0.5 {
        let n = normalize(in.world_normal);
        var l = u.light_dir.xyz;
        if dot(l, l) < 1e-6 {
            l = vec3<f32>(0.45, 1.0, 0.35);
        }
        l = normalize(l);
        let ndotl = max(dot(n, l), 0.0);
        let ambient = u.light_dir.w;
        let lc = u.light_color.xyz;
        var shade = ambient + (1.0 - ambient) * ndotl;
        if u.light_color.w > 0.5 {
            let shadow = scene_shadow(in.world_pos);
            shade = shade * mix(u.light_params.y, 1.0, shadow);
        }
        color = albedo * shade * lc * u.light_params.x;
    }

    let v = normalize(u.cam_pos.xyz - in.world_pos);
    let n = normalize(in.world_normal);
    let rim_factor = pow(1.0 - max(dot(n, v), 0.0), 2.5);
    if in.flag > 1.5 {
        color = mix(color, vec3<f32>(0.12, 0.60, 0.88), 0.38);
        color += vec3<f32>(0.15, 0.65, 0.90) * rim_factor * 1.3;
    } else if in.flag > 0.5 {
        color = mix(color, vec3<f32>(1.0, 0.75, 0.10), 0.38);
        color += vec3<f32>(1.0, 0.80, 0.15) * rim_factor * 2.2;
    }

    var out_alpha = albedo_samp.a * in.alpha_mul;
    if in.render_kind >= 0.5 {
        out_alpha = in.alpha_mul;
    } else {
        out_alpha = 1.0;
    }

    return vec4<f32>(color, out_alpha);
}
