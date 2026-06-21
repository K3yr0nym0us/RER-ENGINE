//! Post-proceso 3D alineado con UE / Unity / Godot 4:
//!
//! 1. **G-buffer ligero (MRT):** `ambient`, `direct`, máscara de sombra, depth en metros (R32), velocity+normal (RGBA8).
//! 2. **Shadow map único 2D** (2048, depth compare).
//! 3. **TAA en máscara de sombra** → **lit-composite:** `lit = ambient + direct × mix(darkness, 1, shadow)`.
//! 4. **TAA de escena** (reproject + depth/velocity, depth vía `texture_2d` filtrable-nearest, no `textureLoad` en depth).
//! 5. **Blit** al swapchain; overlays del editor después del blit.
//!
//! El slider *Shadow darkness* se aplica solo en lit-composite (CPU), no en `light_params` del forward pass.

mod depth_readback;

use bytemuck::{Pod, Zeroable};
use depth_readback::{DepthExportReadback, log_depth_export_stats, log_depth_probe_cpu};
use wgpu::util::DeviceExt;
use wgpu::{include_wgsl, Device, Queue, TextureFormat, TextureView};

/// Acumulación temporal sombras (más alto = bordes más suaves).
const SHADOW_HISTORY_BLEND: f32 = 30.0 / 32.0;
/// Acumulación escena (geometría y bordes de objetos).
const SCENE_HISTORY_BLEND: f32 = 0.62;
/// Umbral depth para disoclusión; algo más laxo evita cortar history en siluetas.
/// Umbral de disoclusión en metros (depth export = distancia view-space).
const DISOCCLUSION_THRESHOLD: f32 = 0.75;

/// G-buffer de superficie: `.r` = máscara de sombra (1=iluminado), `.g` = roughness.
/// `Rg16Float` cuesta 4 B/sample (vs 8 B de Rgba8Unorm) para respetar el límite WebGPU
/// baseline de 32 B/sample, garantizado en TODA plataforma de exportación. El metallic
/// NO cabe aquí; se empaqueta en el alpha del buffer `direct` (canal libre).
pub const SHADOW_MASK_FORMAT: TextureFormat = TextureFormat::Rg16Float;
/// Profundidad en metros (view depth) exportada para TAA y SSR. 4 B/sample.
pub const DEPTH_EXPORT_FORMAT: TextureFormat = TextureFormat::R32Float;
/// Velocity (rg) + normal xy octaédrica empaquetada (ba). 8 B/sample.
pub const VELOCITY_FORMAT: TextureFormat = TextureFormat::Rgba8Unorm;
/// Ambient/direct/scene internos. 8 B/sample.
pub const MRT_LIT_FORMAT: TextureFormat = TextureFormat::Rgba8Unorm;
/// Albedo base del material (SSR/RT F0 y atenuación metal). 4 B/sample.
pub const BASE_COLOR_FORMAT: TextureFormat = TextureFormat::Rgba8Unorm;

/// Estabilidad TAA: mantiene más acumulación a distancia media (menos dientes de sierra lejos).
pub fn zoom_stability_distance(distance: f32) -> f32 {
    const NEAR: f32 = 2.5;
    const FAR: f32 = 48.0;
    let t = ((distance - NEAR) / (FAR - NEAR)).clamp(0.0, 1.0);
    1.0 - t * 0.55
}

