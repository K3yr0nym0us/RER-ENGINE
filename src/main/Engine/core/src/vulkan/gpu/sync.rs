//! Conversión explícita de barreras clásicas (Vulkan 1.0) a Synchronization2.
//!
//! Los bits concretos coinciden en `vulkan_core.h` 1.3/1.4 (`ALL_COMMANDS` es
//! `0x00010000` en Flags y Flags2; `TRANSFER`/`FRAGMENT_SHADER`/… igual).
//! `TOP_OF_PIPE` y `BOTTOM_OF_PIPE` no: en Sync2 el primer scope TOP ≡ `NONE`
//! y el segundo scope BOTTOM ≡ `NONE`; el extremo opuesto ≡ `ALL_COMMANDS`.

use ash::vk;

/// Qué extremo de la barrera se está traduciendo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarrierScope {
    Src,
    Dst,
}

/// `PipelineStageFlags` → `PipelineStageFlags2` según el scope de la barrera.
pub fn pipeline_stage_flags2(
    stage: vk::PipelineStageFlags,
    scope: BarrierScope,
) -> vk::PipelineStageFlags2 {
    let without_pipe =
        stage & !vk::PipelineStageFlags::TOP_OF_PIPE & !vk::PipelineStageFlags::BOTTOM_OF_PIPE;
    let mut out = matching_stages(without_pipe);
    match scope {
        BarrierScope::Src => {
            if stage.contains(vk::PipelineStageFlags::BOTTOM_OF_PIPE) {
                out |= vk::PipelineStageFlags2::ALL_COMMANDS;
            }
        }
        BarrierScope::Dst => {
            if stage.contains(vk::PipelineStageFlags::TOP_OF_PIPE) {
                out |= vk::PipelineStageFlags2::ALL_COMMANDS;
            }
        }
    }
    out
}

/// `AccessFlags` → `AccessFlags2`. Si el stage2 quedó vacío (`NONE`), el acceso
/// también tiene que ser `NONE` (VUID-VkMemoryBarrier2-srcAccessMask-03911).
pub fn access_flags2(access: vk::AccessFlags, stage2: vk::PipelineStageFlags2) -> vk::AccessFlags2 {
    if stage2.is_empty() {
        return vk::AccessFlags2::empty();
    }
    matching_access(access)
}

/// Barrera de imagen Synchronization2 a partir de etapas/accesos clásicos.
pub fn image_memory_barrier2(
    image: vk::Image,
    range: vk::ImageSubresourceRange,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
    src_stage: vk::PipelineStageFlags,
    dst_stage: vk::PipelineStageFlags,
    src_access: vk::AccessFlags,
    dst_access: vk::AccessFlags,
) -> vk::ImageMemoryBarrier2<'static> {
    let src_stage2 = pipeline_stage_flags2(src_stage, BarrierScope::Src);
    let dst_stage2 = pipeline_stage_flags2(dst_stage, BarrierScope::Dst);
    vk::ImageMemoryBarrier2::default()
        .src_stage_mask(src_stage2)
        .src_access_mask(access_flags2(src_access, src_stage2))
        .dst_stage_mask(dst_stage2)
        .dst_access_mask(access_flags2(dst_access, dst_stage2))
        .old_layout(old_layout)
        .new_layout(new_layout)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(range)
}

/// Barrera de buffer Synchronization2 a partir de etapas/accesos clásicos.
pub fn buffer_memory_barrier2(
    buffer: vk::Buffer,
    src_stage: vk::PipelineStageFlags,
    dst_stage: vk::PipelineStageFlags,
    src_access: vk::AccessFlags,
    dst_access: vk::AccessFlags,
) -> vk::BufferMemoryBarrier2<'static> {
    let src_stage2 = pipeline_stage_flags2(src_stage, BarrierScope::Src);
    let dst_stage2 = pipeline_stage_flags2(dst_stage, BarrierScope::Dst);
    vk::BufferMemoryBarrier2::default()
        .src_stage_mask(src_stage2)
        .src_access_mask(access_flags2(src_access, src_stage2))
        .dst_stage_mask(dst_stage2)
        .dst_access_mask(access_flags2(dst_access, dst_stage2))
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .buffer(buffer)
        .offset(0)
        .size(vk::WHOLE_SIZE)
}

