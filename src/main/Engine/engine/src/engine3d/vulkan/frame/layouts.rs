use super::*;

pub(super) struct SetLayouts {
    pub(super) scene: vk::DescriptorSetLayout,
    pub(super) albedo: vk::DescriptorSetLayout,
    pub(super) probe: vk::DescriptorSetLayout,
    pub(super) joints: vk::DescriptorSetLayout,
    pub(super) empty: vk::DescriptorSetLayout,
    pub(super) gizmo: vk::DescriptorSetLayout,
    pub(super) hud: vk::DescriptorSetLayout,
    pub(super) full: vk::DescriptorSetLayout,
    pub(super) taa: vk::DescriptorSetLayout,
    pub(super) denoise: vk::DescriptorSetLayout,
    pub(super) rt: vk::DescriptorSetLayout,
    pub(super) bvh: vk::DescriptorSetLayout,
}

impl SetLayouts {
    /// Destruye los layouts. Se llama en `Drop` del renderer, después de destruir los pipeline
    /// layouts y el descriptor pool, que son los únicos objetos que los referencian.
    pub(super) fn destroy(&self, device: &ash::Device) {
        use ash::vk::Handle;

        let all = [
            self.scene,
            self.albedo,
            self.probe,
            self.joints,
            self.empty,
            self.gizmo,
            self.hud,
            self.full,
            self.taa,
            self.denoise,
            self.rt,
            self.bvh,
        ];
        // Hay que deduplicar: 12 campos no son 12 objetos. `create_sets` hace `let taa = full;`
        // (los dos comparten layout), así que sin esto se destruía el mismo handle dos veces y la
        // validación lo devolvía como `vkDestroyDescriptorSetLayout(): ... Invalid Object`.
        let mut seen: Vec<u64> = Vec::with_capacity(all.len());
        for layout in all {
            let raw = layout.as_raw();
            if seen.contains(&raw) {
                continue;
            }
            seen.push(raw);
            unsafe { device.destroy_descriptor_set_layout(layout, None) };
        }
    }
}

