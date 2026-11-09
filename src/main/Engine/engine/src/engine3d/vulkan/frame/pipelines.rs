use super::*;

pub(super) struct VertexSpec {
    bindings: Vec<vk::VertexInputBindingDescription>,
    attributes: Vec<vk::VertexInputAttributeDescription>,
    /// Muestras del framebuffer al que se dibuja (1 = sin MSAA). Se mete aquí en vez de añadir un
    /// argumento más a `graphics()` porque Vulkan exige que `rasterizationSamples` coincida con el
    /// `sampleCount` de TODOS los adjuntos del render pass.
    samples: u32,
}

/// Copia una spec de vértices con otro número de muestras (para los pipelines MSAA del forward).
pub(super) fn with_samples(mut spec: VertexSpec, samples: u32) -> VertexSpec {
    spec.samples = samples.max(1);
    spec
}

/// `sample_count` (1/2/4/8/16) a banderas de Vulkan. Cualquier otro valor se interpreta como 1×.
pub(super) fn sample_flags(samples: u32) -> vk::SampleCountFlags {
    match samples {
        2 => vk::SampleCountFlags::TYPE_2,
        4 => vk::SampleCountFlags::TYPE_4,
        8 => vk::SampleCountFlags::TYPE_8,
        16 => vk::SampleCountFlags::TYPE_16,
        _ => vk::SampleCountFlags::TYPE_1,
    }
}

pub(super) fn static_vertex() -> VertexSpec {
    VertexSpec {
        bindings: vec![
            vk::VertexInputBindingDescription {
                binding: 0,
                stride: 32,
                input_rate: vk::VertexInputRate::VERTEX,
            },
            vk::VertexInputBindingDescription {
                binding: 1,
                stride: 96,
                input_rate: vk::VertexInputRate::INSTANCE,
            },
        ],
        attributes: vec![
            attr(0, 0, vk::Format::R32G32B32_SFLOAT, 0),
            attr(1, 0, vk::Format::R32G32B32_SFLOAT, 12),
            attr(2, 0, vk::Format::R32G32_SFLOAT, 24),
            attr(3, 1, vk::Format::R32G32B32A32_SFLOAT, 0),
            attr(4, 1, vk::Format::R32G32B32A32_SFLOAT, 16),
            attr(5, 1, vk::Format::R32G32B32A32_SFLOAT, 32),
            attr(6, 1, vk::Format::R32G32B32A32_SFLOAT, 48),
            attr(7, 1, vk::Format::R32G32B32A32_SFLOAT, 64),
            attr(8, 1, vk::Format::R32G32B32A32_SFLOAT, 80),
        ],
        samples: 1,
    }
}

pub(super) fn skinned_vertex() -> VertexSpec {
    VertexSpec {
        bindings: vec![
            vk::VertexInputBindingDescription {
                binding: 0,
                stride: 64,
                input_rate: vk::VertexInputRate::VERTEX,
            },
            vk::VertexInputBindingDescription {
                binding: 1,
                stride: 96,
                input_rate: vk::VertexInputRate::INSTANCE,
            },
        ],
        attributes: vec![
            attr(0, 0, vk::Format::R32G32B32_SFLOAT, 0),
            attr(1, 0, vk::Format::R32G32B32_SFLOAT, 12),
            attr(2, 0, vk::Format::R32G32_SFLOAT, 24),
            attr(3, 0, vk::Format::R32G32B32A32_UINT, 32),
            attr(4, 0, vk::Format::R32G32B32A32_SFLOAT, 48),
            attr(5, 1, vk::Format::R32G32B32A32_SFLOAT, 0),
            attr(6, 1, vk::Format::R32G32B32A32_SFLOAT, 16),
            attr(7, 1, vk::Format::R32G32B32A32_SFLOAT, 32),
            attr(8, 1, vk::Format::R32G32B32A32_SFLOAT, 48),
            attr(9, 1, vk::Format::R32G32B32A32_SFLOAT, 64),
            attr(10, 1, vk::Format::R32G32B32A32_SFLOAT, 80),
        ],
        samples: 1,
    }
}

