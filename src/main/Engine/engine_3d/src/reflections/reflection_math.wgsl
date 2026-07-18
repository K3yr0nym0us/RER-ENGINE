// Shared depth / projection math for SSR, RT and debug passes.
// Depth prepass (R32Float): GL NDC z desde `position.z` Vulkan (near=-1, far=1).
// La marcha SSR usa NDC z invertido (near=1, far=0): ver `ssr_gl_ndc_z_to_march_z`.

/// RTIOW hit interval lower bound (shadow acne / self-intersection).
const REFL_RAY_T_MIN : f32 = 0.001;
/// Misma frontera que `refl_rt_primary_trace_dir` (rayo espejo vs GGX).
const REFL_MIRROR_ROUGHNESS_MAX : f32 = 0.04;
const PI : f32 = 3.14159265359;

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
    return refl_view_depth_m_from_gl_ndc_z(clip.z / clip.w, near_plane, far_plane);
}

/// Profundidad lineal (m) desde NDC z OpenGL (`clip.z / clip.w`), coherente con `shader.wgsl`.
fn refl_view_depth_m_from_gl_ndc_z(ndc_z_gl : f32, near_plane : f32, far_plane : f32) -> f32 {
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
    depth_prepass : f32,
    inv_view_proj : mat4x4<f32>,
    near_plane : f32,
    far_plane : f32,
) -> vec3<f32> {
    _ = near_plane;
    _ = far_plane;
    // Prepass guarda GL NDC z; glam/wgpu `inv_view_proj` espera Vulkan NDC z [0,1].
    let ndc_z_vk = refl_gl_ndc_z_to_vk(depth_prepass);
    let world_h = inv_view_proj * vec4<f32>(refl_uv_to_ndc_xy(uv), ndc_z_vk, 1.0);
    return world_h.xyz / world_h.w;
}

/// Posición world en el fragmento: clip jitterado ÷ w + depth @builtin (misma matriz que depth prepass).
/// NO usar `curr_stable_clip` (sin jitter): desalinea XY/Z en esferas con TAA activo.
fn refl_world_pos_at_frag(
    jitter_clip : vec4<f32>,
    frag_ndc_z_vk : f32,
    inv_view_proj : mat4x4<f32>,
    near_plane : f32,
    far_plane : f32,
) -> vec3<f32> {
    let ndc_xy = jitter_clip.xy / max(jitter_clip.w, 1e-8);
    let uv = refl_ndc_xy_to_uv(ndc_xy);
    return refl_world_pos_from_depth(
        uv,
        refl_vk_ndc_z_to_gl(frag_ndc_z_vk),
        inv_view_proj,
        near_plane,
        far_plane,
    );
}

/// Reconstrucción world desde uv + prepass depth.
fn refl_world_pos_from_prepass_uv(
    uv : vec2<f32>,
    depth_prepass : f32,
    inv_view_proj : mat4x4<f32>,
    near_plane : f32,
    far_plane : f32,
) -> vec3<f32> {
    return refl_world_pos_from_depth(uv, depth_prepass, inv_view_proj, near_plane, far_plane);
}

/// Origen del rayo SSR en NDC del píxel (xy desde uv; z = prepass GL NDC).
fn ssr_ray_start_cs_at_uv(uv : vec2<f32>, depth_prepass : f32) -> vec3<f32> {
    return vec3<f32>(refl_uv_to_ndc_xy(uv), depth_prepass);
}

/// Refleja en view space y devuelve dirección world (misma convención que la proyección).
fn ssr_reflection_world_view_based(
    surface_pos_world    : vec3<f32>,
    surface_normal_world : vec3<f32>,
    view                 : mat4x4<f32>,
    inv_view             : mat4x4<f32>,
) -> vec3<f32> {
    let surface_pos_view = (view * vec4<f32>(surface_pos_world, 1.0)).xyz;
    let surface_normal_view = normalize((view * vec4<f32>(surface_normal_world, 0.0)).xyz);
    let reflection_dir_view = ssr_reflection_dir_view(surface_pos_view, surface_normal_view);
    return normalize((inv_view * vec4<f32>(reflection_dir_view, 0.0)).xyz);
}

/// Origen del rayo SSR en NDC: xy desde la posición world, z = prepass GL NDC.
fn ssr_ray_start_cs(surface_pos_world : vec3<f32>, depth_prepass : f32, view_proj : mat4x4<f32>) -> vec3<f32> {
    var ray_origin_ndc = ssr_world_to_ndc(surface_pos_world, view_proj);
    ray_origin_ndc.z = depth_prepass;
    return ray_origin_ndc;
}

/// Vacío / far plane del depth prepass (GL NDC z ≈ 1).
fn refl_depth_prepass_invalid(depth_prepass : f32) -> bool {
    return depth_prepass > 0.999;
}

/// NDC z para marcha SSR: near=1, far=0 (WebGPU), desde GL NDC z de glam (`near=-1`).
fn ssr_gl_ndc_z_to_march_z(gl_ndc_z : f32) -> f32 {
    let vk_z = gl_ndc_z * 0.5 + 0.5;
    return 1.0 - vk_z;
}

// ── SSR screen-space reflection ─────────────────────────────────────────────

/// Profundidad lineal (m) desde posición en view space (OpenGL: −Z hacia delante).
fn ssr_view_depth_from_view_pos(view_pos : vec3<f32>) -> f32 {
    return max(-view_pos.z, 1e-4);
}

/// Dirección de vista en world: de la superficie hacia la cámara.
fn ssr_view_dir_world(camera_pos_world : vec3<f32>, surface_pos_world : vec3<f32>) -> vec3<f32> {
    return normalize(camera_pos_world - surface_pos_world);
}

/// Dirección reflejada en world: `reflect(-view_dir, surface_normal)` con view = superficie→cámara.
fn ssr_reflection_world(
    camera_pos_world     : vec3<f32>,
    surface_pos_world    : vec3<f32>,
    surface_normal_world : vec3<f32>,
) -> vec3<f32> {
    let view_dir_world = ssr_view_dir_world(camera_pos_world, surface_pos_world);
    return normalize(reflect(-view_dir_world, normalize(surface_normal_world)));
}

/// Primary RT ray on SSR miss: same mirror axis as SSR; GGX lobe only when rough.
fn refl_rt_primary_trace_dir(
    camera_pos_world     : vec3<f32>,
    surface_pos_world    : vec3<f32>,
    surface_normal_world : vec3<f32>,
    roughness            : f32,
    frame_index          : u32,
    uv                   : vec2<f32>,
) -> vec3<f32> {
    let view_dir_world = normalize(camera_pos_world - surface_pos_world);
    if roughness <= 0.04 {
        return ssr_reflection_world(camera_pos_world, surface_pos_world, surface_normal_world);
    }
    let seed = refl_blue_noise_seed(frame_index, uv);
    let T = refl_tangent(surface_normal_world);
    let B = refl_bitangent(surface_normal_world, T);
    let H_local = ggx_sample_ndf(roughness, seed);
    let microfacet_normal = normalize(
        H_local.x * T + H_local.y * B + H_local.z * surface_normal_world,
    );
    return normalize(reflect(-view_dir_world, microfacet_normal));
}