pub(super) fn create_sets(
    device: &ash::Device,
) -> Result<(vk::DescriptorPool, SetLayouts, DescriptorSets), VulkanError> {
    let scene = set_layout(
        device,
        &[
            binding(
                0,
                vk::DescriptorType::UNIFORM_BUFFER,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            ),
            binding(
                1,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                vk::ShaderStageFlags::FRAGMENT,
            ),
        ],
    )?;
    let albedo = set_layout(
        device,
        &[binding(
            0,
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::ShaderStageFlags::FRAGMENT,
        )],
    )?;
    let probe = set_layout(
        device,
        &[binding(
            0,
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::ShaderStageFlags::FRAGMENT,
        )],
    )?;
    let joints = set_layout(
        device,
        &[binding(
            0,
            vk::DescriptorType::STORAGE_BUFFER,
            vk::ShaderStageFlags::VERTEX,
        )],
    )?;
    let empty = set_layout(device, &[])?;
    let gizmo = set_layout(
        device,
        &[binding(
            0,
            vk::DescriptorType::UNIFORM_BUFFER,
            vk::ShaderStageFlags::VERTEX,
        )],
    )?;
    let hud = set_layout(
        device,
        &[binding(
            0,
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::ShaderStageFlags::FRAGMENT,
        )],
    )?;
    let full = set_layout(
        device,
        &[
            binding(
                0,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                vk::ShaderStageFlags::FRAGMENT,
            ),
            binding(
                1,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                vk::ShaderStageFlags::FRAGMENT,
            ),
            binding(
                2,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                vk::ShaderStageFlags::FRAGMENT,
            ),
            binding(
                3,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                vk::ShaderStageFlags::FRAGMENT,
            ),
        ],
    )?;
    // TAA y denoise comparten forma (3 samplers de color en set 0), así que reutilizan el MISMO
    // layout: `SetLayouts` tiene 12 campos pero 11 objetos, y `SetLayouts::destroy` deduplica.
    let taa = full;
    let denoise = set_layout(
        device,
        &[
            binding(
                0,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                vk::ShaderStageFlags::FRAGMENT,
            ),
            binding(
                1,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                vk::ShaderStageFlags::FRAGMENT,
            ),
            binding(
                2,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                vk::ShaderStageFlags::FRAGMENT,
            ),
        ],
    )?;
    let rt_stages = vk::ShaderStageFlags::RAYGEN_KHR
        | vk::ShaderStageFlags::MISS_KHR
        | vk::ShaderStageFlags::CLOSEST_HIT_KHR;
    let rt = set_layout(
        device,
        &[
            binding(0, vk::DescriptorType::UNIFORM_BUFFER, rt_stages),
            binding(1, vk::DescriptorType::STORAGE_IMAGE, rt_stages),
            binding(2, vk::DescriptorType::ACCELERATION_STRUCTURE_KHR, rt_stages),
            binding(3, vk::DescriptorType::STORAGE_BUFFER, rt_stages),
            binding(4, vk::DescriptorType::COMBINED_IMAGE_SAMPLER, rt_stages),
            binding(5, vk::DescriptorType::COMBINED_IMAGE_SAMPLER, rt_stages),
            binding(6, vk::DescriptorType::STORAGE_BUFFER, rt_stages),
        ],
    )?;
    let bvh = set_layout(
        device,
        &[
            binding(
                0,
                vk::DescriptorType::UNIFORM_BUFFER,
                vk::ShaderStageFlags::COMPUTE,
            ),
            binding(
                1,
                vk::DescriptorType::STORAGE_BUFFER,
                vk::ShaderStageFlags::COMPUTE,
            ),
            binding(
                2,
                vk::DescriptorType::STORAGE_BUFFER,
                vk::ShaderStageFlags::COMPUTE,
            ),
            binding(
                3,
                vk::DescriptorType::STORAGE_BUFFER,
                vk::ShaderStageFlags::COMPUTE,
            ),
            binding(
                4,
                vk::DescriptorType::STORAGE_IMAGE,
                vk::ShaderStageFlags::COMPUTE,
            ),
            binding(
                5,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                vk::ShaderStageFlags::COMPUTE,
            ),
            binding(
                6,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                vk::ShaderStageFlags::COMPUTE,
            ),
        ],
    )?;
    let pool = unsafe {
        device.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::default()
                .max_sets(128)
                .pool_sizes(&[
                    vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::UNIFORM_BUFFER,
                        descriptor_count: 64,
                    },
                    vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                        descriptor_count: 128,
                    },
                    vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::STORAGE_BUFFER,
                        descriptor_count: 64,
                    },
                    vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::STORAGE_IMAGE,
                        descriptor_count: 16,
                    },
                    vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::ACCELERATION_STRUCTURE_KHR,
                        descriptor_count: 4,
                    },
                ]),
            None,
        )
    }
    .map_err(|error| vk_error("crear el pool de descriptores", error))?;
    let layouts = SetLayouts {
        scene,
        albedo,
        probe,
        joints,
        empty,
        gizmo,
        hud,
        full,
        taa,
        denoise,
        rt,
        bvh,
    };
    let sets = DescriptorSets {
        scene: [
            alloc_set(device, pool, scene)?,
            alloc_set(device, pool, scene)?,
        ],
        albedo: [
            alloc_set(device, pool, albedo)?,
            alloc_set(device, pool, albedo)?,
        ],
        probe: [
            alloc_set(device, pool, probe)?,
            alloc_set(device, pool, probe)?,
        ],
        joints: [
            alloc_set(device, pool, joints)?,
            alloc_set(device, pool, joints)?,
        ],
        empty: alloc_set(device, pool, empty)?,
        gizmo: [
            alloc_set(device, pool, gizmo)?,
            alloc_set(device, pool, gizmo)?,
        ],
        hud: [alloc_set(device, pool, hud)?, alloc_set(device, pool, hud)?],
        rt: [
            [alloc_set(device, pool, rt)?, alloc_set(device, pool, rt)?],
            [alloc_set(device, pool, rt)?, alloc_set(device, pool, rt)?],
        ],
        bvh: [
            [alloc_set(device, pool, bvh)?, alloc_set(device, pool, bvh)?],
            [alloc_set(device, pool, bvh)?, alloc_set(device, pool, bvh)?],
        ],
        taa: [alloc_set(device, pool, taa)?, alloc_set(device, pool, taa)?],
        ssr: [
            alloc_set(device, pool, full)?,
            alloc_set(device, pool, full)?,
        ],
        denoise: [
            alloc_set(device, pool, denoise)?,
            alloc_set(device, pool, denoise)?,
        ],
        composite: [
            alloc_set(device, pool, full)?,
            alloc_set(device, pool, full)?,
        ],
        present: [
            alloc_set(device, pool, full)?,
            alloc_set(device, pool, full)?,
        ],
        sky: [
            alloc_set(device, pool, full)?,
            alloc_set(device, pool, full)?,
        ],
    };
    Ok((pool, layouts, sets))
}

