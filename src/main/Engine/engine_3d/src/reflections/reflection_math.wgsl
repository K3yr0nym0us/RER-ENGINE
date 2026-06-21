// Shared depth / projection math for SSR, RT and debug passes.
// Must stay inverse-consistent with shader.wgsl `ndc_z_to_view_depth_m` / G-buffer export.

fn refl_uv_to_ndc_xy(uv : vec2<f32>) -> vec2<f32> {
    return vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
}

fn refl_ndc_xy_to_uv(ndc_xy : vec2<f32>) -> vec2<f32> {
    var uv = ndc_xy * 0.5 + vec2<f32>(0.5);
    uv.y = 1.0 - uv.y;
    return uv;
}

fn refl_view_depth_to_ndc_z_vk(view_depth_m : f32, near_plane : f32, far_plane : f32) -> f32 {
    return (far_plane - near_plane * far_plane / view_depth_m) / (far_plane - near_plane);
}

fn refl_vk_ndc_z_to_gl(ndc_z_vk : f32) -> f32 {
    return ndc_z_vk * 2.0 - 1.0;
}

fn refl_gl_ndc_z_to_vk(ndc_z_gl : f32) -> f32 {
    return ndc_z_gl * 0.5 + 0.5;
}

/// Inverse of `world_pos_from_depth`: clip.z/clip.w → Vulkan NDC z → linear view depth (m).
fn refl_view_depth_m_from_world(
    world : vec3<f32>,
    view_proj : mat4x4<f32>,
    near_plane : f32,
    far_plane : f32,
) -> f32 {
    let clip = view_proj * vec4<f32>(world, 1.0);
    if clip.w <= 0.0 {
        return -1.0;
    }
    let ndc_z_gl = clip.z / clip.w;
    let ndc_z_vk = refl_gl_ndc_z_to_vk(ndc_z_gl);
    return (near_plane * far_plane) / (far_plane - ndc_z_vk * (far_plane - near_plane));
}

fn refl_project_uv(world : vec3<f32>, view_proj : mat4x4<f32>) -> vec2<f32> {
    let clip = view_proj * vec4<f32>(world, 1.0);
    if clip.w <= 0.0 {
        return vec2<f32>(-1.0);
    }
    return refl_ndc_xy_to_uv(clip.xy / clip.w);
}

fn refl_world_pos_from_depth(
    uv : vec2<f32>,
    view_depth_m : f32,
    inv_view_proj : mat4x4<f32>,
    near_plane : f32,
    far_plane : f32,
) -> vec3<f32> {
    let ndc_z_vk = refl_view_depth_to_ndc_z_vk(view_depth_m, near_plane, far_plane);
    let ndc_z_gl = refl_vk_ndc_z_to_gl(ndc_z_vk);
    let world_h = inv_view_proj * vec4<f32>(refl_uv_to_ndc_xy(uv), ndc_z_gl, 1.0);
    return world_h.xyz / world_h.w;
}

/// RTIOW metal.rs: reflect(ray toward surface, outward normal).
fn refl_mirror_dir(world_pos : vec3<f32>, cam_pos : vec3<f32>, n : vec3<f32>) -> vec3<f32> {
    let incident = normalize(world_pos - cam_pos);
    let nn = normalize(n);
    return reflect(incident, nn);
}

/// RTIOW fuzzy metal (Book 1, determinista v1): espejo + desvío hacia la normal si rough > 0.5.
/// Rechaza dot(reflected, normal) <= 0 (rayo absorbido). LOD cubemap usa el mismo rough².
fn refl_fuzzy_mirror_dir(
    world_pos : vec3<f32>,
    cam_pos : vec3<f32>,
    n : vec3<f32>,
    roughness : f32,
) -> vec3<f32> {
    let nn = normalize(n);
    var refl = refl_mirror_dir(world_pos, cam_pos, nn);
    if dot(refl, nn) <= 0.0 {
        return vec3<f32>(0.0);
    }
    let r = clamp(roughness, 0.0, 1.0);
    if r > 0.5 {
        refl = normalize(mix(refl, nn, r * r));
        if dot(refl, nn) <= 0.0 {
            return vec3<f32>(0.0);
        }
    }
    return refl;
}

/// Blur kernel spacing (px): más rugosidad → kernel más ancho en SSR/composite.
fn refl_blur_spacing_px(roughness : f32, hit_dist_m : f32) -> f32 {
    let dist_term = min(hit_dist_m * 0.06, 0.5);
    let r = clamp(roughness, 0.0, 1.0);
    return clamp(r * r * 8.0 + dist_term, 0.0, 6.0);
}

/// F0 coherente con forward (`shader.wgsl`: albedo en metales, 0.04 en dieléctricos).
fn refl_metal_f0(albedo_rgb : vec3<f32>, metallic : f32) -> vec3<f32> {
    let dielectric = vec3<f32>(0.04);
    return mix(dielectric, albedo_rgb, clamp(metallic, 0.0, 1.0));
}

fn refl_fresnel_schlick_vec3(cos_theta : f32, f0 : vec3<f32>) -> vec3<f32> {
    return f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - cos_theta, 5.0);
}

fn refl_fresnel_schlick_scalar(cos_theta : f32, f0 : f32) -> f32 {
    return f0 + (1.0 - f0) * pow(1.0 - cos_theta, 5.0);
}

/// Máscara SSR/RT (alpha): RTIOW Fresnel + rugosidad, sin metallic_boost artístico.
fn refl_trace_strength(
    roughness : f32,
    metallic : f32,
    n : vec3<f32>,
    view_dir : vec3<f32>,
    albedo_rgb : vec3<f32>,
) -> f32 {
    let f0 = refl_metal_f0(albedo_rgb, metallic);
    let cos_theta = max(dot(normalize(n), normalize(view_dir)), 0.0);
    let fres = refl_fresnel_schlick_vec3(cos_theta, f0);
    let fres_w = dot(fres, vec3<f32>(0.2126, 0.7152, 0.0722));
    let rough_term = (1.0 - roughness) * (1.0 - roughness);
    return rough_term * fres_w;
}

fn refl_normal_bias(step_size : f32) -> f32 {
    return max(0.001, max(0.002, step_size * 0.05));
}

fn refl_depth_reject_m(step_size : f32) -> f32 {
    return max(0.06, step_size * 0.75);
}

fn refl_thickness_m(step_size : f32, roughness : f32) -> f32 {
    return max(step_size * 1.5, 0.04) + roughness * 0.06;
}

/// TLAS v1: el impacto cae en la AABB, no en la malla; tolerancia > grosor SSR.
fn refl_rt_hit_depth_reject_m(step_size : f32) -> f32 {
    return max(1.6, step_size * 4.0);
}

/// RTIOW metal.rs: color del rebote *= albedo del metal en el punto de reflexión.
fn refl_metal_attenuate(hit_rgb : vec3<f32>, albedo_rgb : vec3<f32>, metallic : f32) -> vec3<f32> {
    if metallic > 0.5 {
        return hit_rgb * albedo_rgb;
    }
    return hit_rgb;
}
