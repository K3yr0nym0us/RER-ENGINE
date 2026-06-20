//! Reflection probes dinámicos (estilo "reflection capture" de Unreal, adaptado a wgpu).
//!
//! Cada esfera/probe captura el entorno alrededor de su centro en un **cubemap** (6 caras)
//! que incluye suelo, esferas vecinas y el **jugador**. El shader principal samplea ese
//! cubemap como reflexión de entorno del metal: cubre TODA la circunferencia de la esfera
//! (no solo el disco que el SSR alcanza) y refleja al player aunque no esté en la vista
//! directa de la cámara.
//!
//! Notas de diseño:
//! - Todas las caras de todos los probes viven en un único `texture_cube_array` (capas =
//!   `6 * MAX_PROBES`). El shader las samplea con `textureSampleLevel(cube, dir, layer)`.
//! - La captura reutiliza los shaders de escena: estáticos vía `vs_main` + `fs_overlay`
//!   (color lit en un solo target) y skinned vía `vs_main_skinned` + `fs_overlay_skinned`.
//! - Sin blur por mip (v1): la rugosidad se aproxima mezclando hacia el promedio del
//!   entorno en el shader principal (igual que con el entorno procedural).

use glam::{Mat4, Vec3};
use wgpu::{Device, Queue, TextureFormat, TextureView};

/// Metadatos por probe: centro xyz + radio w (solo selección de capa cubemap; sin parallax).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ProbeMetaUniform {
    pub entries: [[f32; 4]; MAX_PROBES],
}

/// Resolución por cara del cubemap por defecto (px). El tamaño real lo decide el tier de
/// reflejos vía `ReflectionTier::cubemap_face_size` (Low 128 … Ultra 1024) y se pasa a `new`.
pub(crate) const PROBE_FACE_SIZE: u32 = 128;
/// Máximo de probes capturables simultáneamente (capas = 6 * MAX_PROBES).
pub(crate) const MAX_PROBES: usize = 8;
const FACES: usize = 6;
/// Niveles de mip del cubemap (128,64,32,16,8). Imprescindibles: sin mips, muestrear el
/// cubemap en una esfera espejo aliasa el damero (moiré). El shader usa auto-LOD + bias.
/// El bias máximo por rugosidad en el shader (`rough * 4.0`) coincide con `MIP_LEVELS - 1`.
const MIP_LEVELS: u32 = 5;

pub(crate) struct ProbeEnvPass {
    cube: wgpu::Texture,
    /// Vistas D2 de cada (probe, cara), mip 0, como render target. Índice = probe*6 + cara.
    face_views: Vec<TextureView>,
    capture_depth_view: TextureView,
    /// Uniformes de escena por cara (6): la captura procesa un probe por frame (round-robin).
    face_uniform_buffers: [wgpu::Buffer; FACES],
    face_scene_bind_groups: [wgpu::BindGroup; FACES],
    /// Pipeline de captura para geometría estática (`vs_main` + `fs_overlay`).
    capture_pipeline: wgpu::RenderPipeline,
    /// Pipeline de captura para geometría skinned/jugador (`vs_main_skinned` + `fs_overlay_skinned`).
    capture_skinned_pipeline: wgpu::RenderPipeline,
    /// Pipeline de downsample para generar los mips del cubemap (anti-moiré + blur por rugosidad).
    mip_pipeline: wgpu::RenderPipeline,
    mip_bgl: wgpu::BindGroupLayout,
    mip_sampler: wgpu::Sampler,
    /// Bind group que expone el cube array al shader principal (grupo 2).
    sample_bind_group: wgpu::BindGroup,
    probe_meta_buffer: wgpu::Buffer,
}