pub(super) fn binding(
    slot: u32,
    ty: vk::DescriptorType,
    stages: vk::ShaderStageFlags,
) -> vk::DescriptorSetLayoutBinding<'static> {
    vk::DescriptorSetLayoutBinding::default()
        .binding(slot)
        .descriptor_type(ty)
        .descriptor_count(1)
        .stage_flags(stages)
}

pub(super) fn set_layout(
    device: &ash::Device,
    bindings: &[vk::DescriptorSetLayoutBinding<'_>],
) -> Result<vk::DescriptorSetLayout, VulkanError> {
    unsafe {
        device.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(bindings),
            None,
        )
    }
    .map_err(|error| vk_error("crear un descriptor set layout", error))
}

pub(super) fn alloc_set(
    device: &ash::Device,
    pool: vk::DescriptorPool,
    layout: vk::DescriptorSetLayout,
) -> Result<vk::DescriptorSet, VulkanError> {
    let layouts = [layout];
    unsafe {
        device.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(pool)
                .set_layouts(&layouts),
        )
    }
    .map_err(|error| vk_error("reservar un descriptor set", error))
    .map(|sets| sets[0])
}

pub(super) fn pipeline_layout(
    device: &ash::Device,
    sets: &[vk::DescriptorSetLayout],
    push: u32,
) -> Result<vk::PipelineLayout, VulkanError> {
    let range = vk::PushConstantRange::default()
        .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
        .offset(0)
        .size(push);
    let ranges = if push == 0 {
        &[][..]
    } else {
        std::slice::from_ref(&range)
    };
    unsafe {
        device.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo::default()
                .set_layouts(sets)
                .push_constant_ranges(ranges),
            None,
        )
    }
    .map_err(|error| vk_error("crear un pipeline layout", error))
}

pub(super) fn shader(
    device: &ash::Device,
    bytes: &[u8],
    keep: &mut Vec<vk::ShaderModule>,
) -> Result<vk::ShaderModule, VulkanError> {
    let words: Vec<u32> = bytes
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();
    let module = unsafe {
        device.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&words), None)
    }
    .map_err(|error| vk_error("crear un shader module", error))?;
    keep.push(module);
    Ok(module)
}

pub(super) fn sampler(
    device: &ash::Device,
    _compare: bool,
    address: vk::SamplerAddressMode,
) -> Result<vk::Sampler, VulkanError> {
    unsafe {
        device.create_sampler(
            &vk::SamplerCreateInfo::default()
                .mag_filter(vk::Filter::LINEAR)
                .min_filter(vk::Filter::LINEAR)
                .address_mode_u(address)
                .address_mode_v(address)
                .address_mode_w(address)
                .max_lod(16.0),
            None,
        )
    }
    .map_err(|error| vk_error("crear un sampler", error))
}