/// Dirección reflejada en view space: `reflect(surface_pos_view, surface_normal_view)`.
fn ssr_reflection_dir_view(surface_pos_view : vec3<f32>, surface_normal_view : vec3<f32>) -> vec3<f32> {
    return normalize(reflect(normalize(surface_pos_view), normalize(surface_normal_view)));
}

fn ssr_reflection_dir(view_dir_view : vec3<f32>, surface_normal_view : vec3<f32>) -> vec3<f32> {
    _ = view_dir_view;
    return ssr_reflection_dir_view(-view_dir_view, surface_normal_view);
}

/// World → NDC xyz (post divide).
fn ssr_world_to_ndc(world : vec3<f32>, view_proj : mat4x4<f32>) -> vec3<f32> {
    let h = view_proj * vec4<f32>(world, 1.0);
    let w_div = select(-1.0, 1.0, h.w >= 0.0) * max(abs(h.w), 1e-8);
    return h.xyz / w_div;
}

/// Dirección world → clip (w=0).
fn ssr_direction_world_to_clip(world_dir : vec3<f32>, view_proj : mat4x4<f32>) -> vec4<f32> {
    return view_proj * vec4<f32>(world_dir, 0.0);
}

/// Extremo del rayo SSR en NDC: marcha infinita en clip acotada al frustum.
fn ssr_clip_ray_end_ndc(
    ray_origin_ndc       : vec3<f32>,
    reflection_dir_world : vec3<f32>,
    view_proj            : mat4x4<f32>,
) -> vec3<f32> {
    let reflection_dir_clip = ssr_direction_world_to_clip(reflection_dir_world, view_proj);
    var ray_end_clip = vec4<f32>(ray_origin_ndc, 1.0) + reflection_dir_clip;
    ray_end_clip = ray_end_clip
        / (select(-1.0, 1.0, ray_end_clip.w >= 0.0) * max(abs(ray_end_clip.w), 1e-10));

    var ray_delta_ndc = ray_end_clip.xyz - ray_origin_ndc;
    let near_edge = select(
        vec3<f32>(-1.0, -1.0, -1.0),
        vec3<f32>(1.0, 1.0, 1.0),
        ray_delta_ndc < vec3<f32>(0.0),
    );
    let dist_near = (near_edge - ray_origin_ndc) / ray_delta_ndc;
    let max_dist_near = max(max(dist_near.x, dist_near.y), dist_near.z);
    var clipped_origin_ndc = ray_origin_ndc + ray_delta_ndc * max(0.0, max_dist_near);

    ray_delta_ndc = ray_end_clip.xyz - clipped_origin_ndc;
    let far_edge = select(
        vec3<f32>(-1.0, -1.0, -1.0),
        vec3<f32>(1.0, 1.0, 1.0),
        ray_delta_ndc >= vec3<f32>(0.0),
    );
    let dist_far = (far_edge - clipped_origin_ndc) / ray_delta_ndc;
    let min_dist_far = min(min(dist_far.x, dist_far.y), dist_far.z);
    ray_delta_ndc *= min_dist_far;
    return clipped_origin_ndc + ray_delta_ndc;
}

/// Segmento UV del rayo SSR: extremos NDC vía clip-march.
fn ssr_uv_ray_segment_clip(
    surface_pos_world    : vec3<f32>,
    reflection_dir_world : vec3<f32>,
    view_proj            : mat4x4<f32>,
) -> vec4<f32> {
    let ray_origin_ndc = ssr_world_to_ndc(surface_pos_world, view_proj);
    let ray_end_ndc = ssr_clip_ray_end_ndc(ray_origin_ndc, reflection_dir_world, view_proj);
    let ray_origin_uv = refl_ndc_xy_to_uv(ray_origin_ndc.xy);
    let ray_end_uv = refl_ndc_xy_to_uv(ray_end_ndc.xy);
    if ray_origin_uv.x < -0.5 || ray_end_uv.x < -0.5 {
        return vec4<f32>(0.0);
    }
    let ray_uv_delta = ray_end_uv - ray_origin_uv;
    return vec4<f32>(ray_uv_delta, length(ray_uv_delta), 0.0);
}

/// View → NDC con división perspectiva segura.
fn ssr_proj_view_to_ndc(proj_only : mat4x4<f32>, view_pt : vec3<f32>) -> vec3<f32> {
    let h = proj_only * vec4<f32>(view_pt, 1.0);
    let w_div = select(-1.0, 1.0, h.w >= 0.0) * max(abs(h.w), 1e-8);
    return h.xyz / w_div;
}

/// Mantiene el extremo del rayo delante de la cámara (view RH: Z < 0).
fn ssr_clip_ray_end_view(
    ray_start : vec3<f32>,
    reflection_dir : vec3<f32>,
    max_dist_m : f32,
) -> vec3<f32> {
    let rd = normalize(reflection_dir);
    var end = ray_start + rd * max_dist_m;
    if end.z < -1e-4 {
        return end;
    }
    if abs(rd.z) > 1e-6 {
        let t = (-1e-3 - ray_start.z) / rd.z;
        if t > 1e-6 {
            return ray_start + rd * min(t, max_dist_m);
        }
    }
    return vec3<f32>(ray_start.x + rd.x * max_dist_m, ray_start.y + rd.y * max_dist_m, -1e-3);
}

/// Segmento UV del rayo SSR entre extremos proyectados con `view_proj` (misma convención Y que depth).
fn ssr_uv_ray_segment(
    ray_start : vec3<f32>,
    reflection_dir : vec3<f32>,
    max_dist_m : f32,
    view_proj : mat4x4<f32>,
    inv_view : mat4x4<f32>,
) -> vec4<f32> {
    let end_view = ssr_clip_ray_end_view(ray_start, reflection_dir, max_dist_m);
    let start_world = (inv_view * vec4<f32>(ray_start, 1.0)).xyz;
    let end_world = (inv_view * vec4<f32>(end_view, 1.0)).xyz;
    let start_uv = refl_project_uv(start_world, view_proj);
    let end_uv = refl_project_uv(end_world, view_proj);
    if start_uv.x < -0.5 || end_uv.x < -0.5 {
        return vec4<f32>(0.0);
    }
    let dp = end_uv - start_uv;
    return vec4<f32>(dp, length(dp), 0.0);
}

/// Interpolación perspectiva correcta de profundidad a lo largo del rayo.
fn ssr_perspective_depth(start_d : f32, end_d : f32, search_t : f32) -> f32 {
    return (start_d * end_d) / mix(end_d, start_d, search_t);
}