fn matching_stages(stage: vk::PipelineStageFlags) -> vk::PipelineStageFlags2 {
    const PAIRS: &[(vk::PipelineStageFlags, vk::PipelineStageFlags2)] = &[
        (
            vk::PipelineStageFlags::DRAW_INDIRECT,
            vk::PipelineStageFlags2::DRAW_INDIRECT,
        ),
        (
            vk::PipelineStageFlags::VERTEX_INPUT,
            vk::PipelineStageFlags2::VERTEX_INPUT,
        ),
        (
            vk::PipelineStageFlags::VERTEX_SHADER,
            vk::PipelineStageFlags2::VERTEX_SHADER,
        ),
        (
            vk::PipelineStageFlags::TESSELLATION_CONTROL_SHADER,
            vk::PipelineStageFlags2::TESSELLATION_CONTROL_SHADER,
        ),
        (
            vk::PipelineStageFlags::TESSELLATION_EVALUATION_SHADER,
            vk::PipelineStageFlags2::TESSELLATION_EVALUATION_SHADER,
        ),
        (
            vk::PipelineStageFlags::GEOMETRY_SHADER,
            vk::PipelineStageFlags2::GEOMETRY_SHADER,
        ),
        (
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::PipelineStageFlags2::FRAGMENT_SHADER,
        ),
        (
            vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
            vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS,
        ),
        (
            vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
            vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS,
        ),
        (
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
        ),
        (
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags2::COMPUTE_SHADER,
        ),
        (
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags2::TRANSFER,
        ),
        (vk::PipelineStageFlags::HOST, vk::PipelineStageFlags2::HOST),
        (
            vk::PipelineStageFlags::ALL_GRAPHICS,
            vk::PipelineStageFlags2::ALL_GRAPHICS,
        ),
        (
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::PipelineStageFlags2::ALL_COMMANDS,
        ),
        (
            vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
            vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_BUILD_KHR,
        ),
        (
            vk::PipelineStageFlags::RAY_TRACING_SHADER_KHR,
            vk::PipelineStageFlags2::RAY_TRACING_SHADER_KHR,
        ),
    ];
    let mut out = vk::PipelineStageFlags2::empty();
    for &(src, dst) in PAIRS {
        if stage.contains(src) {
            out |= dst;
        }
    }
    out
}