pub(super) fn compare_sampler(device: &ash::Device) -> Result<vk::Sampler, VulkanError> {
    unsafe {
        device.create_sampler(
            &vk::SamplerCreateInfo::default()
                .mag_filter(vk::Filter::LINEAR)
                .min_filter(vk::Filter::LINEAR)
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .compare_enable(true)
                .compare_op(vk::CompareOp::LESS_OR_EQUAL),
            None,
        )
    }
    .map_err(|error| vk_error("crear el sampler de sombras", error))
}

pub(super) fn write_uniform(
    device: &ash::Device,
    set: vk::DescriptorSet,
    binding: u32,
    buffer: &GpuBuffer,
) -> Result<(), VulkanError> {
    let info = vk::DescriptorBufferInfo {
        buffer: buffer.buffer,
        offset: 0,
        range: buffer.size,
    };
    let write = vk::WriteDescriptorSet::default()
        .dst_set(set)
        .dst_binding(binding)
        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
        .buffer_info(std::slice::from_ref(&info));
    unsafe { device.update_descriptor_sets(&[write], &[]) };
    Ok(())
}

pub(super) fn write_storage(
    device: &ash::Device,
    set: vk::DescriptorSet,
    binding: u32,
    buffer: &GpuBuffer,
) -> Result<(), VulkanError> {
    let info = vk::DescriptorBufferInfo {
        buffer: buffer.buffer,
        offset: 0,
        range: buffer.size,
    };
    let write = vk::WriteDescriptorSet::default()
        .dst_set(set)
        .dst_binding(binding)
        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
        .buffer_info(std::slice::from_ref(&info));
    unsafe { device.update_descriptor_sets(&[write], &[]) };
    Ok(())
}

pub(super) fn write_combined(
    device: &ash::Device,
    set: vk::DescriptorSet,
    binding: u32,
    view: vk::ImageView,
    sampler: vk::Sampler,
    layout: vk::ImageLayout,
) -> Result<(), VulkanError> {
    let info = vk::DescriptorImageInfo {
        sampler,
        image_view: view,
        image_layout: layout,
    };
    let write = vk::WriteDescriptorSet::default()
        .dst_set(set)
        .dst_binding(binding)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .image_info(std::slice::from_ref(&info));
    unsafe { device.update_descriptor_sets(&[write], &[]) };
    Ok(())
}

pub(super) fn write_storage_image(
    device: &ash::Device,
    set: vk::DescriptorSet,
    binding: u32,
    view: vk::ImageView,
) -> Result<(), VulkanError> {
    let info = vk::DescriptorImageInfo {
        sampler: vk::Sampler::null(),
        image_view: view,
        image_layout: vk::ImageLayout::GENERAL,
    };
    let write = vk::WriteDescriptorSet::default()
        .dst_set(set)
        .dst_binding(binding)
        .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
        .image_info(std::slice::from_ref(&info));
    unsafe { device.update_descriptor_sets(&[write], &[]) };
    Ok(())
}

pub(super) fn write_acceleration(
    device: &ash::Device,
    set: vk::DescriptorSet,
    binding: u32,
    accel: vk::AccelerationStructureKHR,
) -> Result<(), VulkanError> {
    let structures = [accel];
    let mut write_as = vk::WriteDescriptorSetAccelerationStructureKHR::default()
        .acceleration_structures(&structures);
    let mut write = vk::WriteDescriptorSet::default()
        .dst_set(set)
        .dst_binding(binding)
        .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
        .push_next(&mut write_as);
    write.descriptor_count = 1;
    unsafe { device.update_descriptor_sets(&[write], &[]) };
    Ok(())
}