/// Proyecta un punto view-space a píxeles.
fn ssr_view_to_frag_px(
    view_pt : vec3<f32>,
    inv_view : mat4x4<f32>,
    view_proj : mat4x4<f32>,
    tex_size : vec2<f32>,
) -> vec2<f32> {
    let projection = view_proj * inv_view;
    let clip = projection * vec4<f32>(view_pt, 1.0);
    if clip.w <= 0.0 {
        return vec2<f32>(-1.0);
    }
    return refl_ndc_xy_to_uv(clip.xy / clip.w) * tex_size;
}

/// Porción de línea [0,1] a lo largo del rayo en pantalla.
fn ssr_line_search_t(
    frag : vec2<f32>,
    start_frag : vec2<f32>,
    delta_x : f32,
    delta_y : f32,
    use_x : f32,
) -> f32 {
    return mix(
        (frag.y - start_frag.y) / max(delta_y, 1e-6),
        (frag.x - start_frag.x) / max(delta_x, 1e-6),
        use_x,
    );
}

/// Visibilidad tras la marcha (sin máscara `facing`: en esferas generaba un arco/cuña
/// donde dot(-view_dir, reflection_dir) ≈ 0).
fn ssr_trace_visibility(
    hit_refined : bool,
    hit_coarse : bool,
    view_dir : vec3<f32>,
    reflection_dir : vec3<f32>,
    depth_delta : f32,
    thickness : f32,
    surface_position_view : vec3<f32>,
    position_to_view : vec3<f32>,
    vis_fade_distance_m : f32,
    hit_uv : vec2<f32>,
) -> f32 {
    _ = view_dir;
    _ = reflection_dir;
    if !hit_refined && !hit_coarse {
        return 0.0;
    }
    var vis = 1.0;
    if !hit_refined {
        vis *= 0.5;
    }
    vis *= 1.0 - clamp(depth_delta / max(thickness, 1e-4), 0.0, 1.0);
    vis *= 1.0 - clamp(
        length(position_to_view - surface_position_view) / max(vis_fade_distance_m, 1e-4),
        0.0,
        1.0,
    );
    if hit_uv.x < 0.0 || hit_uv.x > 1.0 || hit_uv.y < 0.0 || hit_uv.y > 1.0 {
        return 0.0;
    }
    return clamp(vis, 0.0, 1.0);
}

/// Máscara specular con Schlick Fresnel (F0 = albedo en metales, coherente con `refl_metal_f0`).
fn ssr_specular_amount(metallic : f32, roughness : f32, albedo_rgb : vec3<f32>, NdotV : f32) -> f32 {
    let sharp = (1.0 - clamp(roughness, 0.0, 1.0)) * (1.0 - clamp(roughness, 0.0, 1.0));
    let f0v = refl_metal_f0(albedo_rgb, metallic);
    let f0_lum = dot(f0v, vec3<f32>(0.2126, 0.7152, 0.0722));
    let f = f0_lum + (1.0 - f0_lum) * pow(1.0 - clamp(NdotV, 0.0, 1.0), 5.0);
    return f * sharp;
}

/// Rugosidad para mezclar reflejo nítido / borroso (`roughness = 1 - shininess`).
fn ssr_reflection_roughness(surface_roughness : f32) -> f32 {
    return clamp(surface_roughness, 0.0, 1.0);
}

/// `reflect(-view_dir, surface_normal)` con view = superficie→cámara (forward IBL).
fn refl_mirror_dir(
    surface_pos_world    : vec3<f32>,
    camera_pos_world     : vec3<f32>,
    surface_normal_world : vec3<f32>,
) -> vec3<f32> {
    return ssr_reflection_world(camera_pos_world, surface_pos_world, surface_normal_world);
}

/// Dirección de muestreo del cubemap con parallax esférico (meta.w = radio de influencia).
fn refl_cubemap_sample_dir(
    world_pos : vec3<f32>,
    cam_pos : vec3<f32>,
    n : vec3<f32>,
    probe_idx : i32,
    probe_entries : array<vec4<f32>, 8>,
) -> vec3<f32> {
    let refl = refl_mirror_dir(world_pos, cam_pos, n);
    if probe_idx < 0 {
        return refl;
    }
    let probe_entry = probe_entries[probe_idx];
    let probe_r = probe_entry.w;
    if probe_r <= 0.0 {
        return refl;
    }
    let probe_center = probe_entry.xyz;
    let dist_center = length(world_pos - probe_center);
    // En la superficie de la sonda (malla = la esfera del probe), el cubemap ya está
    // centrado en probe_center: parallax empeora el muestreo y genera bandas brillantes.
    if dist_center <= probe_r * 1.05 {
        return refl;
    }
    let oc = world_pos - probe_center;
    let a = dot(refl, refl);
    let b = 2.0 * dot(oc, refl);
    let c = dot(oc, oc) - probe_r * probe_r;
    let disc = b * b - a * c;
    if disc < 0.0 {
        return refl;
    }
    let t = (-b - sqrt(disc)) / max(a, 1e-8);
    if t < 0.0 {
        return refl;
    }
    let sample_pos = world_pos + refl * t;
    return normalize(sample_pos - probe_center);
}

/// Deterministic pseudo-random unit vector (RTIOW `random_in_unit_sphere` sin RNG global).
fn refl_random_unit_vector(seed : vec2<f32>) -> vec3<f32> {
    let h1 = fract(sin(dot(seed, vec2<f32>(127.1, 311.7))) * 43758.5453);
    let h2 = fract(sin(dot(seed + vec2<f32>(17.0, 31.0), vec2<f32>(269.5, 183.3))) * 43758.5453);
    let h3 = fract(sin(dot(seed + vec2<f32>(53.0, 97.0), vec2<f32>(419.2, 371.9))) * 43758.5453);
    let z = 1.0 - 2.0 * h1;
    let r = sqrt(max(1.0 - z * z, 0.0));
    let phi = 6.2831853 * h2;
    return vec3<f32>(r * cos(phi), r * sin(phi), z);
}

/// RTIOW Book 1 fuzzy metal: `normalize(reflect) + fuzz * random_unit_vector`.
fn refl_metal_fuzz_from_roughness(roughness : f32) -> f32 {
    return clamp(roughness, 0.0, 1.0) * 0.5;
}

fn refl_apply_fuzzy_metal(reflected : vec3<f32>, n : vec3<f32>, fuzz : f32, seed : vec2<f32>) -> vec3<f32> {
    let nn = normalize(n);
    var refl = normalize(reflected);
    if dot(refl, nn) <= 0.0 {
        return vec3<f32>(0.0);
    }
    if fuzz <= 1e-6 {
        return refl;
    }
    let scattered = normalize(refl + fuzz * refl_random_unit_vector(seed));
    if dot(scattered, nn) <= 0.0 {
        return vec3<f32>(0.0);
    }
    return scattered;
}

