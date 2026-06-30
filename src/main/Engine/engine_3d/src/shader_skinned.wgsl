const MAX_JOINTS: u32 = 512u;

struct SceneUniforms {
    view_proj         : mat4x4<f32>,
    view_proj_stable  : mat4x4<f32>,
    prev_view_proj    : mat4x4<f32>,
    inv_view_proj   : mat4x4<f32>,
    cam_pos         : vec4<f32>,
    light_dir       : vec4<f32>,
    light_color     : vec4<f32>,
    light_view_proj : mat4x4<f32>,
    light_params    : vec4<f32>,
    jitter          : vec4<f32>,
    depth_plane     : vec4<f32>,
    shadow_bias     : vec4<f32>,
}

struct JointMatrices {
    joints: array<mat4x4<f32>, MAX_JOINTS>,
}

@group(0) @binding(0)
var<uniform> u: SceneUniforms;

@group(0) @binding(1)
var t_shadow: texture_depth_2d;

@group(0) @binding(2)
var s_shadow: sampler_comparison;

@group(1) @binding(0) var t_albedo: texture_2d_array<f32>;
@group(1) @binding(1) var s_albedo: sampler;

@group(2) @binding(0) var t_probe_env: texture_cube_array<f32>;
@group(2) @binding(1) var s_probe_env: sampler;

struct ProbeMeta {
    entries : array<vec4<f32>, 8>,
}

@group(2) @binding(2) var<uniform> probe_meta : ProbeMeta;

@group(3) @binding(0)
var<uniform> joint_mats: JointMatrices;

struct VertexInput {
    @location(0) position : vec3<f32>,
    @location(1) normal   : vec3<f32>,
    @location(2) uv       : vec2<f32>,
    @location(3) joints   : vec4<u32>,
    @location(4) weights  : vec4<f32>,
}

struct InstanceInput {
    @location(5) model_0  : vec4<f32>,
    @location(6) model_1  : vec4<f32>,
    @location(7) model_2  : vec4<f32>,
    @location(8) model_3  : vec4<f32>,
    @location(9) flag_pad : vec4<f32>,
    @location(10) tex_layer_pad : vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_pos  : vec4<f32>,
    @location(0) world_pos       : vec3<f32>,
    @location(1) world_normal    : vec3<f32>,
    @location(2) uv              : vec2<f32>,
    @location(3) flag            : f32,
    @location(4) alpha_mul       : f32,
    @location(5) render_kind     : f32,
    @location(6) tex_layer       : u32,
    @location(7) prev_clip_pos     : vec4<f32>,
    @location(8) curr_stable_clip  : vec4<f32>,
    @location(9) surface_roughness  : f32,
    @location(10) surface_metallic  : f32,
    @location(11) probe_index       : f32,
    @location(12) surface_ior       : f32,
    @location(13) jitter_clip       : vec4<f32>,
}

fn skin_matrix(joints: vec4<u32>, weights: vec4<f32>) -> mat4x4<f32> {
    let j0 = min(joints.x, MAX_JOINTS - 1u);
    let j1 = min(joints.y, MAX_JOINTS - 1u);
    let j2 = min(joints.z, MAX_JOINTS - 1u);
    let j3 = min(joints.w, MAX_JOINTS - 1u);
    return joint_mats.joints[j0] * weights.x
         + joint_mats.joints[j1] * weights.y
         + joint_mats.joints[j2] * weights.z
         + joint_mats.joints[j3] * weights.w;
}

fn skin_matrix_shadow(joints: vec4<u32>, weights: vec4<f32>) -> mat4x4<f32> {
    return skin_matrix(joints, weights);
}

fn scene_light_dir_norm() -> vec3<f32> {
    var l = u.light_dir.xyz;
    if dot(l, l) < 1e-6 {
        l = vec3<f32>(0.45, 1.0, 0.35);
    }
    return normalize(l);
}

