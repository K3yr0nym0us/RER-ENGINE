//! BLAS y TLAS. El `unsafe` es FFI de `VK_KHR_acceleration_structure`: las uniones
//! de geometría no tienen constructor safe y el handle se destruye antes que el buffer.

use ash::vk;
use gpu_allocator::MemoryLocation;

use super::error::{VulkanError, vk_error};
use super::gpu::{GpuBuffer, SharedGpu};

pub struct Accel {
    gpu: SharedGpu,
    pub handle: vk::AccelerationStructureKHR,
    /// Mantiene viva la memoria del AS; su `Drop` la libera después del handle.
    #[allow(dead_code)]
    pub storage: GpuBuffer,
    pub address: u64,
}

impl Drop for Accel {
    fn drop(&mut self) {
        if let Some(accel) = self.gpu.accel() {
            unsafe { accel.destroy_acceleration_structure(self.handle, None) };
        }
    }
}

pub struct BlasInput {
    pub vertex_address: u64,
    pub index_address: u64,
    pub vertex_stride: u64,
    pub max_vertex: u32,
    pub primitive_count: u32,
}

fn as_usage() -> vk::BufferUsageFlags {
    vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR
        | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
}

fn scratch_usage() -> vk::BufferUsageFlags {
    vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
}

fn align_scratch(value: u64, alignment: u64) -> u64 {
    let alignment = alignment.max(1);
    value.div_ceil(alignment) * alignment
}

/// Construye un BLAS de triángulos opacos y espera a que termine.
pub fn build_blas(gpu: &SharedGpu, input: &BlasInput) -> Result<Accel, VulkanError> {
    let accel = gpu.accel().ok_or_else(|| {
        VulkanError::new("El device no expone VK_KHR_acceleration_structure.".to_string())
    })?;
    let primitive_count = input.primitive_count.max(1);
    let triangles = triangle_geometry(input);
    let geometries = [vk::AccelerationStructureGeometryKHR::default()
        .geometry_type(vk::GeometryTypeKHR::TRIANGLES)
        .flags(vk::GeometryFlagsKHR::OPAQUE)];
    let mut geometries = geometries;
    geometries[0].geometry.triangles = triangles;
    let mut build = vk::AccelerationStructureBuildGeometryInfoKHR::default()
        .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
        .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE)
        .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
        .geometries(&geometries);
    let mut sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
    unsafe {
        accel.get_acceleration_structure_build_sizes(
            vk::AccelerationStructureBuildTypeKHR::DEVICE,
            &build,
            &[primitive_count],
            &mut sizes,
        );
    }
    let storage = gpu.create_buffer(
        sizes.acceleration_structure_size.max(256),
        as_usage(),
        MemoryLocation::GpuOnly,
        "blas",
    )?;
    let handle = unsafe {
        accel.create_acceleration_structure(
            &vk::AccelerationStructureCreateInfoKHR::default()
                .buffer(storage.buffer)
                .size(sizes.acceleration_structure_size.max(256))
                .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL),
            None,
        )
    }
    .map_err(|error| vk_error("crear el BLAS", error))?;
    let address = unsafe {
        accel.get_acceleration_structure_device_address(
            &vk::AccelerationStructureDeviceAddressInfoKHR::default()
                .acceleration_structure(handle),
        )
    };
    let scratch_align = gpu
        .rt_properties()
        .map(|props| u64::from(props.min_scratch_alignment))
        .unwrap_or(256);
    let scratch = gpu.create_buffer(
        align_scratch(sizes.build_scratch_size.max(256), scratch_align),
        scratch_usage(),
        MemoryLocation::GpuOnly,
        "blas-scratch",
    )?;
    build = build
        .dst_acceleration_structure(handle)
        .scratch_data(device_address_mut(scratch.address));
    let range = [vk::AccelerationStructureBuildRangeInfoKHR::default()
        .primitive_count(input.primitive_count)
        .primitive_offset(0)
        .first_vertex(0)
        .transform_offset(0)];
    let ranges: &[&[vk::AccelerationStructureBuildRangeInfoKHR]] = &[&range];
    gpu.one_shot(|_device, command| unsafe {
        accel.cmd_build_acceleration_structures(command, &[build], ranges);
    })?;
    Ok(Accel {
        gpu: gpu.clone(),
        handle,
        storage,
        address,
    })
}

pub struct TlasInstance {
    pub blas_address: u64,
    pub transform: [f32; 12],
    pub custom_index: u32,
}

