// Resolve manual de attachments MSAA cuyo formato no soporta MULTISAMPLE_RESOLVE
// (p. ej. R32Float depth-export). Copia el sample 0 al target 1×.

struct VsOut {
    @builtin(position) pos: vec4<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VsOut {
    var o: VsOut;
    let x = f32(i32(idx & 1u) * 4 - 1);
    let y = f32(i32(idx >> 1u) * 4 - 1);
    o.pos = vec4<f32>(x, y, 0.0, 1.0);
    return o;
}

@group(0) @binding(0) var src_ms: texture_multisampled_2d<f32>;

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let c = vec2<i32>(i32(pos.x), i32(pos.y));
    // Sample 0: suficiente para depth/SSR; evita bucles con N variable en WGSL.
    return textureLoad(src_ms, c, 0);
}