fn scene_shadow(world_pos: vec3<f32>, world_normal: vec3<f32>) -> f32 {
    let l = scene_light_dir_norm();
    let n = normalize(world_normal);
    let ndotl = max(dot(n, l), 0.0);
    let normal_scale = u.shadow_bias.x + u.shadow_bias.y * (1.0 - ndotl);
    let biased_pos = world_pos + l * normal_scale;

    let clip = u.light_view_proj * vec4<f32>(biased_pos, 1.0);
    let ndc = clip.xyz / clip.w;
    var uv = ndc.xy * 0.5 + 0.5;
    uv.y = 1.0 - uv.y;
    if uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 {
        return 1.0;
    }
    let slope = sqrt(max(1.0 - ndotl * ndotl, 0.0));
    let depth_ref = ndc.z - (u.shadow_bias.z + u.shadow_bias.w * slope);
    let texel = u.light_params.z;
    let radius = u.light_params.w;
    var sum = 0.0;
    for (var oy = -1; oy <= 1; oy++) {
        for (var ox = -1; ox <= 1; ox++) {
            let off = vec2<f32>(f32(ox), f32(oy)) * texel * radius;
            sum += textureSampleCompareLevel(t_shadow, s_shadow, uv + off, depth_ref);
        }
    }
    return sum / 9.0;
}

fn apply_selection_rim(color: vec3<f32>, flag: f32, world_pos: vec3<f32>, world_normal: vec3<f32>) -> vec3<f32> {
    let v = normalize(u.cam_pos.xyz - world_pos);
    let n = normalize(world_normal);
    let rim_factor = pow(1.0 - max(dot(n, v), 0.0), 2.5);
    var out_c = color;
    if flag > 1.5 {
        out_c = mix(out_c, vec3<f32>(0.12, 0.60, 0.88), 0.38);
        out_c += vec3<f32>(0.15, 0.65, 0.90) * rim_factor * 1.3;
    } else if flag > 0.5 {
        out_c = mix(out_c, vec3<f32>(1.0, 0.75, 0.10), 0.38);
        out_c += vec3<f32>(1.0, 0.80, 0.15) * rim_factor * 2.2;
    }
    return out_c;
}

@vertex
fn vs_shadow_skinned(in: VertexInput, inst: InstanceInput) -> @builtin(position) vec4<f32> {
    let model = mat4x4<f32>(inst.model_0, inst.model_1, inst.model_2, inst.model_3);
    let skin = skin_matrix_shadow(in.joints, in.weights);
    let world = model * skin * vec4<f32>(in.position, 1.0);
    return u.light_view_proj * world;
}

@vertex
fn vs_main_skinned(in: VertexInput, inst: InstanceInput) -> VertexOutput {
    let model = mat4x4<f32>(inst.model_0, inst.model_1, inst.model_2, inst.model_3);
    let skin = skin_matrix(in.joints, in.weights);
    let skinned_pos = skin * vec4<f32>(in.position, 1.0);
    let skinned_norm = normalize((skin * vec4<f32>(in.normal, 0.0)).xyz);
    let world_pos4 = model * skinned_pos;
    var out: VertexOutput;
    let world_pos = world_pos4.xyz;
    out.clip_pos = u.view_proj * world_pos4;
    out.jitter_clip = out.clip_pos;
    out.world_pos = world_pos;
    out.world_normal = normalize((model * vec4<f32>(skinned_norm, 0.0)).xyz);
    out.uv = in.uv;
    out.flag = inst.flag_pad.x;
    out.alpha_mul = inst.flag_pad.y;
    out.render_kind = inst.flag_pad.z;
    out.tex_layer = u32(inst.tex_layer_pad.x);
    out.surface_roughness = inst.flag_pad.w;
    out.surface_metallic = inst.tex_layer_pad.y;
    out.probe_index = inst.tex_layer_pad.z;
    out.surface_ior = inst.tex_layer_pad.w;
    let prev_h = u.prev_view_proj * vec4<f32>(world_pos, 1.0);
    out.prev_clip_pos = prev_h;
    out.curr_stable_clip = u.view_proj_stable * world_pos4;
    return out;
}

