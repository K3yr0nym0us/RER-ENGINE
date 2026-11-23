//! BLAS y TLAS. El `unsafe` es FFI de `VK_KHR_acceleration_structure`: las uniones
//! de geometría no tienen constructor safe y el handle se destruye antes que el buffer.

use ash::vk;
use gpu_allocator::MemoryLocation;

use super::error::{VulkanError, vk_error};
use super::gpu::{GpuBuffer, SharedGpu};

const BLAS_FRAMES: usize = 2;

/// Huella de la geometría de un BLAS: solo cambia si cambia la topología o el formato.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BlasSignature {
    primitive_count: u32,
    vertex_stride: u64,
    vertex_address: u64,
    index_address: u64,
    fast_trace: bool,
}

pub struct Accel {
    gpu: SharedGpu,
    pub handle: vk::AccelerationStructureKHR,
    /// Mantiene viva la memoria del AS; su `Drop` libera el handle antes que el buffer.
    pub storage: Option<GpuBuffer>,
    pub address: u64,
    /// Scratch, firma y generación del BLAS de este frame en vuelo.
    scratch: Option<GpuBuffer>,
    signature: Option<BlasSignature>,
    built_generation: u64,
    /// Generación compartida por los dos estados: sube cuando cambia la geometría.
    generation: u64,
    /// Estado del otro frame en vuelo: AS y scratch separados para no reutilizarlos
    /// mientras el frame anterior todavía los está leyendo.
    twin: Option<Box<Accel>>,
}

impl Accel {
    /// Estado vacío para un BLAS que aún no se ha grabado en ningún command buffer.
    pub fn empty(gpu: &SharedGpu) -> Self {
        Self {
            gpu: gpu.clone(),
            handle: vk::AccelerationStructureKHR::null(),
            storage: None,
            address: 0,
            scratch: None,
            signature: None,
            built_generation: 0,
            generation: 0,
            twin: None,
        }
    }

    /// Dirección del BLAS del frame en vuelo `frame_slot`.
    pub fn address_for(&self, frame_slot: usize) -> u64 {
        if frame_slot.is_multiple_of(BLAS_FRAMES) {
            self.address
        } else {
            self.twin.as_ref().map_or(self.address, |twin| twin.address)
        }
    }
}

