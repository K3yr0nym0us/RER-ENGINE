@group(0) @binding(0) var t_ssr : texture_2d<f32>;
@group(0) @binding(1) var reflection_out : texture_storage_2d<rgba8unorm, write>;

@compute @workgroup_size(8, 8)
fn cs_main(@builtin(global_invocation_id) gid : vec3<u32>) {
    let dims = textureDimensions(t_ssr);
    if gid.x >= dims.x || gid.y >= dims.y {
        return;
    }
    let px = vec2<i32>(i32(gid.x), i32(gid.y));
    textureStore(reflection_out, px, textureLoad(t_ssr, px, 0));
}