struct SceneFragOut {
    @location(0) ambient : vec4<f32>,
    @location(1) shadow  : vec4<f32>,
    @location(2) direct  : vec4<f32>,
    @location(3) depth   : vec4<f32>,
    @location(4) velocity_normal : vec4<f32>,
}

fn encode_octahedral(n: vec3<f32>) -> vec2<f32> {
    let inv_sum = 1.0 / (abs(n.x) + abs(n.y) + abs(n.z));
    var p = n.xy * inv_sum;
    if n.z < 0.0 {
        let ox = p.x;
        let oy = p.y;
        p.x = (1.0 - abs(oy)) * sign(ox);
        p.y = (1.0 - abs(ox)) * sign(oy);
    }
    return p * 0.5 + vec2<f32>(0.5);
}

fn pack_velocity_normal(velocity: vec2<f32>, n: vec3<f32>) -> vec4<f32> {
    return vec4<f32>(velocity, encode_octahedral(normalize(n)));
}

/// PBR-friendly: por defecto, superficies mate (0.9). El SSR/RT solo afecta materiales
/// con `SurfacePbr` explícito. Coherente con Unreal.
fn resolve_surface_roughness(inst_roughness: f32) -> f32 {
    return select(0.9, inst_roughness, inst_roughness >= 0.0);
}

/// Bevy prepass: GL NDC z desde `position.z` Vulkan [0,1].
fn pack_depth_export(ndc_z_vk: f32) -> vec4<f32> {
    return vec4<f32>(ndc_z_vk * 2.0 - 1.0, 0.0, 0.0, 0.0);
}

fn ndc_z_to_view_depth_m(ndc_z: f32) -> f32 {
    let n = u.depth_plane.x;
    let f = u.depth_plane.y;
    return (n * f) / (f - ndc_z * (f - n));
}

fn evaluate_scene(in: VertexOutput, env_override: vec3<f32>, has_env_override: bool) -> SceneFragOut {
    let view_depth_m = ndc_z_to_view_depth_m(in.clip_pos.z);
    let curr_ndc = in.curr_stable_clip.xy / in.curr_stable_clip.w;
    let prev_ndc = in.prev_clip_pos.xy / in.prev_clip_pos.w;
    let velocity = (curr_ndc - prev_ndc) * vec2<f32>(0.5, 0.5);
    let frag_world = refl_world_pos_at_frag(
        in.jitter_clip,
        in.clip_pos.z,
        u.inv_view_proj,
        u.depth_plane.x,
        u.depth_plane.y,
    );
    let surface_roughness = resolve_surface_roughness(in.surface_roughness);
    let surface_metallic = in.surface_metallic;

    let albedo = textureSample(t_albedo, s_albedo, in.uv, i32(in.tex_layer));
    let albedo_rgb = albedo.rgb;
    let n = normalize(in.world_normal);
    let l = scene_light_dir_norm();
    let ndotl = max(dot(n, l), 0.0);
    let ambient_factor = u.light_dir.w;
    let lc = u.light_color.xyz;
    let intensity = u.light_params.x;
    var shadow = 1.0;
    if u.light_color.w > 0.5 {
        shadow = scene_shadow(in.world_pos, n);
    }
    var amb = albedo_rgb * ambient_factor * lc * intensity;
    var dir = albedo_rgb * (1.0 - ambient_factor) * ndotl * lc * intensity * shadow;
    if surface_metallic > 0.5 {
        let ssr_active = u.depth_plane.z > 0.5;
        let ssr_traceable = forward_probe_surface_eligible(in.surface_roughness, surface_roughness);
        let metal = forward_evaluate_metallic_pbr_skinned(
            albedo_rgb,
            frag_world,
            n,
            surface_roughness,
            in.uv,
            u.cam_pos.xyz,
            l,
            lc,
            intensity,
            ndotl * shadow,
            env_override,
            has_env_override,
            ssr_active && ssr_traceable,
        );
        amb = metal.ambient;
        dir = metal.direct;
    } else if has_env_override && surface_metallic <= 0.5 {
        dir = dir + env_override;
    }
    let lit = apply_selection_rim(amb + dir, in.flag, in.world_pos, in.world_normal);
    let base = amb + dir;
    let rim_add = lit - base;
    let amb_out = amb + rim_add * 0.5;
    let dir_out = dir + rim_add * 0.5;
    let ior_b = select(
        dir_out.b,
        clamp(in.surface_ior * 0.1, 0.0, 1.0),
        in.surface_ior > 1.0,
    );
    return SceneFragOut(
        vec4<f32>(amb_out, albedo.a * in.alpha_mul),
        vec4<f32>(shadow, surface_roughness, 0.0, 0.0),
        vec4<f32>(dir_out.r, dir_out.g, ior_b, surface_metallic),
        pack_depth_export(in.clip_pos.z),
        pack_velocity_normal(velocity, n),
    );
}