impl ProbeEnvPass {
    /// Layout del grupo 2 del shader principal: cube array + sampler.
    pub(crate) fn sample_bind_group_layout(device: &Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("probe-env-sample-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::CubeArray,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        device: &Device,
        color_format: TextureFormat,
        depth_format: TextureFormat,
        scene_uniform_size: u64,
        scene_bgl: &wgpu::BindGroupLayout,
        texture_bgl: &wgpu::BindGroupLayout,
        joint_bgl: &wgpu::BindGroupLayout,
        sample_bgl: &wgpu::BindGroupLayout,
        shadow_view: &TextureView,
        shader: &wgpu::ShaderModule,
        skinned_shader: &wgpu::ShaderModule,
        face_size: u32,
    ) -> Self {
        let face_size = face_size.max(8);
        let layers = (FACES * MAX_PROBES) as u32;
        let cube = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("probe-env-cube"),
            size: wgpu::Extent3d {
                width: face_size,
                height: face_size,
                depth_or_array_layers: layers,
            },
            mip_level_count: MIP_LEVELS,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: color_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let cube_array_view = cube.create_view(&wgpu::TextureViewDescriptor {
            label: Some("probe-env-cube-array-view"),
            dimension: Some(wgpu::TextureViewDimension::CubeArray),
            ..Default::default()
        });

        let mut face_views = Vec::with_capacity(layers as usize);
        for slot in 0..layers {
            face_views.push(cube.create_view(&wgpu::TextureViewDescriptor {
                label: Some("probe-env-face-view"),
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: slot,
                array_layer_count: Some(1),
                base_mip_level: 0,
                mip_level_count: Some(1),
                ..Default::default()
            }));
        }

        let depth_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("probe-env-depth"),
            size: wgpu::Extent3d {
                width: face_size,
                height: face_size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: depth_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let capture_depth_view = depth_tex.create_view(&wgpu::TextureViewDescriptor::default());

        // Sampler de comparación para reusar el layout de escena (binding 2 = shadow sampler).
        let shadow_cmp_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("probe-env-shadow-cmp"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });

        let face_uniform_buffers: [wgpu::Buffer; FACES] = std::array::from_fn(|_f| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("probe-env-face-uniforms"),
                size: scene_uniform_size,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        let face_scene_bind_groups: [wgpu::BindGroup; FACES] = std::array::from_fn(|f| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("probe-env-face-scene-bg"),
                layout: scene_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: face_uniform_buffers[f].as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(shadow_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&shadow_cmp_sampler),
                    },
                ],
            })
        });

        let capture_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("probe-env-capture-layout"),
            bind_group_layouts: &[scene_bgl, texture_bgl],
            push_constant_ranges: &[],
        });
        let capture_skinned_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("probe-env-capture-skinned-layout"),
                bind_group_layouts: &[scene_bgl, texture_bgl, sample_bgl, joint_bgl],
                push_constant_ranges: &[],
            });

        let color_target = [Some(wgpu::ColorTargetState {
            format: color_format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })];
        let depth_state = wgpu::DepthStencilState {
            format: depth_format,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        };

        let capture_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("probe-env-capture-pipeline"),
            layout: Some(&capture_layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: "vs_main",
                buffers: &[
                    crate::mesh::Vertex::desc(),
                    crate::mesh::InstanceData::desc(),
                ],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: "fs_overlay",
                targets: &color_target,
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(depth_state.clone()),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let capture_skinned_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("probe-env-capture-skinned-pipeline"),
                layout: Some(&capture_skinned_layout),
                vertex: wgpu::VertexState {
                    module: skinned_shader,
                    entry_point: "vs_main_skinned",
                    buffers: &[
                        crate::mesh::SkinnedVertex::desc(),
                        crate::mesh::SkinnedInstanceData::desc(),
                    ],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: skinned_shader,
                    entry_point: "fs_overlay_skinned",
                    targets: &color_target,
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    cull_mode: Some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: Some(depth_state),
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

        // Pipeline de generación de mips (downsample box vía bilinear).
        let mip_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("probe-mip-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let mip_shader = device.create_shader_module(wgpu::include_wgsl!("probe_mip.wgsl"));
        let mip_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("probe-mip-layout"),
            bind_group_layouts: &[&mip_bgl],
            push_constant_ranges: &[],
        });
        let mip_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("probe-mip-pipeline"),
            layout: Some(&mip_layout),
            vertex: wgpu::VertexState {
                module: &mip_shader,
                entry_point: "vs_mip",
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &mip_shader,
                entry_point: "fs_mip",
                targets: &color_target,
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let mip_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("probe-mip-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let sample_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("probe-env-sample-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let probe_meta_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("probe-meta-uniform"),
            size: std::mem::size_of::<ProbeMetaUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sample_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("probe-env-sample-bg"),
            layout: sample_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&cube_array_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sample_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: probe_meta_buffer.as_entire_binding(),
                },
            ],
        });

        Self {
            cube,
            face_views,
            capture_depth_view,
            face_uniform_buffers,
            face_scene_bind_groups,
            capture_pipeline,
            capture_skinned_pipeline,
            mip_pipeline,
            mip_bgl,
            mip_sampler,
            sample_bind_group,
            probe_meta_buffer,
        }
    }

    pub(crate) fn write_probe_meta(&self, queue: &Queue, meta: &ProbeMetaUniform) {
        queue.write_buffer(&self.probe_meta_buffer, 0, bytemuck::bytes_of(meta));
    }

    /// Genera los mips del cubemap del probe recién capturado (downsample por cara).
    /// Sin esto, el shader (auto-LOD) solo tendría mip 0 y el reflejo aliasa (moiré).
    pub(crate) fn generate_mips(
        &self,
        device: &Device,
        enc: &mut wgpu::CommandEncoder,
        probe_idx: usize,
    ) {
        for face in 0..FACES {
            let slot = (probe_idx * FACES + face) as u32;
            for mip in 1..MIP_LEVELS {
                let src_view = self.cube.create_view(&wgpu::TextureViewDescriptor {
                    label: Some("probe-mip-src"),
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_array_layer: slot,
                    array_layer_count: Some(1),
                    base_mip_level: mip - 1,
                    mip_level_count: Some(1),
                    ..Default::default()
                });
                let dst_view = self.cube.create_view(&wgpu::TextureViewDescriptor {
                    label: Some("probe-mip-dst"),
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_array_layer: slot,
                    array_layer_count: Some(1),
                    base_mip_level: mip,
                    mip_level_count: Some(1),
                    ..Default::default()
                });
                let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("probe-mip-bg"),
                    layout: &self.mip_bgl,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&src_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.mip_sampler),
                        },
                    ],
                });
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("probe-mip-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &dst_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.mip_pipeline);
                pass.set_bind_group(0, &bg, &[]);
                pass.draw(0..3, 0..1);
            }
        }
    }

    pub(crate) fn sample_bind_group(&self) -> &wgpu::BindGroup {
        &self.sample_bind_group
    }

    pub(crate) fn capture_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.capture_pipeline
    }

    pub(crate) fn capture_skinned_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.capture_skinned_pipeline
    }

    pub(crate) fn capture_depth_view(&self) -> &TextureView {
        &self.capture_depth_view
    }

    pub(crate) fn face_view(&self, probe_idx: usize, face: usize) -> &TextureView {
        &self.face_views[probe_idx * FACES + face]
    }

    pub(crate) fn face_scene_bind_group(&self, face: usize) -> &wgpu::BindGroup {
        &self.face_scene_bind_groups[face]
    }

    pub(crate) fn write_face_uniforms(&self, queue: &wgpu::Queue, face: usize, bytes: &[u8]) {
        queue.write_buffer(&self.face_uniform_buffers[face], 0, bytes);
    }
}

/// Las 6 `view_proj` del cubemap (orden wgpu: +X,-X,+Y,-Y,+Z,-Z) desde `center`.
/// FOV 90°, aspecto 1, profundidad [0,1] (perspective_rh, convención wgpu/Vulkan).
pub(crate) fn cube_face_view_projs(center: Vec3, near: f32, far: f32) -> [[[f32; 4]; 4]; FACES] {
    let proj = Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, near, far);
    // (dir, up) según convención Khronos/WGSL para cubemap (+Y up = -Z, -Y up = +Z).
    let faces: [(Vec3, Vec3); FACES] = [
        (Vec3::X, Vec3::NEG_Y),
        (Vec3::NEG_X, Vec3::NEG_Y),
        (Vec3::Y, Vec3::NEG_Z),
        (Vec3::NEG_Y, Vec3::Z),
        (Vec3::Z, Vec3::NEG_Y),
        (Vec3::NEG_Z, Vec3::NEG_Y),
    ];
    let mut out = [[[0.0f32; 4]; 4]; FACES];
    for (i, (dir, up)) in faces.iter().enumerate() {
        let view = Mat4::look_to_rh(center, *dir, *up);
        out[i] = (proj * view).to_cols_array_2d();
    }
    out
}