/// Dirección de traza metálica (SSR/RT): RTIOW fuzzy + rechazo grazing.
fn refl_fuzzy_mirror_dir(
    world_pos : vec3<f32>,
    cam_pos : vec3<f32>,
    n : vec3<f32>,
    roughness : f32,
    seed : vec2<f32>,
) -> vec3<f32> {
    let refl = refl_mirror_dir(world_pos, cam_pos, n);
    let fuzz = refl_metal_fuzz_from_roughness(roughness);
    return refl_apply_fuzzy_metal(refl, n, fuzz, seed);
}

/// Fuzzy con semilla blue-noise temporal (frame + UV).
fn refl_blue_noise_seed(frame_index : u32, uv : vec2<f32>) -> vec2<f32> {
    let f = f32(frame_index) * 0.6180339887;
    return vec2<f32>(uv.x + f * 0.173, uv.y + f * 0.271);
}

fn refl_fuzzy_mirror_dir_temporal(
    world_pos : vec3<f32>,
    cam_pos : vec3<f32>,
    n : vec3<f32>,
    roughness : f32,
    frame_index : u32,
    uv : vec2<f32>,
) -> vec3<f32> {
    return refl_fuzzy_mirror_dir(
        world_pos,
        cam_pos,
        n,
        roughness,
        refl_blue_noise_seed(frame_index, uv),
    );
}

