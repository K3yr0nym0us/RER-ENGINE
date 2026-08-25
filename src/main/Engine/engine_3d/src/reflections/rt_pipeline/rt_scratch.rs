//! Copia SSR al scratch de RT antes del pass full-screen (solo píxeles SSR miss se sobrescriben).

use wgpu::{Device, TextureView};

pub struct RtScratchCopy {
    copy_pipeline: wgpu::ComputePipeline,
    copy_bgl: wgpu::BindGroupLayout,
    width: u32,
    height: u32,
}

impl RtScratchCopy {
    pub fn new(device: &Device, color_format: wgpu::TextureFormat) -> Self {
        let copy_shader = crate::reflections::load_refl_wgsl(
            device,
            "rt-copy-ssr",
            include_str!("rt_copy_ssr.wgsl"),
        );
        let copy_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rt-copy-ssr-bgl"),
            entries: &[
                texture_entry(0, false),
                storage_texture_entry(1, color_format, false),
            ],
        });
        let copy_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rt-copy-pl"),
            bind_group_layouts: &[Some(&copy_bgl)],
            immediate_size: 0,
        });
        let copy_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("rt-copy-ssr-pipeline"),
            layout: Some(&copy_pl),
            module: &copy_shader,
            entry_point: Some("cs_main"),
            compilation_options: Default::default(),
            cache: None,
        });

        Self {
            copy_pipeline,
            copy_bgl,
            width: 1,
            height: 1,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width.max(1);
        self.height = height.max(1);
    }

    pub fn seed_output_from_ssr(
        &self,
        device: &Device,
        encoder: &mut wgpu::CommandEncoder,
        ssr_view: &TextureView,
        output_view: &TextureView,
    ) {
        let copy_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rt-copy-ssr-bg"),
            layout: &self.copy_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(ssr_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(output_view),
                },
            ],
        });
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("rt-copy-ssr-pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.copy_pipeline);
        pass.set_bind_group(0, &copy_bg, &[]);
        pass.dispatch_workgroups(self.width.div_ceil(8), self.height.div_ceil(8), 1);
    }
}

fn texture_entry(binding: u32, filterable: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn storage_texture_entry(
    binding: u32,
    format: wgpu::TextureFormat,
    read_write: bool,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::StorageTexture {
            access: if read_write {
                wgpu::StorageTextureAccess::ReadWrite
            } else {
                wgpu::StorageTextureAccess::WriteOnly
            },
            format,
            view_dimension: wgpu::TextureViewDimension::D2,
        },
        count: None,
    }
}
