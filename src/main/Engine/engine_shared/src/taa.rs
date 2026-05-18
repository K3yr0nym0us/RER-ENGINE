//! TAA en máscara de sombras (3D) + TAA suave opcional en color de escena.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;
use wgpu::{include_wgsl, Device, Queue, TextureFormat, TextureView};

const SHADOW_HISTORY_BLEND: f32 = 27.0 / 32.0;
/// Mezcla temporal suave en geometría (bordes de objetos).
const SCENE_HISTORY_BLEND: f32 = 0.38;

/// Máscara de sombra por píxel (1 = iluminado, 0 = sombra).
pub const SHADOW_MASK_FORMAT: TextureFormat = TextureFormat::R32Float;
/// Alias histórico usado por los pipelines MRT.
pub const DEPTH_EXPORT_FORMAT: TextureFormat = SHADOW_MASK_FORMAT;

/// Estabilidad TAA en sombras: alta cerca (suaviza), baja lejos (menos prioridad).
pub fn zoom_stability_distance(distance: f32) -> f32 {
    const NEAR: f32 = 2.5;
    const FAR: f32 = 32.0;
    let t = ((distance - NEAR) / (FAR - NEAR)).clamp(0.0, 1.0);
    1.0 - t * 0.88
}

/// Estabilidad temporal según zoom 2D ortográfico (`half_h` mayor = más lejos).
pub fn zoom_stability_half_h(half_h: f32) -> f32 {
    const REF: f32 = 3.5;
    const MIN: f32 = 0.08;
    (REF / half_h.max(0.1)).clamp(MIN, 1.0)
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
struct CompositeUniforms {
    shadow_darkness: f32,
    shadows_enabled: f32,
    _pad0: f32,
    _pad1: f32,
}

/// Recursos GPU: escena + TAA suave en color + TAA en sombras + composite.
pub struct TaaPass {
    pub enabled: bool,
    /// TAA suave en bordes de objetos (3D).
    pub scene_taa_enabled: bool,
    scene_color_view: TextureView,
    shadow_mask_view: TextureView,
    shadow_history_views: [TextureView; 2],
    scene_history_views: [TextureView; 2],
    shadow_history_index: u8,
    scene_history_index: u8,
    shadow_resolve_pipeline: wgpu::RenderPipeline,
    scene_resolve_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,
    blit_pipeline: wgpu::RenderPipeline,
    shadow_resolve_bgl: wgpu::BindGroupLayout,
    scene_resolve_bgl: wgpu::BindGroupLayout,
    composite_bgl: wgpu::BindGroupLayout,
    blit_bgl: wgpu::BindGroupLayout,
    shadow_resolve_bind_group: wgpu::BindGroup,
    scene_resolve_bind_group: wgpu::BindGroup,
    taa_uniform_buffer: wgpu::Buffer,
    composite_uniform_buffer: wgpu::Buffer,
    sampler: wgpu::Sampler,
    shadow_sampler: wgpu::Sampler,
    frame_index: u32,
    shadow_first_frame: bool,
    scene_first_frame: bool,
    _scene_color_texture: wgpu::Texture,
    _shadow_mask_texture: wgpu::Texture,
    _shadow_history_textures: [wgpu::Texture; 2],
    _scene_history_textures: [wgpu::Texture; 2],
    color_format: TextureFormat,
}

impl TaaPass {
    pub fn new(
        device: &Device,
        color_format: TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        let (scene_color_texture, scene_color_view) =
            create_texture(device, color_format, width, height, "scene-color");
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
        let composite_bgl = composite_bind_group_layout(device);
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

        let composite_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("taa-composite-uniforms"),
            contents: bytemuck::bytes_of(&CompositeUniforms {
                shadow_darkness: 0.35,
                shadows_enabled: 1.0,
                _pad0: 0.0,
                _pad1: 0.0,
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let shadow_resolve_bind_group = make_shadow_resolve_bind_group(
            device,
            &shadow_resolve_bgl,
            &taa_uniform_buffer,
            &shadow_mask_view,
            &v0,
            &shadow_sampler,
        );
        let scene_resolve_bind_group = make_scene_resolve_bind_group(
            device,
            &scene_resolve_bgl,
            &taa_uniform_buffer,
            &scene_color_view,
            &sv0,
            &sampler,
        );

        let shadow_resolve_shader = device.create_shader_module(include_wgsl!("taa.wgsl"));
        let scene_resolve_shader = device.create_shader_module(include_wgsl!("taa_scene.wgsl"));
        let composite_shader = device.create_shader_module(include_wgsl!("taa_composite.wgsl"));
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
        let composite_pipeline = build_fullscreen_pipeline(
            device,
            &composite_bgl,
            &composite_shader,
            "shadow-composite",
            color_format,
            wgpu::ColorWrites::ALL,
        );
        let blit_pipeline = build_fullscreen_pipeline(
            device,
            &blit_bgl,
            &blit_shader,
            "scene-blit",
            color_format,
            wgpu::ColorWrites::ALL,
        );

        Self {
            enabled: true,
            scene_taa_enabled: true,
            scene_color_view,
            shadow_mask_view,
            shadow_history_views: [v0, v1],
            scene_history_views: [sv0, sv1],
            shadow_history_index: 0,
            scene_history_index: 0,
            shadow_resolve_pipeline,
            scene_resolve_pipeline,
            composite_pipeline,
            blit_pipeline,
            shadow_resolve_bgl,
            scene_resolve_bgl,
            composite_bgl,
            blit_bgl,
            shadow_resolve_bind_group,
            scene_resolve_bind_group,
            taa_uniform_buffer,
            composite_uniform_buffer,
            sampler,
            shadow_sampler,
            frame_index: 0,
            shadow_first_frame: true,
            scene_first_frame: true,
            _scene_color_texture: scene_color_texture,
            _shadow_mask_texture: shadow_mask_texture,
            _shadow_history_textures: [h0, h1],
            _scene_history_textures: [s0, s1],
            color_format,
        }
    }

    pub fn scene_color_view(&self) -> &TextureView {
        &self.scene_color_view
    }

    pub fn shadow_mask_view(&self) -> &TextureView {
        &self.shadow_mask_view
    }

    /// Alias de [`Self::shadow_mask_view`] (MRT legado).
    pub fn depth_export_view(&self) -> &TextureView {
        &self.shadow_mask_view
    }

    pub fn resize(
        &mut self,
        device: &Device,
        color_format: TextureFormat,
        width: u32,
        height: u32,
    ) {
        let (scene_color_texture, scene_color_view) =
            create_texture(device, color_format, width, height, "scene-color");
        let (shadow_mask_texture, shadow_mask_view) =
            create_texture(device, SHADOW_MASK_FORMAT, width, height, "shadow-mask");
        let (h0, v0) = create_texture(device, SHADOW_MASK_FORMAT, width, height, "shadow-history-0");
        let (h1, v1) = create_texture(device, SHADOW_MASK_FORMAT, width, height, "shadow-history-1");
        let (s0, sv0) = create_texture(device, color_format, width, height, "scene-history-0");
        let (s1, sv1) = create_texture(device, color_format, width, height, "scene-history-1");

        self.color_format = color_format;
        self._scene_color_texture = scene_color_texture;
        self.scene_color_view = scene_color_view;
        self._shadow_mask_texture = shadow_mask_texture;
        self.shadow_mask_view = shadow_mask_view;
        self._shadow_history_textures = [h0, h1];
        self.shadow_history_views = [v0, v1];
        self._scene_history_textures = [s0, s1];
        self.scene_history_views = [sv0, sv1];
        self.shadow_history_index = 0;
        self.scene_history_index = 0;
        self.shadow_first_frame = true;
        self.scene_first_frame = true;
        self.frame_index = 0;

        self.shadow_resolve_bind_group = make_shadow_resolve_bind_group(
            device,
            &self.shadow_resolve_bgl,
            &self.taa_uniform_buffer,
            &self.shadow_mask_view,
            &self.shadow_history_views[0],
            &self.shadow_sampler,
        );
        self.scene_resolve_bind_group = make_scene_resolve_bind_group(
            device,
            &self.scene_resolve_bgl,
            &self.taa_uniform_buffer,
            &self.scene_color_view,
            &self.scene_history_views[0],
            &self.sampler,
        );
    }

    pub fn begin_frame(&mut self, force_invalidate: bool) {
        if force_invalidate {
            self.shadow_first_frame = true;
            self.scene_first_frame = true;
        }
    }

    /// Presenta la escena (2D directo; 3D con TAA suave opcional en geometría).
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
    ) {
        if soft_scene_taa && self.enabled && self.scene_taa_enabled {
            self.resolve_scene_soft(device, queue, encoder, zoom_stability, width, height);
            let idx = self.scene_history_index as usize;
            blit(
                encoder,
                device,
                &self.blit_pipeline,
                &self.blit_bgl,
                &self.sampler,
                &self.scene_history_views[idx],
                swapchain_view,
            );
            self.frame_index = self.frame_index.wrapping_add(1);
        } else {
            blit(
                encoder,
                device,
                &self.blit_pipeline,
                &self.blit_bgl,
                &self.sampler,
                &self.scene_color_view,
                swapchain_view,
            );
        }
    }

    /// TAA suave en escena + TAA en sombras + composite (3D con sombras).
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
    ) {
        if !shadows_enabled || !self.enabled {
            self.present_scene(
                device,
                queue,
                encoder,
                swapchain_view,
                true,
                zoom_stability,
                width,
                height,
            );
            return;
        }

        let use_scene_taa = self.scene_taa_enabled;
        if use_scene_taa {
            self.resolve_scene_soft(device, queue, encoder, zoom_stability, width, height);
        }
        self.resolve_shadow_mask(device, queue, encoder, zoom_stability, width, height);

        let scene_idx = self.scene_history_index as usize;
        let shadow_idx = self.shadow_history_index as usize;
        let scene_view = if use_scene_taa {
            &self.scene_history_views[scene_idx]
        } else {
            &self.scene_color_view
        };
        let shadow_view = &self.shadow_history_views[shadow_idx];

        queue.write_buffer(
            &self.composite_uniform_buffer,
            0,
            bytemuck::bytes_of(&CompositeUniforms {
                shadow_darkness,
                shadows_enabled: 1.0,
                _pad0: 0.0,
                _pad1: 0.0,
            }),
        );

        let composite_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow-composite-bg"),
            layout: &self.composite_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.composite_uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(scene_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.shadow_sampler),
                },
            ],
        });

        draw_fullscreen(
            encoder,
            &self.composite_pipeline,
            &composite_bg,
            swapchain_view,
            wgpu::LoadOp::Clear(wgpu::Color::BLACK),
        );

        self.frame_index = self.frame_index.wrapping_add(1);
    }

    fn resolve_scene_soft(
        &mut self,
        device: &Device,
        queue: &Queue,
        encoder: &mut wgpu::CommandEncoder,
        zoom_stability: f32,
        width: u32,
        height: u32,
    ) -> &TextureView {
        let read_idx = self.scene_history_index as usize;
        let write_idx = 1 - read_idx;
        let history_write = &self.scene_history_views[write_idx];

        self.scene_resolve_bind_group = make_scene_resolve_bind_group(
            device,
            &self.scene_resolve_bgl,
            &self.taa_uniform_buffer,
            &self.scene_color_view,
            &self.scene_history_views[read_idx],
            &self.sampler,
        );

        let blend = if self.scene_first_frame {
            0.0
        } else {
            SCENE_HISTORY_BLEND
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
            &self.scene_resolve_pipeline,
            &self.scene_resolve_bind_group,
            history_write,
            wgpu::LoadOp::Clear(wgpu::Color::BLACK),
        );

        self.scene_history_index = write_idx as u8;
        self.scene_first_frame = false;
        history_write
    }

    fn resolve_shadow_mask(
        &mut self,
        device: &Device,
        queue: &Queue,
        encoder: &mut wgpu::CommandEncoder,
        zoom_stability: f32,
        width: u32,
        height: u32,
    ) -> &TextureView {
        let read_idx = self.shadow_history_index as usize;
        let write_idx = 1 - read_idx;
        let history_write = &self.shadow_history_views[write_idx];

        self.shadow_resolve_bind_group = make_shadow_resolve_bind_group(
            device,
            &self.shadow_resolve_bgl,
            &self.taa_uniform_buffer,
            &self.shadow_mask_view,
            &self.shadow_history_views[read_idx],
            &self.shadow_sampler,
        );

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
            &self.shadow_resolve_bind_group,
            history_write,
            wgpu::LoadOp::Clear(wgpu::Color::BLACK),
        );

        self.shadow_history_index = write_idx as u8;
        self.shadow_first_frame = false;
        history_write
    }

    pub fn invalidate_history(&mut self) {
        self.shadow_first_frame = true;
        self.scene_first_frame = true;
        self.frame_index = 0;
    }
}

fn blit(
    encoder: &mut wgpu::CommandEncoder,
    device: &Device,
    pipeline: &wgpu::RenderPipeline,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    source: &TextureView,
    output: &TextureView,
) {
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
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
    });
    draw_fullscreen(
        encoder,
        pipeline,
        &bind_group,
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
            ops: wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        occlusion_query_set: None,
        timestamp_writes: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bind_group, &[]);
    pass.draw(0..3, 0..1);
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
    sampler: &wgpu::Sampler,
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
        ],
    })
}

fn composite_bind_group_layout(device: &Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("shadow-composite-bgl"),
        entries: &[
            uniform_bgl_entry(0),
            color_tex_bgl_entry(1),
            sampler_bgl_entry(2),
            unfilterable_float_tex_bgl_entry(3),
            non_filtering_sampler_bgl_entry(4),
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
        bind_group_layouts: &[bind_group_layout],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: "vs_main",
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: "fs_main",
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
        multiview: None,
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
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
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