/// Mip LOD del cubemap alineado al fuzz RTIOW (split-sum aproximado).
fn refl_env_cubemap_lod(roughness : f32) -> f32 {
    let r = clamp(roughness, 0.0, 1.0);
    let fuzz = refl_metal_fuzz_from_roughness(r);
    return clamp(r * r * 4.0 + fuzz * 2.0, 0.0, 4.0);
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

// ── GGX Microfacet BRDF ───────────────────────────────────────────────

fn ggx_alpha(roughness : f32) -> f32 {
    return max(roughness * roughness, 0.001);
}

/// Sample microfacet normal from GGX NDF (Walter 2007)
fn ggx_sample_ndf(roughness : f32, seed : vec2<f32>) -> vec3<f32> {
    let alpha = ggx_alpha(roughness);
    let phi = 6.2831853 * seed.x;
    let cos_theta = sqrt((1.0 - seed.y) / max(1.0 + (alpha * alpha - 1.0) * seed.y, 1e-8));
    let sin_theta = sqrt(max(1.0 - cos_theta * cos_theta, 0.0));
    return vec3<f32>(sin_theta * cos(phi), sin_theta * sin(phi), cos_theta);
}

/// Orthonormal tangent from normal (Y-up, fallback Z-up when N ∥ Y)
fn refl_tangent(normal : vec3<f32>) -> vec3<f32> {
    let N = normalize(normal);
    let up = select(
        vec3<f32>(0.0, 0.0, 1.0),
        vec3<f32>(0.0, 1.0, 0.0),
        abs(N.y) < 0.999,
    );
    return normalize(cross(up, N));
}

/// Orthonormal bitangent from normal and tangent
fn refl_bitangent(normal : vec3<f32>, tangent : vec3<f32>) -> vec3<f32> {
    return cross(normalize(normal), tangent);
}

/// Cosine-weighted hemisphere sample for diffuse BRDF
fn refl_cosine_hemisphere_sample(seed : vec2<f32>) -> vec3<f32> {
    let phi = 6.2831853 * seed.x;
    let r = sqrt(seed.y);
    return vec3<f32>(r * cos(phi), r * sin(phi), sqrt(max(1.0 - seed.y, 0.0)));
}

/// Smith G1 geometry term for GGX (Walter et al. 2007 full form).
fn ggx_smith_g1(cos_theta : f32, alpha : f32) -> f32 {
    let a2 = alpha * alpha;
    let cos2 = cos_theta * cos_theta;
    return 2.0 * cos_theta / max(cos_theta + sqrt(a2 + cos2 - a2 * cos2), 1e-8);
}

/// GGX NDF (Trowbridge-Reitz) distribution term.
fn ggx_d(alpha : f32, ndoth : f32) -> f32 {
    let a2 = alpha * alpha;
    let denom = ndoth * ndoth * (a2 - 1.0) + 1.0;
    return a2 / (PI * denom * denom);
}

/// BRDF/PDF weight for GGX importance-sampled reflections.
/// Derived from Cook-Torrance fr * |n·wi| / pdf_ggx:
///   fr * |n·wi| / pdf = F * G * |wo·h| / (|n·wo| * |n·h|)
fn ggx_reflection_weight(roughness : f32, ndotv : f32, ndotl : f32, ndoth : f32, vdoth : f32, f0 : vec3<f32>) -> vec3<f32> {
    let alpha = ggx_alpha(roughness);
    let F = refl_fresnel_schlick_vec3(vdoth, f0);
    let G = ggx_smith_g1(ndotv, alpha) * ggx_smith_g1(ndotl, alpha);
    return F * G * vdoth / max(ndotv * ndoth, 1e-8);
}

/// Firefly clamp: scale color when luminance exceeds max_lum.
fn refl_firefly_clamp(color : vec3<f32>, max_lum : f32) -> vec3<f32> {
    let lum = dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
    return select(color, color * (max_lum / lum), lum > max_lum);
}

/// Entorno procedural (cielo esférico uniforme del muro de límites).
fn refl_procedural_environment(_refl_dir : vec3<f32>) -> vec3<f32> {
    return vec3<f32>(0.72, 0.86, 0.98);
}

/// Grosor de hit SSR: escala con profundidad + expansión por ángulo rasante.
/// `ray_dir` es la dirección del rayo en view space (normalizada).
/// Cuando el rayo viaja horizontal (grazing), el depth_delta salta más por píxel,
/// por lo que necesitamos más tolerancia para no perder el impacto.
fn ssr_hit_thickness_m(view_depth_m : f32, roughness : f32, ray_dir : vec3<f32>) -> f32 {
    let base = clamp(view_depth_m * 0.008, 0.12, 0.55) + roughness * 0.04;
    let horiz = length(ray_dir.xy);
    let grazing = 1.0 + horiz * horiz * 8.0;
    return min(base * grazing, 2.0);
}

/// Desplaza el origen del rayo en view space para evitar auto-intersección (grosor en origen).
fn ssr_view_normal_bias_m(view_depth_m : f32) -> f32 {
    return clamp(view_depth_m * 0.002, 0.02, 0.12);
}

/// Posición coherente con prepass depth + sesgo hacia afuera para reflect().
fn ssr_reflect_surface_at_uv(
    uv                   : vec2<f32>,
    depth_prepass        : f32,
    surface_normal_world : vec3<f32>,
    inv_view_proj        : mat4x4<f32>,
    near_plane           : f32,
    far_plane            : f32,
) -> vec3<f32> {
    let surface_pos_world = refl_world_pos_from_depth(
        uv,
        depth_prepass,
        inv_view_proj,
        near_plane,
        far_plane,
    );
    let surface_depth_m = refl_view_depth_m_from_gl_ndc_z(depth_prepass, near_plane, far_plane);
    let normal_bias_m = ssr_view_normal_bias_m(surface_depth_m);
    return surface_pos_world + normalize(surface_normal_world) * normal_bias_m;
}

/// RT primario; SSR refina en pantalla; en miss RT conserva SSR (cubemap/procedural).
fn refl_compose_rt_ssr(
    rt_rgb : vec3<f32>,
    rt_hit : bool,
    ssr : vec4<f32>,
    ssr_screen_hit : bool,
    rt_blend : f32,
    strength : f32,
) -> vec4<f32> {
    if rt_hit {
        var rgb = rt_rgb;
        if ssr_screen_hit {
            // SSR ya encontró geometría en pantalla; no dejar que RT probe/off-screen domine.
            let w = clamp(rt_blend * max(ssr.a, 0.2), 0.55, 1.0);
            rgb = mix(rt_rgb, ssr.rgb, w);
        }
        let a = max(strength, select(0.0, ssr.a * rt_blend, ssr_screen_hit));
        return vec4<f32>(rgb, max(a, 0.01));
    }
    return ssr;
}

/// Skip al inicio de la marcha: solo en rayos largos; rayos cortos (corona esfera) no se saltan.
const SSR_MIN_MARCH_START_PX : f32 = 2.0;
const SSR_SKIP_RAY_MIN_PX : f32 = 14.0;
const SSR_SKIP_MAX_FRAC : f32 = 0.08;

/// Avanza el origen de la marcha a lo largo del segmento NDC (post near-clip XY).
fn ssr_march_skip_self_intersection_t(
    start_cs : vec3<f32>,
    end_cs : vec3<f32>,
    tex_size : vec2<f32>,
) -> f32 {
    let start_uv = refl_ndc_xy_to_uv(start_cs.xy);
    let end_uv = refl_ndc_xy_to_uv(end_cs.xy);
    let ray_len_px = length((end_uv - start_uv) * tex_size);
    // Rayos cortos/intermedios (corona esfera): skip=0 para no perder el primer hit válido.
    if ray_len_px < SSR_SKIP_RAY_MIN_PX {
        return 0.0;
    }
    return clamp(SSR_MIN_MARCH_START_PX / ray_len_px, 0.0, SSR_SKIP_MAX_FRAC);
}

/// Rechazo post-marcha: misma lámina / cáscara convexa (esferas).
fn ssr_reject_self_reflection(
    surface_pos_world    : vec3<f32>,
    hit_pos_world        : vec3<f32>,
    surface_normal_world : vec3<f32>,
    hit_normal_world     : vec3<f32>,
    surface_depth_m      : f32,
    hit_depth_m          : f32,
    surface_uv           : vec2<f32>,
    reflection_hit_uv    : vec2<f32>,
    tex_size             : vec2<f32>,
) -> bool {
    _ = surface_pos_world;
    _ = hit_pos_world;
    let surface_normal = normalize(surface_normal_world);
    let hit_normal = normalize(hit_normal_world);
    let abs_depth = abs(hit_depth_m - surface_depth_m);
    let rel_depth = abs_depth / max(surface_depth_m, 0.05);
    let normals_dot = dot(surface_normal, hit_normal);
    let pixel_distance = length((reflection_hit_uv - surface_uv) * tex_size);

    // Mismo píxel / misma lámina: autoreflexión (penetración ≈ 0).
    if pixel_distance < 2.0 && (abs_depth < 0.05 || rel_depth < 0.008) {
        return true;
    }
    // Misma cáscara convexa: solo hits casi coincidentes (no toda la corona 5–64 px).
    if normals_dot > 0.94 && pixel_distance < 10.0 && rel_depth < 0.012 {
        return true;
    }
    return false;
}

/// Máscara SSR/RT (alpha): RTIOW Fresnel + rugosidad (RT usa esta; SSR usa `ssr_specular_amount`).
fn refl_trace_strength(
    roughness : f32,
    metallic : f32,
    n : vec3<f32>,
    view_dir : vec3<f32>,
    albedo_rgb : vec3<f32>,
    ior : f32,
) -> f32 {
    var f0 = refl_metal_f0(albedo_rgb, metallic);
    let cos_theta = max(dot(normalize(n), normalize(view_dir)), 0.0);
    if metallic <= 0.5 && ior > 1.01 {
        let fr = refl_dielectric_fresnel(cos_theta, ior);
        f0 = vec3<f32>(fr);
    }
    let fres = refl_fresnel_schlick_vec3(cos_theta, f0);
    let fres_w = dot(fres, vec3<f32>(0.2126, 0.7152, 0.0722));
    let rough_sq = (1.0 - roughness) * (1.0 - roughness);
    let rough_lin = 1.0 - roughness * 0.82;
    let rough_term = select(rough_sq, rough_lin, metallic > 0.5);
    return rough_term * fres_w;
}

fn refl_normal_bias(step_size : f32) -> f32 {
    return max(REFL_RAY_T_MIN, max(0.002, step_size * 0.5));
}

fn refl_depth_reject_m(step_size : f32) -> f32 {
    return max(0.06, step_size * 0.75);
}

fn refl_thickness_m(step_size : f32, roughness : f32) -> f32 {
    return max(step_size * 1.5, 0.04) + roughness * 0.06;
}

/// Tolerancia reproyección depth tras hit de triángulo (superficie real, no AABB).
fn refl_rt_hit_depth_reject_m(step_size : f32) -> f32 {
    return max(0.12, step_size * 1.25);
}

/// RTIOW metal.rs: color del rebote *= albedo del metal en el punto de reflexión.
fn refl_metal_attenuate(hit_rgb : vec3<f32>, albedo_rgb : vec3<f32>, metallic : f32) -> vec3<f32> {
    if metallic > 0.5 {
        return hit_rgb * albedo_rgb;
    }
    return hit_rgb;
}

const REFL_MAX_PROBES : u32 = 8u;

struct RtTri {
    v0 : vec4<f32>,
    v1 : vec4<f32>,
    v2 : vec4<f32>,
    uv0 : vec4<f32>,
    uv1 : vec4<f32>,
    uv2 : vec4<f32>,
}

fn refl_tri_bary_uv(tri : RtTri, bary : vec3<f32>) -> vec2<f32> {
    return tri.uv0.xy * bary.x + tri.uv1.xy * bary.y + tri.uv2.xy * bary.z;
}

struct ReflProbeMeta {
    entries : array<vec4<f32>, 8>,
}

/// Entorno procedural (mismo gradiente que `world_bounds` / forward IBL).
fn refl_fake_environment(refl_dir : vec3<f32>) -> vec3<f32> {
    let sky = vec3<f32>(0.42, 0.68, 0.95);
    let horizon = vec3<f32>(0.72, 0.86, 0.98);
    let ground = vec3<f32>(0.58, 0.62, 0.68);
    let y = clamp(refl_dir.y, -1.0, 1.0);
    if y >= 0.0 {
        return mix(horizon, sky, pow(y, 0.45));
    }
    return mix(horizon, ground, pow(-y, 0.55));
}

fn refl_nearest_probe_layer(hit_pos : vec3<f32>, probe_meta : ReflProbeMeta) -> i32 {
    return refl_nearest_probe_layer_entries(hit_pos, probe_meta.entries);
}

/// Política unificada: entidad probe (inst_probe_layer ≥ 0) → su ranura; resto → nearest.
fn refl_resolve_probe_layer(
    world_pos: vec3<f32>,
    inst_probe_layer: i32,
    entries: array<vec4<f32>, 8>,
) -> i32 {
    if inst_probe_layer >= 0 {
        return inst_probe_layer;
    }
    return refl_nearest_probe_layer_entries(world_pos, entries);
}

fn refl_nearest_probe_layer_entries(hit_pos : vec3<f32>, entries : array<vec4<f32>, 8>) -> i32 {
    var best_i = -1;
    var best_d = 1e30;
    for (var i = 0u; i < REFL_MAX_PROBES; i++) {
        let e = entries[i];
        // w <= 0: ranura vacía. w > 0: probe activa (radio reservado para blend futuro).
        if e.w <= 0.0 {
            continue;
        }
        let d = distance(hit_pos, e.xyz);
        if d < best_d {
            best_d = d;
            best_i = i32(i);
        }
    }
    return best_i;
}

fn refl_sample_probe_at_hit(
    hit_pos : vec3<f32>,
    sample_dir : vec3<f32>,
    roughness : f32,
    probe_meta : ReflProbeMeta,
    t_probe : texture_cube_array<f32>,
    s_probe : sampler,
) -> vec3<f32> {
    let dir_n = normalize(sample_dir);
    let layer_i = refl_nearest_probe_layer(hit_pos, probe_meta);
    if layer_i >= 0 {
        let lod = refl_env_cubemap_lod(roughness);
        return textureSampleLevel(t_probe, s_probe, dir_n, layer_i, lod).rgb;
    }
    return refl_fake_environment(dir_n);
}

fn refl_sample_probe_for_material(
    hit_pos : vec3<f32>,
    sample_dir : vec3<f32>,
    roughness : f32,
    mat : RtInstanceMaterial,
    has_material : bool,
    probe_meta : ReflProbeMeta,
    t_probe : texture_cube_array<f32>,
    s_probe : sampler,
) -> vec3<f32> {
    let dir_n = normalize(sample_dir);
    var layer_i = -1;
    if has_material && mat.probe.x >= 0.0 {
        layer_i = i32(mat.probe.x);
    }
    if layer_i < 0 {
        layer_i = refl_nearest_probe_layer(hit_pos, probe_meta);
    }
    if layer_i >= 0 {
        let lod = refl_env_cubemap_lod(roughness);
        return textureSampleLevel(t_probe, s_probe, dir_n, layer_i, lod).rgb;
    }
    return refl_fake_environment(dir_n);
}

// ── RT Hit Lighting lite (Fase A/D/C) ────────────────────────────────────────

const RT_MAT_FLAG_DIELECTRIC : u32 = 1u;

struct RtInstanceMaterial {
    albedo : vec4<f32>,
    pbr : vec4<f32>,
    /// x = probe layer (-1 = nearest to hit), yzw unused
    probe : vec4<f32>,
}

struct RtLightUniform {
    light_dir : vec4<f32>,
    light_view_proj : mat4x4<f32>,
    light_params : vec4<f32>,
    shadow_bias : vec4<f32>,
    light_color : vec4<f32>,
    rt_flags : vec4<f32>,
}

fn refl_tri_normal(tri : RtTri) -> vec3<f32> {
    let e1 = tri.v1.xyz - tri.v0.xyz;
    let e2 = tri.v2.xyz - tri.v0.xyz;
    return normalize(cross(e1, e2));
}

fn refl_tri_instance_slot(tri : RtTri) -> u32 {
    return bitcast<u32>(tri.v0.w);
}

fn refl_rt_shadow_at(
    world_pos : vec3<f32>,
    world_normal : vec3<f32>,
    rt_light : RtLightUniform,
    t_shadow : texture_depth_2d,
    s_shadow : sampler_comparison,
    rt_occlusion : f32,
) -> f32 {
    if rt_light.rt_flags.w > 0.5 {
        return rt_occlusion;
    }
    if rt_light.light_color.w <= 0.5 {
        return 1.0;
    }
    var l = rt_light.light_dir.xyz;
    if dot(l, l) < 1e-6 {
        l = vec3<f32>(0.45, 1.0, 0.35);
    }
    l = normalize(l);
    let n = normalize(world_normal);
    let ndotl = max(dot(n, l), 0.0);
    let normal_scale = rt_light.shadow_bias.x + rt_light.shadow_bias.y * (1.0 - ndotl);
    let biased_pos = world_pos + l * normal_scale;
    let clip = rt_light.light_view_proj * vec4<f32>(biased_pos, 1.0);
    let ndc = clip.xyz / clip.w;
    var uv = ndc.xy * 0.5 + 0.5;
    uv.y = 1.0 - uv.y;
    if uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 {
        return 1.0;
    }
    let slope = sqrt(max(1.0 - ndotl * ndotl, 0.0));
    let depth_ref = ndc.z - (rt_light.shadow_bias.z + rt_light.shadow_bias.w * slope);
    let texel = rt_light.light_params.z;
    let radius = rt_light.light_params.w;
    var sum = 0.0;
    for (var oy = -1; oy <= 1; oy++) {
        for (var ox = -1; ox <= 1; ox++) {
            let off = vec2<f32>(f32(ox), f32(oy)) * texel * radius;
            sum += textureSampleCompareLevel(t_shadow, s_shadow, uv + off, depth_ref);
        }
    }
    return sum / 9.0;
}

fn refl_tri_barycentric(tri : RtTri, p : vec3<f32>) -> vec3<f32> {
    let v0 = tri.v0.xyz;
    let v1 = tri.v1.xyz;
    let v2 = tri.v2.xyz;
    let v0v1 = v1 - v0;
    let v0v2 = v2 - v0;
    let v0p = p - v0;
    let d00 = dot(v0v1, v0v1);
    let d01 = dot(v0v1, v0v2);
    let d11 = dot(v0v2, v0v2);
    let d20 = dot(v0p, v0v1);
    let d21 = dot(v0p, v0v2);
    let denom = d00 * d11 - d01 * d01;
    if abs(denom) < 1e-8 {
        return vec3<f32>(1.0, 0.0, 0.0);
    }
    let v = (d11 * d20 - d01 * d21) / denom;
    let w = (d00 * d21 - d01 * d20) / denom;
    let u = 1.0 - v - w;
    return vec3<f32>(u, v, w);
}

fn refl_normal_from_packed(packed : vec4<f32>) -> vec3<f32> {
    var p = packed.zw * 2.0 - vec2<f32>(1.0);
    var n = vec3<f32>(p.x, p.y, 1.0 - abs(p.x) - abs(p.y));
    if n.z < 0.0 {
        let ox = n.x;
        let oy = n.y;
        n.x = (1.0 - abs(oy)) * sign(ox);
        n.y = (1.0 - abs(ox)) * sign(oy);
    }
    return normalize(n);
}

fn refl_material_albedo(
    mat : RtInstanceMaterial,
    has_tri : bool,
    tri : RtTri,
    bary : vec3<f32>,
    t_albedo : texture_2d_array<f32>,
    s_albedo : sampler,
) -> vec3<f32> {
    if has_tri {
        let layer = i32(mat.pbr.w);
        if layer >= 0 {
            let uv = refl_tri_bary_uv(tri, bary);
            return textureSampleLevel(t_albedo, s_albedo, uv, layer, 0.0).rgb;
        }
    }
    return mat.albedo.xyz;
}

fn refl_hit_lighting_lite(
    hit_pos : vec3<f32>,
    hit_normal : vec3<f32>,
    view_dir : vec3<f32>,
    mat : RtInstanceMaterial,
    has_material : bool,
    has_tri : bool,
    hit_tri : RtTri,
    bary : vec3<f32>,
    probe_meta : ReflProbeMeta,
    t_probe : texture_cube_array<f32>,
    s_probe : sampler,
    t_albedo : texture_2d_array<f32>,
    s_albedo : sampler,
    rt_light : RtLightUniform,
    t_shadow : texture_depth_2d,
    s_shadow : sampler_comparison,
    rt_occlusion : f32,
) -> vec3<f32> {
    if !has_material {
        return refl_sample_probe_at_hit(hit_pos, view_dir, 0.0, probe_meta, t_probe, s_probe);
    }
    let flags = bitcast<u32>(mat.albedo.w);
    let is_dielectric = (flags & RT_MAT_FLAG_DIELECTRIC) != 0u;
    let albedo = refl_material_albedo(mat, has_tri, hit_tri, bary, t_albedo, s_albedo);
    let metallic = select(mat.pbr.y, 0.0, is_dielectric);
    let roughness = mat.pbr.x;
    let ior = max(mat.pbr.z, 1.01);
    let env = refl_sample_probe_for_material(
        hit_pos,
        view_dir,
        roughness,
        mat,
        has_material,
        probe_meta,
        t_probe,
        s_probe,
    );
    let n = normalize(hit_normal);
    let v = normalize(view_dir);
    let ndotv = max(dot(n, v), 0.0);
    var f0 = mix(vec3<f32>(0.04), albedo, metallic);
    if is_dielectric {
        let cos_theta = max(dot(-v, n), 0.0);
        let fr = refl_dielectric_fresnel(cos_theta, ior);
        f0 = vec3<f32>(fr);
    }
    let fresnel = f0 + (vec3<f32>(1.0) - f0) * pow(vec3<f32>(1.0 - ndotv), vec3<f32>(5.0));
    let spec = env * fresnel;
    let diff = env * (1.0 - metallic) * albedo * (vec3<f32>(1.0) - fresnel);

    var l = rt_light.light_dir.xyz;
    if dot(l, l) < 1e-6 {
        l = vec3<f32>(0.45, 1.0, 0.35);
    }
    l = normalize(l);
    let shadow = refl_rt_shadow_at(hit_pos, n, rt_light, t_shadow, s_shadow, rt_occlusion);
    let ndotl = max(dot(n, l), 0.0);
    let direct = rt_light.light_color.rgb * ndotl * shadow;
    let direct_diff = direct * albedo * (1.0 - metallic) * (vec3<f32>(1.0) - fresnel);
    let direct_spec = direct * fresnel * (1.0 - roughness) * metallic;
    return spec + diff + direct_diff + direct_spec;
}

/// Off-screen RT hit: direct lighting on traced geometry (no probe cubemap).
fn refl_hit_lighting_geometry(
    hit_pos : vec3<f32>,
    hit_normal : vec3<f32>,
    view_dir : vec3<f32>,
    mat : RtInstanceMaterial,
    has_material : bool,
    has_tri : bool,
    hit_tri : RtTri,
    bary : vec3<f32>,
    t_albedo : texture_2d_array<f32>,
    s_albedo : sampler,
    rt_light : RtLightUniform,
    t_shadow : texture_depth_2d,
    s_shadow : sampler_comparison,
    rt_occlusion : f32,
) -> vec3<f32> {
    if !has_material {
        return vec3<f32>(0.04);
    }
    let flags = bitcast<u32>(mat.albedo.w);
    let is_dielectric = (flags & RT_MAT_FLAG_DIELECTRIC) != 0u;
    let albedo = refl_material_albedo(mat, has_tri, hit_tri, bary, t_albedo, s_albedo);
    let metallic = select(mat.pbr.y, 0.0, is_dielectric);
    let roughness = mat.pbr.x;
    let ior = max(mat.pbr.z, 1.01);
    let n = normalize(hit_normal);
    let v = normalize(view_dir);
    let ndotv = max(dot(n, v), 0.0);
    var f0 = mix(vec3<f32>(0.04), albedo, metallic);
    if is_dielectric {
        let cos_theta = max(dot(-v, n), 0.0);
        let fr = refl_dielectric_fresnel(cos_theta, ior);
        f0 = vec3<f32>(fr);
    }
    let fresnel = f0 + (vec3<f32>(1.0) - f0) * pow(vec3<f32>(1.0 - ndotv), vec3<f32>(5.0));

    var l = rt_light.light_dir.xyz;
    if dot(l, l) < 1e-6 {
        l = vec3<f32>(0.45, 1.0, 0.35);
    }
    l = normalize(l);
    let shadow = refl_rt_shadow_at(hit_pos, n, rt_light, t_shadow, s_shadow, rt_occlusion);
    let ndotl = max(dot(n, l), 0.0);
    let direct = rt_light.light_color.rgb * ndotl * shadow;
    let direct_diff = direct * albedo * (1.0 - metallic) * (vec3<f32>(1.0) - fresnel);
    let direct_spec = direct * fresnel * (1.0 - roughness) * metallic;
    let fill = rt_light.light_color.rgb * 0.12 * albedo * (1.0 - metallic * 0.5);
    return direct_diff + direct_spec + fill;
}

fn refl_refract_dir(incident : vec3<f32>, normal : vec3<f32>, eta : f32) -> vec3<f32> {
    let n = normalize(normal);
    let uv = normalize(incident);
    let cos_theta = min(dot(-uv, n), 1.0);
    let sin_theta = sqrt(max(1.0 - cos_theta * cos_theta, 0.0));
    let cannot_refract = eta * sin_theta > 1.0;
    if cannot_refract {
        return reflect(uv, n);
    }
    let r_out_perp = eta * (uv + cos_theta * n);
    let r_out_parallel = -sqrt(max(1.0 - dot(r_out_perp, r_out_perp), 0.0)) * n;
    return normalize(r_out_perp + r_out_parallel);
}

fn refl_dielectric_fresnel(cos_theta : f32, ref_idx : f32) -> f32 {
    var r0 = (1.0 - ref_idx) / (1.0 + ref_idx);
    r0 = r0 * r0;
    return r0 + (1.0 - r0) * pow(1.0 - cos_theta, 5.0);
}

fn refl_screen_gbuffer_blend_weight(
    hit_pos : vec3<f32>,
    hit_normal : vec3<f32>,
    sample_uv : vec2<f32>,
    on_screen : bool,
    step_size : f32,
    t_depth : texture_2d<f32>,
    t_normal_roughness : texture_2d<f32>,
    view_proj : mat4x4<f32>,
    near_plane : f32,
    far_plane : f32,
    gb_resolution : vec2<f32>,
) -> f32 {
    if !on_screen {
        return 0.0;
    }
    let hit_px = vec2<i32>(sample_uv * gb_resolution);
    let hit_depth_m = refl_view_depth_m_from_gl_ndc_z(
        textureLoad(t_depth, hit_px, 0).r,
        near_plane,
        far_plane,
    );
    let clip = view_proj * vec4<f32>(hit_pos, 1.0);
    let ray_depth_m = (near_plane * far_plane)
        / (far_plane - refl_gl_ndc_z_to_vk(clip.z / clip.w) * (far_plane - near_plane));
    let depth_delta = abs(ray_depth_m - hit_depth_m);
    let depth_reject = refl_rt_hit_depth_reject_m(step_size);
    if depth_delta > depth_reject {
        return 0.0;
    }
    var w = 1.0 - clamp(depth_delta / depth_reject, 0.0, 1.0);
    let gb_n = refl_normal_from_packed(textureLoad(t_normal_roughness, hit_px, 0));
    let n_dot = max(dot(normalize(hit_normal), gb_n), 0.0);
    w *= smoothstep(0.3, 0.95, n_dot);
    return w;
}

fn refl_resolve_hit_radiance(
    hit_pos : vec3<f32>,
    hit_normal : vec3<f32>,
    refl_dir : vec3<f32>,
    sample_uv : vec2<f32>,
    on_screen : bool,
    spacing_px : f32,
    mat : RtInstanceMaterial,
    has_material : bool,
    has_tri : bool,
    hit_tri : RtTri,
    bary : vec3<f32>,
    probe_meta : ReflProbeMeta,
    t_probe : texture_cube_array<f32>,
    s_probe : sampler,
    t_albedo : texture_2d_array<f32>,
    s_albedo : sampler,
    rt_light : RtLightUniform,
    t_shadow : texture_depth_2d,
    s_shadow : sampler_comparison,
    t_lit_scene : texture_2d<f32>,
    t_depth : texture_2d<f32>,
    t_normal_roughness : texture_2d<f32>,
    t_direct : texture_2d<f32>,
    t_base_color : texture_2d<f32>,
    resolution : vec2<f32>,
    step_size : f32,
    view_proj : mat4x4<f32>,
    near_plane : f32,
    far_plane : f32,
    rt_occlusion : f32,
    material_quality : f32,
) -> vec3<f32> {
    _ = spacing_px;
    let view_dir = normalize(refl_dir);
    let allow_screen_blend = material_quality >= 1.0;
    if has_material && !on_screen {
        return refl_hit_lighting_geometry(
            hit_pos,
            hit_normal,
            view_dir,
            mat,
            has_material,
            has_tri,
            hit_tri,
            bary,
            t_albedo,
            s_albedo,
            rt_light,
            t_shadow,
            s_shadow,
            rt_occlusion,
        );
    }
    if has_material {
        let instance_col = refl_hit_lighting_lite(
            hit_pos,
            hit_normal,
            view_dir,
            mat,
            has_material,
            has_tri,
            hit_tri,
            bary,
            probe_meta,
            t_probe,
            s_probe,
            t_albedo,
            s_albedo,
            rt_light,
            t_shadow,
            s_shadow,
            rt_occlusion,
        );
        let blend_w = select(
            0.0,
            refl_screen_gbuffer_blend_weight(
                hit_pos,
                hit_normal,
                sample_uv,
                on_screen,
                step_size,
                t_depth,
                t_normal_roughness,
                view_proj,
                near_plane,
                far_plane,
                resolution,
            ),
            allow_screen_blend,
        );
        if blend_w > 0.001 {
            let hit_px = vec2<i32>(sample_uv * resolution);
            let hit_metallic = textureLoad(t_direct, hit_px, 0).a;
            let hit_albedo = textureLoad(t_base_color, hit_px, 0).rgb;
            let lit = textureLoad(t_lit_scene, hit_px, 0).rgb;
            let screen_col = refl_metal_attenuate(lit, hit_albedo, hit_metallic);
            return mix(instance_col, screen_col, blend_w);
        }
        return instance_col;
    }
    if on_screen {
        let blend_w = select(
            0.0,
            refl_screen_gbuffer_blend_weight(
                hit_pos,
                hit_normal,
                sample_uv,
                on_screen,
                step_size,
                t_depth,
                t_normal_roughness,
                view_proj,
                near_plane,
                far_plane,
                resolution,
            ),
            allow_screen_blend,
        );
        if blend_w > 0.001 {
            let hit_px = vec2<i32>(sample_uv * resolution);
            let hit_metallic = textureLoad(t_direct, hit_px, 0).a;
            let hit_albedo = textureLoad(t_base_color, hit_px, 0).rgb;
            let lit = textureLoad(t_lit_scene, hit_px, 0).rgb;
            return refl_metal_attenuate(lit, hit_albedo, hit_metallic);
        }
    }
    return refl_hit_lighting_lite(
        hit_pos,
        hit_normal,
        view_dir,
        mat,
        has_material,
        has_tri,
        hit_tri,
        bary,
        probe_meta,
        t_probe,
        s_probe,
        t_albedo,
        s_albedo,
        rt_light,
        t_shadow,
        s_shadow,
        rt_occlusion,
    );
}
