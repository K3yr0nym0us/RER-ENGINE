//! Prefija `reflection_math.wgsl` + `forward_ibl.wgsl` en shaders de escena (forward + skinned).

use wgpu::Device;

pub fn load_scene_wgsl(device: &Device, label: &'static str, body: &str) -> wgpu::ShaderModule {
    let source = format!(
        "{}\n{}\n{}",
        include_str!("reflections/reflection_math.wgsl"),
        include_str!("reflections/forward_ibl.wgsl"),
        body
    );
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    })
}