pub(super) fn gizmo_vertex() -> VertexSpec {
    VertexSpec {
        bindings: vec![vk::VertexInputBindingDescription {
            binding: 0,
            stride: 28,
            input_rate: vk::VertexInputRate::VERTEX,
        }],
        attributes: vec![
            attr(0, 0, vk::Format::R32G32B32_SFLOAT, 0),
            attr(1, 0, vk::Format::R32G32B32A32_SFLOAT, 12),
        ],
        samples: 1,
    }
}

pub(super) fn empty_vertex() -> VertexSpec {
    VertexSpec {
        bindings: Vec::new(),
        attributes: Vec::new(),
        samples: 1,
    }
}

pub(super) fn attr(
    location: u32,
    binding: u32,
    format: vk::Format,
    offset: u32,
) -> vk::VertexInputAttributeDescription {
    vk::VertexInputAttributeDescription {
        location,
        binding,
        format,
        offset,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn graphics(
    device: &ash::Device,
    layout: vk::PipelineLayout,
    vert: vk::ShaderModule,
    frag: Option<vk::ShaderModule>,
    vertex: &VertexSpec,
    colors: &[vk::Format],
    depth: Option<vk::Format>,
    blend: bool,
    depth_bias: bool,
    keep: &mut Vec<vk::Pipeline>,
) -> Result<vk::Pipeline, VulkanError> {
    graphics_with_depth_test(
        device,
        layout,
        vert,
        frag,
        vertex,
        colors,
        depth,
        blend,
        depth_bias,
        vk::PrimitiveTopology::TRIANGLE_LIST,
        true,
        keep,
    )
}

/// Igual que `graphics` pero sin prueba ni escritura de profundidad.
///
/// Lo usa el quad del cielo dentro del pase MSAA: el adjunto de profundidad existe (el resto de
/// adjuntos sí la usan), pero el fondo no debe tapar la geometría que se dibuja después.
#[allow(clippy::too_many_arguments)]
pub(super) fn graphics_no_depth_test(
    device: &ash::Device,
    layout: vk::PipelineLayout,
    vert: vk::ShaderModule,
    frag: Option<vk::ShaderModule>,
    vertex: &VertexSpec,
    colors: &[vk::Format],
    depth: Option<vk::Format>,
    keep: &mut Vec<vk::Pipeline>,
) -> Result<vk::Pipeline, VulkanError> {
    graphics_with_depth_test(
        device,
        layout,
        vert,
        frag,
        vertex,
        colors,
        depth,
        false,
        false,
        vk::PrimitiveTopology::TRIANGLE_LIST,
        false,
        keep,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn graphics_topology(
    device: &ash::Device,
    layout: vk::PipelineLayout,
    vert: vk::ShaderModule,
    frag: Option<vk::ShaderModule>,
    vertex: &VertexSpec,
    colors: &[vk::Format],
    depth: Option<vk::Format>,
    blend: bool,
    depth_bias: bool,
    topology: vk::PrimitiveTopology,
    keep: &mut Vec<vk::Pipeline>,
) -> Result<vk::Pipeline, VulkanError> {
    graphics_with_depth_test(
        device, layout, vert, frag, vertex, colors, depth, blend, depth_bias, topology, true, keep,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn graphics_with_depth_test(
    device: &ash::Device,
    layout: vk::PipelineLayout,
    vert: vk::ShaderModule,
    frag: Option<vk::ShaderModule>,
    vertex: &VertexSpec,
    colors: &[vk::Format],
    depth: Option<vk::Format>,
    blend: bool,
    depth_bias: bool,
    topology: vk::PrimitiveTopology,
    depth_test: bool,
    keep: &mut Vec<vk::Pipeline>,
) -> Result<vk::Pipeline, VulkanError> {
    let name = c"main";
    let mut stages = vec![
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert)
            .name(name),
    ];
    if let Some(frag) = frag {
        stages.push(
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(frag)
                .name(name),
        );
    }
    let vertex_info = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(&vertex.bindings)
        .vertex_attribute_descriptions(&vertex.attributes);
    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default().topology(topology);
    let viewport = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let raster = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(if depth_bias {
            vk::CullModeFlags::BACK
        } else {
            vk::CullModeFlags::NONE
        })
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0)
        .depth_bias_enable(depth_bias)
        .depth_bias_constant_factor(if depth_bias { 1.5 } else { 0.0 })
        .depth_bias_slope_factor(if depth_bias { 2.0 } else { 0.0 });
    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(sample_flags(vertex.samples));
    let depth_state = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(depth.is_some() && depth_test)
        .depth_write_enable(depth.is_some() && depth_test && !blend)
        .depth_compare_op(vk::CompareOp::LESS);
    let blend_attachment = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(blend)
        .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .alpha_blend_op(vk::BlendOp::ADD);
    let attachments = vec![blend_attachment; colors.len()];
    let blend_state = vk::PipelineColorBlendStateCreateInfo::default().attachments(&attachments);
    let dynamics = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamics);
    let mut rendering = vk::PipelineRenderingCreateInfo::default().color_attachment_formats(colors);
    if let Some(depth) = depth {
        rendering = rendering.depth_attachment_format(depth);
    }
    let info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&stages)
        .vertex_input_state(&vertex_info)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport)
        .rasterization_state(&raster)
        .multisample_state(&multisample)
        .depth_stencil_state(&depth_state)
        .color_blend_state(&blend_state)
        .dynamic_state(&dynamic)
        .layout(layout)
        .push_next(&mut rendering);
    let pipeline =
        match unsafe { device.create_graphics_pipelines(vk::PipelineCache::null(), &[info], None) }
        {
            Ok(mut created) => created.remove(0),
            Err((created, error)) => {
                for pipeline in created {
                    unsafe { device.destroy_pipeline(pipeline, None) };
                }
                return Err(vk_error("crear un pipeline gráfico", error));
            }
        };
    keep.push(pipeline);
    Ok(pipeline)
}

pub(super) fn create_compute(
    device: &ash::Device,
    bytes: &[u8],
    layout_set: vk::DescriptorSetLayout,
    shaders: &mut Vec<vk::ShaderModule>,
    layouts: &mut Vec<vk::PipelineLayout>,
    pipelines: &mut Vec<vk::Pipeline>,
) -> Result<vk::Pipeline, VulkanError> {
    let module = shader(device, bytes, shaders)?;
    let layout = pipeline_layout(device, &[layout_set], 0)?;
    layouts.push(layout);
    let name = c"main";
    let stage = vk::PipelineShaderStageCreateInfo::default()
        .stage(vk::ShaderStageFlags::COMPUTE)
        .module(module)
        .name(name);
    let info = vk::ComputePipelineCreateInfo::default()
        .stage(stage)
        .layout(layout);
    let pipeline = match unsafe {
        device.create_compute_pipelines(vk::PipelineCache::null(), &[info], None)
    } {
        Ok(mut created) => created.remove(0),
        Err((created, error)) => {
            for pipeline in created {
                unsafe { device.destroy_pipeline(pipeline, None) };
            }
            return Err(vk_error("crear el pipeline de BVH", error));
        }
    };
    pipelines.push(pipeline);
    Ok(pipeline)
}

pub(super) fn create_rt_pipeline(
    gpu: &SharedGpu,
    set: &vk::DescriptorSetLayout,
    shaders: &mut Vec<vk::ShaderModule>,
    layouts: &mut Vec<vk::PipelineLayout>,
    pipelines: &mut Vec<vk::Pipeline>,
) -> Result<(vk::Pipeline, GpuBuffer, u32, u64), VulkanError> {
    let device = gpu.device();
    let rt = gpu.rt_pipeline().ok_or_else(|| {
        VulkanError::new("El device no cargó VK_KHR_ray_tracing_pipeline.".to_string())
    })?;
    let rgen = shader(device, spv("primary.rgen"), shaders)?;
    let reflect = shader(device, spv("reflect.rgen"), shaders)?;
    let miss = shader(device, spv("primary.rmiss"), shaders)?;
    let hit = shader(device, spv("primary.rchit"), shaders)?;
    let name = c"main";
    let stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::RAYGEN_KHR)
            .module(rgen)
            .name(name),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::RAYGEN_KHR)
            .module(reflect)
            .name(name),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::MISS_KHR)
            .module(miss)
            .name(name),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::CLOSEST_HIT_KHR)
            .module(hit)
            .name(name),
    ];
    let unused = vk::SHADER_UNUSED_KHR;
    let groups = [
        vk::RayTracingShaderGroupCreateInfoKHR::default()
            .ty(vk::RayTracingShaderGroupTypeKHR::GENERAL)
            .general_shader(0)
            .closest_hit_shader(unused)
            .any_hit_shader(unused)
            .intersection_shader(unused),
        vk::RayTracingShaderGroupCreateInfoKHR::default()
            .ty(vk::RayTracingShaderGroupTypeKHR::GENERAL)
            .general_shader(1)
            .closest_hit_shader(unused)
            .any_hit_shader(unused)
            .intersection_shader(unused),
        vk::RayTracingShaderGroupCreateInfoKHR::default()
            .ty(vk::RayTracingShaderGroupTypeKHR::GENERAL)
            .general_shader(2)
            .closest_hit_shader(unused)
            .any_hit_shader(unused)
            .intersection_shader(unused),
        vk::RayTracingShaderGroupCreateInfoKHR::default()
            .ty(vk::RayTracingShaderGroupTypeKHR::TRIANGLES_HIT_GROUP)
            .general_shader(unused)
            .closest_hit_shader(3)
            .any_hit_shader(unused)
            .intersection_shader(unused),
    ];
    let layout = pipeline_layout(device, &[*set], 0)?;
    layouts.push(layout);
    let info = vk::RayTracingPipelineCreateInfoKHR::default()
        .stages(&stages)
        .groups(&groups)
        .max_pipeline_ray_recursion_depth(1)
        .layout(layout);
    let pipeline = match unsafe {
        rt.create_ray_tracing_pipelines(
            vk::DeferredOperationKHR::null(),
            vk::PipelineCache::null(),
            &[info],
            None,
        )
    } {
        Ok(mut created) => created.remove(0),
        Err((created, error)) => {
            for pipeline in created {
                if pipeline != vk::Pipeline::null() {
                    unsafe { device.destroy_pipeline(pipeline, None) };
                }
            }
            return Err(vk_error("crear el pipeline de ray tracing", error));
        }
    };
    pipelines.push(pipeline);
    let props = gpu.rt_properties().ok_or_else(|| {
        VulkanError::new("Faltan las propiedades de ray tracing del device.".to_string())
    })?;
    let handle_size = props.shader_group_handle_size;
    let stride = align_up(handle_size, props.shader_group_handle_alignment.max(1))
        .max(props.shader_group_base_alignment.max(1));
    let handles = unsafe {
        rt.get_ray_tracing_shader_group_handles(pipeline, 0, 4, 4 * handle_size as usize)
    }
    .map_err(|error| vk_error("leer los handles del SBT", error))?;
    let mut table = vec![0u8; stride as usize * 4];
    for group in 0..4 {
        let src = group * handle_size as usize;
        let dst = group * stride as usize;
        table[dst..dst + handle_size as usize]
            .copy_from_slice(&handles[src..src + handle_size as usize]);
    }
    let buffer = gpu.create_buffer_with_data(
        &table,
        vk::BufferUsageFlags::SHADER_BINDING_TABLE_KHR
            | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
        MemoryLocation::CpuToGpu,
        "sbt",
    )?;
    let address = buffer.address;
    Ok((pipeline, buffer, stride, address))
}