/// Jitter subpíxel Halton (2, 3) centrado en [-0.5, 0.5].
pub fn halton_jitter(frame_index: u32) -> [f32; 2] {
    fn halton(mut index: u32, base: u32) -> f32 {
        let mut f = 1.0f32;
        let mut r = 0.0f32;
        let b = base as f32;
        while index > 0 {
            f /= b;
            r += f * (index % base) as f32;
            index /= base;
        }
        r
    }
    let i = frame_index.wrapping_add(1);
    [halton(i, 2) - 0.5, halton(i, 3) - 0.5]
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TaaUniforms {
    resolution: [f32; 2],
    blend: f32,
    enabled: f32,
    zoom_stability: f32,
    _pad: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SceneTaaUniforms {
    resolution: [f32; 2],
    blend: f32,
    enabled: f32,
    zoom_stability: f32,
    jitter: [f32; 2],
    disocclusion: f32,
    _pad0: f32,
    /// Alineación WGSL: `mat4x4` empieza en offset 48 (12 bytes tras `_pad0`).
    _pad_align: [f32; 3],
    inv_view_proj: [[f32; 4]; 4],
    prev_view_proj: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LitCompositeUniforms {
    shadow_darkness: f32,
    shadows_enabled: f32,
    _pad0: f32,
    _pad1: f32,
}

/// Recursos GPU: MRT + lit-composite + TAA sombra/escena.
pub struct TaaPass {
    pub enabled: bool,
    pub scene_taa_enabled: bool,
    ambient_view: TextureView,
    direct_view: TextureView,
    depth_export_view: TextureView,
    velocity_view: TextureView,
    base_color_view: TextureView,
    scene_color_view: TextureView,
    shadow_mask_view: TextureView,
    shadow_history_views: [TextureView; 2],
    scene_history_views: [TextureView; 2],
    shadow_history_index: u8,
    scene_history_index: u8,
    shadow_resolve_pipeline: wgpu::RenderPipeline,
    scene_resolve_pipeline: wgpu::RenderPipeline,
    lit_composite_pipeline: wgpu::RenderPipeline,
    blit_pipeline: wgpu::RenderPipeline,
    shadow_resolve_bgl: wgpu::BindGroupLayout,
    scene_resolve_bgl: wgpu::BindGroupLayout,
    lit_composite_bgl: wgpu::BindGroupLayout,
    blit_bgl: wgpu::BindGroupLayout,
    shadow_resolve_bind_groups: [wgpu::BindGroup; 2],
    scene_resolve_bind_groups: [wgpu::BindGroup; 2],
    lit_composite_bind_groups: [wgpu::BindGroup; 3],
    blit_bind_groups: [wgpu::BindGroup; 3],
    taa_uniform_buffer: wgpu::Buffer,
    scene_taa_uniform_buffer: wgpu::Buffer,
    lit_composite_uniform_buffer: wgpu::Buffer,
    sampler: wgpu::Sampler,
    shadow_sampler: wgpu::Sampler,
    frame_index: u32,
    pub current_jitter: [f32; 2],
    shadow_first_frame: bool,
    scene_first_frame: bool,
    _ambient_texture: wgpu::Texture,
    _direct_texture: wgpu::Texture,
    _depth_export_texture: wgpu::Texture,
    _velocity_texture: wgpu::Texture,
    _base_color_texture: wgpu::Texture,
    _scene_color_texture: wgpu::Texture,
    _shadow_mask_texture: wgpu::Texture,
    _shadow_history_textures: [wgpu::Texture; 2],
    _scene_history_textures: [wgpu::Texture; 2],
    color_format: TextureFormat,
    present_format: TextureFormat,
    depth_readback: DepthExportReadback,
}

impl TaaPass {
    pub fn new(
        device: &Device,
        color_format: TextureFormat,
        present_format: TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        let (ambient_texture, ambient_view) =
            create_texture(device, color_format, width, height, "ambient");
        let (direct_texture, direct_view) =
            create_texture(device, color_format, width, height, "direct");
        let (depth_export_texture, depth_export_view) =
            create_texture(device, DEPTH_EXPORT_FORMAT, width, height, "depth-export");
        let (velocity_texture, velocity_view) =
            create_texture(device, VELOCITY_FORMAT, width, height, "velocity-normal");
        let (base_color_texture, base_color_view) =
            create_texture(device, BASE_COLOR_FORMAT, width, height, "base-color");
        let (scene_color_texture, scene_color_view) =
            create_texture(device, color_format, width, height, "scene-lit");
        let (shadow_mask_texture, shadow_mask_view) =
            create_texture(device, SHADOW_MASK_FORMAT, width, height, "shadow-mask");
        let (h0, v0) = create_texture(device, SHADOW_MASK_FORMAT, width, height, "shadow-history-0");
        let (h1, v1) = create_texture(device, SHADOW_MASK_FORMAT, width, height, "shadow-history-1");
        let (s0, sv0) = create_texture(device, color_format, width, height, "scene-history-0");
        let (s1, sv1) = create_texture(device, color_format, width, height, "scene-history-1");

        let sampler = linear_sampler(device, "taa-linear");
        let shadow_sampler = non_filtering_sampler(device, "taa-shadow");

        let shadow_resolve_bgl = shadow_resolve_bind_group_layout(device);
        let scene_resolve_bgl = scene_resolve_bind_group_layout(device);
        let lit_composite_bgl = lit_composite_bind_group_layout(device);
        let blit_bgl = blit_bind_group_layout(device);

        let taa_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("taa-uniforms"),
            contents: bytemuck::bytes_of(&TaaUniforms {
                resolution: [width.max(1) as f32, height.max(1) as f32],
                blend: 0.0,
                enabled: 1.0,
                zoom_stability: 1.0,
                _pad: [0.0; 3],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let scene_taa_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("scene-taa-uniforms"),
            contents: bytemuck::bytes_of(&SceneTaaUniforms {
                resolution: [width.max(1) as f32, height.max(1) as f32],
                blend: 0.0,
                enabled: 1.0,
                zoom_stability: 1.0,
                jitter: [0.0; 2],
                disocclusion: DISOCCLUSION_THRESHOLD,
                _pad0: 0.0,
                _pad_align: [0.0; 3],
                inv_view_proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
                prev_view_proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let lit_composite_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("lit-composite-uniforms"),
            contents: bytemuck::bytes_of(&LitCompositeUniforms {
                shadow_darkness: 0.35,
                shadows_enabled: 1.0,
                _pad0: 0.0,
                _pad1: 0.0,
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let shadow_resolve_shader = device.create_shader_module(include_wgsl!("taa.wgsl"));
        let scene_resolve_shader = device.create_shader_module(include_wgsl!("taa_scene.wgsl"));
        let lit_composite_shader = device.create_shader_module(include_wgsl!("lit_composite.wgsl"));
        let blit_shader = device.create_shader_module(include_wgsl!("taa_blit.wgsl"));

        let shadow_resolve_pipeline = build_fullscreen_pipeline(
            device,
            &shadow_resolve_bgl,
            &shadow_resolve_shader,
            "shadow-taa-resolve",
            SHADOW_MASK_FORMAT,
            wgpu::ColorWrites::RED,
        );
        let scene_resolve_pipeline = build_fullscreen_pipeline(
            device,
            &scene_resolve_bgl,
            &scene_resolve_shader,
            "scene-taa-resolve",
            color_format,
            wgpu::ColorWrites::ALL,
        );
        let lit_composite_pipeline = build_fullscreen_pipeline(
            device,
            &lit_composite_bgl,
            &lit_composite_shader,
            "lit-composite",
            color_format,
            wgpu::ColorWrites::ALL,
        );
        let blit_pipeline = build_fullscreen_pipeline(
            device,
            &blit_bgl,
            &blit_shader,
            "scene-blit",
            present_format,
            wgpu::ColorWrites::ALL,
        );

        let shadow_resolve_bind_groups = [
            make_shadow_resolve_bind_group(
                device,
                &shadow_resolve_bgl,
                &taa_uniform_buffer,
                &shadow_mask_view,
                &v0,
                &shadow_sampler,
            ),
            make_shadow_resolve_bind_group(
                device,
                &shadow_resolve_bgl,
                &taa_uniform_buffer,
                &shadow_mask_view,
                &v1,
                &shadow_sampler,
            ),
        ];
        let scene_resolve_bind_groups = [
            make_scene_resolve_bind_group(
                device,
                &scene_resolve_bgl,
                &scene_taa_uniform_buffer,
                &scene_color_view,
                &sv0,
                &depth_export_view,
                &velocity_view,
                &sampler,
                &shadow_sampler,
            ),
            make_scene_resolve_bind_group(
                device,
                &scene_resolve_bgl,
                &scene_taa_uniform_buffer,
                &scene_color_view,
                &sv1,
                &depth_export_view,
                &velocity_view,
                &sampler,
                &shadow_sampler,
            ),
        ];
        let lit_composite_bind_groups = [
            make_lit_composite_bind_group(
                device,
                &lit_composite_bgl,
                &lit_composite_uniform_buffer,
                &ambient_view,
                &direct_view,
                &v0,
                &sampler,
                &shadow_sampler,
            ),
            make_lit_composite_bind_group(
                device,
                &lit_composite_bgl,
                &lit_composite_uniform_buffer,
                &ambient_view,
                &direct_view,
                &v1,
                &sampler,
                &shadow_sampler,
            ),
            make_lit_composite_bind_group(
                device,
                &lit_composite_bgl,
                &lit_composite_uniform_buffer,
                &ambient_view,
                &direct_view,
                &shadow_mask_view,
                &sampler,
                &shadow_sampler,
            ),
        ];
        let blit_bind_groups = [
            make_blit_bind_group(device, &blit_bgl, &sampler, &scene_color_view),
            make_blit_bind_group(device, &blit_bgl, &sampler, &sv0),
            make_blit_bind_group(device, &blit_bgl, &sampler, &sv1),
        ];

        Self {
            enabled: false,
            scene_taa_enabled: false,
            ambient_view,
            direct_view,
            depth_export_view,
            velocity_view,
            base_color_view,
            scene_color_view,
            shadow_mask_view,
            shadow_history_views: [v0, v1],
            scene_history_views: [sv0, sv1],
            shadow_history_index: 0,
            scene_history_index: 0,
            shadow_resolve_pipeline,
            scene_resolve_pipeline,
            lit_composite_pipeline,
            blit_pipeline,
            shadow_resolve_bgl,
            scene_resolve_bgl,
            lit_composite_bgl,
            blit_bgl,
            shadow_resolve_bind_groups,
            scene_resolve_bind_groups,
            lit_composite_bind_groups,
            blit_bind_groups,
            taa_uniform_buffer,
            scene_taa_uniform_buffer,
            lit_composite_uniform_buffer,
            sampler,
            shadow_sampler,
            frame_index: 0,
            current_jitter: [0.0; 2],
            shadow_first_frame: true,
            scene_first_frame: true,
            _ambient_texture: ambient_texture,
            _direct_texture: direct_texture,
            _depth_export_texture: depth_export_texture,
            _velocity_texture: velocity_texture,
            _base_color_texture: base_color_texture,
            _scene_color_texture: scene_color_texture,
            _shadow_mask_texture: shadow_mask_texture,
            _shadow_history_textures: [h0, h1],
            _scene_history_textures: [s0, s1],
            color_format,
            present_format,
            depth_readback: DepthExportReadback::new(device, width, height),
        }
    }

    fn rebuild_cached_bind_groups(&mut self, device: &Device) {
        self.shadow_resolve_bind_groups = [
            make_shadow_resolve_bind_group(
                device,
                &self.shadow_resolve_bgl,
                &self.taa_uniform_buffer,
                &self.shadow_mask_view,
                &self.shadow_history_views[0],
                &self.shadow_sampler,
            ),
            make_shadow_resolve_bind_group(
                device,
                &self.shadow_resolve_bgl,
                &self.taa_uniform_buffer,
                &self.shadow_mask_view,
                &self.shadow_history_views[1],
                &self.shadow_sampler,
            ),
        ];
        self.scene_resolve_bind_groups = [
            make_scene_resolve_bind_group(
                device,
                &self.scene_resolve_bgl,
                &self.scene_taa_uniform_buffer,
                &self.scene_color_view,
                &self.scene_history_views[0],
                &self.depth_export_view,
                &self.velocity_view,
                &self.sampler,
                &self.shadow_sampler,
            ),
            make_scene_resolve_bind_group(
                device,
                &self.scene_resolve_bgl,
                &self.scene_taa_uniform_buffer,
                &self.scene_color_view,
                &self.scene_history_views[1],
                &self.depth_export_view,
                &self.velocity_view,
                &self.sampler,
                &self.shadow_sampler,
            ),
        ];
        self.lit_composite_bind_groups = [
            make_lit_composite_bind_group(
                device,
                &self.lit_composite_bgl,
                &self.lit_composite_uniform_buffer,
                &self.ambient_view,
                &self.direct_view,
                &self.shadow_history_views[0],
                &self.sampler,
                &self.shadow_sampler,
            ),
            make_lit_composite_bind_group(
                device,
                &self.lit_composite_bgl,
                &self.lit_composite_uniform_buffer,
                &self.ambient_view,
                &self.direct_view,
                &self.shadow_history_views[1],
                &self.sampler,
                &self.shadow_sampler,
            ),
            make_lit_composite_bind_group(
                device,
                &self.lit_composite_bgl,
                &self.lit_composite_uniform_buffer,
                &self.ambient_view,
                &self.direct_view,
                &self.shadow_mask_view,
                &self.sampler,
                &self.shadow_sampler,
            ),
        ];
        self.blit_bind_groups = [
            make_blit_bind_group(device, &self.blit_bgl, &self.sampler, &self.scene_color_view),
            make_blit_bind_group(device, &self.blit_bgl, &self.sampler, &self.scene_history_views[0]),
            make_blit_bind_group(device, &self.blit_bgl, &self.sampler, &self.scene_history_views[1]),
        ];
    }

    pub(crate) fn ambient_view(&self) -> &TextureView {
        &self.ambient_view
    }

    pub(crate) fn direct_view(&self) -> &TextureView {
        &self.direct_view
    }

    pub(crate) fn depth_export_view(&self) -> &TextureView {
        &self.depth_export_view
    }

    /// Infra de readback depth (sin logs activos).
    #[allow(dead_code)]
    pub(crate) fn queue_depth_export_readback(&mut self, encoder: &mut wgpu::CommandEncoder) {
        self.depth_readback.queue_copy(encoder, &self._depth_export_texture);
    }

    #[allow(dead_code)]
    pub(crate) fn log_depth_export_readback(
        &mut self,
        device: &Device,
        near_plane: f32,
        far_plane: f32,
        debug_label: &str,
        view_proj: Option<[[f32; 4]; 4]>,
        inv_view_proj: Option<[[f32; 4]; 4]>,
        probe_world: Option<[f32; 3]>,
    ) -> bool {
        if let Some(stats) = self.depth_readback.finish_and_stats(device) {
            log_depth_export_stats(&stats, near_plane, far_plane, debug_label);
            if let (Some(vp), Some(inv_vp), Some(w)) = (view_proj, inv_view_proj, probe_world) {
                log_depth_probe_cpu(&vp, &inv_vp, w, near_plane, far_plane);
            }
            return true;
        }
        false
    }

    pub(crate) fn velocity_view(&self) -> &TextureView {
        &self.velocity_view
    }

    pub(crate) fn base_color_view(&self) -> &TextureView {
        &self.base_color_view
    }

    pub(crate) fn normal_roughness_view(&self) -> &TextureView {
        &self.velocity_view
    }

    pub(crate) fn scene_color_view(&self) -> &TextureView {
        &self.scene_color_view
    }

    pub(crate) fn scene_color_texture(&self) -> &wgpu::Texture {
        &self._scene_color_texture
    }

    pub(crate) fn shadow_mask_view(&self) -> &TextureView {
        &self.shadow_mask_view
    }

    pub fn resize(
        &mut self,
        device: &Device,
        color_format: TextureFormat,
        present_format: TextureFormat,
        width: u32,
        height: u32,
    ) {
        let (ambient_texture, ambient_view) =
            create_texture(device, color_format, width, height, "ambient");
        let (direct_texture, direct_view) =
            create_texture(device, color_format, width, height, "direct");
        let (depth_export_texture, depth_export_view) =
            create_texture(device, DEPTH_EXPORT_FORMAT, width, height, "depth-export");
        let (velocity_texture, velocity_view) =
            create_texture(device, VELOCITY_FORMAT, width, height, "velocity-normal");
        let (base_color_texture, base_color_view) =
            create_texture(device, BASE_COLOR_FORMAT, width, height, "base-color");
        let (scene_color_texture, scene_color_view) =
            create_texture(device, color_format, width, height, "scene-lit");
        let (shadow_mask_texture, shadow_mask_view) =
            create_texture(device, SHADOW_MASK_FORMAT, width, height, "shadow-mask");
        let (h0, v0) = create_texture(device, SHADOW_MASK_FORMAT, width, height, "shadow-history-0");
        let (h1, v1) = create_texture(device, SHADOW_MASK_FORMAT, width, height, "shadow-history-1");
        let (s0, sv0) = create_texture(device, color_format, width, height, "scene-history-0");
        let (s1, sv1) = create_texture(device, color_format, width, height, "scene-history-1");

        self.color_format = color_format;
        self.present_format = present_format;
        self._ambient_texture = ambient_texture;
        self.ambient_view = ambient_view;
        self._direct_texture = direct_texture;
        self.direct_view = direct_view;
        self._depth_export_texture = depth_export_texture;
        self.depth_export_view = depth_export_view;
        self._velocity_texture = velocity_texture;
        self.velocity_view = velocity_view;
        self._base_color_texture = base_color_texture;
        self.base_color_view = base_color_view;
        self._scene_color_texture = scene_color_texture;
        self.scene_color_view = scene_color_view;
        self._shadow_mask_texture = shadow_mask_texture;
        self.shadow_mask_view = shadow_mask_view;
        self._shadow_history_textures = [h0, h1];
        self.shadow_history_views = [v0, v1];
        self._scene_history_textures = [s0, s1];
        self.scene_history_views = [sv0, sv1];
        self.depth_readback
            .resize(device, width.max(1), height.max(1));
        self.shadow_history_index = 0;
        self.scene_history_index = 0;
        self.shadow_first_frame = true;
        self.scene_first_frame = true;
        self.frame_index = 0;

        self.rebuild_cached_bind_groups(device);
    }

    pub fn begin_frame(&mut self, force_invalidate: bool) {
        if force_invalidate {
            self.shadow_first_frame = true;
            self.scene_first_frame = true;
        }
        self.current_jitter = halton_jitter(self.frame_index);
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        if self.enabled == enabled && self.scene_taa_enabled == enabled {
            return;
        }
        self.enabled = enabled;
        self.scene_taa_enabled = enabled;
        self.shadow_first_frame = true;
        self.scene_first_frame = true;
    }

    pub fn present_scene(
        &mut self,
        device: &Device,
        queue: &Queue,
        encoder: &mut wgpu::CommandEncoder,
        swapchain_view: &TextureView,
        soft_scene_taa: bool,
        zoom_stability: f32,
        width: u32,
        height: u32,
        inv_view_proj: [[f32; 4]; 4],
        prev_view_proj: [[f32; 4]; 4],
    ) {
        self.run_lit_composite(device, queue, encoder, 0.35, false);

        if soft_scene_taa && self.enabled && self.scene_taa_enabled {
            self.resolve_scene_soft(
                device,
                queue,
                encoder,
                zoom_stability,
                width,
                height,
                inv_view_proj,
                prev_view_proj,
            );
            let idx = 1 + self.scene_history_index as usize;
            blit_cached(
                encoder,
                &self.blit_pipeline,
                &self.blit_bind_groups[idx],
                swapchain_view,
            );
        } else {
            blit_cached(
                encoder,
                &self.blit_pipeline,
                &self.blit_bind_groups[0],
                swapchain_view,
            );
        }
        self.frame_index = self.frame_index.wrapping_add(1);
    }

    pub fn resolve_shadow_and_present(
        &mut self,
        device: &Device,
        queue: &Queue,
        encoder: &mut wgpu::CommandEncoder,
        swapchain_view: &TextureView,
        shadow_darkness: f32,
        shadows_enabled: bool,
        zoom_stability: f32,
        width: u32,
        height: u32,
        inv_view_proj: [[f32; 4]; 4],
        prev_view_proj: [[f32; 4]; 4],
    ) {
        if !shadows_enabled {
            self.present_scene(
                device,
                queue,
                encoder,
                swapchain_view,
                true,
                zoom_stability,
                width,
                height,
                inv_view_proj,
                prev_view_proj,
            );
            return;
        }

        if self.enabled {
            self.resolve_shadow_mask(device, queue, encoder, zoom_stability, width, height);
        }
        self.run_lit_composite(device, queue, encoder, shadow_darkness, true);

        let use_scene_taa = self.enabled && self.scene_taa_enabled;
        if use_scene_taa {
            self.resolve_scene_soft(
                device,
                queue,
                encoder,
                zoom_stability,
                width,
                height,
                inv_view_proj,
                prev_view_proj,
            );
        }

        let blit_idx = if use_scene_taa {
            1 + self.scene_history_index as usize
        } else {
            0
        };
        blit_cached(
            encoder,
            &self.blit_pipeline,
            &self.blit_bind_groups[blit_idx],
            swapchain_view,
        );
        self.frame_index = self.frame_index.wrapping_add(1);
    }

    pub(crate) fn run_lit_composite_pub(
        &self,
        device: &Device,
        queue: &Queue,
        encoder: &mut wgpu::CommandEncoder,
        shadow_darkness: f32,
        shadows_enabled: bool,
    ) {
        self.run_lit_composite(device, queue, encoder, shadow_darkness, shadows_enabled);
    }

    pub(crate) fn resolve_shadow_mask_pub(
        &mut self,
        device: &Device,
        queue: &Queue,
        encoder: &mut wgpu::CommandEncoder,
        zoom_stability: f32,
        width: u32,
        height: u32,
    ) {
        self.resolve_shadow_mask(device, queue, encoder, zoom_stability, width, height);
    }

    pub(crate) fn resolve_scene_soft_pub(
        &mut self,
        device: &Device,
        queue: &Queue,
        encoder: &mut wgpu::CommandEncoder,
        zoom_stability: f32,
        width: u32,
        height: u32,
        inv_view_proj: [[f32; 4]; 4],
        prev_view_proj: [[f32; 4]; 4],
    ) {
        self.resolve_scene_soft(
            device,
            queue,
            encoder,
            zoom_stability,
            width,
            height,
            inv_view_proj,
            prev_view_proj,
        );
    }

    pub(crate) fn blit_present_pub(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        swapchain_view: &TextureView,
        use_scene_taa: bool,
    ) {
        let blit_idx = if use_scene_taa {
            1 + self.scene_history_index as usize
        } else {
            0
        };
        blit_cached(
            encoder,
            &self.blit_pipeline,
            &self.blit_bind_groups[blit_idx],
            swapchain_view,
        );
    }

    pub(crate) fn scene_taa_active(&self) -> bool {
        self.enabled && self.scene_taa_enabled
    }

    pub(crate) fn tick_frame_index(&mut self) {
        self.frame_index = self.frame_index.wrapping_add(1);
    }

    fn run_lit_composite(
        &self,
        _device: &Device,
        queue: &Queue,
        encoder: &mut wgpu::CommandEncoder,
        shadow_darkness: f32,
        shadows_enabled: bool,
    ) {
        let bind_group = if shadows_enabled && self.enabled {
            &self.lit_composite_bind_groups[self.shadow_history_index as usize]
        } else {
            // Sin TAA de sombra: máscara cruda del MRT (índice 2).
            &self.lit_composite_bind_groups[2]
        };

        queue.write_buffer(
            &self.lit_composite_uniform_buffer,
            0,
            bytemuck::bytes_of(&LitCompositeUniforms {
                shadow_darkness,
                shadows_enabled: if shadows_enabled { 1.0 } else { 0.0 },
                _pad0: 0.0,
                _pad1: 0.0,
            }),
        );

        draw_fullscreen(
            encoder,
            &self.lit_composite_pipeline,
            bind_group,
            &self.scene_color_view,
            wgpu::LoadOp::Clear(wgpu::Color::BLACK),
        );
    }

    fn resolve_scene_soft(
        &mut self,
        _device: &Device,
        queue: &Queue,
        encoder: &mut wgpu::CommandEncoder,
        zoom_stability: f32,
        width: u32,
        height: u32,
        inv_view_proj: [[f32; 4]; 4],
        prev_view_proj: [[f32; 4]; 4],
    ) {
        let read_idx = self.scene_history_index as usize;
        let write_idx = 1 - read_idx;
        let history_write = &self.scene_history_views[write_idx];

        let blend = if self.scene_first_frame {
            0.0
        } else {
            SCENE_HISTORY_BLEND
        };
        queue.write_buffer(
            &self.scene_taa_uniform_buffer,
            0,
            bytemuck::bytes_of(&SceneTaaUniforms {
                resolution: [width.max(1) as f32, height.max(1) as f32],
                blend,
                enabled: 1.0,
                zoom_stability: zoom_stability.clamp(0.0, 1.0),
                jitter: self.current_jitter,
                disocclusion: DISOCCLUSION_THRESHOLD,
                _pad0: 0.0,
                _pad_align: [0.0; 3],
                inv_view_proj,
                prev_view_proj,
            }),
        );

        draw_fullscreen(
            encoder,
            &self.scene_resolve_pipeline,
            &self.scene_resolve_bind_groups[read_idx],
            history_write,
            wgpu::LoadOp::Clear(wgpu::Color::BLACK),
        );

        self.scene_history_index = write_idx as u8;
        self.scene_first_frame = false;
    }

    fn resolve_shadow_mask(
        &mut self,
        _device: &Device,
        queue: &Queue,
        encoder: &mut wgpu::CommandEncoder,
        zoom_stability: f32,
        width: u32,
        height: u32,
    ) {
        let read_idx = self.shadow_history_index as usize;
        let write_idx = 1 - read_idx;
        let history_write = &self.shadow_history_views[write_idx];

        let blend = if self.shadow_first_frame {
            0.0
        } else {
            SHADOW_HISTORY_BLEND
        };
        queue.write_buffer(
            &self.taa_uniform_buffer,
            0,
            bytemuck::bytes_of(&TaaUniforms {
                resolution: [width.max(1) as f32, height.max(1) as f32],
                blend,
                enabled: 1.0,
                zoom_stability: zoom_stability.clamp(0.0, 1.0),
                _pad: [0.0; 3],
            }),
        );

        draw_fullscreen(
            encoder,
            &self.shadow_resolve_pipeline,
            &self.shadow_resolve_bind_groups[read_idx],
            history_write,
            wgpu::LoadOp::Clear(wgpu::Color::BLACK),
        );

        self.shadow_history_index = write_idx as u8;
        self.shadow_first_frame = false;
    }
}

fn blit_cached(
    encoder: &mut wgpu::CommandEncoder,
    pipeline: &wgpu::RenderPipeline,
    bind_group: &wgpu::BindGroup,
    output: &TextureView,
) {
    draw_fullscreen(
        encoder,
        pipeline,
        bind_group,
        output,
        wgpu::LoadOp::Clear(wgpu::Color::BLACK),
    );
}

fn draw_fullscreen(
    encoder: &mut wgpu::CommandEncoder,
    pipeline: &wgpu::RenderPipeline,
    bind_group: &wgpu::BindGroup,
    output_view: &TextureView,
    load: wgpu::LoadOp<wgpu::Color>,
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("fullscreen-pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: output_view,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        occlusion_query_set: None,
        timestamp_writes: None,
        multiview_mask: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bind_group, &[]);
    pass.draw(0..3, 0..1);
}

fn make_lit_composite_bind_group(
    device: &Device,
    layout: &wgpu::BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
    ambient: &TextureView,
    direct: &TextureView,
    shadow: &TextureView,
    sampler: &wgpu::Sampler,
    shadow_sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("lit-composite-bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(ambient),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(direct),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(shadow),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::Sampler(shadow_sampler),
            },
        ],
    })
}

fn make_blit_bind_group(
    device: &Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    source: &TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("scene-blit-bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(source),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

fn make_shadow_resolve_bind_group(
    device: &Device,
    layout: &wgpu::BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
    curr_shadow: &TextureView,
    history_shadow: &TextureView,
    shadow_sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("shadow-taa-bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(curr_shadow),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(shadow_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(history_shadow),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(shadow_sampler),
            },
        ],
    })
}

fn make_scene_resolve_bind_group(
    device: &Device,
    layout: &wgpu::BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
    curr_scene: &TextureView,
    history_scene: &TextureView,
    depth_export: &TextureView,
    velocity: &TextureView,
    sampler: &wgpu::Sampler,
    depth_sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("scene-taa-bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(curr_scene),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(history_scene),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(depth_export),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::Sampler(depth_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::TextureView(velocity),
            },
            wgpu::BindGroupEntry {
                binding: 8,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

fn shadow_resolve_bind_group_layout(device: &Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("shadow-taa-bgl"),
        entries: &[
            uniform_bgl_entry(0),
            unfilterable_float_tex_bgl_entry(1),
            non_filtering_sampler_bgl_entry(2),
            unfilterable_float_tex_bgl_entry(3),
            non_filtering_sampler_bgl_entry(4),
        ],
    })
}

fn scene_resolve_bind_group_layout(device: &Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("scene-taa-bgl"),
        entries: &[
            uniform_bgl_entry(0),
            color_tex_bgl_entry(1),
            sampler_bgl_entry(2),
            color_tex_bgl_entry(3),
            sampler_bgl_entry(4),
            unfilterable_float_tex_bgl_entry(5),
            non_filtering_sampler_bgl_entry(6),
            color_tex_bgl_entry(7),
            sampler_bgl_entry(8),
        ],
    })
}

fn lit_composite_bind_group_layout(device: &Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("lit-composite-bgl"),
        entries: &[
            uniform_bgl_entry(0),
            color_tex_bgl_entry(1),
            color_tex_bgl_entry(2),
            unfilterable_float_tex_bgl_entry(3),
            sampler_bgl_entry(4),
            non_filtering_sampler_bgl_entry(5),
        ],
    })
}

fn blit_bind_group_layout(device: &Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("scene-blit-bgl"),
        entries: &[color_tex_bgl_entry(0), sampler_bgl_entry(1)],
    })
}

fn build_fullscreen_pipeline(
    device: &Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    shader: &wgpu::ShaderModule,
    label: &str,
    color_format: TextureFormat,
    write_mask: wgpu::ColorWrites,
) -> wgpu::RenderPipeline {
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("fullscreen-pl"),
        bind_group_layouts: &[Some(bind_group_layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: color_format,
                blend: None,
                write_mask,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn create_texture(
    device: &Device,
    format: TextureFormat,
    width: u32,
    height: u32,
    label: &str,
) -> (wgpu::Texture, TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | if label == "scene-lit" {
                wgpu::TextureUsages::COPY_DST
            } else if label == "depth-export" {
                wgpu::TextureUsages::COPY_SRC
            } else {
                wgpu::TextureUsages::empty()
            },
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn linear_sampler(device: &Device, label: &str) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some(label),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    })
}

fn non_filtering_sampler(device: &Device, label: &str) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some(label),
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    })
}

fn uniform_bgl_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn color_tex_bgl_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn sampler_bgl_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

fn unfilterable_float_tex_bgl_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn non_filtering_sampler_bgl_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
        count: None,
    }
}
