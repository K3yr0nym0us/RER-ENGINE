//! Compila los shaders GLSL de los motores 2D y 3D a SPIR-V con `glslc`.
//!
//! Solo se compila la lista del motor cuya feature esta activa. Los `.spv` del
//! 2D llevan prefijo `2d_` para no colisionar con los del 3D en el mismo
//! `OUT_DIR`.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

const SHADERS_3D: &[(&str, &str)] = &[
    ("shaders/rt/primary.rgen", "primary.rgen.spv"),
    ("shaders/rt/reflect.rgen", "reflect.rgen.spv"),
    ("shaders/rt/primary.rmiss", "primary.rmiss.spv"),
    ("shaders/rt/primary.rchit", "primary.rchit.spv"),
    ("shaders/bvh/trace.comp", "bvh.comp.spv"),
    ("shaders/raster/forward.vert", "forward.vert.spv"),
    ("shaders/raster/forward.frag", "forward.frag.spv"),
    ("shaders/raster/shadow.vert", "shadow.vert.spv"),
    ("shaders/raster/gizmo.vert", "gizmo.vert.spv"),
    ("shaders/raster/gizmo.frag", "gizmo.frag.spv"),
    ("shaders/raster/hud.vert", "hud.vert.spv"),
    ("shaders/raster/hud.frag", "hud.frag.spv"),
    ("shaders/raster/fullscreen.vert", "fullscreen.vert.spv"),
    ("shaders/raster/taa.frag", "taa.frag.spv"),
    ("shaders/raster/ssr.frag", "ssr.frag.spv"),
    ("shaders/raster/temporal.frag", "temporal.frag.spv"),
    ("shaders/raster/denoise.frag", "denoise.frag.spv"),
    ("shaders/raster/composite.frag", "composite.frag.spv"),
    ("shaders/raster/skinned.vert", "skinned.vert.spv"),
    ("shaders/raster/hud_inst.vert", "hud_inst.vert.spv"),
    ("shaders/raster/hud_inst.frag", "hud_inst.frag.spv"),
    ("shaders/raster/present.frag", "present.frag.spv"),
    ("shaders/raster/depth_only.frag", "depth_only.frag.spv"),
    ("shaders/raster/sky.frag", "sky.frag.spv"),
];

const SHADERS_2D: &[(&str, &str)] = &[
    ("shaders/2d/shader.vert", "2d_shader.vert.spv"),
    ("shaders/2d/shader.frag", "2d_shader.frag.spv"),
    (
        "shaders/2d/shader_screen_hud.vert",
        "2d_shader_screen_hud.vert.spv",
    ),
    (
        "shaders/2d/shader_screen_hud.frag",
        "2d_shader_screen_hud.frag.spv",
    ),
    ("shaders/2d/gizmo.vert", "2d_gizmo.vert.spv"),
    ("shaders/2d/gizmo.frag", "2d_gizmo.frag.spv"),
    ("shaders/2d/scene_blit.vert", "2d_scene_blit.vert.spv"),
    ("shaders/2d/scene_blit.frag", "2d_scene_blit.frag.spv"),
];

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));

    let mut shaders: Vec<(&str, &str)> = Vec::new();
    if env::var_os("CARGO_FEATURE_ENGINE3D").is_some() {
        shaders.extend_from_slice(SHADERS_3D);
    }
    if env::var_os("CARGO_FEATURE_ENGINE2D").is_some() {
        shaders.extend_from_slice(SHADERS_2D);
    }
    if shaders.is_empty() {
        return;
    }

    let glslc = find_glslc().unwrap_or_else(|| {
        panic!(
            "No se encontró glslc.\n\
             Instala el Vulkan SDK (https://vulkan.lunarg.com/) y abre una terminal nueva,\n\
             o define la variable VULKAN_SDK apuntando a la raíz del SDK\n\
             (glslc está en %VULKAN_SDK%\\Bin\\glslc.exe)."
        );
    });

    for (src, out_name) in shaders {
        let source = manifest_dir.join(src);
        let output = out_dir.join(out_name);
        println!("cargo:rerun-if-changed={}", source.display());
        let status = Command::new(&glslc)
            .arg("--target-env=vulkan1.3")
            .arg(source.as_os_str())
            .arg("-o")
            .arg(output.as_os_str())
            .status()
            .unwrap_or_else(|error| {
                panic!("No se pudo ejecutar glslc para {src}: {error}");
            });
        if !status.success() {
            panic!("glslc falló al compilar {src}");
        }
    }
}

fn find_glslc() -> Option<PathBuf> {
    if let Some(path) = which_in_path("glslc.exe").or_else(|| which_in_path("glslc")) {
        return Some(path);
    }
    if let Ok(sdk) = env::var("VULKAN_SDK") {
        let candidate = Path::new(&sdk).join("Bin").join("glslc.exe");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    let root = Path::new(r"C:\VulkanSDK");
    let entries = std::fs::read_dir(root).ok()?;
    let mut versions: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path().join("Bin").join("glslc.exe"))
        .filter(|path| path.is_file())
        .collect();
    versions.sort();
    versions.pop()
}

fn which_in_path(name: &str) -> Option<PathBuf> {
    let paths = env::var_os("PATH")?;
    env::split_paths(&paths)
        .map(|dir| dir.join(name))
        .find(|path| path.is_file())
}