pub fn build_tlas(gpu: &SharedGpu, instances: &[TlasInstance]) -> Result<Accel, VulkanError> {
    let accel = gpu.accel().ok_or_else(|| {
        VulkanError::new("El device no expone VK_KHR_acceleration_structure.".to_string())
    })?;
    let packed: Vec<vk::AccelerationStructureInstanceKHR> = instances
        .iter()
        .map(|instance| vk::AccelerationStructureInstanceKHR {
            transform: vk::TransformMatrixKHR {
                matrix: instance.transform,
            },
            instance_custom_index_and_mask: vk::Packed24_8::new(instance.custom_index, 0xff),
            instance_shader_binding_table_record_offset_and_flags: vk::Packed24_8::new(
                0,
                vk::GeometryInstanceFlagsKHR::FORCE_OPAQUE.as_raw() as u8,
            ),
            acceleration_structure_reference: vk::AccelerationStructureReferenceKHR {
                device_handle: instance.blas_address,
            },
        })
        .collect();
    let instance_bytes = if packed.is_empty() {
        vec![0u8; 64]
    } else {
        unsafe {
            std::slice::from_raw_parts(
                packed.as_ptr().cast::<u8>(),
                std::mem::size_of_val(packed.as_slice()),
            )
            .to_vec()
        }
    };
    let instance_buffer = gpu.create_buffer_with_data(
        &instance_bytes,
        vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR
            | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
        MemoryLocation::CpuToGpu,
        "tlas-instances",
    )?;
    let count = instances.len() as u32;
    let instances_data = vk::AccelerationStructureGeometryInstancesDataKHR::default()
        .array_of_pointers(false)
        .data(device_address(instance_buffer.address));
    let geometries = [vk::AccelerationStructureGeometryKHR::default()
        .geometry_type(vk::GeometryTypeKHR::INSTANCES)
        .flags(vk::GeometryFlagsKHR::OPAQUE)];
    let mut geometries = geometries;
    geometries[0].geometry.instances = instances_data;
    let mut build = vk::AccelerationStructureBuildGeometryInfoKHR::default()
        .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL)
        .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_BUILD)
        .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
        .geometries(&geometries);
    let mut sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
    unsafe {
        accel.get_acceleration_structure_build_sizes(
            vk::AccelerationStructureBuildTypeKHR::DEVICE,
            &build,
            &[count.max(1)],
            &mut sizes,
        );
    }
    let storage = gpu.create_buffer(
        sizes.acceleration_structure_size.max(256),
        as_usage(),
        MemoryLocation::GpuOnly,
        "tlas",
    )?;
    let handle = unsafe {
        accel.create_acceleration_structure(
            &vk::AccelerationStructureCreateInfoKHR::default()
                .buffer(storage.buffer)
                .size(sizes.acceleration_structure_size.max(256))
                .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL),
            None,
        )
    }
    .map_err(|error| vk_error("crear el TLAS", error))?;
    let address = unsafe {
        accel.get_acceleration_structure_device_address(
            &vk::AccelerationStructureDeviceAddressInfoKHR::default()
                .acceleration_structure(handle),
        )
    };
    let scratch_align = gpu
        .rt_properties()
        .map(|props| u64::from(props.min_scratch_alignment))
        .unwrap_or(256);
    let scratch = gpu.create_buffer(
        align_scratch(sizes.build_scratch_size.max(256), scratch_align),
        scratch_usage(),
        MemoryLocation::GpuOnly,
        "tlas-scratch",
    )?;
    build = build
        .dst_acceleration_structure(handle)
        .scratch_data(device_address_mut(scratch.address));
    let range = [vk::AccelerationStructureBuildRangeInfoKHR::default().primitive_count(count)];
    let ranges: &[&[vk::AccelerationStructureBuildRangeInfoKHR]] = &[&range];
    gpu.one_shot(|_device, command| unsafe {
        accel.cmd_build_acceleration_structures(command, &[build], ranges);
    })?;
    Ok(Accel {
        gpu: gpu.clone(),
        handle,
        storage,
        address,
    })
}

fn triangle_geometry(input: &BlasInput) -> vk::AccelerationStructureGeometryTrianglesDataKHR<'_> {
    vk::AccelerationStructureGeometryTrianglesDataKHR::default()
        .vertex_format(vk::Format::R32G32B32_SFLOAT)
        .vertex_data(device_address(input.vertex_address))
        .vertex_stride(input.vertex_stride)
        .max_vertex(input.max_vertex)
        .index_type(vk::IndexType::UINT32)
        .index_data(device_address(input.index_address))
}

fn device_address(address: u64) -> vk::DeviceOrHostAddressConstKHR {
    let mut data = vk::DeviceOrHostAddressConstKHR::default();
    data.device_address = address;
    data
}

fn device_address_mut(address: u64) -> vk::DeviceOrHostAddressKHR {
    let mut data = vk::DeviceOrHostAddressKHR::default();
    data.device_address = address;
    data
}
