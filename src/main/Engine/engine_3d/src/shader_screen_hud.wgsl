// EXCLUSIVO: pass de imágenes HUD en espacio de pantalla (ver screen_hud_image.rs).
// No reutilizar para el mundo 3D ni para texture_2d_array de escena.

struct SceneUniforms {
    view_proj         : mat4x4<f32>,
    view_proj_stable  : mat4x4<f32>,
    prev_view_proj    : mat4x4<f32>,
    inv_view_proj     : mat4x4<f32>,
    cam_pos           : vec4<f32>,
    light_dir         : vec4<f32>,
    light_color       : vec4<f32>,
    light_view_proj   : mat4x4<f32>,
    light_params      : vec4<f32>,
    jitter            : vec4<f32>,
    depth_plane       : vec4<f32>,
    shadow_bias       : vec4<f32>,
}

@group(0) @binding(0)
var<uniform> u: SceneUniforms;

@group(1) @binding(0) var t_hud: texture_2d<f32>;
@group(1) @binding(1) var s_hud: sampler;

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
    @builtin(position) clip_pos : vec4<f32>,
    @location(0) uv             : vec2<f32>,
    @location(1) alpha_mul      : f32,
}

@vertex
fn vs_screen_hud(in: VertexInput, inst: InstanceInput) -> VertexOutput {
    let model = mat4x4<f32>(inst.model_0, inst.model_1, inst.model_2, inst.model_3);
    var out: VertexOutput;
    out.clip_pos = u.view_proj * model * vec4<f32>(in.position, 1.0);
    out.uv = inst.uv_rect.xy + in.uv * (inst.uv_rect.zw - inst.uv_rect.xy);
    out.alpha_mul = inst.flag_pad.y;
    return out;
}

@fragment
fn fs_screen_hud(in: VertexOutput) -> @location(0) vec4<f32> {
    let c = textureSample(t_hud, s_hud, in.uv);
    return vec4<f32>(c.rgb, c.a * in.alpha_mul);
}
