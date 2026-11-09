//! Compila los shaders GLSL del motor 2D a SPIR-V con `glslc`.
//!
//! Es el mismo esquema que `engine_3d/build.rs`: busca `glslc` en `PATH`, luego en `VULKAN_SDK` y
//! por último en `C:\VulkanSDK\*`. Los `.spv` quedan en `OUT_DIR` y la ruta Vulkan los carga con
//! `include_bytes!(concat!(env!("OUT_DIR"), "/<nombre>.spv"))`.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let glslc = find_glslc().unwrap_or_else(|| {
        panic!(
            "No se encontró glslc.\n\
             Instala el Vulkan SDK (https://vulkan.lunarg.com/) y abre una terminal nueva,\n\
             o define la variable VULKAN_SDK apuntando a la raíz del SDK\n\
             (glslc está en %VULKAN_SDK%\\Bin\\glslc.exe)."
        );
    });

    let shaders: &[(&str, &str)] = &[
        ("shaders/shader.vert", "shader.vert.spv"),
        ("shaders/shader.frag", "shader.frag.spv"),
        (
            "shaders/shader_screen_hud.vert",
            "shader_screen_hud.vert.spv",
        ),
        (
            "shaders/shader_screen_hud.frag",
            "shader_screen_hud.frag.spv",
        ),
        ("shaders/gizmo.vert", "gizmo.vert.spv"),
        ("shaders/gizmo.frag", "gizmo.frag.spv"),
        ("shaders/scene_blit.vert", "scene_blit.vert.spv"),
        ("shaders/scene_blit.frag", "scene_blit.frag.spv"),
    ];

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