fn sample_surface_albedo_skinned(in: VertexOutput) -> vec3<f32> {
    let layer = i32(in.tex_layer);
    return textureSample(t_albedo, s_albedo, in.uv, layer).rgb;
}

struct SurfaceGbufferExport {
    @location(0) base_color : vec4<f32>,
    @location(1) world_pos  : vec4<f32>,
}

@fragment
fn fs_export_base_color_skinned(in: VertexOutput) -> SurfaceGbufferExport {
    var out : SurfaceGbufferExport;
    out.base_color = vec4<f32>(sample_surface_albedo_skinned(in), 1.0);
    out.world_pos = vec4<f32>(refl_world_pos_at_frag(
        in.jitter_clip,
        in.clip_pos.z,
        u.inv_view_proj,
        u.depth_plane.x,
        u.depth_plane.y,
    ), 1.0);
    return out;
}

fn scene_has_any_probe() -> bool {
    for (var i = 0u; i < 8u; i++) {
        if probe_meta.entries[i].w > 0.0 {
            return true;
        }
    }
    return false;
}

fn scene_probe_env_sample(in: VertexOutput) -> vec4<f32> {
    if !scene_has_any_probe() {
        return vec4<f32>(0.0);
    }
    let n = normalize(in.world_normal);
    let frag_world = refl_world_pos_at_frag(
        in.jitter_clip,
        in.clip_pos.z,
        u.inv_view_proj,
        u.depth_plane.x,
        u.depth_plane.y,
    );
    let layer = refl_resolve_probe_layer(
        frag_world,
        i32(in.probe_index),
        probe_meta.entries,
    );
    let inst_roughness = in.surface_roughness;
    let surface_roughness = resolve_surface_roughness(inst_roughness);
    return forward_sample_probe_env(
        t_probe_env,
        s_probe_env,
        frag_world,
        u.cam_pos.xyz,
        n,
        in.surface_metallic,
        inst_roughness,
        surface_roughness,
        sample_surface_albedo_skinned(in),
        layer,
        probe_meta.entries,
    );
}

@fragment
fn fs_main_skinned(in: VertexOutput) -> SceneFragOut {
    let probe = scene_probe_env_sample(in);
    let ssr_active = u.depth_plane.z > 0.5;
    let surface_roughness = resolve_surface_roughness(in.surface_roughness);
    let defer_probe = forward_defer_probe_to_ssr(
        ssr_active,
        in.surface_roughness,
        surface_roughness,
        in.surface_metallic,
    );
    let use_probe = probe.a > 0.5 && !defer_probe;
    return evaluate_scene(in, probe.rgb, use_probe);
}

/// Captura del jugador en el cubemap del probe: IBL sin glint especular en metales.
@fragment
fn fs_overlay_skinned(in: VertexOutput) -> @location(0) vec4<f32> {
    let out = evaluate_scene(in, vec3<f32>(0.0), false);
    var rgb = out.ambient.rgb;
    if in.surface_metallic <= 0.5 {
        rgb += out.direct.rgb;
    }
    return vec4<f32>(rgb, out.ambient.a);
}
