//! Fachada del frame de reflejos: probes + pases de pantalla (SSR/RT/composite).
//!
//! ## Orden del frame (no cambiar sin revisar capas A/B/C en `policy.rs`)
//! 1. [`prepare_probes`] — ranuras, meta GPU, `probe_index_map` (antes del main pass).
//! 2. Main geometry pass — instancias con `tex_layer_pad.z` = ranura o `-1`.
//! 3. Export albedo / G-buffer.
//! 4. [`encode_probe_captures`] — cubemap 6 caras (+ mips) por probe activo.
//! 5. `State::prepare_lit_scene` (sigue en `render.rs`).
//! 6. [`ReflectionPass::run_screen`] — SSR, temporal, RT, denoise (sin composite).

use glam::{Mat4, Vec3};
use wgpu::{CommandEncoder, Device, Queue, TextureView};

use crate::config_3d::reflection_graphics::{ReflectionDebugView, ReflectionSettings};
use crate::engine::{SceneUniforms, State};
use crate::reflections::probes_pipeline::capture::{self, ProbeFrameData};
use crate::reflections::ReflectionPass;

/// Prepara lista de probes, meta y mapa entidad→ranura para el main pass.
pub fn prepare_probes(state: &mut State, settings: &ReflectionSettings) -> ProbeFrameData {
    capture::prepare_probe_frame(state, settings)
}

/// Entrada para codificar capturas de cubemap (tras shadow pass, antes del main pass).
pub struct ProbeCaptureInput<'a> {
    pub settings: &'a ReflectionSettings,
    pub probe_frame: &'a ProbeFrameData,
    pub scene_uni: &'a SceneUniforms,
    pub skinned_probe: &'a [(usize, crate::mesh::SkinnedInstanceData)],
}

pub fn encode_probe_captures(
    state: &mut State,
    enc: &mut CommandEncoder,
    input: &ProbeCaptureInput<'_>,
) {
    capture::encode_probe_captures(
        state,
        enc,
        input.settings,
        input.probe_frame,
        input.scene_uni,
        input.skinned_probe,
    );
}

/// Parámetros del pase SSR/RT/temporal (composite sigue en `render.rs`).
pub struct ReflectionScreenInput<'a> {
    pub settings: ReflectionSettings,
    pub debug_view: ReflectionDebugView,
    pub depth_view: &'a TextureView,
    pub normal_roughness_view: &'a TextureView,
    pub lit_scene_view: &'a TextureView,
    pub direct_view: &'a TextureView,
    pub surface_view: &'a TextureView,
    pub base_color_view: &'a TextureView,
    pub depth_export_view: &'a TextureView,
    pub velocity_view: &'a TextureView,
    pub inv_view_proj: Mat4,
    pub view_proj: Mat4,
    pub view: Mat4,
    pub cam_pos: Vec3,
    pub near_plane: f32,
    pub far_plane: f32,
    pub clear_color: wgpu::Color,
    pub probe_bind_group: &'a wgpu::BindGroup,
    pub shadow_view: &'a TextureView,
    pub shadow_sampler: &'a wgpu::Sampler,
    pub scene_uniforms: &'a SceneUniforms,
    pub texture_bind_group: &'a wgpu::BindGroup,
    pub ssr_debug_mode: bool,
}

impl ReflectionPass {
    pub fn run_screen(
        &mut self,
        device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
        input: ReflectionScreenInput<'_>,
    ) -> bool {
        self.run(
            device,
            queue,
            encoder,
            input.settings,
            input.debug_view,
            input.depth_view,
            input.normal_roughness_view,
            input.lit_scene_view,
            input.direct_view,
            input.surface_view,
            input.base_color_view,
            input.depth_export_view,
            input.velocity_view,
            input.inv_view_proj,
            input.view_proj,
            input.view,
            input.cam_pos,
            input.near_plane,
            input.far_plane,
            input.clear_color,
            input.probe_bind_group,
            input.shadow_view,
            input.shadow_sampler,
            input.scene_uniforms,
            input.texture_bind_group,
            input.ssr_debug_mode,
        )
    }
}
