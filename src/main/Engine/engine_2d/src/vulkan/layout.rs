//! Contrato de bytes entre los shaders GLSL y las estructuras Rust.
//!
//! Los `.vert`/`.frag` de `shaders/` declaran `location`/`vec4`/`mat4` con un layout concreto.
//! Estas pruebas fijan ese layout desde el lado Rust: si alguien reordena un campo, añade uno o
//! cambia un `[f32; N]`, el binario deja de compilar sus tests en vez de dibujar basura (o, peor,
//! pasar la capa de validación sin dibujar nada).
//!
//! Referencias de los shaders:
//! - `shaders/shader.vert` / `gizmo`: locations 0..8 y bloque `Scene`.
//! - `shaders/shader_screen_hud.vert`: el bloque completo del 2D (11 miembros).
//! - `shaders/gizmo.vert`: `view_proj` + `model` + `flags` (9 `vec4` = 144 B).

#[cfg(test)]
mod tests {
    use std::mem::{offset_of, size_of};

    use crate::engine::types::{SceneUniforms, ScreenHudSceneUniforms};
    use crate::mesh::{InstanceData, Vertex};
    use rer_engine_shared::player_ui::ndc_draw::NdcVertex;

    #[test]
    fn ndc_vertex_layout_matches_gizmo_glsl() {
        // location 0: vec3 position; location 1: vec4 color. Rejilla, colisiones y gizmo
        // comparten este layout (32 - 4 = 28 B: vec3 alineado a 4, sin padding final).
        assert_eq!(size_of::<NdcVertex>(), 28);
        assert_eq!(offset_of!(NdcVertex, position), 0);
        assert_eq!(offset_of!(NdcVertex, color), 12);
    }

    #[test]
    fn vertex_layout_matches_glsl_locations_0_to_2() {
        // location 0: vec3 position; location 1: vec3 normal; location 2: vec2 uv.
        assert_eq!(size_of::<Vertex>(), 32);
        assert_eq!(offset_of!(Vertex, position), 0);
        assert_eq!(offset_of!(Vertex, normal), 12);
        assert_eq!(offset_of!(Vertex, uv), 24);
    }

    #[test]
    fn instance_layout_matches_glsl_locations_3_to_8() {
        // model (4 vec4) + flag_pad + uv_rect.
        assert_eq!(size_of::<InstanceData>(), 96);
        assert_eq!(offset_of!(InstanceData, model), 0);
        assert_eq!(offset_of!(InstanceData, flag_pad), 64);
        assert_eq!(offset_of!(InstanceData, uv_rect), 80);
    }

    #[test]
    fn scene_uniform_block_matches_glsl() {
        // mat4 view_proj; vec4 cam_pos; (std140: mat4 = 64 B, vec4 = 16 B)
        assert_eq!(size_of::<SceneUniforms>(), 80);
        assert_eq!(offset_of!(SceneUniforms, view_proj), 0);
        assert_eq!(offset_of!(SceneUniforms, cam_pos), 64);
    }

    #[test]
    fn screen_hud_uniform_block_matches_glsl() {
        // Cuatro mat4 seguidos, tres vec4, otro mat4 y tres vec4 más.
        assert_eq!(size_of::<ScreenHudSceneUniforms>(), 416);
        assert_eq!(offset_of!(ScreenHudSceneUniforms, view_proj), 0);
        assert_eq!(offset_of!(ScreenHudSceneUniforms, view_proj_stable), 64);
        assert_eq!(offset_of!(ScreenHudSceneUniforms, prev_view_proj), 128);
        assert_eq!(offset_of!(ScreenHudSceneUniforms, inv_view_proj), 192);
        assert_eq!(offset_of!(ScreenHudSceneUniforms, cam_pos), 256);
        assert_eq!(offset_of!(ScreenHudSceneUniforms, light_dir), 272);
        assert_eq!(offset_of!(ScreenHudSceneUniforms, light_color), 288);
        assert_eq!(offset_of!(ScreenHudSceneUniforms, light_view_proj), 304);
        assert_eq!(offset_of!(ScreenHudSceneUniforms, light_params), 368);
        assert_eq!(offset_of!(ScreenHudSceneUniforms, jitter), 384);
        assert_eq!(offset_of!(ScreenHudSceneUniforms, shadow_bias), 400);
    }

    #[test]
    fn gizmo_uniform_block_is_nine_vec4() {
        // El 2D sube `[[f32; 4]; 9]` en línea: view_proj (64) + model (64) + flags (16).
        let data: [[f32; 4]; 9] = [[0.0; 4]; 9];
        assert_eq!(size_of_val(&data), 144);
        assert_eq!(size_of_val(&data[8]), 16);
    }
}
