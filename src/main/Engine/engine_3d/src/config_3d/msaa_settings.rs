//! Ajustes MSAA del pase forward 3D: targets multisample + recreate de pipelines.

use crate::config_3d::msaa_graphics::{DEFAULT_MSAA_TIER, MsaaTier, clamp_sample_count};
use crate::engine::{DEPTH_FORMAT, State, create_depth_texture};
use crate::ipc::{EngineEvent, send_event};
use crate::mesh;

pub(crate) fn multisample_state(count: u32) -> wgpu::MultisampleState {
    wgpu::MultisampleState {
        count: count.max(1),
        mask: !0,
        alpha_to_coverage_enabled: false,
    }
}

impl State {
    pub(crate) fn set_msaa_tier(&mut self, tier: MsaaTier) {
        let prev = self.msaa_tier;
        let desired = tier.desired_sample_count();
        let sample_count = clamp_sample_count(&self.device, desired);
        if prev == tier && self.msaa_sample_count == sample_count {
            log::info!(
                "[msaa] Nivel sin cambios: {} ({}x)",
                tier.wire(),
                sample_count
            );
            return;
        }
        if sample_count != desired {
            log::warn!(
                "[msaa] Tier {} pide {}x; dispositivo soporta {}x (clamp)",
                tier.wire(),
                desired,
                sample_count
            );
        }
        self.msaa_tier = tier;
        self.apply_msaa_sample_count(sample_count);
        send_event(&EngineEvent::MsaaTierChanged {
            tier: tier.wire().to_string(),
            sample_count,
        });
        log::info!(
            "[msaa] Nivel: {} -> {} ({}x)",
            prev.wire(),
            tier.wire(),
            sample_count
        );
    }

    pub(crate) fn apply_msaa_sample_count(&mut self, sample_count: u32) {
        let sample_count = sample_count.max(1);
        if self.msaa_sample_count == sample_count && self.taa.sample_count() == sample_count {
            return;
        }
        self.msaa_sample_count = sample_count;
        self.taa.set_sample_count(&self.device, sample_count);
        let (depth_tex, depth_view) =
            create_depth_texture(&self.device, &self.config, sample_count);
        self._depth_texture = depth_tex;
        self.depth_view = depth_view;
        self.recreate_forward_msaa_pipelines();
    }

    /// Recrea pipelines que escriben al MRT / G-buffer forward con el `sample_count` actual.
    pub(crate) fn recreate_forward_msaa_pipelines(&mut self) {
        let device = &self.device;
        let ms = multisample_state(self.msaa_sample_count);
        let mrt_targets = mrt_color_targets();
        let transparent_prepass_targets = transparent_prepass_color_targets();
        let surface_gbuffer_export_targets = surface_gbuffer_color_targets();

        let shader = crate::shader_loader::load_scene_wgsl(
            device,
            "main-shader-msaa",
            include_str!("../shader.wgsl"),
        );
        let skinned_shader = crate::shader_loader::load_scene_wgsl(
            device,
            "skinned-shader-msaa",
            include_str!("../shader_skinned.wgsl"),
        );
        let gizmo_shader = device.create_shader_module(wgpu::include_wgsl!("../gizmo.wgsl"));

        let base_color_depth = wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        };

        self.render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("main-pipeline"),
            layout: Some(&self.forward_main_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[mesh::Vertex::desc(), mesh::InstanceData::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &mrt_targets,
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: ms,
            multiview_mask: None,
            cache: None,
        });

        self.render_pipeline_transparent_prepass =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("transparent-gbuffer-prepass-pipeline"),
                layout: Some(&self.forward_main_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[mesh::Vertex::desc(), mesh::InstanceData::desc()],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_transparent_prepass"),
                    targets: &transparent_prepass_targets,
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    cull_mode: Some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: ms,
                multiview_mask: None,
                cache: None,
            });

        self.base_color_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("base-color-export-pipeline"),
            layout: Some(&self.forward_main_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[mesh::Vertex::desc(), mesh::InstanceData::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_export_base_color"),
                targets: &surface_gbuffer_export_targets,
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(base_color_depth.clone()),
            multisample: ms,
            multiview_mask: None,
            cache: None,
        });

        self.sky_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("world-sky-pipeline"),
            layout: Some(&self.forward_sky_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &gizmo_shader,
                entry_point: Some("vs_main"),
                buffers: &[crate::gizmo::GizmoVertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &gizmo_shader,
                entry_point: Some("fs_sky_mrt"),
                targets: &mrt_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: ms,
            multiview_mask: None,
            cache: None,
        });

        self.skinned_render_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("skinned-main-pipeline"),
                layout: Some(&self.forward_skinned_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &skinned_shader,
                    entry_point: Some("vs_main_skinned"),
                    buffers: &[
                        mesh::SkinnedVertex::desc(),
                        mesh::SkinnedInstanceData::desc(),
                    ],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &skinned_shader,
                    entry_point: Some("fs_main_skinned"),
                    targets: &mrt_targets,
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    cull_mode: Some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: ms,
                multiview_mask: None,
                cache: None,
            });

        self.skinned_base_color_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("skinned-base-color-export-pipeline"),
                layout: Some(&self.forward_skinned_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &skinned_shader,
                    entry_point: Some("vs_main_skinned"),
                    buffers: &[
                        mesh::SkinnedVertex::desc(),
                        mesh::SkinnedInstanceData::desc(),
                    ],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &skinned_shader,
                    entry_point: Some("fs_export_base_color_skinned"),
                    targets: &surface_gbuffer_export_targets,
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    cull_mode: Some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: Some(base_color_depth),
                multisample: ms,
                multiview_mask: None,
                cache: None,
            });
    }
}

fn mrt_color_targets() -> [Option<wgpu::ColorTargetState>; 5] {
    let shadow_mask_target = Some(wgpu::ColorTargetState {
        format: crate::taa::SHADOW_MASK_FORMAT,
        blend: None,
        write_mask: wgpu::ColorWrites::RED | wgpu::ColorWrites::GREEN,
    });
    let depth_export_target = Some(wgpu::ColorTargetState {
        format: crate::taa::DEPTH_EXPORT_FORMAT,
        blend: None,
        write_mask: wgpu::ColorWrites::ALL,
    });
    let velocity_target = Some(wgpu::ColorTargetState {
        format: crate::taa::VELOCITY_FORMAT,
        blend: None,
        write_mask: wgpu::ColorWrites::ALL,
    });
    [
        Some(wgpu::ColorTargetState {
            format: crate::taa::MRT_LIT_FORMAT,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        }),
        shadow_mask_target,
        Some(wgpu::ColorTargetState {
            format: crate::taa::MRT_LIT_FORMAT,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        }),
        depth_export_target,
        velocity_target,
    ]
}

fn transparent_prepass_color_targets() -> [Option<wgpu::ColorTargetState>; 5] {
    let mut t = mrt_color_targets();
    t[0] = None;
    t
}

fn surface_gbuffer_color_targets() -> [Option<wgpu::ColorTargetState>; 2] {
    [
        Some(wgpu::ColorTargetState {
            format: crate::taa::BASE_COLOR_FORMAT,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        }),
        Some(wgpu::ColorTargetState {
            format: crate::taa::WORLD_POS_FORMAT,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        }),
    ]
}

pub(crate) fn apply_msaa_settings_from_world_wire(state: &mut State, tier: Option<&str>) {
    let resolved = tier
        .and_then(MsaaTier::from_wire)
        .unwrap_or(DEFAULT_MSAA_TIER);
    if state.msaa_tier != resolved {
        state.set_msaa_tier(resolved);
    }
}
