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
pub fn build_blas(
    gpu: &SharedGpu,
    input: &BlasInput,
    fast_trace: bool,
) -> Result<Accel, VulkanError> {
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
    let build_flags = if fast_trace {
        vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE
    } else {
        vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_BUILD
    };
    let mut build = vk::AccelerationStructureBuildGeometryInfoKHR::default()
        .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
        .flags(build_flags)
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

/// Huella de topología (sin transforms): mismo conjunto y orden de BLAS/custom_index.
pub fn tlas_topology(instances: &[TlasInstance]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    instances.len().hash(&mut hasher);
    for instance in instances {
        instance.blas_address.hash(&mut hasher);
        instance.custom_index.hash(&mut hasher);
    }
    hasher.finish()
}

/// `Update` solo es legal si el BUILD anterior se hizo con `ALLOW_UPDATE` y el
/// número/orden de instancias no cambió.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TlasOp {
    Build,
    Update,
}

pub fn tlas_op(
    has_accel: bool,
    prev_topology: u64,
    prev_count: u32,
    prev_fast_trace: bool,
    topology: u64,
    count: u32,
    fast_trace: bool,
    force_build: bool,
    storage_fits: bool,
) -> TlasOp {
    if force_build
        || !has_accel
        || !storage_fits
        || prev_count != count
        || prev_topology != topology
        || prev_fast_trace != fast_trace
    {
        TlasOp::Build
    } else {
        TlasOp::Update
    }
}

/// TLAS + scratch + buffer de instancias de un slot en vuelo.
/// El fence del slot ya esperó en `begin_frame`, así que se puede UPDATE in-place.
pub struct TlasSlot {
    accel: Option<Accel>,
    instances: Option<GpuBuffer>,
    scratch: Option<GpuBuffer>,
    topology: u64,
    instance_count: u32,
    fast_trace: bool,
}

impl TlasSlot {
    pub fn new() -> Self {
        Self {
            accel: None,
            instances: None,
            scratch: None,
            topology: 0,
            instance_count: 0,
            fast_trace: false,
        }
    }

    pub fn handle(&self) -> Option<vk::AccelerationStructureKHR> {
        self.accel.as_ref().map(|accel| accel.handle)
    }

    pub fn invalidate(&mut self) {
        self.accel = None;
        self.topology = 0;
        self.instance_count = 0;
    }
}

impl Default for TlasSlot {
    fn default() -> Self {
        Self::new()
    }
}

fn instance_usage() -> vk::BufferUsageFlags {
    vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR
        | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
}

fn tlas_flags(fast_trace: bool) -> vk::BuildAccelerationStructureFlagsKHR {
    let quality = if fast_trace {
        vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE
    } else {
        vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_BUILD
    };
    quality | vk::BuildAccelerationStructureFlagsKHR::ALLOW_UPDATE
}

fn pack_instances(instances: &[TlasInstance]) -> Vec<u8> {
    if instances.is_empty() {
        return vec![0u8; 64];
    }
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
    unsafe {
        std::slice::from_raw_parts(
            packed.as_ptr().cast::<u8>(),
            std::mem::size_of_val(packed.as_slice()),
        )
        .to_vec()
    }
}

fn grow_gpu_buffer(
    gpu: &SharedGpu,
    buffer: &mut Option<GpuBuffer>,
    size: u64,
    usage: vk::BufferUsageFlags,
    location: MemoryLocation,
    name: &str,
) -> Result<(), VulkanError> {
    let size = size.max(256).next_power_of_two();
    if buffer
        .as_ref()
        .is_some_and(|existing| existing.size >= size)
    {
        return Ok(());
    }
    *buffer = Some(gpu.create_buffer(size, usage, location, name)?);
    Ok(())
}