fn matching_access(access: vk::AccessFlags) -> vk::AccessFlags2 {
    const PAIRS: &[(vk::AccessFlags, vk::AccessFlags2)] = &[
        (
            vk::AccessFlags::INDIRECT_COMMAND_READ,
            vk::AccessFlags2::INDIRECT_COMMAND_READ,
        ),
        (vk::AccessFlags::INDEX_READ, vk::AccessFlags2::INDEX_READ),
        (
            vk::AccessFlags::VERTEX_ATTRIBUTE_READ,
            vk::AccessFlags2::VERTEX_ATTRIBUTE_READ,
        ),
        (
            vk::AccessFlags::UNIFORM_READ,
            vk::AccessFlags2::UNIFORM_READ,
        ),
        (
            vk::AccessFlags::INPUT_ATTACHMENT_READ,
            vk::AccessFlags2::INPUT_ATTACHMENT_READ,
        ),
        (vk::AccessFlags::SHADER_READ, vk::AccessFlags2::SHADER_READ),
        (
            vk::AccessFlags::SHADER_WRITE,
            vk::AccessFlags2::SHADER_WRITE,
        ),
        (
            vk::AccessFlags::COLOR_ATTACHMENT_READ,
            vk::AccessFlags2::COLOR_ATTACHMENT_READ,
        ),
        (
            vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
        ),
        (
            vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ,
            vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ,
        ),
        (
            vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
            vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
        ),
        (
            vk::AccessFlags::TRANSFER_READ,
            vk::AccessFlags2::TRANSFER_READ,
        ),
        (
            vk::AccessFlags::TRANSFER_WRITE,
            vk::AccessFlags2::TRANSFER_WRITE,
        ),
        (vk::AccessFlags::HOST_READ, vk::AccessFlags2::HOST_READ),
        (vk::AccessFlags::HOST_WRITE, vk::AccessFlags2::HOST_WRITE),
        (vk::AccessFlags::MEMORY_READ, vk::AccessFlags2::MEMORY_READ),
        (
            vk::AccessFlags::MEMORY_WRITE,
            vk::AccessFlags2::MEMORY_WRITE,
        ),
        (
            vk::AccessFlags::ACCELERATION_STRUCTURE_READ_KHR,
            vk::AccessFlags2::ACCELERATION_STRUCTURE_READ_KHR,
        ),
        (
            vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR,
            vk::AccessFlags2::ACCELERATION_STRUCTURE_WRITE_KHR,
        ),
    ];
    let mut out = vk::AccessFlags2::empty();
    for &(src, dst) in PAIRS {
        if access.contains(src) {
            out |= dst;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{BarrierScope, access_flags2, image_memory_barrier2, pipeline_stage_flags2};
    use ash::vk;

    #[test]
    fn all_commands_maps_to_named_flags2() {
        assert_eq!(
            pipeline_stage_flags2(vk::PipelineStageFlags::ALL_COMMANDS, BarrierScope::Src),
            vk::PipelineStageFlags2::ALL_COMMANDS
        );
        assert_eq!(
            pipeline_stage_flags2(vk::PipelineStageFlags::ALL_COMMANDS, BarrierScope::Dst),
            vk::PipelineStageFlags2::ALL_COMMANDS
        );
        assert_ne!(
            vk::PipelineStageFlags2::ALL_COMMANDS,
            vk::PipelineStageFlags2::COPY
        );
    }

    #[test]
    fn concrete_stages_keep_named_flags2() {
        let src = vk::PipelineStageFlags::FRAGMENT_SHADER | vk::PipelineStageFlags::COMPUTE_SHADER;
        let got = pipeline_stage_flags2(src, BarrierScope::Src);
        assert_eq!(
            got,
            vk::PipelineStageFlags2::FRAGMENT_SHADER | vk::PipelineStageFlags2::COMPUTE_SHADER
        );
        assert_eq!(
            pipeline_stage_flags2(vk::PipelineStageFlags::TRANSFER, BarrierScope::Dst),
            vk::PipelineStageFlags2::TRANSFER
        );
    }

    #[test]
    fn top_of_pipe_src_is_none_dst_is_all_commands() {
        assert!(
            pipeline_stage_flags2(vk::PipelineStageFlags::TOP_OF_PIPE, BarrierScope::Src)
                .is_empty()
        );
        assert_eq!(
            pipeline_stage_flags2(vk::PipelineStageFlags::TOP_OF_PIPE, BarrierScope::Dst),
            vk::PipelineStageFlags2::ALL_COMMANDS
        );
    }

    #[test]
    fn bottom_of_pipe_src_is_all_commands_dst_is_none() {
        assert_eq!(
            pipeline_stage_flags2(vk::PipelineStageFlags::BOTTOM_OF_PIPE, BarrierScope::Src),
            vk::PipelineStageFlags2::ALL_COMMANDS
        );
        assert!(
            pipeline_stage_flags2(vk::PipelineStageFlags::BOTTOM_OF_PIPE, BarrierScope::Dst)
                .is_empty()
        );
    }

    #[test]
    fn empty_stage2_forces_empty_access() {
        assert!(
            access_flags2(
                vk::AccessFlags::MEMORY_WRITE,
                vk::PipelineStageFlags2::empty()
            )
            .is_empty()
        );
        assert_eq!(
            access_flags2(
                vk::AccessFlags::TRANSFER_WRITE,
                vk::PipelineStageFlags2::TRANSFER
            ),
            vk::AccessFlags2::TRANSFER_WRITE
        );
    }

    #[test]
    fn image_barrier_top_of_pipe_src_has_none_access() {
        let barrier = image_memory_barrier2(
            vk::Image::null(),
            vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            },
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::GENERAL,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::TRANSFER,
            vk::AccessFlags::empty(),
            vk::AccessFlags::TRANSFER_WRITE,
        );
        assert!(barrier.src_stage_mask.is_empty());
        assert!(barrier.src_access_mask.is_empty());
        assert_eq!(barrier.dst_stage_mask, vk::PipelineStageFlags2::TRANSFER);
        assert_eq!(barrier.dst_access_mask, vk::AccessFlags2::TRANSFER_WRITE);
    }
}
