// Downsample para generar los mips del cubemap de reflection probes.
// Caja 2×2 vía filtrado bilineal: cada téxel destino samplea el mip previo en su centro.
// Sin mips, muestrear el cubemap (128 px) en una esfera espejo produce moiré del damero;
// con mips + auto-LOD (textureSampleBias) ese aliasing desaparece.

@group(0) @binding(0) var t_src: texture_2d<f32>;
@group(0) @binding(1) var s_src: sampler;

struct VsOut {
    @builtin(position) pos : vec4<f32>,
    @location(0) uv        : vec2<f32>,
};

@vertex
fn vs_mip(@builtin(vertex_index) vi: u32) -> VsOut {
    var out: VsOut;
    // Triángulo a pantalla completa.
    let x = f32((vi << 1u) & 2u);
    let y = f32(vi & 2u);
    out.uv = vec2<f32>(x, y);
    out.pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    return out;
}

@fragment
fn fs_mip(in: VsOut) -> @location(0) vec4<f32> {
    return textureSampleLevel(t_src, s_src, in.uv, 0.0);
}
