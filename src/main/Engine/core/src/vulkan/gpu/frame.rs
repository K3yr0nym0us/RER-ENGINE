use super::*;

pub(super) struct FrameSync {
    pub(super) acquire: vk::Semaphore,
    pub(super) fence: vk::Fence,
    pub(super) command: vk::CommandBuffer,
}

#[derive(Clone, Copy)]
pub struct FrameTarget {
    pub slot: usize,
    pub image_index: u32,
    pub command: vk::CommandBuffer,
    pub color_view: vk::ImageView,
    pub color_image: vk::Image,
    pub extent: vk::Extent2D,
    /// Origen del área de dibujo dentro de la imagen. El standalone lo deja en (0, 0);
    /// el editor lo usa para renderizar la escena solo en el rectángulo del viewport.
    pub offset: vk::Offset2D,
    #[allow(dead_code)]
    pub format: vk::Format,
}

pub enum FrameAcquire {
    Skip,
    Outdated,
    Fail(VulkanError),
}

pub(super) unsafe fn create_frames(
    device: &ash::Device,
    pool: vk::CommandPool,
) -> Result<[FrameSync; FRAMES_IN_FLIGHT], VulkanError> {
    let buffers = device
        .allocate_command_buffers(
            &vk::CommandBufferAllocateInfo::default()
                .command_pool(pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(FRAMES_IN_FLIGHT as u32),
        )
        .map_err(|error| vk_error("reservar los command buffers del frame", error))?;
    let mut frames = Vec::with_capacity(FRAMES_IN_FLIGHT);
    for command in buffers {
        let acquire = device
            .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
            .map_err(|error| vk_error("crear el semáforo de adquisición", error))?;
        let fence = device
            .create_fence(
                &vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED),
                None,
            )
            .map_err(|error| vk_error("crear el fence del frame", error))?;
        frames.push(FrameSync {
            acquire,
            fence,
            command,
        });
    }
    frames
        .try_into()
        .map_err(|_| VulkanError::new("No se pudieron crear los frames en vuelo.".to_string()))
}