impl Drop for Accel {
    fn drop(&mut self) {
        if self.handle != vk::AccelerationStructureKHR::null()
            && let Some(accel) = self.gpu.accel()
        {
            unsafe { accel.destroy_acceleration_structure(self.handle, None) };
        }
        // `twin` se destruye solo: su Drop libera su propio handle y scratch.
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BlasOp {
    Build,
    Update,
}

/// UPDATE solo es legal si el BUILD anterior usó `ALLOW_UPDATE` y no cambió la firma.
fn blas_op(
    has_accel: bool,
    prev: Option<BlasSignature>,
    now: BlasSignature,
    storage_fits: bool,
    force_build: bool,
) -> BlasOp {
    if force_build || !has_accel || !storage_fits || prev != Some(now) {
        BlasOp::Build
    } else {
        BlasOp::Update
    }
}

fn blas_flags(fast_trace: bool) -> vk::BuildAccelerationStructureFlagsKHR {
    let quality = if fast_trace {
        vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE
    } else {
        vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_BUILD
    };
    // Sin ALLOW_UPDATE en el BUILD inicial, un UPDATE posterior sería inválido.
    quality | vk::BuildAccelerationStructureFlagsKHR::ALLOW_UPDATE
}

fn blas_signature(input: &BlasInput, fast_trace: bool) -> BlasSignature {
    BlasSignature {
        primitive_count: input.primitive_count,
        vertex_stride: input.vertex_stride,
        vertex_address: input.vertex_address,
        index_address: input.index_address,
        fast_trace,
    }
}

/// Graba el BUILD/UPDATE del BLAS en `cmd` y lo deja listo para el trazado de rayos
/// del frame (barrera AS_BUILD → RT). No hace submit ni espera fences.
pub fn record_blas(
    gpu: &SharedGpu,
    slot: &mut Accel,
    input: &BlasInput,
    fast_trace: bool,
    dirty: bool,
    frame_slot: usize,
    cmd: vk::CommandBuffer,
) -> Result<(), VulkanError> {
    let loader = gpu.accel().ok_or_else(|| {
        VulkanError::new("El device no expone VK_KHR_acceleration_structure.".to_string())
    })?;
    if dirty {
        slot.generation = slot.generation.wrapping_add(1);
    }
    let generation = slot.generation;
    let state: &mut Accel = if frame_slot.is_multiple_of(BLAS_FRAMES) {
        slot
    } else {
        slot.twin
            .get_or_insert_with(|| Box::new(Accel::empty(gpu)))
            .as_mut()
    };
    let signature = blas_signature(input, fast_trace);
    let has_accel = state.handle != vk::AccelerationStructureKHR::null();
    // Cada slot se actualiza en su propio frame, con la generación vigente: el slot
    // hermano se pondrá al día en su próximo frame sin mostrar una pose atrasada.
    if has_accel && state.signature == Some(signature) && state.built_generation == generation {
        return Ok(());
    }

    let primitive_count = input.primitive_count.max(1);
    let mut geometries = [vk::AccelerationStructureGeometryKHR::default()
        .geometry_type(vk::GeometryTypeKHR::TRIANGLES)
        .flags(vk::GeometryFlagsKHR::OPAQUE)];
    geometries[0].geometry.triangles = triangle_geometry(input);
    let flags = blas_flags(fast_trace);
    let size_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
        .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
        .flags(flags)
        .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
        .geometries(&geometries);
    let mut sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
    unsafe {
        loader.get_acceleration_structure_build_sizes(
            vk::AccelerationStructureBuildTypeKHR::DEVICE,
            &size_info,
            &[primitive_count],
            &mut sizes,
        );
    }
    let storage_needed = sizes.acceleration_structure_size.max(256);
    let storage_fits = state
        .storage
        .as_ref()
        .is_some_and(|storage| storage.size >= storage_needed);
    let op = blas_op(has_accel, state.signature, signature, storage_fits, false);
    let scratch_needed = match op {
        BlasOp::Build => sizes.build_scratch_size,
        BlasOp::Update => sizes.update_scratch_size.max(sizes.build_scratch_size),
    };
    let scratch_align = gpu
        .rt_properties()
        .map(|props| u64::from(props.min_scratch_alignment))
        .unwrap_or(256);
    grow_gpu_buffer(
        gpu,
        &mut state.scratch,
        align_scratch(scratch_needed.max(256), scratch_align),
        scratch_usage(),
        MemoryLocation::GpuOnly,
        "blas-scratch",
    )?;
    if op == BlasOp::Build && !storage_fits {
        if has_accel {
            unsafe { loader.destroy_acceleration_structure(state.handle, None) };
        }
        state.handle = vk::AccelerationStructureKHR::null();
        let storage =
            gpu.create_buffer(storage_needed, as_usage(), MemoryLocation::GpuOnly, "blas")?;
        let storage_buffer = storage.buffer;
        state.storage = Some(storage);
        state.handle = unsafe {
            loader.create_acceleration_structure(
                &vk::AccelerationStructureCreateInfoKHR::default()
                    .buffer(storage_buffer)
                    .size(storage_needed)
                    .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL),
                None,
            )
        }
        .map_err(|error| vk_error("crear el BLAS", error))?;
        state.address = unsafe {
            loader.get_acceleration_structure_device_address(
                &vk::AccelerationStructureDeviceAddressInfoKHR::default()
                    .acceleration_structure(state.handle),
            )
        };
    }
    let scratch_address = state
        .scratch
        .as_ref()
        .ok_or_else(|| VulkanError::new("No hay scratch para el BLAS.".to_string()))?
        .address;
    let mode = match op {
        BlasOp::Build => vk::BuildAccelerationStructureModeKHR::BUILD,
        BlasOp::Update => vk::BuildAccelerationStructureModeKHR::UPDATE,
    };
    let mut build = vk::AccelerationStructureBuildGeometryInfoKHR::default()
        .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
        .flags(flags)
        .mode(mode)
        .geometries(&geometries)
        .dst_acceleration_structure(state.handle)
        .scratch_data(device_address_mut(scratch_address));
    if op == BlasOp::Update {
        build = build.src_acceleration_structure(state.handle);
    }
    let range = [vk::AccelerationStructureBuildRangeInfoKHR::default()
        .primitive_count(input.primitive_count)
        .primitive_offset(0)
        .first_vertex(0)
        .transform_offset(0)];
    let ranges: &[&[vk::AccelerationStructureBuildRangeInfoKHR]] = &[&range];
    unsafe {
        loader.cmd_build_acceleration_structures(cmd, &[build], ranges);
        let barrier = vk::MemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_BUILD_KHR)
            .src_access_mask(vk::AccessFlags2::ACCELERATION_STRUCTURE_WRITE_KHR)
            .dst_stage_mask(
                vk::PipelineStageFlags2::RAY_TRACING_SHADER_KHR
                    | vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_BUILD_KHR,
            )
            .dst_access_mask(
                vk::AccessFlags2::ACCELERATION_STRUCTURE_READ_KHR | vk::AccessFlags2::SHADER_READ,
            );
        let barriers = [barrier];
        let dependency = vk::DependencyInfo::default().memory_barriers(&barriers);
        gpu.device().cmd_pipeline_barrier2(cmd, &dependency);
    }
    state.signature = Some(signature);
    state.built_generation = generation;
    Ok(())
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
    let storage_fits = slot.accel.as_ref().is_some_and(|accel| {
        accel
            .storage
            .as_ref()
            .is_some_and(|storage| storage.size >= storage_needed)
    });
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
            storage: Some(storage),
            address,
            scratch: None,
            signature: None,
            built_generation: 0,
            generation: 0,
            twin: None,
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
        let barrier = vk::MemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_BUILD_KHR)
            .src_access_mask(vk::AccessFlags2::ACCELERATION_STRUCTURE_WRITE_KHR)
            .dst_stage_mask(
                vk::PipelineStageFlags2::RAY_TRACING_SHADER_KHR
                    | vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_BUILD_KHR,
            )
            .dst_access_mask(
                vk::AccessFlags2::ACCELERATION_STRUCTURE_READ_KHR | vk::AccessFlags2::SHADER_READ,
            );
        let barriers = [barrier];
        let dependency = vk::DependencyInfo::default().memory_barriers(&barriers);
        gpu.device().cmd_pipeline_barrier2(cmd, &dependency);
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
    use super::{
        BlasInput, BlasOp, TlasInstance, TlasOp, align_scratch, blas_op, blas_signature, tlas_op,
        tlas_topology,
    };

    fn blas_in(vertex: u64, index: u64, stride: u64, triangles: u32) -> BlasInput {
        BlasInput {
            vertex_address: vertex,
            index_address: index,
            vertex_stride: stride,
            max_vertex: 3,
            primitive_count: triangles,
        }
    }

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

    #[test]
    fn blas_update_only_when_signature_and_storage_match() {
        let sig = blas_signature(&blas_in(100, 200, 12, 4), false);
        assert_eq!(blas_op(true, Some(sig), sig, true, false), BlasOp::Update);
        assert_eq!(blas_op(false, Some(sig), sig, true, false), BlasOp::Build);
        assert_eq!(blas_op(true, Some(sig), sig, false, false), BlasOp::Build);
        assert_eq!(blas_op(true, Some(sig), sig, true, true), BlasOp::Build);
        assert_eq!(blas_op(true, None, sig, true, false), BlasOp::Build);
    }

    #[test]
    fn blas_build_when_topology_or_flags_change() {
        let base = blas_signature(&blas_in(100, 200, 12, 4), false);
        let cases = [
            blas_signature(&blas_in(100, 200, 12, 5), false),
            blas_signature(&blas_in(100, 200, 16, 4), false),
            blas_signature(&blas_in(300, 200, 12, 4), false),
            blas_signature(&blas_in(100, 400, 12, 4), false),
            blas_signature(&blas_in(100, 200, 12, 4), true),
        ];
        for changed in cases {
            assert_eq!(
                blas_op(true, Some(base), changed, true, false),
                BlasOp::Build
            );
        }
    }

    #[test]
    fn scratch_alignment_only_grows_when_needed() {
        assert_eq!(align_scratch(100, 256), 256);
        assert_eq!(align_scratch(256, 256), 256);
        assert_eq!(align_scratch(257, 256), 512);
    }
}
