const MAX_JOINTS: u32 = 256u;

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

/// Pase de sombras skinned: layout [shadow_pass, joints] → grupo 1.
@group(1) @binding(0)
var<uniform> joint_mats_shadow: JointMatrices;

@group(2) @binding(0)
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
    let j0 = min(joints.x, MAX_JOINTS - 1u);
    let j1 = min(joints.y, MAX_JOINTS - 1u);
    let j2 = min(joints.z, MAX_JOINTS - 1u);
    let j3 = min(joints.w, MAX_JOINTS - 1u);
    return joint_mats_shadow.joints[j0] * weights.x
         + joint_mats_shadow.joints[j1] * weights.y
         + joint_mats_shadow.joints[j2] * weights.z
         + joint_mats_shadow.joints[j3] * weights.w;
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
    out.world_pos = world_pos;
    out.world_normal = normalize((model * vec4<f32>(skinned_norm, 0.0)).xyz);
    out.uv = in.uv;
    out.flag = inst.flag_pad.x;
    out.alpha_mul = inst.flag_pad.y;
    out.render_kind = inst.flag_pad.z;
    out.tex_layer = u32(inst.tex_layer_pad.x);
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
    @location(4) velocity : vec4<f32>,
}

fn evaluate_scene(in: VertexOutput) -> SceneFragOut {
    let linear_depth = in.clip_pos.z / in.clip_pos.w;
    let curr_ndc = in.curr_stable_clip.xy / in.curr_stable_clip.w;
    let prev_ndc = in.prev_clip_pos.xy / in.prev_clip_pos.w;
    let velocity = (curr_ndc - prev_ndc) * vec2<f32>(0.5, 0.5);

    let albedo = textureSample(t_albedo, s_albedo, in.uv, i32(in.tex_layer));
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
    let amb = albedo.rgb * ambient_factor * lc * intensity;
    let dir = albedo.rgb * (1.0 - ambient_factor) * ndotl * lc * intensity * shadow;
    let lit = apply_selection_rim(amb + dir, in.flag, in.world_pos, in.world_normal);
    let base = amb + dir;
    let rim_add = lit - base;
    let amb_out = amb + rim_add * 0.5;
    let dir_out = dir + rim_add * 0.5;
    return SceneFragOut(
        vec4<f32>(amb_out, albedo.a * in.alpha_mul),
        vec4<f32>(shadow, 0.0, 0.0, 0.0),
        vec4<f32>(dir_out, albedo.a * in.alpha_mul),
        vec4<f32>(linear_depth, 0.0, 0.0, 0.0),
        vec4<f32>(velocity, 0.0, 0.0),
    );
}

@fragment
fn fs_main_skinned(in: VertexOutput) -> SceneFragOut {
    return evaluate_scene(in);
}