/// Escribe instancias, reconstruye o actualiza el TLAS del slot y lo deja listo
/// para `cmd_trace_rays` en el mismo command buffer (barrera AS_BUILD → RT).
pub fn record_tlas(
    gpu: &SharedGpu,
    slot: &mut TlasSlot,
    instances: &[TlasInstance],
    fast_trace: bool,
    force_build: bool,
    cmd: vk::CommandBuffer,
) -> Result<(), VulkanError> {
    let loader = gpu.accel().ok_or_else(|| {
        VulkanError::new("El device no expone VK_KHR_acceleration_structure.".to_string())
    })?;
    let topology = tlas_topology(instances);
    let count = instances.len() as u32;
    let packed = pack_instances(instances);
    grow_gpu_buffer(
        gpu,
        &mut slot.instances,
        packed.len() as u64,
        instance_usage(),
        MemoryLocation::CpuToGpu,
        "tlas-instances",
    )?;
    let instance_buffer = slot
        .instances
        .as_mut()
        .ok_or_else(|| VulkanError::new("No hay buffer de instancias para el TLAS.".to_string()))?;
    instance_buffer.write(0, &packed)?;
    let instance_address = instance_buffer.address;

    let instances_data = vk::AccelerationStructureGeometryInstancesDataKHR::default()
        .array_of_pointers(false)
        .data(device_address(instance_address));
    let geometries = [vk::AccelerationStructureGeometryKHR::default()
        .geometry_type(vk::GeometryTypeKHR::INSTANCES)
        .flags(vk::GeometryFlagsKHR::OPAQUE)];
    let mut geometries = geometries;
    geometries[0].geometry.instances = instances_data;
    let flags = tlas_flags(fast_trace);
    let mut size_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
        .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL)
        .flags(flags)
        .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
        .geometries(&geometries);
    let mut sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
    unsafe {
        loader.get_acceleration_structure_build_sizes(
            vk::AccelerationStructureBuildTypeKHR::DEVICE,
            &size_info,
            &[count.max(1)],
            &mut sizes,
        );
    }
    let storage_needed = sizes.acceleration_structure_size.max(256);
    let storage_fits = slot
        .accel
        .as_ref()
        .is_some_and(|accel| accel.storage.size >= storage_needed);
    let op = tlas_op(
        slot.accel.is_some(),
        slot.topology,
        slot.instance_count,
        slot.fast_trace,
        topology,
        count,
        fast_trace,
        force_build,
        storage_fits,
    );
    let scratch_needed = match op {
        TlasOp::Build => sizes.build_scratch_size,
        TlasOp::Update => sizes.update_scratch_size.max(sizes.build_scratch_size),
    };
    let scratch_align = gpu
        .rt_properties()
        .map(|props| u64::from(props.min_scratch_alignment))
        .unwrap_or(256);
    grow_gpu_buffer(
        gpu,
        &mut slot.scratch,
        align_scratch(scratch_needed.max(256), scratch_align),
        scratch_usage(),
        MemoryLocation::GpuOnly,
        "tlas-scratch",
    )?;
    let scratch_address = slot
        .scratch
        .as_ref()
        .ok_or_else(|| VulkanError::new("No hay scratch para el TLAS.".to_string()))?
        .address;

    if op == TlasOp::Build && !storage_fits {
        slot.accel = None;
        let storage =
            gpu.create_buffer(storage_needed, as_usage(), MemoryLocation::GpuOnly, "tlas")?;
        let handle = unsafe {
            loader.create_acceleration_structure(
                &vk::AccelerationStructureCreateInfoKHR::default()
                    .buffer(storage.buffer)
                    .size(storage_needed)
                    .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL),
                None,
            )
        }
        .map_err(|error| vk_error("crear el TLAS", error))?;
        let address = unsafe {
            loader.get_acceleration_structure_device_address(
                &vk::AccelerationStructureDeviceAddressInfoKHR::default()
                    .acceleration_structure(handle),
            )
        };
        slot.accel = Some(Accel {
            gpu: gpu.clone(),
            handle,
            storage,
            address,
        });
    }
    let Some(accel) = slot.accel.as_ref() else {
        return Err(VulkanError::new("El TLAS no se pudo crear.".to_string()));
    };
    let mode = match op {
        TlasOp::Build => vk::BuildAccelerationStructureModeKHR::BUILD,
        TlasOp::Update => vk::BuildAccelerationStructureModeKHR::UPDATE,
    };
    size_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
        .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL)
        .flags(flags)
        .mode(mode)
        .geometries(&geometries)
        .dst_acceleration_structure(accel.handle)
        .scratch_data(device_address_mut(scratch_address));
    if op == TlasOp::Update {
        size_info = size_info.src_acceleration_structure(accel.handle);
    }
    let range = [vk::AccelerationStructureBuildRangeInfoKHR::default().primitive_count(count)];
    let ranges: &[&[vk::AccelerationStructureBuildRangeInfoKHR]] = &[&range];
    unsafe {
        loader.cmd_build_acceleration_structures(cmd, &[size_info], ranges);
        let barrier = vk::MemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR)
            .dst_access_mask(
                vk::AccessFlags::ACCELERATION_STRUCTURE_READ_KHR | vk::AccessFlags::SHADER_READ,
            );
        gpu.device().cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
            vk::PipelineStageFlags::RAY_TRACING_SHADER_KHR
                | vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
            vk::DependencyFlags::empty(),
            &[barrier],
            &[],
            &[],
        );
    }
    slot.topology = topology;
    slot.instance_count = count;
    slot.fast_trace = fast_trace;
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::{TlasInstance, TlasOp, tlas_op, tlas_topology};

    fn inst(blas: u64, custom: u32, tx: f32) -> TlasInstance {
        TlasInstance {
            blas_address: blas,
            transform: [1.0, 0.0, 0.0, tx, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            custom_index: custom,
        }
    }

    #[test]
    fn topology_ignores_transforms_and_sees_set_changes() {
        let a = [inst(10, 0, 0.0), inst(20, 1, 0.0)];
        let moved = [inst(10, 0, 4.0), inst(20, 1, -1.0)];
        let extra = [inst(10, 0, 0.0), inst(20, 1, 0.0), inst(30, 2, 0.0)];
        let swapped = [inst(20, 0, 0.0), inst(10, 1, 0.0)];
        assert_eq!(tlas_topology(&a), tlas_topology(&moved));
        assert_ne!(tlas_topology(&a), tlas_topology(&extra));
        assert_ne!(tlas_topology(&a), tlas_topology(&swapped));
    }

    #[test]
    fn update_only_when_topology_count_and_flags_match() {
        let topology = tlas_topology(&[inst(1, 0, 0.0)]);
        assert_eq!(
            tlas_op(true, topology, 1, false, topology, 1, false, false, true),
            TlasOp::Update
        );
        assert_eq!(
            tlas_op(true, topology, 1, false, topology, 1, false, true, true),
            TlasOp::Build
        );
        assert_eq!(
            tlas_op(false, topology, 1, false, topology, 1, false, false, true),
            TlasOp::Build
        );
        assert_eq!(
            tlas_op(true, topology, 1, false, topology, 2, false, false, true),
            TlasOp::Build
        );
        assert_eq!(
            tlas_op(
                true,
                topology,
                1,
                false,
                topology.wrapping_add(1),
                1,
                false,
                false,
                true
            ),
            TlasOp::Build
        );
        assert_eq!(
            tlas_op(true, topology, 1, false, topology, 1, true, false, true),
            TlasOp::Build
        );
        assert_eq!(
            tlas_op(true, topology, 1, false, topology, 1, false, false, false),
            TlasOp::Build
        );
    }
}
